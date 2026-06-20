//! GPU compute pipeline for parametric curves and surfaces.
//!
//! A single WGSL compute shader interprets a small RPN bytecode stream so that
//! arbitrary expressions (within the supported opcode set) can be evaluated on
//! the GPU without recompiling shaders. The Rust side compiles the parametric
//! expressions into bytecode, dispatches the compute kernel, reads back the
//! samples and stores them in the object's cache.
//!
//! If an expression uses operations that are not supported by the bytecode
//! machine, compilation fails and the caller falls back to the CPU evaluator.

use crate::implicit_compute::{compile_expr_with_mapping, BytecodeProgram, CompileError};
use grafito_core::object::{
    Curve2DSamples, Curve3DSamples, ParametricCurve2DObj, ParametricCurve3DObj, PolarCurveObj,
    Surface3DObj, SurfaceSamples,
};
use grafito_core::parametric_sampling;
use std::collections::HashMap;

const MAX_CURVE_STEPS: usize = 4000;
const MAX_SURFACE_RES: usize = 128;

/// GPU resources needed to evaluate parametric objects.
pub struct ParametricComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    bytecode_buffer: wgpu::Buffer,
    constants_buffer: wgpu::Buffer,
    values_buffer: wgpu::Buffer,
    values_readback: wgpu::Buffer,
    max_curve_samples: usize,
    max_surface_res: usize,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParametricParamsUniform {
    mode: u32,
    n: u32,
    m: u32,
    t_min: f32,
    t_max: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    code_len: u32,
    _pad: [u32; 2],
}

