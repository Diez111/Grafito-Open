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

use crate::implicit_compute::{
    compile_expr_with_mapping, f32_bounds_have_precision, BytecodeProgram, CompileError,
};
use grafito_core::object::{
    Curve2DSamples, Curve3DSamples, ParametricCurve2DObj, ParametricCurve3DObj, PolarCurveObj,
    Surface3DObj, SurfaceSamples,
};
use grafito_core::parametric_sampling;
use std::collections::HashMap;

/// Presupuesto de muestras por curva paramétrica: una curva densa se evalúa en
/// UN solo dispatch de `steps + 1 <= MAX_CURVE_STEPS + 1` workgroups (64 hilos
/// por workgroup). Ver `docs/architecture.md:8` — MAX_CURVE_STEPS 4000.
const MAX_CURVE_STEPS: usize = 4000;
/// Presupuesto de resolución por superficie 3D (grid `(res+1)²`, 129×129 máx).
/// Ver `docs/architecture.md:8` — MAX_SURFACE_RES 128.
const MAX_SURFACE_RES: usize = 128;

/// Rechaza resoluciones por encima del presupuesto [`MAX_SURFACE_RES`] antes de
/// tocar la GPU. `res == 0` se normaliza a 1 por el caller.
fn surface_res_within_budget(res: usize) -> bool {
    (1..=MAX_SURFACE_RES).contains(&res)
}
// `exp(88.8)` is near `f32::MAX`; leave a margin so results do not depend on
// whether an individual GPU backend saturates or produces infinity.
const MAX_SAFE_F32_EXP_ARGUMENT: f64 = 88.0;

fn has_strictly_increasing_finite_bounds(bounds: &[f64]) -> bool {
    bounds.iter().all(|bound| bound.is_finite())
        && bounds.windows(2).all(|bounds| bounds[0] < bounds[1])
}

fn any_exp_argument_matches(
    expression: &grafito_geometry::ast::Expr,
    predicate: &impl Fn(&grafito_geometry::ast::Expr) -> bool,
) -> bool {
    use grafito_geometry::ast::Expr;

    match expression {
        Expr::Const(_) | Expr::Var(_) => false,
        Expr::Exp(argument) => predicate(argument) || any_exp_argument_matches(argument, predicate),
        Expr::Neg(argument)
        | Expr::Sin(argument)
        | Expr::Cos(argument)
        | Expr::Tan(argument)
        | Expr::Asin(argument)
        | Expr::Acos(argument)
        | Expr::Atan(argument)
        | Expr::Ln(argument)
        | Expr::Log(argument)
        | Expr::Sqrt(argument)
        | Expr::Abs(argument)
        | Expr::Sinh(argument)
        | Expr::Cosh(argument)
        | Expr::Tanh(argument)
        | Expr::Floor(argument)
        | Expr::Ceil(argument)
        | Expr::Round(argument)
        | Expr::Sec(argument)
        | Expr::Csc(argument)
        | Expr::Cot(argument)
        | Expr::Asinh(argument)
        | Expr::Acosh(argument)
        | Expr::Atanh(argument)
        | Expr::Sign(argument)
        | Expr::Heaviside(argument)
        | Expr::Cbrt(argument)
        | Expr::Re(argument)
        | Expr::Im(argument)
        | Expr::Arg(argument)
        | Expr::Conj(argument)
        | Expr::Erf(argument)
        | Expr::Erfc(argument)
        | Expr::Gamma(argument)
        | Expr::LnGamma(argument)
        | Expr::Digamma(argument)
        | Expr::Trigamma(argument) => any_exp_argument_matches(argument, predicate),
        Expr::Add(left, right)
        | Expr::Sub(left, right)
        | Expr::Mul(left, right)
        | Expr::Div(left, right)
        | Expr::Pow(left, right)
        | Expr::Atan2(left, right)
        | Expr::Modulo(left, right)
        | Expr::Min(left, right)
        | Expr::Max(left, right)
        | Expr::Beta(left, right)
        | Expr::BesselJ(left, right)
        | Expr::BesselY(left, right)
        | Expr::BesselI(left, right)
        | Expr::Lt(left, right)
        | Expr::Gt(left, right)
        | Expr::Le(left, right)
        | Expr::Ge(left, right)
        | Expr::Eq(left, right)
        | Expr::Ne(left, right) => {
            any_exp_argument_matches(left, predicate) || any_exp_argument_matches(right, predicate)
        }
        Expr::Clamp(value, lower, upper) => {
            any_exp_argument_matches(value, predicate)
                || any_exp_argument_matches(lower, predicate)
                || any_exp_argument_matches(upper, predicate)
        }
        Expr::Sum(body, _, start, end) | Expr::Product(body, _, start, end) => {
            any_exp_argument_matches(body, predicate)
                || any_exp_argument_matches(start, predicate)
                || any_exp_argument_matches(end, predicate)
        }
        Expr::Piecewise(parts, default) => {
            parts.iter().any(|(condition, value)| {
                any_exp_argument_matches(condition, predicate)
                    || any_exp_argument_matches(value, predicate)
            }) || any_exp_argument_matches(default, predicate)
        }
    }
}

