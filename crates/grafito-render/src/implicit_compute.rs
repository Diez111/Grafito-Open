//! GPU compute pipeline for implicit-curve scalar-field evaluation.
//!
//! A single WGSL compute shader interprets a small RPN bytecode stream so that
//! arbitrary expressions (within the supported opcode set) can be evaluated on
//! the GPU without recompiling shaders. The Rust side compiles `lhs - rhs` into
//! bytecode, dispatches the compute kernel, reads back the scalar field and
//! runs marching squares on the CPU to extract contour segments.
//!
//! If an expression uses operations that are not supported by the bytecode
//! machine, compilation fails and the caller falls back to the CPU evaluator.

use grafito_core::object::{ImplicitCurveObj, ImplicitCurveSegments};
use grafito_core::RenderQuality;
use grafito_geometry::{Point2, ViewTransform};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub(crate) enum Op {
    Nop = 0,
    PushConst = 1,
    PushVar = 2,
    Add = 3,
    Sub = 4,
    Mul = 5,
    Div = 6,
    Pow = 7,
    Neg = 8,
    Sin = 9,
    Cos = 10,
    Tan = 11,
    Exp = 12,
    Log = 13,
    Sqrt = 14,
    Abs = 15,
    Min = 16,
    Max = 17,
    Floor = 18,
    Ceil = 19,
    Pi = 20,
    E = 21,
    // Extended opcodes (Fase 1)
    Asin = 22,
    Acos = 23,
    Atan = 24,
    Sinh = 25,
    Cosh = 26,
    Tanh = 27,
    Asinh = 28,
    Acosh = 29,
    Atanh = 30,
    Sec = 31,
    Csc = 32,
    Cot = 33,
    Sign = 34,
    Heaviside = 35,
    Cbrt = 36,
    Mod = 37,
    Round = 38,
    Log10 = 39,
    Log2 = 40,
    Exp2 = 41,
    Atan2 = 42,
    Clamp = 43,
    Lt = 44,
    Gt = 45,
    Le = 46,
    Ge = 47,
    Eq = 48,
    Ne = 49,
}

impl Op {
    pub(crate) fn encode(self, operand: u32) -> u32 {
        (self as u32) | (operand << 8)
    }
}

/// Compiled GPU program for one expression.
#[derive(Debug, Default)]
pub(crate) struct BytecodeProgram {
    pub(crate) code: Vec<u32>,
    pub(crate) constants: Vec<f32>,
}

