//! GPU compute pipeline for implicit-region fill masks.
//!
//! A WGSL compute shader interprets the same RPN bytecode stream as
//! [`crate::implicit_compute`] but evaluates `f(x, y) = lhs - rhs` at every
//! screen pixel (not on a coarse grid). The sign of `f` determines whether the
//! pixel is inside the filled region, and the shader writes a premultiplied
//! RGBA8 white mask into a `u32` output buffer. The resulting `Vec<u8>` can be
//! uploaded directly as an `egui::ColorImage` and tinted with the fill color
//! at draw time.
//!
//! If an expression uses operations not supported by the bytecode machine,
//! compilation fails and the caller falls back to the CPU scanline evaluator.

use crate::gpu_readback::{PendingGpuReadback, ReadbackPoll};
use crate::implicit_compute::{compile_expr, BytecodeProgram};
use grafito_core::RelationOperator;
use grafito_geometry::ast::Expr;
use std::collections::BTreeMap;
use std::sync::{atomic::AtomicBool, Arc};

/// Maximum number of output pixels the pipeline can handle in one dispatch.
/// 4096 × 4096 covers any practical desktop resolution; larger canvases fall
/// back to the CPU evaluator.
const MAX_PIXELS: usize = 4096 * 4096;

/// GPU resources needed to evaluate one fill mask per dispatch.
pub struct FillComputePipeline {
    pipeline: wgpu::ComputePipeline,
    /// FIX 3: `BindGroup` persistente creado en `new` (los buffers nunca se
    /// reasignan); se reusa en cada dispatch.
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    bytecode_buffer: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    output_readback: wgpu::Buffer,
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
    /// Caché de uploads de bytecode/constants (ver `UploadCache`).
    uploads: crate::gpu_readback::UploadCache,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FillParamsUniform {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    width: u32,
    height: u32,
    code_len: u32,
    op_code: u32, // 0=Less, 1=LessEq, 2=Greater, 3=GreaterEq
    fill_alpha: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// Submit compilado y validado, listo para `submit_buffers` (origen único
/// sync/async). Todo lo que toca la GPU sale de acá; el plan es puro CPU.
struct FillSubmit {
    params: FillParamsUniform,
    code: Vec<u32>,
    constants: Vec<f32>,
    pixel_count: usize,
}

/// Dispatch en vuelo: el submit ya está en la GPU y la espera se distribuye
/// en frames vía [`PendingGpuReadback`]. Se resuelve con `resolve_fill`.
/// El buffer readback pertenece al pipeline (persistente), así que el `wait`
/// puede cruzar frames sin mover memoria GPU entre threads.
#[derive(Debug)]
pub struct PendingFillEval {
    pixel_count: usize,
    wait: PendingGpuReadback,
}

impl PendingFillEval {
    /// Poll non-blocking delegado al waiter (para futuro slot en `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.wait.poll()
    }
}

impl FillComputePipeline {
    /// Create the fill compute pipeline and pre-allocate GPU buffers.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fill Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fill_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fill Compute Bind Group Layout"),
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
            label: Some("Fill Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Fill Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Compute Params"),
            size: std::mem::size_of::<FillParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Compute Bytecode"),
            size: 4096 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Zero-initialize the bytecode buffer so residual opcodes from a
        // previous evaluation cannot corrupt the interpreter stack.
        queue.write_buffer(
            &bytecode_buffer,
            0,
            &[0u8; 4096 * std::mem::size_of::<u32>()],
        );

        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Compute Constants"),
            size: 256 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Zero-initialize the constants buffer so residual values from a
        // previous evaluation cannot leak into the interpreter.
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 256 * std::mem::size_of::<f32>()],
        );

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Compute Output"),
            size: (MAX_PIXELS * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            // Map at creation so we can zero the buffer once. This avoids
            // leaving uninitialized GPU memory in the tail of the buffer
            // when the dispatch writes fewer than `MAX_PIXELS` samples.
            mapped_at_creation: true,
        });
        {
            let mut view = output_buffer.slice(..).get_mapped_range_mut();
            view.fill(0);
        }
        output_buffer.unmap();

        let output_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fill Compute Output Readback"),
            size: (MAX_PIXELS * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // FIX 3: BindGroup persistente (los buffers nunca se reasignan).
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fill Compute Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bytecode_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: constants_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            bind_group,
            params_buffer,
            bytecode_buffer,
            constants_buffer,
            output_buffer,
            output_readback,
            timing: crate::gpu_timing::create(device, queue, "Fill Compute", 1),
            uploads: crate::gpu_readback::UploadCache::new(),
        }
    }

    /// Compila y valida un submit sin tocar la GPU: origen único para el path
    /// síncrono (`evaluate_fill`) y el asíncrono (`dispatch_fill`).
    #[allow(clippy::too_many_arguments)]
    fn plan_submit(
        lhs: &Expr,
        rhs: &Expr,
        operator: RelationOperator,
        view_bounds: (f64, f64, f64, f64),
        canvas_size: (u32, u32),
        document_vars: &BTreeMap<String, f64>,
    ) -> Option<FillSubmit> {
        // Eq is a contour, not a region — no fill.
        let op_code = match operator {
            RelationOperator::Less => 0u32,
            RelationOperator::LessEq => 1u32,
            RelationOperator::Greater => 2u32,
            RelationOperator::GreaterEq => 3u32,
            RelationOperator::Eq => return None,
        };

        let (width, height) = canvas_size;
        if width == 0 || height == 0 {
            return None;
        }
        let pixel_count = (width as usize).checked_mul(height as usize)?;
        if pixel_count > MAX_PIXELS {
            return None;
        }

        // Build f = lhs - rhs and simplify (e.g. eliminate "x - 0" → "x").
        let combined = Expr::Sub(Box::new(lhs.clone()), Box::new(rhs.clone())).simplify();

        let mut prog = BytecodeProgram::default();
        compile_expr(&combined, document_vars, &mut prog).ok()?;

        let (x_min, x_max, y_min, y_max) = view_bounds;
        let params = FillParamsUniform {
            x_min: x_min as f32,
            x_max: x_max as f32,
            y_min: y_min as f32,
            y_max: y_max as f32,
            width,
            height,
            code_len: prog.code.len() as u32,
            op_code,
            fill_alpha: 1.0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        Some(FillSubmit {
            params,
            code: prog.code,
            constants: prog.constants,
            pixel_count,
        })
    }

    /// Escribe uniformes, hace submit del dispatch y arma el `map_async`.
    /// Barato y non-blocking: no espera a la GPU. Retorna el flag que el
    /// waiter background (o el poll síncrono legacy) observará.
    fn submit_buffers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submit: &FillSubmit,
    ) -> Arc<AtomicBool> {
        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::cast_slice(&[submit.params]),
        );
        // Los params cambian por viewport: siempre se suben. Bytecode y
        // constants se saltean si el programa no cambió (ver `UploadCache`).
        if self
            .uploads
            .code_changed(bytemuck::cast_slice(&submit.code))
        {
            queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&submit.code));
        }
        if self
            .uploads
            .constants_changed(bytemuck::cast_slice(&submit.constants))
        {
            queue.write_buffer(
                &self.constants_buffer,
                0,
                bytemuck::cast_slice(&submit.constants),
            );
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Fill Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Fill Compute Pass"),
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &self.bind_group, &[]);
            // Workgroup size is 8×8 = 64 threads.
            let wg_x = submit.params.width.div_ceil(8).max(1);
            let wg_y = submit.params.height.div_ceil(8).max(1);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }
        let copy_size = (submit.pixel_count * std::mem::size_of::<u32>()) as u64;
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.output_readback, 0, copy_size);
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.output_readback.slice(..copy_size);
        let map_ok = Arc::new(AtomicBool::new(false));
        let map_ok_clone = Arc::clone(&map_ok);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            } else {
                log::error!("Fill compute readback failed: {:?}", result.err());
            }
        });
        map_ok
    }

    /// Copia inmediata del readback ya mapeado (sin espera GPU). El llamante
    /// debe garantizar que el buffer está mapeado (`ReadbackPoll::Mapped`).
    fn copy_mapped_mask(&self, pixel_count: usize) -> Option<Vec<u8>> {
        let copy_size = (pixel_count * std::mem::size_of::<u32>()) as u64;
        let slice = self.output_readback.slice(..copy_size);
        let data = slice.get_mapped_range();
        // The u32 buffer is already packed as premultiplied RGBA8 via
        // packUnorm4x8(vec4(a,a,a,a)). In little-endian memory layout each u32
        // maps to bytes [R, G, B, A], which matches egui::ColorImage::RGBA.
        let pixels: Vec<u8> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        self.output_readback.unmap();
        Some(pixels)
    }

    /// Libera el buffer readback sin bloquear. Idempotente: si el `map_async`
    /// falló o sigue pendiente, es no-op seguro; se llama en todo camino de
    /// descarte (job obsoleto, timeout, objeto borrado) para no dejar el
    /// pipeline inutilizado.
    pub fn abort_fill(&self) {
        self.output_readback.unmap();
    }

    /// Dispatch sin espera: hace submit + `map_async` y retorna
    /// inmediatamente con un [`PendingFillEval`]. El hilo del frame nunca
    /// bloquea; el resolve llega en un frame posterior vía
    /// [`Self::resolve_fill`]. Retorna `None` en los mismos casos que
    /// [`Self::evaluate_fill`].
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_fill(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lhs: &Expr,
        rhs: &Expr,
        operator: RelationOperator,
        view_bounds: (f64, f64, f64, f64),
        canvas_size: (u32, u32),
        document_vars: &BTreeMap<String, f64>,
    ) -> Option<PendingFillEval> {
        let submit =
            Self::plan_submit(lhs, rhs, operator, view_bounds, canvas_size, document_vars)?;
        let pixel_count = submit.pixel_count;
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Fill compute async dispatch (wait distributed over frames)");
        Some(PendingFillEval {
            pixel_count,
            wait: PendingGpuReadback::submit(&map_ok),
        })
    }

    /// Resolve non-blocking de un dispatch previo. Solo copia si el poll ya
    /// reportó `Mapped`; en cualquier otro caso hace `unmap` y retorna `None`
    /// (el llamante usa el fallback CPU honesto). Nunca espera: el llamante
    /// debe haber hecho el `device.poll(Maintain::Poll)` no-bloqueante del
    /// frame antes de llamar.
    pub fn resolve_fill(&self, pending: PendingFillEval) -> Option<Vec<u8>> {
        let PendingFillEval {
            pixel_count,
            mut wait,
        } = pending;
        if wait.poll() != ReadbackPoll::Mapped {
            self.abort_fill();
            return None;
        }
        self.copy_mapped_mask(pixel_count)
    }

    /// Evaluate the fill mask for `lhs <op> rhs` on the GPU and return a
    /// premultiplied RGBA8 `Vec<u8>` of size `width * height * 4`.
    ///
    /// Returns `None` if:
    /// - the operator is `Eq` (no fill for equality — it is a contour only),
    /// - the expression cannot be compiled to GPU bytecode (caller falls back
    ///   to CPU),
    /// - the canvas exceeds the pre-allocated buffer capacity.
    ///
    /// The output is a white premultiplied mask: inside pixels have
    /// `R = G = B = A = fill_alpha * 255`, outside pixels are fully
    /// transparent. The caller can upload the bytes as `egui::ColorImage::RGBA`
    /// and tint with the actual fill color at draw time.
    ///
    /// Path síncrono legacy (bloquea hasta 250 ms): solo para callers sin slot
    /// background. El prepare usa `dispatch_fill` + `resolve_fill`.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_fill(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lhs: &Expr,
        rhs: &Expr,
        operator: RelationOperator,
        view_bounds: (f64, f64, f64, f64),
        canvas_size: (u32, u32),
        document_vars: &BTreeMap<String, f64>,
    ) -> Option<Vec<u8>> {
        let submit =
            Self::plan_submit(lhs, rhs, operator, view_bounds, canvas_size, document_vars)?;
        let pixel_count = submit.pixel_count;
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Fill compute sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Fill Compute");

        if !mapped {
            // `unmap` is idempotent: when `map_async` reported an error the
            // buffer was never mapped, so this is a no-op.
            self.output_readback.unmap();
            return None;
        }

        self.copy_mapped_mask(pixel_count)
    }
}