fn curve_expression_has_unsafe_f32_exp(
    expression: &str,
    parameter: &str,
    bounds: (f64, f64),
    variables: &HashMap<String, f64>,
) -> bool {
    let Ok(expression) = grafito_geometry::ast::parse_ast(expression) else {
        return false;
    };
    let expression = expression.substitute_vars(variables, &[parameter]);
    let samples = [bounds.0, (bounds.0 + bounds.1) * 0.5, bounds.1];
    any_exp_argument_matches(&expression, &|argument| {
        samples
            .iter()
            .any(|sample| argument.eval_at(parameter, *sample) >= MAX_SAFE_F32_EXP_ARGUMENT)
    })
}

fn surface_expression_has_unsafe_f32_exp(
    expression: &str,
    x_bounds: (f64, f64),
    y_bounds: (f64, f64),
    variables: &HashMap<String, f64>,
) -> bool {
    let Ok(expression) = grafito_geometry::ast::parse_ast(expression) else {
        return false;
    };
    let expression = expression.substitute_vars(variables, &["x", "y"]);
    let x_samples = [x_bounds.0, (x_bounds.0 + x_bounds.1) * 0.5, x_bounds.1];
    let y_samples = [y_bounds.0, (y_bounds.0 + y_bounds.1) * 0.5, y_bounds.1];
    any_exp_argument_matches(&expression, &|argument| {
        x_samples.iter().any(|x| {
            y_samples
                .iter()
                .any(|y| argument.eval_2d("x", *x, "y", *y) >= MAX_SAFE_F32_EXP_ARGUMENT)
        })
    })
}

/// One parametric curve preflighted and compiled for a batched GPU dispatch.
struct PreparedCurve {
    params: ParametricParamsUniform,
    prog: BytecodeProgram,
    output_count: usize,
}

fn prepare_curve_2d(
    pc: &ParametricCurve2DObj,
    steps: usize,
    max_curve_samples: usize,
    variables: &HashMap<String, f64>,
) -> Option<PreparedCurve> {
    let steps = steps.clamp(1, max_curve_samples);
    let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return None;
    }
    if curve_expression_has_unsafe_f32_exp(&pc.expr_x, "t", (t_min, t_max), variables)
        || curve_expression_has_unsafe_f32_exp(&pc.expr_y, "t", (t_min, t_max), variables)
    {
        return None;
    }
    let min_step = (t_max - t_min).abs() / steps.max(1) as f64;
    if !f32_bounds_have_precision(&[t_min, t_max], min_step) {
        return None;
    }
    let mut prog = BytecodeProgram::default();
    ParametricComputePipeline::compile_parametric_expr(&pc.expr_x, variables, "t", &mut prog)
        .ok()?;
    ParametricComputePipeline::compile_parametric_expr(&pc.expr_y, variables, "t", &mut prog)
        .ok()?;
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
    Some(PreparedCurve {
        params,
        prog,
        output_count: (steps + 1) * 2,
    })
}

fn prepare_curve_3d(
    pc: &ParametricCurve3DObj,
    steps: usize,
    max_curve_samples: usize,
    variables: &HashMap<String, f64>,
) -> Option<PreparedCurve> {
    let steps = steps.clamp(1, max_curve_samples);
    let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return None;
    }
    if curve_expression_has_unsafe_f32_exp(
        &pc.expr_x,
        pc.parameter.as_str(),
        (t_min, t_max),
        variables,
    ) || curve_expression_has_unsafe_f32_exp(
        &pc.expr_y,
        pc.parameter.as_str(),
        (t_min, t_max),
        variables,
    ) || curve_expression_has_unsafe_f32_exp(
        &pc.expr_z,
        pc.parameter.as_str(),
        (t_min, t_max),
        variables,
    ) {
        return None;
    }
    let min_step = (t_max - t_min).abs() / steps.max(1) as f64;
    if !f32_bounds_have_precision(&[t_min, t_max], min_step) {
        return None;
    }
    let prog = compile_curve_3d_program(pc, variables).ok()?;
    if !curve_3d_samples_preserve_f32_resolution(pc, steps, variables) {
        return None;
    }
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
    Some(PreparedCurve {
        params,
        prog,
        output_count: (steps + 1) * 3,
    })
}