/// Reason why an expression cannot be compiled to GPU bytecode.
#[derive(Debug)]
pub(crate) enum CompileError {
    UnsupportedNode(String),
    UnsupportedVariable(String),
    StackTooDeep,
    TooManyConstants,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnsupportedNode(n) => write!(f, "unsupported AST node: {}", n),
            CompileError::UnsupportedVariable(v) => {
                write!(f, "variable '{}' not available on GPU evaluator", v)
            }
            CompileError::StackTooDeep => write!(f, "expression too deep for GPU stack"),
            CompileError::TooManyConstants => write!(f, "too many constants for GPU buffer"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Compile an AST expression into RPN bytecode that the WGSL interpreter can
/// execute. Document variables are baked in as constants. Variables listed in
/// `var_map` are mapped to GPU operand indices; unknown variables must be
/// present in `document_vars` or compilation fails.
pub(crate) fn compile_expr_with_mapping(
    expr: &grafito_geometry::ast::Expr,
    document_vars: &HashMap<String, f64>,
    var_map: &[(&str, u32)],
    prog: &mut BytecodeProgram,
) -> Result<(), CompileError> {
    use grafito_geometry::ast::Expr;

    match expr {
        Expr::Const(c) => {
            if prog.constants.len() >= 256 {
                return Err(CompileError::TooManyConstants);
            }
            let idx = prog.constants.len() as u32;
            prog.constants.push(*c as f32);
            prog.code.push(Op::PushConst.encode(idx));
        }
        Expr::Var(name) => {
            let name = name.as_str();
            if let Some((_, operand)) = var_map.iter().find(|(n, _)| *n == name) {
                prog.code.push(Op::PushVar.encode(*operand));
            } else if let Some(v) = document_vars.get(name) {
                if prog.constants.len() >= 256 {
                    return Err(CompileError::TooManyConstants);
                }
                let idx = prog.constants.len() as u32;
                prog.constants.push(*v as f32);
                prog.code.push(Op::PushConst.encode(idx));
            } else {
                return Err(CompileError::UnsupportedVariable(name.to_string()));
            }
        }
        Expr::Add(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Add.encode(0));
        }
        Expr::Sub(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Sub.encode(0));
        }
        Expr::Mul(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Mul.encode(0));
        }
        Expr::Div(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Div.encode(0));
        }
        Expr::Pow(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Pow.encode(0));
        }
        Expr::Neg(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Neg.encode(0));
        }
        Expr::Sin(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Sin.encode(0));
        }
        Expr::Cos(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Cos.encode(0));
        }
        Expr::Tan(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Tan.encode(0));
        }
        Expr::Exp(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Exp.encode(0));
        }
        Expr::Ln(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Log.encode(0));
        }
        Expr::Sqrt(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Sqrt.encode(0));
        }
        Expr::Abs(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Abs.encode(0));
        }
        Expr::Min(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Min.encode(0));
        }
        Expr::Max(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Max.encode(0));
        }
        Expr::Floor(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Floor.encode(0));
        }
        Expr::Ceil(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Ceil.encode(0));
        }
        Expr::Asin(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Asin.encode(0));
        }
        Expr::Acos(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Acos.encode(0));
        }
        Expr::Atan(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Atan.encode(0));
        }
        Expr::Sinh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Sinh.encode(0));
        }
        Expr::Cosh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Cosh.encode(0));
        }
        Expr::Tanh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Tanh.encode(0));
        }
        Expr::Asinh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Asinh.encode(0));
        }
        Expr::Acosh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Acosh.encode(0));
        }
        Expr::Atanh(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Atanh.encode(0));
        }
        Expr::Sec(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Sec.encode(0));
        }
        Expr::Csc(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Csc.encode(0));
        }
        Expr::Cot(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Cot.encode(0));
        }
        Expr::Sign(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Sign.encode(0));
        }
        Expr::Heaviside(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Heaviside.encode(0));
        }
        Expr::Cbrt(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Cbrt.encode(0));
        }
        Expr::Round(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Round.encode(0));
        }
        Expr::Log(a) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            prog.code.push(Op::Log10.encode(0));
        }
        Expr::Modulo(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Mod.encode(0));
        }
        Expr::Atan2(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Atan2.encode(0));
        }
        Expr::Clamp(a, lo, hi) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(lo, document_vars, var_map, prog)?;
            compile_expr_with_mapping(hi, document_vars, var_map, prog)?;
            prog.code.push(Op::Clamp.encode(0));
        }
        Expr::Lt(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Lt.encode(0));
        }
        Expr::Gt(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Gt.encode(0));
        }
        Expr::Le(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Le.encode(0));
        }
        Expr::Ge(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Ge.encode(0));
        }
        Expr::Eq(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Eq.encode(0));
        }
        Expr::Ne(a, b) => {
            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(b, document_vars, var_map, prog)?;
            prog.code.push(Op::Ne.encode(0));
        }
        Expr::Piecewise(_, _) => {
            // Piecewise requires a Select opcode and complex control flow.
            // Falls back to CPU evaluator which handles it correctly.
            return Err(CompileError::UnsupportedNode("Piecewise".to_string()));
        }
        other => {
            return Err(CompileError::UnsupportedNode(format!("{:?}", other)));
        }
    }

    // Verificar profundidad real de pila simulando los efectos de cada opcode.
    let mut sp: i32 = 0;
    let mut max_sp: i32 = 0;
    for &instr in &prog.code {
        let op = instr & 0xFFu32;
        match op {
            // Push 1 value: PushConst(1), PushVar(2), Pi(20), E(21)
            1 | 2 | 20 | 21 => {
                sp += 1;
                if sp > max_sp {
                    max_sp = sp;
                }
            }
            // Binary ops: pop 2, push 1 → net -1
            // Add(3), Sub(4), Mul(5), Div(6), Pow(7), Min(16), Max(17),
            // Mod(37), Atan2(42), Clamp(43), Lt(44), Gt(45), Le(46), Ge(47), Eq(48), Ne(49)
            3 | 4 | 5 | 6 | 7 | 16 | 17 | 37 | 42 | 44 | 45 | 46 | 47 | 48 | 49 => {
                sp -= 1;
            }
            // Clamp pops 3, pushes 1 → net -2
            43 => {
                sp -= 2;
            }
            // Unary ops: pop 1, push 1 → net 0. No stack change.
            // Neg(8), Sin(9), Cos(10), Tan(11), Exp(12), Log(13), Sqrt(14),
            // Abs(15), Floor(18), Ceil(19), Asin(22), Acos(23), Atan(24),
            // Sinh(25), Cosh(26), Tanh(27), Asinh(28), Acosh(29), Atanh(30),
            // Sec(31), Csc(32), Cot(33), Sign(34), Heaviside(35), Cbrt(36),
            // Round(38), Log10(39), Log2(40), Exp2(41)
            _ => {}
        }
    }
    if max_sp > 32 || prog.code.len() > 4096 {
        return Err(CompileError::StackTooDeep);
    }
    Ok(())
}