impl ParametricComputePipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        max_curve_samples: usize,
        max_surface_res: usize,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Parametric Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("parametric_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Parametric Compute Bind Group Layout"),
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
            label: Some("Parametric Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Parametric Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let max_curve_values = (max_curve_samples + 1) * 3;
        let max_surface_values = (max_surface_res + 1) * (max_surface_res + 1);
        let max_values = max_curve_values.max(max_surface_values);

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Parametric Compute Params"),
            size: std::mem::size_of::<ParametricParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Parametric Compute Bytecode"),
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
            label: Some("Parametric Compute Constants"),
            size: 256 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let values_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Parametric Compute Values"),
            size: (max_values * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let values_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Parametric Compute Values Readback"),
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
            max_curve_samples,
            max_surface_res,
        }
    }

    fn dispatch_and_readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: ParametricParamsUniform,
        prog: &BytecodeProgram,
        output_count: usize,
    ) -> Option<Vec<f32>> {
        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[params]));
        queue.write_buffer(&self.bytecode_buffer, 0, bytemuck::cast_slice(&prog.code));
        queue.write_buffer(
            &self.constants_buffer,
            0,
            bytemuck::cast_slice(&prog.constants),
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Parametric Compute Bind Group"),
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
            label: Some("Parametric Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Parametric Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);

            if params.mode == 3 {
                let wg = params.n.div_ceil(16).max(1);
                let wg_y = params.m.div_ceil(16).max(1);
                cpass.dispatch_workgroups(wg, wg_y, 1);
            } else {
                let wg = params.n.div_ceil(64).max(1);
                cpass.dispatch_workgroups(wg, 1, 1);
            }
        }
        encoder.copy_buffer_to_buffer(
            &self.values_buffer,
            0,
            &self.values_readback,
            0,
            (output_count * std::mem::size_of::<f32>()) as u64,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = self.values_readback.slice(..);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            } else {
                log::error!("Parametric compute readback failed: {:?}", result.err());
            }
        });
        device.poll(wgpu::Maintain::Wait);

        if !map_ok.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let out: Vec<f32> = values_f32[..output_count].to_vec();
        drop(data);
        self.values_readback.unmap();

        Some(out)
    }

    fn compile_parametric_expr(
        expr: &str,
        variables: &HashMap<String, f64>,
        var: &str,
        prog: &mut BytecodeProgram,
    ) -> Result<(), CompileError> {
        let ast = grafito_geometry::expr::prepare_function_ast(expr, variables, &[var])
            .map_err(CompileError::UnsupportedNode)?;
        compile_expr_with_mapping(&ast, variables, &[(var, 0)], prog)
    }

    fn resolve_expr(expr: &Option<String>, fallback: f64, variables: &HashMap<String, f64>) -> f64 {
        match expr {
            Some(e) => {
                let vars: Vec<(String, f64)> =
                    variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
                grafito_geometry::expr::evaluate(e, &vars)
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(fallback)
            }
            None => fallback,
        }
    }

    /// Evaluate a 2D parametric curve on the GPU.
    pub fn evaluate_curve_2d(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pc: &ParametricCurve2DObj,
        steps: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Curve2DSamples> {
        let steps = steps.clamp(1, self.max_curve_samples);
        let t_min = Self::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
        let t_max = Self::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
        if !t_min.is_finite() || !t_max.is_finite() {
            return None;
        }

        let mut prog = BytecodeProgram::default();
        Self::compile_parametric_expr(&pc.expr_x, variables, "t", &mut prog).ok()?;
        Self::compile_parametric_expr(&pc.expr_y, variables, "t", &mut prog).ok()?;

        let params = ParametricParamsUniform {
            mode: 0,
            n: (steps + 1) as u32,
            m: 0,
            t_min: t_min as f32,
            t_max: t_max as f32,
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            code_len: prog.code.len() as u32,
            _pad: [0; 2],
        };

        let output_count = (steps + 1) * 2;
        let values = self.dispatch_and_readback(device, queue, params, &prog, output_count)?;

        Some(
            values
                .chunks_exact(2)
                .map(|c| {
                    let x = if c[0].is_finite() {
                        c[0] as f64
                    } else {
                        f64::NAN
                    };
                    let y = if c[1].is_finite() {
                        c[1] as f64
                    } else {
                        f64::NAN
                    };
                    (x, y)
                })
                .collect(),
        )
    }

    /// Evaluate a 3D parametric curve on the GPU.
    pub fn evaluate_curve_3d(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pc: &ParametricCurve3DObj,
        steps: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Curve3DSamples> {
        let steps = steps.clamp(1, self.max_curve_samples);
        let t_min = Self::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
        let t_max = Self::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
        if !t_min.is_finite() || !t_max.is_finite() {
            return None;
        }

        let mut prog = BytecodeProgram::default();
        Self::compile_parametric_expr(&pc.expr_x, variables, "t", &mut prog).ok()?;
        Self::compile_parametric_expr(&pc.expr_y, variables, "t", &mut prog).ok()?;
        Self::compile_parametric_expr(&pc.expr_z, variables, "t", &mut prog).ok()?;

        let params = ParametricParamsUniform {
            mode: 1,
            n: (steps + 1) as u32,
            m: 0,
            t_min: t_min as f32,
            t_max: t_max as f32,
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            code_len: prog.code.len() as u32,
            _pad: [0; 2],
        };

        let output_count = (steps + 1) * 3;
        let values = self.dispatch_and_readback(device, queue, params, &prog, output_count)?;

        Some(
            values
                .chunks_exact(3)
                .map(|c| {
                    let x = if c[0].is_finite() {
                        c[0] as f64
                    } else {
                        f64::NAN
                    };
                    let y = if c[1].is_finite() {
                        c[1] as f64
                    } else {
                        f64::NAN
                    };
                    let z = if c[2].is_finite() {
                        c[2] as f64
                    } else {
                        f64::NAN
                    };
                    (x, y, z)
                })
                .collect(),
        )
    }

    /// Evaluate a polar curve on the GPU.
    pub fn evaluate_polar(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pol: &PolarCurveObj,
        steps: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Curve2DSamples> {
        let steps = steps.clamp(1, self.max_curve_samples);
        let t_min = Self::resolve_expr(&pol.t_min_expr, pol.t_min, variables);
        let t_max = Self::resolve_expr(&pol.t_max_expr, pol.t_max, variables);
        if !t_min.is_finite() || !t_max.is_finite() {
            return None;
        }

        let mut prog = BytecodeProgram::default();
        Self::compile_parametric_expr(&pol.expr_r, variables, "t", &mut prog).ok()?;

        let params = ParametricParamsUniform {
            mode: 2,
            n: (steps + 1) as u32,
            m: 0,
            t_min: t_min as f32,
            t_max: t_max as f32,
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            code_len: prog.code.len() as u32,
            _pad: [0; 2],
        };

        let output_count = (steps + 1) * 2;
        let values = self.dispatch_and_readback(device, queue, params, &prog, output_count)?;

        Some(
            values
                .chunks_exact(2)
                .map(|c| {
                    let x = if c[0].is_finite() {
                        c[0] as f64
                    } else {
                        f64::NAN
                    };
                    let y = if c[1].is_finite() {
                        c[1] as f64
                    } else {
                        f64::NAN
                    };
                    (x, y)
                })
                .collect(),
        )
    }

    /// Evaluate a 3D parametric surface on the GPU.
    pub fn evaluate_surface(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surf: &Surface3DObj,
        res: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<SurfaceSamples> {
        let res = res.clamp(1, self.max_surface_res);
        let x_min = Self::resolve_expr(&surf.x_min_expr, surf.x_min, variables);
        let x_max = Self::resolve_expr(&surf.x_max_expr, surf.x_max, variables);
        let y_min = Self::resolve_expr(&surf.y_min_expr, surf.y_min, variables);
        let y_max = Self::resolve_expr(&surf.y_max_expr, surf.y_max, variables);
        if !x_min.is_finite() || !x_max.is_finite() || !y_min.is_finite() || !y_max.is_finite() {
            return None;
        }

        let mut prog = BytecodeProgram::default();
        let ast = grafito_geometry::expr::prepare_function_ast(&surf.expr, variables, &["x", "y"])
            .ok()?;
        compile_expr_with_mapping(&ast, variables, &[("x", 0), ("y", 1)], &mut prog).ok()?;

        let params = ParametricParamsUniform {
            mode: 3,
            n: (res + 1) as u32,
            m: (res + 1) as u32,
            t_min: 0.0,
            t_max: 0.0,
            x_min: x_min as f32,
            x_max: x_max as f32,
            y_min: y_min as f32,
            y_max: y_max as f32,
            code_len: prog.code.len() as u32,
            _pad: [0; 2],
        };

        let output_count = (res + 1) * (res + 1);
        let values = self.dispatch_and_readback(device, queue, params, &prog, output_count)?;

        let mut grid = Vec::with_capacity(res + 1);
        for i in 0..=res {
            let mut row = Vec::with_capacity(res + 1);
            for j in 0..=res {
                let idx = j * (res + 1) + i;
                let v = values[idx];
                row.push(if v.is_finite() { v as f64 } else { f64::NAN });
            }
            grid.push(row);
        }
        Some(grid)
    }
}

/// Try to populate the 2D parametric curve cache using the GPU.
pub fn maybe_compute_curve_2d_on_gpu(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pc: &ParametricCurve2DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    let steps = steps.min(MAX_CURVE_STEPS);
    let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_curve_2d(device, queue, pc, steps, variables) else {
        return false;
    };

    *pc.cached_samples.write().unwrap_or_else(|p| p.into_inner()) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}

/// Try to populate the 3D parametric curve cache using the GPU.
pub fn maybe_compute_curve_3d_on_gpu(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pc: &ParametricCurve3DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    let steps = steps.min(MAX_CURVE_STEPS);
    let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_curve_3d(device, queue, pc, steps, variables) else {
        return false;
    };

    *pc.cached_samples.write().unwrap_or_else(|p| p.into_inner()) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}

/// Try to populate the polar curve cache using the GPU.
pub fn maybe_compute_polar_on_gpu(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pol: &PolarCurveObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    let steps = steps.min(MAX_CURVE_STEPS);
    let t_min = ParametricComputePipeline::resolve_expr(&pol.t_min_expr, pol.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pol.t_max_expr, pol.t_max, variables);
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pol.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_polar(device, queue, pol, steps, variables) else {
        return false;
    };

    *pol.cached_samples
        .write()
        .unwrap_or_else(|p| p.into_inner()) = samples;
    *pol.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}

/// Try to populate the 3D surface cache using the GPU.
pub fn maybe_compute_surface_on_gpu(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surf: &Surface3DObj,
    res: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    let res = res.min(MAX_SURFACE_RES);
    let x_min = ParametricComputePipeline::resolve_expr(&surf.x_min_expr, surf.x_min, variables);
    let x_max = ParametricComputePipeline::resolve_expr(&surf.x_max_expr, surf.x_max, variables);
    let y_min = ParametricComputePipeline::resolve_expr(&surf.y_min_expr, surf.y_min, variables);
    let y_max = ParametricComputePipeline::resolve_expr(&surf.y_max_expr, surf.y_max, variables);
    let key = grafito_core::SurfaceCacheKey {
        x_domain: (x_min, x_max),
        y_domain: (y_min, y_max),
        res,
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = surf.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(grid) = compute.evaluate_surface(device, queue, surf, res, variables) else {
        return false;
    };

    *surf.cached_grid.write().unwrap_or_else(|p| p.into_inner()) = grid;
    *surf.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    true
}