fn prepare_polar(
    pol: &PolarCurveObj,
    steps: usize,
    max_curve_samples: usize,
    variables: &HashMap<String, f64>,
) -> Option<PreparedCurve> {
    let steps = steps.clamp(1, max_curve_samples);
    let t_min = ParametricComputePipeline::resolve_expr(&pol.t_min_expr, pol.t_min, variables);
    let t_max = ParametricComputePipeline::resolve_expr(&pol.t_max_expr, pol.t_max, variables);
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return None;
    }
    if curve_expression_has_unsafe_f32_exp(&pol.expr_r, "t", (t_min, t_max), variables) {
        return None;
    }
    let min_step = (t_max - t_min).abs() / steps.max(1) as f64;
    if !f32_bounds_have_precision(&[t_min, t_max], min_step) {
        return None;
    }
    let mut prog = BytecodeProgram::default();
    ParametricComputePipeline::compile_parametric_expr(&pol.expr_r, variables, "t", &mut prog)
        .ok()?;
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
    Some(PreparedCurve {
        params,
        prog,
        output_count: (steps + 1) * 2,
    })
}

fn curve_2d_from_values(values: &[f32]) -> Curve2DSamples {
    values
        .as_chunks::<2>()
        .0
        .iter()
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
        .collect()
}