/// Compile an AST expression into RPN bytecode using the default variable
/// mapping: `x` -> operand 0, `y` -> operand 1.
pub(crate) fn compile_expr(
    expr: &grafito_geometry::ast::Expr,
    document_vars: &HashMap<String, f64>,
    prog: &mut BytecodeProgram,
) -> Result<(), CompileError> {
    compile_expr_with_mapping(expr, document_vars, &[("x", 0), ("y", 1)], prog)
}

/// GPU resources needed to evaluate one implicit curve per dispatch.
pub struct ImplicitComputePipeline {
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
struct GridParamsUniform {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    grid_size: u32,
    code_len: u32,
    _pad0: u32,
    _pad1: u32,
}

impl ImplicitComputePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, max_grid: usize) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Implicit Curve Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("implicit_compute.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Implicit Compute Bind Group Layout"),
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
            label: Some("Implicit Compute Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Implicit Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let max_values = (max_grid + 1) * (max_grid + 1);

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Implicit Compute Params"),
            size: std::mem::size_of::<GridParamsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bytecode_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Implicit Compute Bytecode"),
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
            label: Some("Implicit Compute Constants"),
            size: 256 * std::mem::size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let values_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Implicit Compute Values"),
            size: (max_values * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let values_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Implicit Compute Values Readback"),
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

    /// Evaluate the implicit curve `lhs - rhs` on the GPU and return a grid of
    /// scalar values. Returns `None` if the expression cannot be compiled to
    /// GPU bytecode (caller should fall back to CPU).
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ic: &ImplicitCurveObj,
        view_bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<Vec<f64>>> {
        let lhs_ast =
            grafito_geometry::expr::prepare_function_ast(&ic.expr_lhs, variables, &["x", "y"])
                .ok()?;
        let rhs_ast =
            grafito_geometry::expr::prepare_function_ast(&ic.expr_rhs, variables, &["x", "y"])
                .ok()?;

        // Build lhs - rhs and simplify (e.g., eliminate "x - 0" → "x")
        let combined =
            grafito_geometry::ast::Expr::Sub(Box::new(lhs_ast), Box::new(rhs_ast)).simplify();

        let mut prog = BytecodeProgram::default();
        compile_expr(&combined, variables, &mut prog).ok()?;

        if grid_size > self.max_grid {
            return None;
        }

        let (x_min, x_max, y_min, y_max) = view_bounds;
        let params = GridParamsUniform {
            x_min: x_min as f32,
            x_max: x_max as f32,
            y_min: y_min as f32,
            y_max: y_max as f32,
            grid_size: (grid_size + 1) as u32,
            code_len: prog.code.len() as u32,
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
            label: Some("Implicit Compute Bind Group"),
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
            label: Some("Implicit Compute Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Implicit Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (grid_size as u32 + 1).div_ceil(16).max(1);
            cpass.dispatch_workgroups(wg, wg, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.values_buffer,
            0,
            &self.values_readback,
            0,
            ((grid_size + 1) * (grid_size + 1) * std::mem::size_of::<f32>()) as u64,
        );
        queue.submit(std::iter::once(encoder.finish()));

        // Synchronously map the readback buffer. This blocks the CPU until the
        // GPU work finishes, which is acceptable because the subsequent
        // marching-squares step still runs on the CPU.
        let slice = self.values_readback.slice(..);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            } else {
                log::error!("Implicit compute readback failed: {:?}", result.err());
            }
        });
        device.poll(wgpu::Maintain::Wait);

        if !map_ok.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let mut rows = Vec::with_capacity(grid_size + 1);
        for j in 0..=grid_size {
            let mut row = Vec::with_capacity(grid_size + 1);
            for i in 0..=grid_size {
                let v = values_f32[j * (grid_size + 1) + i] as f64;
                row.push(if v.is_finite() { v } else { f64::NAN });
            }
            rows.push(row);
        }
        drop(data);
        self.values_readback.unmap();

        Some(rows)
    }
}

