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

use crate::implicit_compute::{compile_expr, BytecodeProgram, CompileError};
use grafito_core::function_sampling;
use grafito_core::object::{FunctionObj, FunctionSamples};
use std::collections::HashMap;

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
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FunctionParamsUniform {
    x_min: f32,
    x_max: f32,
    n: u32,
    code_len: u32,
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
        }
    }

    /// Evaluate the function `y = f(x)` on the GPU for an arbitrary expression
    /// string and return a vector of y values. Returns `None` if the expression
    /// cannot be compiled to GPU bytecode (caller should fall back to CPU).
    pub fn evaluate_expr(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        expr: &str,
        domain: (f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<f64>> {
        if grid_size > self.max_grid {
            return None;
        }

        let ast = grafito_geometry::expr::prepare_function_ast(expr, variables, &["x"]).ok()?;

        let mut prog = BytecodeProgram::default();
        compile_function_expr(&ast, variables, &mut prog).ok()?;

        let (x_min, x_max) = domain;
        let params = FunctionParamsUniform {
            x_min: x_min as f32,
            x_max: x_max as f32,
            n: (grid_size + 1) as u32,
            code_len: prog.code.len() as u32,
        };

        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&prog.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&prog.constants),
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
                timestamp_writes: None,
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
        queue.submit(std::iter::once(encoder.finish()));

        // Synchronously map the readback buffer. This blocks the CPU until the
        // GPU work finishes, matching the implicit-curve compute path.
        let slice = self.values_readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            if let Err(e) = result {
                log::error!("Function compute readback failed: {:?}", e);
            }
        });
        device.poll(wgpu::Maintain::Wait);

        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let ys: Vec<f64> = values_f32[..=grid_size]
            .iter()
            .map(|&v| if v.is_finite() { v as f64 } else { f64::NAN })
            .collect();
        drop(data);
        self.values_readback.unmap();

        Some(ys)
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

/// Try to populate the function cache using the GPU compute pipeline.
/// Returns `true` if the cache was populated (either already cached or freshly
/// computed on the GPU). Returns `false` if the GPU path is unavailable or the
/// expression is not supported by the bytecode machine.
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
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(ys) = compute.evaluate(device, queue, fun, padded_domain, grid_size, variables) else {
        return false;
    };

    let (x_min, x_max) = padded_domain;
    let dx = (x_max - x_min) / grid_size as f64;
    let samples: FunctionSamples = ys
        .into_iter()
        .enumerate()
        .map(|(i, y)| {
            let x = x_min + i as f64 * dx;
            let y_opt = if y.is_finite() && y.abs() < 1e6 {
                Some(y)
            } else {
                None
            };
            (x, y_opt)
        })
        .collect();

    *fun.cached_samples
        .write()
        .unwrap_or_else(|p| p.into_inner()) = samples;
    *fun.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}