fn curve_3d_from_values(values: &[f32]) -> Curve3DSamples {
    values
        .as_chunks::<3>()
        .0
        .iter()
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
        .collect()
}

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
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
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
        // Zero-initialize the constants buffer so residual constants from a
        // previous evaluation cannot leak into the interpreter.
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 256 * std::mem::size_of::<f32>()],
        );

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
            timing: crate::gpu_timing::create(device, queue, "Parametric Compute", 1),
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
                // GPU timing opt-in tras la feature `profiling` (ver gpu_timing);
                // sin la feature esto es `None` — cero costo en release.
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
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
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
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
        // TODO P1: mover a spawn_blocking — el readback síncrono sigue bloqueando
        // el hilo de prepare (acotado a 1 intento por frame via
        // MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE en canvas.rs). Mitigación:
        // poll acotado con timeout en vez de Wait infinito.
        log::trace!("Parametric compute sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Parametric Compute");

        if !mapped {
            // `unmap` is idempotent: when `map_async` reported an
            // error the buffer was never mapped, so this is a no-op.
            self.values_readback.unmap();
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
        let prepared = prepare_curve_2d(pc, steps, self.max_curve_samples, variables)?;
        let values = self.dispatch_and_readback(
            device,
            queue,
            prepared.params,
            &prepared.prog,
            prepared.output_count,
        )?;
        Some(curve_2d_from_values(&values))
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
        let prepared = prepare_curve_3d(pc, steps, self.max_curve_samples, variables)?;
        let values = self.dispatch_and_readback(
            device,
            queue,
            prepared.params,
            &prepared.prog,
            prepared.output_count,
        )?;
        Some(curve_3d_from_values(&values))
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
        let prepared = prepare_polar(pol, steps, self.max_curve_samples, variables)?;
        let values = self.dispatch_and_readback(
            device,
            queue,
            prepared.params,
            &prepared.prog,
            prepared.output_count,
        )?;
        Some(curve_2d_from_values(&values))
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
        if surf.is_parametric || surf.is_complex || surf.legacy_axis_swap {
            return None;
        }
        let res = res.max(1);
        if res > self.max_surface_res {
            return None;
        }
        debug_assert!(surface_res_within_budget(res));
        let x_min = Self::resolve_expr(&surf.x_min_expr, surf.x_min, variables);
        let x_max = Self::resolve_expr(&surf.x_max_expr, surf.x_max, variables);
        let y_min = Self::resolve_expr(&surf.y_min_expr, surf.y_min, variables);
        let y_max = Self::resolve_expr(&surf.y_max_expr, surf.y_max, variables);
        if !has_strictly_increasing_finite_bounds(&[x_min, x_max])
            || !has_strictly_increasing_finite_bounds(&[y_min, y_max])
        {
            return None;
        }
        if surface_expression_has_unsafe_f32_exp(
            &surf.expr,
            (x_min, x_max),
            (y_min, y_max),
            variables,
        ) {
            return None;
        }
        let min_step = ((x_max - x_min).abs() / res.max(1) as f64)
            .min((y_max - y_min).abs() / res.max(1) as f64);
        if !f32_bounds_have_precision(&[x_min, x_max, y_min, y_max], min_step) {
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
                let x = x_min + (i as f64 / res as f64) * (x_max - x_min);
                let y = y_min + (j as f64 / res as f64) * (y_max - y_min);
                let z = if v.is_finite() { v as f64 } else { f64::NAN };
                row.push(grafito_geometry::Point3D::new(x, y, z));
            }
            grid.push(row);
        }
        Some(grid)
    }

    /// Evaluate multiple 2D parametric curves in a single GPU submit.
    ///
    /// Cada curva densa se despacha en un solo dispatch de
    /// `steps + 1 <= MAX_CURVE_STEPS + 1` workgroups (64 hilos/workgroup), pero
    /// todas las curvas comparten un único encoder, un único `submit` y un
    /// único poll de readback acotado — en vez de N submits + N polls. Devuelve
    /// una entrada por curva de entrada; `None` si la curva falla el preflight
    /// o el readback del batch falla.
    pub fn evaluate_curves_2d_batched(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        curves: &[(&ParametricCurve2DObj, usize)],
        variables: &HashMap<String, f64>,
    ) -> Vec<Option<Curve2DSamples>> {
        let jobs: Vec<Option<PreparedCurve>> = curves
            .iter()
            .map(|(pc, steps)| prepare_curve_2d(pc, *steps, self.max_curve_samples, variables))
            .collect();
        let prepared: Vec<&PreparedCurve> = jobs.iter().flatten().collect();
        let results = self.dispatch_batch(device, queue, &prepared);
        let mut results_iter = results.into_iter();
        jobs.into_iter()
            .map(|job| {
                job?;
                let values = results_iter.next().flatten()?;
                Some(curve_2d_from_values(&values))
            })
            .collect()
    }

    /// Evaluate multiple 3D parametric curves in a single GPU submit. Misma
    /// semántica de batch que [`Self::evaluate_curves_2d_batched`].
    pub fn evaluate_curves_3d_batched(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        curves: &[(&ParametricCurve3DObj, usize)],
        variables: &HashMap<String, f64>,
    ) -> Vec<Option<Curve3DSamples>> {
        let jobs: Vec<Option<PreparedCurve>> = curves
            .iter()
            .map(|(pc, steps)| prepare_curve_3d(pc, *steps, self.max_curve_samples, variables))
            .collect();
        let prepared: Vec<&PreparedCurve> = jobs.iter().flatten().collect();
        let results = self.dispatch_batch(device, queue, &prepared);
        let mut results_iter = results.into_iter();
        jobs.into_iter()
            .map(|job| {
                job?;
                let values = results_iter.next().flatten()?;
                Some(curve_3d_from_values(&values))
            })
            .collect()
    }

    /// Evaluate multiple polar curves in a single GPU submit. Misma semántica
    /// de batch que [`Self::evaluate_curves_2d_batched`].
    pub fn evaluate_polars_batched(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        curves: &[(&PolarCurveObj, usize)],
        variables: &HashMap<String, f64>,
    ) -> Vec<Option<Curve2DSamples>> {
        let jobs: Vec<Option<PreparedCurve>> = curves
            .iter()
            .map(|(pol, steps)| prepare_polar(pol, *steps, self.max_curve_samples, variables))
            .collect();
        let prepared: Vec<&PreparedCurve> = jobs.iter().flatten().collect();
        let results = self.dispatch_batch(device, queue, &prepared);
        let mut results_iter = results.into_iter();
        jobs.into_iter()
            .map(|job| {
                job?;
                let values = results_iter.next().flatten()?;
                Some(curve_2d_from_values(&values))
            })
            .collect()
    }

    /// Recorda `jobs` en un único encoder (un dispatch por curva), un único
    /// submit y un único poll de readback acotado. Devuelve una entrada por
    /// job; `None` si el readback del batch falla.
    #[allow(clippy::let_unit_value)] // `timing` es `()` sin la feature `profiling`
    fn dispatch_batch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        jobs: &[&PreparedCurve],
    ) -> Vec<Option<Vec<f32>>> {
        if jobs.is_empty() {
            return Vec::new();
        }
        let total_outputs: usize = jobs.iter().map(|job| job.output_count).sum();
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Parametric Batch Readback"),
            size: (total_outputs * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Cada curva enlaza su propio set de buffers params/bytecode/constants:
        // `queue.write_buffer` encola todos los writes antes del submit, así que
        // con buffers compartidos el último write pisaría a los anteriores antes
        // de que los passes los lean. Buffers por curva mantienen el orden
        // correcto (wgpu 22 no expone `CommandEncoder::write_buffer`).
        let mut per_curve_buffers: Vec<(wgpu::Buffer, wgpu::Buffer, wgpu::Buffer)> =
            Vec::with_capacity(jobs.len());
        for job in jobs {
            let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Parametric Batch Params"),
                size: std::mem::size_of::<ParametricParamsUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Parametric Batch Bytecode"),
                size: 4096 * std::mem::size_of::<u32>() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let constants_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Parametric Batch Constants"),
                size: 256 * std::mem::size_of::<f32>() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&params_buffer, 0, bytemuck::cast_slice(&[job.params]));
            queue.write_buffer(&bytecode_buffer, 0, bytemuck::cast_slice(&job.prog.code));
            queue.write_buffer(
                &constants_buffer,
                0,
                bytemuck::cast_slice(&job.prog.constants),
            );
            per_curve_buffers.push((params_buffer, bytecode_buffer, constants_buffer));
        }

        let timing =
            crate::gpu_timing::create(device, queue, "Parametric Batch", jobs.len() as u32);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Parametric Batch Encoder"),
        });
        let mut byte_offset = 0u64;
        for (pass_index, (job, (params_buffer, bytecode_buffer, constants_buffer))) in
            jobs.iter().zip(&per_curve_buffers).enumerate()
        {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Parametric Batch Bind Group"),
                layout: &self.bind_group_layout,
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
                        resource: self.values_buffer.as_entire_binding(),
                    },
                ],
            });
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Parametric Batch Pass"),
                    timestamp_writes: crate::gpu_timing::timestamp_writes(
                        &timing,
                        pass_index as u32,
                    ),
                });
                cpass.set_pipeline(&self.pipeline);
                cpass.set_bind_group(0, &bind_group, &[]);
                let wg = job.params.n.div_ceil(64).max(1);
                cpass.dispatch_workgroups(wg, 1, 1);
            }
            let copy_bytes = (job.output_count * std::mem::size_of::<f32>()) as u64;
            encoder.copy_buffer_to_buffer(
                &self.values_buffer,
                0,
                &readback,
                byte_offset,
                copy_bytes,
            );
            byte_offset += copy_bytes;
        }
        crate::gpu_timing::resolve(&timing, &mut encoder);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            } else {
                log::error!("Parametric batch readback failed: {:?}", result.err());
            }
        });
        // TODO P1: mover a spawn_blocking — el readback síncrono sigue bloqueando
        // el hilo de prepare (acotado a 1 intento por frame via
        // MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE en canvas.rs). Mitigación:
        // poll acotado con timeout en vez de Wait infinito.
        log::trace!("Parametric batch sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&timing, device, "Parametric Batch");

        if !mapped {
            readback.unmap();
            return jobs.iter().map(|_| None).collect();
        }
        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let mut results = Vec::with_capacity(jobs.len());
        let mut offset = 0usize;
        for job in jobs {
            results.push(Some(values_f32[offset..offset + job.output_count].to_vec()));
            offset += job.output_count;
        }
        drop(data);
        readback.unmap();
        results
    }
}