/// Try to populate the implicit-curve cache using the GPU compute pipeline.
/// Returns `true` if the cache was populated (either already cached or freshly
/// computed on the GPU). Returns `false` if the GPU path is unavailable or the
/// expression is not supported by the bytecode machine.
pub fn maybe_compute_on_gpu(
    compute: &ImplicitComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ic: &ImplicitCurveObj,
    view: &ViewTransform,
    variables: &HashMap<String, f64>,
    quality: RenderQuality,
) -> bool {
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(view.screen_size);
    let view_bounds = (
        world_tl.x.min(world_br.x),
        world_tl.x.max(world_br.x),
        world_br.y.min(world_tl.y),
        world_br.y.max(world_tl.y),
    );
    let padded_bounds = grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);
    let grid_size = match quality {
        RenderQuality::Preview => grafito_core::implicit_curve::recommended_grid_size(
            view.screen_size.x,
            view.screen_size.y,
        )
        .min(128),
        RenderQuality::Normal => grafito_core::implicit_curve::recommended_grid_size(
            view.screen_size.x,
            view.screen_size.y,
        )
        .min(512),
        RenderQuality::High => grafito_core::implicit_curve::recommended_grid_size(
            view.screen_size.x,
            view.screen_size.y,
        )
        .min(1024),
    };

    let key = ic.cache_key(padded_bounds, grid_size, variables);
    {
        let cached_key = ic.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(rows) = compute.evaluate(device, queue, ic, padded_bounds, grid_size, variables)
    else {
        return false;
    };

    let levels: Vec<f64> = ic
        .contour_levels
        .as_ref()
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![0.0]);
    let segments = marching_squares_from_grid(
        &rows,
        &levels,
        padded_bounds.0,
        padded_bounds.2,
        padded_bounds.1,
        padded_bounds.3,
    );
    *ic.cached_segments
        .write()
        .unwrap_or_else(|p| p.into_inner()) = segments;
    *ic.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    *ic.cached_region.write().unwrap_or_else(|p| p.into_inner()) = Some(padded_bounds);
    true
}

/// Run marching squares on a scalar grid and return per-level segments.
pub fn marching_squares_from_grid(
    rows: &[Vec<f64>],
    levels: &[f64],
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
) -> ImplicitCurveSegments {
    let grid_size = rows.len().saturating_sub(1);
    if grid_size == 0 {
        return Vec::new();
    }
    let dx = (x_max - x_min) / grid_size as f64;
    let dy = (y_max - y_min) / grid_size as f64;

    levels
        .iter()
        .map(|level| {
            let mut segs = Vec::new();
            for i in 0..grid_size {
                let x0 = x_min + i as f64 * dx;
                let x1 = x0 + dx;
                for j in 0..grid_size {
                    let y0 = y_min + j as f64 * dy;
                    let y1 = y0 + dy;

                    let v00 = rows[j][i];
                    let v10 = rows[j][i + 1];
                    let v01 = rows[j + 1][i];
                    let v11 = rows[j + 1][i + 1];

                    if v00.is_nan() || v10.is_nan() || v01.is_nan() || v11.is_nan() {
                        continue;
                    }

                    let s00 = (v00 - level) >= 0.0;
                    let s10 = (v10 - level) >= 0.0;
                    let s01 = (v01 - level) >= 0.0;
                    let s11 = (v11 - level) >= 0.0;

                    let case =
                        (s00 as u8) | ((s10 as u8) << 1) | ((s11 as u8) << 2) | ((s01 as u8) << 3);

                    if case == 0 || case == 15 {
                        continue;
                    }

                    let interp = |va: f64, vb: f64, pa: f64, pb: f64| -> f64 {
                        let denom = (va - level) - (vb - level);
                        if denom.abs() < f64::EPSILON * (va.abs() + vb.abs()).max(1.0) {
                            (pa + pb) * 0.5
                        } else {
                            let t = (va - level) / denom;
                            pa + t * (pb - pa)
                        }
                    };

                    let bottom = |t: f64| Point2::new(x0 + t * (x1 - x0), y0);
                    let top = |t: f64| Point2::new(x0 + t * (x1 - x0), y1);
                    let left = |t: f64| Point2::new(x0, y0 + t * (y1 - y0));
                    let right = |t: f64| Point2::new(x1, y0 + t * (y1 - y0));

                    let ib = interp(v00, v10, 0.0, 1.0);
                    let ir = interp(v10, v11, 0.0, 1.0);
                    let it = interp(v01, v11, 0.0, 1.0);
                    let il = interp(v00, v01, 0.0, 1.0);

                    let mut push = |a: Point2, b: Point2| segs.push((a, b));
                    match case {
                        1 | 14 => push(bottom(ib), left(il)),
                        2 | 13 => push(right(ir), bottom(ib)),
                        3 | 12 => push(right(ir), left(il)),
                        4 | 11 => push(top(it), right(ir)),
                        5 => {
                            push(bottom(ib), left(il));
                            push(top(it), right(ir));
                        }
                        6 | 9 => push(top(it), bottom(ib)),
                        7 | 8 => push(top(it), left(il)),
                        10 => {
                            push(right(ir), bottom(ib));
                            push(left(il), top(it));
                        }
                        _ => {}
                    }
                }
            }
            (*level, segs)
        })
        .collect()
}
