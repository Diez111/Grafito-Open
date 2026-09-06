//! GPU compute pipeline for 1D function sampling.
//!
//! A single WGSL compute shader interprets a small RPN bytecode stream so that
//! arbitrary expressions (within the supported opcode set) can be evaluated on
//! the GPU without recompiling shaders. The Rust side compiles `y = f(x)` into
//! bytecode, dispatches the compute kernel, reads back the samples and stores
//! them in the function object's cache.
//!
//! If an expression uses operations that are not supported by the bytecode
//! machine, compilation fails and the caller falls back to the CPU evaluator.

use crate::gpu_readback::{PendingGpuReadback, ReadbackPoll};
use crate::implicit_compute::{
    compile_expr, f32_bounds_have_precision, BytecodeProgram, CompileError,
};
use grafito_core::function_sampling;
use grafito_core::object::{FunctionCacheKey, FunctionObj, FunctionSamples};
use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc};

const OP_PUSH_VAR_MASK: u32 = 0xFF;
const OP_PUSH_VAR_VALUE: u32 = 2;

/// Compile a 1D function expression into bytecode, rejecting references to
/// variables other than `x`.
fn compile_function_expr(
    expr: &grafito_geometry::ast::Expr,
    document_vars: &HashMap<String, f64>,
    prog: &mut BytecodeProgram,
) -> Result<(), CompileError> {
    compile_expr(expr, document_vars, prog)?;

    // The shared bytecode compiler maps `x` to operand 0 and `y` to operand 1.
    // Function objects are 1D, so any non-x variable reference is unsupported.
    for instr in &prog.code {
        let op = instr & OP_PUSH_VAR_MASK;
        let operand = instr >> 8;
        if op == OP_PUSH_VAR_VALUE && operand != 0 {
            return Err(CompileError::UnsupportedVariable("y".to_string()));
        }
    }
    Ok(())
}

/// GPU resources needed to evaluate one function per dispatch.
pub struct FunctionComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    bytecode_buffer: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    values_buffer: wgpu::Buffer,
    values_readback: wgpu::Buffer,
    max_grid: usize,
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FunctionParamsUniform {
    x_min: f32,
    x_max: f32,
    n: u32,
    code_len: u32,
}

/// Submit compilado y validado, listo para `submit_buffers` (origen único
/// sync/async).
struct FunctionSubmit {
    params: FunctionParamsUniform,
    code: Vec<u32>,
    constants: Vec<f32>,
}

/// Dispatch en vuelo: el submit ya está en la GPU y la espera corre en el
/// waiter background. Se resuelve con `resolve_eval` en un frame posterior.
/// El buffer readback pertenece al pipeline (persistente), así que el `wait`
/// puede cruzar frames sin mover memoria GPU entre threads.
#[derive(Debug)]
pub struct PendingFunctionEval {
    grid_size: usize,
    wait: PendingGpuReadback,
}

impl PendingFunctionEval {
    /// Poll non-blocking delegado al waiter (para el slot de `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.wait.poll()
    }
}