fn compile_curve_3d_program(
    pc: &ParametricCurve3DObj,
    variables: &HashMap<String, f64>,
) -> Result<BytecodeProgram, CompileError> {
    let mut prog = BytecodeProgram::default();
    let parameter = pc.parameter.as_str();
    ParametricComputePipeline::compile_parametric_expr(
        &pc.expr_x, variables, parameter, &mut prog,
    )?;
    ParametricComputePipeline::compile_parametric_expr(
        &pc.expr_y, variables, parameter, &mut prog,
    )?;
    ParametricComputePipeline::compile_parametric_expr(
        &pc.expr_z, variables, parameter, &mut prog,
    )?;
    Ok(prog)
}

/// Ensure that sampling the curve on the GPU will not collapse distinct CPU
/// samples after its expression values are narrowed to `f32`.
fn curve_3d_samples_preserve_f32_resolution(
    pc: &ParametricCurve3DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    let samples = parametric_sampling::evaluate_parametric_curve_3d(pc, steps, variables);
    if samples.len() < 2 {
        return false;
    }

    let mut has_distinct_coordinate = false;
    for pair in samples.windows(2) {
        let [(ax, ay, az), (bx, by, bz)] = pair else {
            return false;
        };
        for (a, b) in [(ax, bx), (ay, by), (az, bz)] {
            if !a.is_finite() || !b.is_finite() {
                return false;
            }
            let narrowed_a = *a as f32;
            let narrowed_b = *b as f32;
            if !narrowed_a.is_finite() || !narrowed_b.is_finite() {
                return false;
            }

            let separation = (*b - *a).abs();
            if separation == 0.0 {
                continue;
            }
            has_distinct_coordinate = true;
            let allowed_error = separation * 0.25;
            if narrowed_a == narrowed_b
                || (*a - narrowed_a as f64).abs() > allowed_error
                || (*b - narrowed_b as f64).abs() > allowed_error
            {
                return false;
            }
        }
    }

    has_distinct_coordinate
}

fn finite_layout_matches_2d(gpu: &Curve2DSamples, cpu: &Curve2DSamples) -> bool {
    gpu.len() == cpu.len()
        && gpu.iter().zip(cpu).all(|(gpu, cpu)| {
            gpu.0.is_finite() == cpu.0.is_finite() && gpu.1.is_finite() == cpu.1.is_finite()
        })
}

fn finite_layout_matches_3d(gpu: &Curve3DSamples, cpu: &Curve3DSamples) -> bool {
    gpu.len() == cpu.len()
        && gpu.iter().zip(cpu).all(|(gpu, cpu)| {
            gpu.0.is_finite() == cpu.0.is_finite()
                && gpu.1.is_finite() == cpu.1.is_finite()
                && gpu.2.is_finite() == cpu.2.is_finite()
        })
}

