//! GPU compute pipeline for 2D vector-field sampling.
//!
//! A single WGSL compute shader interprets a small RPN bytecode stream so that
//! arbitrary P(x,y) and Q(x,y) expressions (within the supported opcode set) can
//! be evaluated on the GPU without recompiling shaders. The Rust side compiles
//! both expressions into a single bytecode program, dispatches a 2D compute
//! kernel, reads back (x, y, u, v) samples and stores them in the vector-field
//! object's cache.
//!
//! If an expression uses operations that are not supported by the bytecode
//! machine, compilation fails and the caller falls back to the CPU evaluator.

use crate::implicit_compute::{compile_expr, BytecodeProgram};
use grafito_core::object::{VectorField2DObj, VectorFieldSamples};
use grafito_core::vector_field_sampling;
use std::collections::HashMap;

/// GPU resources needed to evaluate one 2D vector field per dispatch.
pub struct VectorComputePipeline {
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
struct VectorParamsUniform {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    nx: u32,
    ny: u32,
    _pad0: u32,
    _pad1: u32,
}

impl VectorComputePipeline {
    pub fn new(device: &wgpu::Device, max_grid: usize) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vector Field Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("vector_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Vector Compute Bind Group Layout"),
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
            label: Some("Vector Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Vector Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let max_values = (max_grid + 1) * (max_grid + 1) * 4;

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Compute Params"),
            size: std::mem::size_of::<VectorParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Compute Bytecode"),
            size: 4096 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Compute Constants"),
            size: 256 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let values_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Compute Values"),
            size: (max_values * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let values_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vector Compute Values Readback"),
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

    /// Evaluate the 2D vector field on the GPU and return (x, y, u, v) samples.
    /// Returns `None` if the expression cannot be compiled to GPU bytecode.
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vf: &VectorField2DObj,
        bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<VectorFieldSamples> {
        if grid_size > self.max_grid {
            return None;
        }

        let ast_u =
            grafito_geometry::expr::prepare_function_ast(&vf.expr_u, variables, &["x", "y"])
                .ok()?;
        let ast_v =
            grafito_geometry::expr::prepare_function_ast(&vf.expr_v, variables, &["x", "y"])
                .ok()?;

        let mut prog = BytecodeProgram::default();
        compile_expr(&ast_u, variables, &mut prog).ok()?;
        compile_expr(&ast_v, variables, &mut prog).ok()?;

        let (x_min, x_max, y_min, y_max) = bounds;
        let params = VectorParamsUniform {
            x_min: x_min as f32,
            x_max: x_max as f32,
            y_min: y_min as f32,
            y_max: y_max as f32,
            nx: grid_size as u32,
            ny: grid_size as u32,
            _pad0: 0,
            _pad1: 0,
        };

        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&prog.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&prog.constants),
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vector Compute Bind Group"),
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
            label: Some("Vector Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Vector Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg_x = (grid_size as u32 + 1).div_ceil(16).max(1);
            let wg_y = (grid_size as u32 + 1).div_ceil(16).max(1);
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        let output_count = (grid_size + 1) * (grid_size + 1) * 4;
        encoder.copy_buffer_to_buffer(
            &self.values_buffer,
            0,
            &self.values_readback,
            0,
            (output_count * std::mem::size_of::<f32>()) as u64,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.values_readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            if let Err(e) = result {
                log::error!("Vector field compute readback failed: {:?}", e);
            }
        });
        device.poll(wgpu::Maintain::Wait);

        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let mut samples = Vec::with_capacity((grid_size + 1) * (grid_size + 1));

        for j in 0..=grid_size {
            for i in 0..=grid_size {
                let base = (j * (grid_size + 1) + i) * 4;
                let x = values_f32[base] as f64;
                let y = values_f32[base + 1] as f64;
                let u = values_f32[base + 2] as f64;
                let v = values_f32[base + 3] as f64;
                let u = if u.is_finite() && u.abs() < 1e6 {
                    u
                } else {
                    f64::NAN
                };
                let v = if v.is_finite() && v.abs() < 1e6 {
                    v
                } else {
                    f64::NAN
                };
                samples.push((x, y, u, v));
            }
        }

        drop(data);
        self.values_readback.unmap();

        Some(samples)
    }
}

/// Try to populate the 2D vector-field cache using the GPU compute pipeline.
/// Returns `true` if the cache was populated (either already cached or freshly
/// computed on the GPU). Returns `false` if the GPU path is unavailable or the
/// expression is not supported by the bytecode machine.
pub fn maybe_compute_vector_field_on_gpu(
    compute: &VectorComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vf: &VectorField2DObj,
    view: &grafito_geometry::ViewTransform,
    variables: &HashMap<String, f64>,
) -> bool {
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(view.screen_size);
    let view_bounds = (world_tl.x, world_br.x, world_tl.y, world_br.y);
    let padded_bounds = vector_field_sampling::padded_snapped_bounds(view_bounds, 2.0, 64);
    let grid_size = vf.density.clamp(5, 128);

    let key = vector_field_sampling::cache_key(vf, padded_bounds, grid_size, variables);
    {
        let cached_key = vf.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate(device, queue, vf, padded_bounds, grid_size, variables)
    else {
        return false;
    };

    *vf.cached_samples.write().unwrap_or_else(|p| p.into_inner()) = samples;
    *vf.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}