impl FunctionComputePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, max_grid: usize) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Function Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("function_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Function Compute Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Function Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Function Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let max_values = max_grid + 1;

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Function Compute Params"),
            size: std::mem::size_of::<FunctionParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Function Compute Bytecode"),
            size: 4096 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &bytecode_buffer,
            0,
            &[0u8; 4096 * std::mem::size_of::<u32>()],
        );

        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Function Compute Constants"),
            size: 256 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Zero-initialize the constants buffer so residual constants from a
        // previous evaluation cannot leak into the interpreter.
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 256 * std::mem::size_of::<f32>()],
        );

        let values_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Function Compute Values"),
            size: (max_values * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let values_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Function Compute Values Readback"),
            size: (max_values * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            bytecode_buffer,
            constants_buffer,
            values_buffer,
            values_readback,
            max_grid,
            timing: crate::gpu_timing::create(device, queue, "Function Compute", 1),
        }
    }

    /// Compila y valida un submit sin tocar la GPU: origen único para el path
    /// síncrono (`evaluate_expr`) y el asíncrono (`dispatch_expr`).
    fn plan_submit(
        &self,
        expr: &str,
        domain: (f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<FunctionSubmit> {
        if grid_size > self.max_grid {
            return None;
        }

        let ast = grafito_geometry::expr::prepare_function_ast(expr, variables, &["x"]).ok()?;

        let mut prog = BytecodeProgram::default();
        compile_function_expr(&ast, variables, &mut prog).ok()?;

        let (x_min, x_max) = domain;
        let min_step = (x_max - x_min).abs() / grid_size.max(1) as f64;
        if !f32_bounds_have_precision(&[x_min, x_max], min_step) {
            return None;
        }
        Some(FunctionSubmit {
            params: FunctionParamsUniform {
                x_min: x_min as f32,
                x_max: x_max as f32,
                n: (grid_size + 1) as u32,
                code_len: prog.code.len() as u32,
            },
            code: prog.code,
            constants: prog.constants,
        })
    }

    /// Escribe uniformes, hace submit del dispatch y arma el `map_async`.
    /// Barato y non-blocking: no espera a la GPU. Retorna el flag que el
    /// waiter background (o el poll síncrono legacy) observará.
    fn submit_buffers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submit: &FunctionSubmit,
        grid_size: usize,
    ) -> Arc<AtomicBool> {
        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::cast_slice(&[submit.params]),
        );
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&submit.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&submit.constants),
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Function Compute Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.bytecode_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.constants_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.values_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Function Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Function Compute Pass"),
                // GPU timing opt-in tras la feature `profiling` (ver gpu_timing);
                // sin la feature esto es `None` — cero costo en release.
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (grid_size as u32 + 1).div_ceil(64).max(1);
            cpass.dispatch_workgroups(wg, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.values_buffer,
            0,
            &self.values_readback,
            0,
            ((grid_size + 1) * std::mem::size_of::<f32>()) as u64,
        );
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        // Synchronously map the readback buffer. This blocks the CPU until the
        // GPU work finishes, matching the implicit-curve compute path.
        let slice = self.values_readback.slice(..);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            } else {
                log::error!("Function compute readback failed: {:?}", result.err());
            }
        });
        map_ok
    }

    /// Copia inmediata del readback ya mapeado (sin espera GPU). El llamante
    /// debe garantizar que el buffer está mapeado (`ReadbackPoll::Mapped`);
    /// si el rango no es válido se hace `unmap` y se retorna `None`.
    fn copy_mapped_values(&self, grid_size: usize) -> Option<Vec<f64>> {
        let slice = self.values_readback.slice(..);
        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let ys: Vec<f64> = values_f32
            .get(..=grid_size)?
            .iter()
            .map(|&v| if v.is_finite() { v as f64 } else { f64::NAN })
            .collect();
        drop(data);
        self.values_readback.unmap();
        Some(ys)
    }

    /// Libera el buffer readback sin bloquear. Idempotente: si el `map_async`
    /// falló o sigue pendiente, es no-op seguro; se llama en todo camino de
    /// descarte (job obsoleto, timeout, objeto borrado) para no dejar el
    /// pipeline inutilizado.
    pub fn abort_eval(&self) {
        self.values_readback.unmap();
    }

    /// Dispatch sin espera (frente B6): hace submit + `map_async` y retorna
    /// inmediatamente con un [`PendingFunctionEval`]. El hilo del frame nunca
    /// bloquea; el resolve llega en un frame posterior vía [`Self::resolve_eval`].
    /// Retorna `None` en los mismos casos que [`Self::evaluate_expr`].
    pub fn dispatch_expr(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &str,
        domain: (f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<PendingFunctionEval> {
        let submit = self.plan_submit(expr, domain, grid_size, variables)?;
        let map_ok = self.submit_buffers(device, queue, &submit, grid_size);
        log::trace!("Function compute async dispatch (wait distributed over frames)");
        Some(PendingFunctionEval {
            grid_size,
            wait: PendingGpuReadback::submit(&map_ok),
        })
    }

    /// Resolve non-blocking de un dispatch previo. Solo copia si el poll ya
    /// reportó `Mapped`; en cualquier otro caso hace `unmap` y retorna `None`
    /// (el llamante usa el fallback CPU honesto). Nunca espera: el llamante
    /// debe haber hecho el `device.poll(Maintain::Poll)` no-bloqueante del
    /// frame antes de llamar.
    pub fn resolve_eval(&self, pending: PendingFunctionEval) -> Option<Vec<f64>> {
        // Nota profiling: el path síncrono lee `gpu_timing::read_and_log` tras
        // el wait; aquí se omite a propósito porque su poll de 50 ms
        // reintroduciría bloqueo en el hilo UI. El `resolve` del encoder ya
        // quedó encolado en el submit y no afecta al resultado.
        let PendingFunctionEval {
            grid_size,
            mut wait,
        } = pending;
        if wait.poll() != ReadbackPoll::Mapped {
            self.abort_eval();
            return None;
        }
        self.copy_mapped_values(grid_size)
    }

    /// Evaluate the function `y = f(x)` on the GPU for an arbitrary expression
    /// string and return a vector of y values. Returns `None` if the expression
    /// cannot be compiled to GPU bytecode (caller should fall back to CPU).
    ///
    /// Path síncrono legacy (bloquea hasta 250 ms): solo para callers sin slot
    /// background. El prepare 2D usa `dispatch_expr` + `resolve_eval`.
    pub fn evaluate_expr(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &str,
        domain: (f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<f64>> {
        let submit = self.plan_submit(expr, domain, grid_size, variables)?;
        let map_ok = self.submit_buffers(device, queue, &submit, grid_size);
        log::trace!("Function compute sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Function Compute");

        if !mapped {
            // `unmap` is idempotent: when `map_async` reported an
            // error the buffer was never mapped, so this is a no-op.
            self.values_readback.unmap();
            return None;
        }
        self.copy_mapped_values(grid_size)
    }

    /// Evaluate a `FunctionObj` on the GPU by delegating to [`Self::evaluate_expr`].
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fun: &FunctionObj,
        domain: (f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<f64>> {
        self.evaluate_expr(device, queue, &fun.expr, domain, grid_size, variables)
    }
}

/// Evaluate `f(x)` on a uniform grid `[a, b]` with `samples` points using the
/// GPU compute pipeline. Returns only the `y` values.
///
/// This is the entry point for the hybrid integral path: the caller runs the
/// GPU kernel for the bulk evaluation and then applies a CPU quadrature rule
/// (for example, `grafito_geometry::integral::composite_simpson`) to obtain the
/// definite integral.
pub fn evaluate_function_batch_gpu(
    pipeline: &FunctionComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    expr: &str,
    a: f64,
    b: f64,
    samples: usize,
    variables: &HashMap<String, f64>,
) -> Result<Vec<f64>, String> {
    if samples < 2 {
        return Err("samples must be at least 2".to_string());
    }
    let grid_size = samples - 1;
    pipeline
        .evaluate_expr(device, queue, expr, (a, b), grid_size, variables)
        .ok_or_else(|| "GPU function evaluation failed (unsupported expression?)".to_string())
}

/// Job de cache en vuelo: conserva la `key` del dispatch para re-chequear
/// vigencia en el resolve (si la expresión/variables cambiaron entre frames,
/// el resultado se descarta en vez de escribir basura en el cache).
#[derive(Debug)]
pub struct PendingFunctionJob {
    key: FunctionCacheKey,
    padded_domain: (f64, f64),
    grid_size: usize,
    eval: PendingFunctionEval,
}

impl PendingFunctionJob {
    /// Poll non-blocking del waiter subyacente (para el slot de `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.eval.poll()
    }
}

/// Resultado de [`dispatch_function_on_gpu`]: `Cached` no tocó la GPU,
/// `Dispatched` ocupa el slot background (cap 1), `Unsupported` pide CPU.
#[derive(Debug)]
pub enum FunctionDispatchOutcome {
    Cached,
    Dispatched(PendingFunctionJob),
    Unsupported,
}

/// Dispatch sin espera a nivel cache (frente B6): replica los chequeos de
/// [`maybe_compute_function_on_gpu`] y retorna el job en vuelo en vez de
/// bloquear. El llamante guarda el job en el slot (cap 1) y lo resuelve con
/// [`resolve_function_job`] cuando el poll reporte `Mapped`.
pub fn dispatch_function_on_gpu(
    compute: &FunctionComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fun: &FunctionObj,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> FunctionDispatchOutcome {
    if fun.is_integral {
        // Integral functions need adaptive quadrature; GPU only evaluates the integrand.
        return FunctionDispatchOutcome::Unsupported;
    }
    let padded_domain = function_sampling::padded_snapped_domain(domain, 2.0, 64);
    let key = function_sampling::cache_key(fun, padded_domain, grid_size, variables);

    {
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return FunctionDispatchOutcome::Cached;
        }
    }

    match compute.dispatch_expr(
        device,
        queue,
        &fun.expr,
        padded_domain,
        grid_size,
        variables,
    ) {
        Some(eval) => FunctionDispatchOutcome::Dispatched(PendingFunctionJob {
            key,
            padded_domain,
            grid_size,
            eval,
        }),
        None => FunctionDispatchOutcome::Unsupported,
    }
}

/// Resolve non-blocking de un job de cache. Re-chequea la key contra el
/// estado actual del objeto: si cambió (edición entre dispatch y resolve),
/// hace `unmap` y retorna `false` sin escribir. Nunca espera.
/// Retorna `true` si el cache quedó poblado con la key vigente.
pub fn resolve_function_job(
    compute: &FunctionComputePipeline,
    fun: &FunctionObj,
    variables: &HashMap<String, f64>,
    job: PendingFunctionJob,
) -> bool {
    let PendingFunctionJob {
        key,
        padded_domain,
        grid_size,
        eval,
    } = job;
    // Vigencia: la key incluye expr, dominio con padding/snapping y variables
    // referenciadas; si el documento cambió, el resultado es de otra escena.
    let fresh = function_sampling::cache_key(fun, padded_domain, grid_size, variables);
    if fresh != key {
        log::debug!("Function GPU job obsoleto (key cambió); descartando sin escribir");
        compute.abort_eval();
        return false;
    }
    // Otro frame pudo poblar el mismo cache mientras la GPU trabajaba.
    {
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            compute.abort_eval();
            return true;
        }
    }
    let Some(ys) = compute.resolve_eval(eval) else {
        return false;
    };
    populate_function_cache(fun, &key, padded_domain, grid_size, ys);
    true
}

/// Escribe samples + key en el cache del objeto (origen único sync/async).
fn populate_function_cache(
    fun: &FunctionObj,
    key: &FunctionCacheKey,
    padded_domain: (f64, f64),
    grid_size: usize,
    ys: Vec<f64>,
) {
    let (x_min, x_max) = padded_domain;
    let dx = (x_max - x_min) / grid_size as f64;
    let samples: FunctionSamples = ys
        .into_iter()
        .enumerate()
        .map(|(i, y)| {
            let x = x_min + i as f64 * dx;
            // GPU results are already f32; retain every finite sample and let the
            // geometry builder apply its view-aware screen-space bounds.
            let y_opt = y.is_finite().then_some(y);
            (x, y_opt)
        })
        .collect();

    *fun.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *fun.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key.clone());
}

/// Try to populate the function cache using the GPU compute pipeline.
/// Returns `true` if the cache was populated (either already cached or freshly
/// computed on the GPU). Returns `false` if the GPU path is unavailable or the
/// expression is not supported by the bytecode machine.
///
/// Path síncrono legacy (bloquea hasta 250 ms). El prepare 2D usa
/// `dispatch_function_on_gpu` + `resolve_function_job`.
pub fn maybe_compute_function_on_gpu(
    compute: &FunctionComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fun: &FunctionObj,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    if fun.is_integral {
        // Integral functions need adaptive quadrature; GPU only evaluates the integrand.
        return false;
    }
    let padded_domain = function_sampling::padded_snapped_domain(domain, 2.0, 64);
    let key = function_sampling::cache_key(fun, padded_domain, grid_size, variables);

    {
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(ys) = compute.evaluate(device, queue, fun, padded_domain, grid_size, variables) else {
        return false;
    };

    populate_function_cache(fun, &key, padded_domain, grid_size, ys);
    true
}