fn nonfinite_curve_2d_samples_match_cpu(
    gpu: &Curve2DSamples,
    pc: &ParametricCurve2DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    if gpu.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
        return true;
    }
    let cpu = parametric_sampling::evaluate_parametric_curve_2d(pc, steps, variables);
    finite_layout_matches_2d(gpu, &cpu)
}

fn nonfinite_curve_3d_samples_match_cpu(
    gpu: &Curve3DSamples,
    pc: &ParametricCurve3DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    if gpu
        .iter()
        .all(|(x, y, z)| x.is_finite() && y.is_finite() && z.is_finite())
    {
        return true;
    }
    let cpu = parametric_sampling::evaluate_parametric_curve_3d(pc, steps, variables);
    finite_layout_matches_3d(gpu, &cpu)
}

fn nonfinite_polar_samples_match_cpu(
    gpu: &Curve2DSamples,
    pol: &PolarCurveObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    if gpu.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
        return true;
    }
    let cpu = parametric_sampling::evaluate_polar_curve(pol, steps, variables);
    finite_layout_matches_2d(gpu, &cpu)
}

fn surface_samples_are_finite(gpu: &SurfaceSamples) -> bool {
    gpu.iter().flatten().all(|point| point.is_finite())
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
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return false;
    }
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        expr_hash: parametric_sampling::curve_2d_expr_hash(pc),
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_curve_2d(device, queue, pc, steps, variables) else {
        return false;
    };
    if !nonfinite_curve_2d_samples_match_cpu(&samples, pc, steps, variables) {
        return false;
    }

    *pc.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
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
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return false;
    }
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        expr_hash: parametric_sampling::curve_3d_expr_hash(pc),
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_curve_3d(device, queue, pc, steps, variables) else {
        return false;
    };
    if !nonfinite_curve_3d_samples_match_cpu(&samples, pc, steps, variables) {
        return false;
    }

    *pc.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
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
    if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
        return false;
    }
    let key = grafito_core::ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        expr_hash: parametric_sampling::polar_expr_hash(pol),
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = pol.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(samples) = compute.evaluate_polar(device, queue, pol, steps, variables) else {
        return false;
    };
    if !nonfinite_polar_samples_match_cpu(&samples, pol, steps, variables) {
        return false;
    }

    *pol.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pol.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
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
    if surf.is_parametric || surf.is_complex || surf.legacy_axis_swap {
        return false;
    }
    let res = res.max(1);
    if !surface_res_within_budget(res) {
        return false;
    }
    let x_min = ParametricComputePipeline::resolve_expr(&surf.x_min_expr, surf.x_min, variables);
    let x_max = ParametricComputePipeline::resolve_expr(&surf.x_max_expr, surf.x_max, variables);
    let y_min = ParametricComputePipeline::resolve_expr(&surf.y_min_expr, surf.y_min, variables);
    let y_max = ParametricComputePipeline::resolve_expr(&surf.y_max_expr, surf.y_max, variables);
    if !has_strictly_increasing_finite_bounds(&[x_min, x_max])
        || !has_strictly_increasing_finite_bounds(&[y_min, y_max])
    {
        return false;
    }
    let key = grafito_core::SurfaceCacheKey {
        x_domain: (x_min, x_max),
        y_domain: (y_min, y_max),
        res,
        is_parametric: false,
        expr_hash: parametric_sampling::surface_expr_hash(surf),
        variables_hash: parametric_sampling::variables_hash(variables),
    };
    {
        let cached_key = surf.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(grid) = compute.evaluate_surface(device, queue, surf, res, variables) else {
        return false;
    };
    if !surface_samples_are_finite(&grid) {
        return false;
    }

    *surf.cached_grid.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = grid;
    *surf.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    true
}

/// Batch entry for [`maybe_compute_curves_2d_on_gpu_batched`].
struct Curve2DBatchEntry<'a> {
    curve: &'a ParametricCurve2DObj,
    steps: usize,
    key: grafito_core::ParametricCacheKey,
}

/// Batch entry for [`maybe_compute_curves_3d_on_gpu_batched`].
struct Curve3DBatchEntry<'a> {
    curve: &'a ParametricCurve3DObj,
    steps: usize,
    key: grafito_core::ParametricCacheKey,
}

/// Batch entry for [`maybe_compute_polars_on_gpu_batched`].
struct PolarBatchEntry<'a> {
    curve: &'a PolarCurveObj,
    steps: usize,
    key: grafito_core::ParametricCacheKey,
}

/// Try to populate the 2D parametric curve caches for many curves in a single
/// GPU submit (un encoder, un submit, un poll). Devuelve un bool por curva de
/// entrada: `true` cuando la GPU pobló (o ya tenía) la caché. Cada curva densa
/// sigue siendo un solo dispatch de `steps + 1 <= MAX_CURVE_STEPS + 1` muestras.
pub fn maybe_compute_curves_2d_on_gpu_batched(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    curves: &[(&ParametricCurve2DObj, usize)],
    variables: &HashMap<String, f64>,
) -> Vec<bool> {
    let mut results = vec![false; curves.len()];
    let mut batch: Vec<Curve2DBatchEntry<'_>> = Vec::new();
    let mut batch_indices: Vec<usize> = Vec::new();
    for (index, (pc, steps)) in curves.iter().enumerate() {
        let steps = (*steps).min(MAX_CURVE_STEPS);
        let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
        let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
        if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
            continue;
        }
        let key = grafito_core::ParametricCacheKey {
            t_domain: (t_min, t_max),
            steps,
            expr_hash: parametric_sampling::curve_2d_expr_hash(pc),
            variables_hash: parametric_sampling::variables_hash(variables),
        };
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            results[index] = true;
            continue;
        }
        drop(cached_key);
        batch.push(Curve2DBatchEntry {
            curve: pc,
            steps,
            key,
        });
        batch_indices.push(index);
    }
    if batch.is_empty() {
        return results;
    }
    let batch_refs: Vec<(&ParametricCurve2DObj, usize)> = batch
        .iter()
        .map(|entry| (entry.curve, entry.steps))
        .collect();
    let samples = compute.evaluate_curves_2d_batched(device, queue, &batch_refs, variables);
    for (batch_index, entry) in batch.into_iter().enumerate() {
        let Some(samples) = &samples[batch_index] else {
            continue;
        };
        if !nonfinite_curve_2d_samples_match_cpu(samples, entry.curve, entry.steps, variables) {
            continue;
        }
        *entry.curve.cached_samples.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = samples.clone();
        *entry.curve.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = Some(entry.key);
        results[batch_indices[batch_index]] = true;
    }
    results
}

/// Try to populate the 3D parametric curve caches for many curves in a single
/// GPU submit. Misma semántica que [`maybe_compute_curves_2d_on_gpu_batched`].
pub fn maybe_compute_curves_3d_on_gpu_batched(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    curves: &[(&ParametricCurve3DObj, usize)],
    variables: &HashMap<String, f64>,
) -> Vec<bool> {
    let mut results = vec![false; curves.len()];
    let mut batch: Vec<Curve3DBatchEntry<'_>> = Vec::new();
    let mut batch_indices: Vec<usize> = Vec::new();
    for (index, (pc, steps)) in curves.iter().enumerate() {
        let steps = (*steps).min(MAX_CURVE_STEPS);
        let t_min = ParametricComputePipeline::resolve_expr(&pc.t_min_expr, pc.t_min, variables);
        let t_max = ParametricComputePipeline::resolve_expr(&pc.t_max_expr, pc.t_max, variables);
        if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
            continue;
        }
        let key = grafito_core::ParametricCacheKey {
            t_domain: (t_min, t_max),
            steps,
            expr_hash: parametric_sampling::curve_3d_expr_hash(pc),
            variables_hash: parametric_sampling::variables_hash(variables),
        };
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            results[index] = true;
            continue;
        }
        drop(cached_key);
        batch.push(Curve3DBatchEntry {
            curve: pc,
            steps,
            key,
        });
        batch_indices.push(index);
    }
    if batch.is_empty() {
        return results;
    }
    let batch_refs: Vec<(&ParametricCurve3DObj, usize)> = batch
        .iter()
        .map(|entry| (entry.curve, entry.steps))
        .collect();
    let samples = compute.evaluate_curves_3d_batched(device, queue, &batch_refs, variables);
    for (batch_index, entry) in batch.into_iter().enumerate() {
        let Some(samples) = &samples[batch_index] else {
            continue;
        };
        if !nonfinite_curve_3d_samples_match_cpu(samples, entry.curve, entry.steps, variables) {
            continue;
        }
        *entry.curve.cached_samples.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = samples.clone();
        *entry.curve.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = Some(entry.key);
        results[batch_indices[batch_index]] = true;
    }
    results
}

/// Try to populate the polar curve caches for many curves in a single GPU
/// submit. Misma semántica que [`maybe_compute_curves_2d_on_gpu_batched`].
pub fn maybe_compute_polars_on_gpu_batched(
    compute: &ParametricComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    curves: &[(&PolarCurveObj, usize)],
    variables: &HashMap<String, f64>,
) -> Vec<bool> {
    let mut results = vec![false; curves.len()];
    let mut batch: Vec<PolarBatchEntry<'_>> = Vec::new();
    let mut batch_indices: Vec<usize> = Vec::new();
    for (index, (pol, steps)) in curves.iter().enumerate() {
        let steps = (*steps).min(MAX_CURVE_STEPS);
        let t_min = ParametricComputePipeline::resolve_expr(&pol.t_min_expr, pol.t_min, variables);
        let t_max = ParametricComputePipeline::resolve_expr(&pol.t_max_expr, pol.t_max, variables);
        if !has_strictly_increasing_finite_bounds(&[t_min, t_max]) {
            continue;
        }
        let key = grafito_core::ParametricCacheKey {
            t_domain: (t_min, t_max),
            steps,
            expr_hash: parametric_sampling::polar_expr_hash(pol),
            variables_hash: parametric_sampling::variables_hash(variables),
        };
        let cached_key = pol.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            results[index] = true;
            continue;
        }
        drop(cached_key);
        batch.push(PolarBatchEntry {
            curve: pol,
            steps,
            key,
        });
        batch_indices.push(index);
    }
    if batch.is_empty() {
        return results;
    }
    let batch_refs: Vec<(&PolarCurveObj, usize)> = batch
        .iter()
        .map(|entry| (entry.curve, entry.steps))
        .collect();
    let samples = compute.evaluate_polars_batched(device, queue, &batch_refs, variables);
    for (batch_index, entry) in batch.into_iter().enumerate() {
        let Some(samples) = &samples[batch_index] else {
            continue;
        };
        if !nonfinite_polar_samples_match_cpu(samples, entry.curve, entry.steps, variables) {
            continue;
        }
        *entry.curve.cached_samples.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = samples.clone();
        *entry.curve.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = Some(entry.key);
        results[batch_indices[batch_index]] = true;
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implicit_compute::Op;

    #[test]
    fn curve_3d_compilation_keeps_its_declared_parameter_dynamic() {
        let curve = ParametricCurve3DObj::new("s", "s + 1", "s^2", 0.0, 1.0).with_parameter("s");
        let variables = HashMap::from([("s".to_string(), 99.0)]);

        let program = compile_curve_3d_program(&curve, &variables).unwrap();

        assert_eq!(
            program
                .code
                .iter()
                .filter(|instruction| **instruction & 0xFF == Op::PushVar as u32)
                .count(),
            3
        );
        assert!(!program.constants.contains(&99.0));
    }

    #[test]
    fn curve_3d_precision_preflight_rejects_large_offsets_at_small_sample_intervals() {
        let constant_offset =
            ParametricCurve3DObj::new("999999+s", "0", "0", 0.0, 0.01).with_parameter("s");
        let variable_offset =
            ParametricCurve3DObj::new("offset+s", "0", "0", 0.0, 0.01).with_parameter("s");
        let variables = HashMap::from([("offset".to_string(), 999999.0)]);

        assert!(!curve_3d_samples_preserve_f32_resolution(
            &constant_offset,
            4,
            &HashMap::new(),
        ));
        assert!(!curve_3d_samples_preserve_f32_resolution(
            &variable_offset,
            4,
            &variables,
        ));
    }

    #[test]
    fn surface_post_validation_checks_the_evaluated_z_coordinate() {
        let gpu = vec![vec![grafito_geometry::Point3D::new(1.0, 1.0, f64::NAN)]];

        assert!(!surface_samples_are_finite(&gpu));
    }

    #[test]
    fn surface_resolution_budget_rejects_over_128_without_clamping() {
        // MAX_SURFACE_RES 128 es un presupuesto duro: por encima se rechaza
        // (None / false) en vez de recortar silenciosamente a 128.
        assert!(surface_res_within_budget(1));
        assert!(surface_res_within_budget(MAX_SURFACE_RES));
        assert!(!surface_res_within_budget(MAX_SURFACE_RES + 1));
        assert!(
            !surface_res_within_budget(0),
            "res 0 se normaliza a 1 por el caller"
        );
    }

    #[test]
    fn curve_steps_budget_is_documented_at_4000_and_clamped_per_curve() {
        // MAX_CURVE_STEPS 4000: una curva densa se evalúa en un solo dispatch
        // de `steps + 1` muestras; el preflight recorta a 4000 sin rechazar.
        let curve = ParametricCurve2DObj::new("t", "t", 0.0, 1.0);
        let prepared = prepare_curve_2d(&curve, MAX_CURVE_STEPS + 500, 4000, &HashMap::new())
            .expect("steps por encima del presupuesto se recortan a 4000");
        assert_eq!(prepared.params.n, (MAX_CURVE_STEPS + 1) as u32);
        assert_eq!(prepared.output_count, (MAX_CURVE_STEPS + 1) * 2);
    }

    #[test]
    fn batch_preflight_keeps_one_entry_per_input_curve() {
        let circle = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU);
        let line = ParametricCurve2DObj::new("t", "t", 0.0, 1.0);
        let reversed = ParametricCurve2DObj::new("t", "t", 1.0, 0.0);

        let jobs: Vec<Option<PreparedCurve>> = [&circle, &line, &reversed]
            .iter()
            .map(|pc| prepare_curve_2d(pc, 8, 4000, &HashMap::new()))
            .collect();

        assert!(jobs[0].is_some());
        assert!(jobs[1].is_some());
        assert!(
            jobs[2].is_none(),
            "dominio invertido se rechaza en preflight"
        );
    }
}
