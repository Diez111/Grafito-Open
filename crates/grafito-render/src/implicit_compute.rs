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

use crate::gpu_readback::{PendingGpuReadback, ReadbackPoll};
use grafito_core::object::{
    ImplicitCurveCacheKey, ImplicitCurveObj, ImplicitCurveSegments, RelationOperator,
};
use grafito_core::RenderQuality;
use grafito_geometry::ViewTransform;
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::AtomicBool,
    mpsc::{sync_channel, Receiver},
    Arc,
};

pub use grafito_core::implicit_curve::{
    marching_squares_from_grid, MAX_IMPLICIT_GRID_SIZE, MAX_MARCHING_SQUARES_SEGMENTS,
};

/// Matches `STACK_SIZE` in each scalar WGSL bytecode interpreter.
const GPU_SCALAR_STACK_SIZE: i32 = 32;
const CLAMP_FORCE_INVALID_OPERAND: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)] // TODO P2: variantes Nop/Pi/E/Log2/Exp2 reservadas para bytecode extendido (usadas en shaders/fuzz)
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

pub(crate) fn f32_bounds_have_precision(values: &[f64], min_step: f64) -> bool {
    if !min_step.is_finite() || min_step <= 0.0 {
        return false;
    }
    let max_error = min_step * 0.25;
    values.iter().all(|v| {
        if !v.is_finite() {
            return false;
        }
        let narrowed = *v as f32;
        narrowed.is_finite() && (*v - narrowed as f64).abs() <= max_error
    })
}

/// Reason why an expression cannot be compiled to GPU bytecode.
#[derive(Debug)]
pub(crate) enum CompileError {
    UnsupportedNode(String),
    UnsupportedVariable(String),
    RuntimeClampBounds,
    PrecisionLoss,
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
            CompileError::RuntimeClampBounds => {
                write!(f, "clamp bounds must be static for GPU evaluation")
            }
            CompileError::PrecisionLoss => {
                write!(f, "clamp bounds lose ordering when narrowed to f32")
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
            let mut bound_variables = HashSet::new();
            lo.get_variables(&mut bound_variables);
            hi.get_variables(&mut bound_variables);
            if !bound_variables.is_empty() {
                return Err(CompileError::RuntimeClampBounds);
            }

            let lower = lo.eval_at("", 0.0);
            let upper = hi.eval_at("", 0.0);
            let force_invalid = !lower.is_finite() || !upper.is_finite() || lower > upper;
            if !force_invalid {
                let narrowed_lower = lower as f32;
                let narrowed_upper = upper as f32;
                if !narrowed_lower.is_finite()
                    || !narrowed_upper.is_finite()
                    || (lower < upper && narrowed_lower == narrowed_upper)
                {
                    return Err(CompileError::PrecisionLoss);
                }
            }

            compile_expr_with_mapping(a, document_vars, var_map, prog)?;
            compile_expr_with_mapping(lo, document_vars, var_map, prog)?;
            compile_expr_with_mapping(hi, document_vars, var_map, prog)?;
            prog.code.push(Op::Clamp.encode(if force_invalid {
                CLAMP_FORCE_INVALID_OPERAND
            } else {
                0
            }));
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
    if max_sp > GPU_SCALAR_STACK_SIZE || prog.code.len() > 4096 {
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

fn prepare_implicit_field(
    ic: &ImplicitCurveObj,
    variables: &HashMap<String, f64>,
) -> Option<grafito_geometry::ast::Expr> {
    let lhs =
        grafito_geometry::expr::prepare_function_ast(&ic.expr_lhs, variables, &["x", "y"]).ok()?;
    let rhs =
        grafito_geometry::expr::prepare_function_ast(&ic.expr_rhs, variables, &["x", "y"]).ok()?;

    Some(
        match ic.operator {
            RelationOperator::Greater | RelationOperator::GreaterEq => {
                grafito_geometry::ast::Expr::Sub(Box::new(rhs), Box::new(lhs))
            }
            _ => grafito_geometry::ast::Expr::Sub(Box::new(lhs), Box::new(rhs)),
        }
        .simplify(),
    )
}

fn nonfinite_gpu_field_matches_cpu(
    ic: &ImplicitCurveObj,
    rows: &[Vec<f64>],
    bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> bool {
    if rows.iter().flatten().all(|value| value.is_finite()) {
        return true;
    }
    let Some(field) = prepare_implicit_field(ic, variables) else {
        return false;
    };
    if rows.len() != grid_size + 1 || rows.iter().any(|row| row.len() != grid_size + 1) {
        return false;
    }
    let (x_min, x_max, y_min, y_max) = bounds;
    rows.iter().enumerate().all(|(j, row)| {
        row.iter().enumerate().all(|(i, gpu)| {
            let x = x_min + i as f64 / grid_size as f64 * (x_max - x_min);
            let y = y_min + j as f64 / grid_size as f64 * (y_max - y_min);
            gpu.is_finite() == field.eval_2d("x", x, "y", y).is_finite()
        })
    })
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
    /// GPU timestamp queries (feature `profiling`); no-op sin la feature.
    timing: crate::gpu_timing::GpuTimingHandle,
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

/// Submit compilado y validado, listo para `submit_buffers` (origen único
/// sync/async).
struct ImplicitSubmit {
    params: GridParamsUniform,
    code: Vec<u32>,
    constants: Vec<f32>,
    sample_axis: usize,
    sample_count: usize,
}

/// Dispatch en vuelo: el submit ya está en la GPU y la espera se distribuye
/// en frames vía [`PendingGpuReadback`]. El buffer readback pertenece al
/// pipeline (persistente), así que el `wait` puede cruzar frames sin mover
/// memoria GPU entre threads.
#[derive(Debug)]
pub struct PendingImplicitEval {
    grid_size: usize,
    wait: PendingGpuReadback,
}

impl PendingImplicitEval {
    /// Poll non-blocking delegado al waiter (para el slot de `canvas.rs`).
    pub fn poll(&mut self) -> ReadbackPoll {
        self.wait.poll()
    }
}

/// Fase de un job de cache implícito en vuelo.
#[derive(Debug)]
enum ImplicitJobStage {
    /// Esperando a la GPU (poll non-blocking por frame).
    AwaitingMap(PendingImplicitEval),
    /// Marching-squares corriendo en background thread (patrón repo:
    /// `thread::spawn` + `sync_channel(1)`); el frame recoge con `try_recv`.
    /// `Mutex` porque `Receiver` es `Send` pero no `Sync`, y el job vive en
    /// el slot dentro de `CallbackResources` (`Send + Sync` por egui-wgpu).
    Processing(std::sync::Mutex<Receiver<ImplicitCurveSegments>>),
}

/// Job de cache en vuelo: conserva `key` + `padded_bounds` + `grid_size` para
/// re-chequear vigencia en el avance (si el objeto cambió entre frames, el
/// resultado se descarta en vez de escribir basura en el cache).
/// `Debug` manual: el `Receiver` interior no es `Debug` (solo fase + key).
pub struct PendingImplicitJob {
    key: ImplicitCurveCacheKey,
    padded_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    levels: Vec<f64>,
    stage: ImplicitJobStage,
}

impl std::fmt::Debug for PendingImplicitJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stage = match &self.stage {
            ImplicitJobStage::AwaitingMap(_) => "AwaitingMap",
            ImplicitJobStage::Processing(_) => "Processing",
        };
        f.debug_struct("PendingImplicitJob")
            .field("key", &self.key)
            .field("grid_size", &self.grid_size)
            .field("stage", &stage)
            .finish()
    }
}

/// Resultado del poll al thread de marching-squares (se materializa antes de
/// reconstruir el job para no prestar el `Mutex` mientras se mueve).
#[derive(Debug)]
enum SegmentsPoll {
    Ready(ImplicitCurveSegments),
    Pending,
    Gone,
}

fn poll_segments_thread(rx: &Receiver<ImplicitCurveSegments>) -> SegmentsPoll {
    match rx.try_recv() {
        Ok(segments) => SegmentsPoll::Ready(segments),
        Err(std::sync::mpsc::TryRecvError::Empty) => SegmentsPoll::Pending,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => SegmentsPoll::Gone,
    }
}

/// Resultado de [`dispatch_implicit_on_gpu`]: `Cached` no tocó la GPU,
/// `Dispatched` ocupa el slot background (cap 1), `Unsupported` pide CPU.
/// `Box` por `large_enum_variant` (el job supera 240 B: key + levels + wait).
#[derive(Debug)]
pub enum ImplicitDispatchOutcome {
    Cached,
    Dispatched(Box<PendingImplicitJob>),
    Unsupported,
}

/// Paso non-blocking de [`advance_implicit_job`]: `NotReady` devuelve el job
/// para re-encolarlo en el slot (sigue en vuelo); `Done` es terminal y libera
/// el slot (el bool indica si el cache quedó poblado).
/// `Box` por `large_enum_variant` (ver arriba).
#[derive(Debug)]
pub enum ImplicitResolveStep {
    NotReady(Box<PendingImplicitJob>),
    Done(bool),
}

/// Marching-squares en background con datos `owned` (patrón repo). Medición
/// B6: 513×513 ≈ 4.5 ms en este box (debug) — fuera del hilo UI. El `send`
/// nunca bloquea (`sync_channel(1)` + un único mensaje).
fn spawn_marching_squares(
    rows: Vec<Vec<f64>>,
    levels: Vec<f64>,
    padded_bounds: (f64, f64, f64, f64),
) -> Receiver<ImplicitCurveSegments> {
    let (segments_tx, segments_rx) = sync_channel::<ImplicitCurveSegments>(1);
    std::thread::spawn(move || {
        // Mismo orden de args que el path síncrono legacy (`maybe_compute_on_gpu`).
        let segments = marching_squares_from_grid(
            &rows,
            &levels,
            padded_bounds.0,
            padded_bounds.2,
            padded_bounds.1,
            padded_bounds.3,
        );
        let _ = segments_tx.send(segments);
    });
    segments_rx
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

        let max_grid = max_grid.min(MAX_IMPLICIT_GRID_SIZE);
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
        // Zero-initialize the constants buffer so residual constants from a
        // previous evaluation cannot leak into the interpreter.
        queue.write_buffer(
            &constants_buffer,
            0,
            &[0u8; 256 * std::mem::size_of::<f32>()],
        );

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
            timing: crate::gpu_timing::create(device, queue, "Implicit Compute", 1),
        }
    }

    /// Compila y valida un submit sin tocar la GPU: origen único para el path
    /// síncrono (`evaluate`) y el asíncrono (`dispatch_eval`).
    fn plan_submit(
        &self,
        ic: &ImplicitCurveObj,
        view_bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<ImplicitSubmit> {
        let combined = prepare_implicit_field(ic, variables)?;

        let mut prog = BytecodeProgram::default();
        compile_expr(&combined, variables, &mut prog).ok()?;

        if grid_size == 0 || grid_size > self.max_grid || grid_size > MAX_IMPLICIT_GRID_SIZE {
            return None;
        }
        let sample_axis = grid_size.checked_add(1)?;
        let sample_count = sample_axis.checked_mul(sample_axis)?;

        let (x_min, x_max, y_min, y_max) = view_bounds;
        let min_step = ((x_max - x_min).abs() / grid_size.max(1) as f64)
            .min((y_max - y_min).abs() / grid_size.max(1) as f64);
        if !f32_bounds_have_precision(&[x_min, x_max, y_min, y_max], min_step) {
            return None;
        }
        Some(ImplicitSubmit {
            params: GridParamsUniform {
                x_min: x_min as f32,
                x_max: x_max as f32,
                y_min: y_min as f32,
                y_max: y_max as f32,
                grid_size: sample_axis as u32,
                code_len: prog.code.len() as u32,
                _pad0: 0,
                _pad1: 0,
            },
            code: prog.code,
            constants: prog.constants,
            sample_axis,
            sample_count,
        })
    }

    /// Escribe uniformes, hace submit del dispatch y arma el `map_async`.
    /// Barato y non-blocking: no espera a la GPU. Retorna el flag que el
    /// waiter distribuido en frames (o el poll síncrono legacy) observará.
    fn submit_buffers(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submit: &ImplicitSubmit,
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
                // GPU timing opt-in tras la feature `profiling` (ver gpu_timing);
                // sin la feature esto es `None` — cero costo en release.
                timestamp_writes: crate::gpu_timing::timestamp_writes(&self.timing, 0),
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg = (submit.sample_axis as u32).div_ceil(16).max(1);
            cpass.dispatch_workgroups(wg, wg, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.values_buffer,
            0,
            &self.values_readback,
            0,
            (submit.sample_count * std::mem::size_of::<f32>()) as u64,
        );
        crate::gpu_timing::resolve(&self.timing, &mut encoder);
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
        map_ok
    }

    /// Copia inmediata del readback ya mapeado (sin espera GPU). Indexación
    /// defensiva con `get`: un buffer corto retorna `None` honesto en vez de
    /// panic (el path legacy indexaba directo).
    fn copy_mapped_rows(&self, grid_size: usize) -> Option<Vec<Vec<f64>>> {
        let slice = self.values_readback.slice(..);
        let data = slice.get_mapped_range();
        let values_f32: &[f32] = bytemuck::cast_slice(&data);
        let mut rows = Vec::with_capacity(grid_size + 1);
        for j in 0..=grid_size {
            let mut row = Vec::with_capacity(grid_size + 1);
            for i in 0..=grid_size {
                let v = values_f32
                    .get(j * (grid_size + 1) + i)
                    .copied()
                    .unwrap_or(f32::NAN);
                row.push(if v.is_finite() { v as f64 } else { f64::NAN });
            }
            rows.push(row);
        }
        drop(data);
        self.values_readback.unmap();
        Some(rows)
    }

    /// Libera el buffer readback sin bloquear. Idempotente: si el `map_async`
    /// falló o sigue pendiente, es no-op seguro; se llama en todo camino de
    /// descarte (job obsoleto, timeout, objeto borrado) para no dejar el
    /// pipeline inutilizado.
    pub fn abort_eval(&self) {
        self.values_readback.unmap();
    }

    /// Dispatch sin espera (frente B6): hace submit + `map_async` y retorna
    /// inmediatamente con un [`PendingImplicitEval`]. El hilo del frame nunca
    /// bloquea; el resolve llega en frames posteriores vía avance del job.
    /// Retorna `None` en los mismos casos que [`Self::evaluate`].
    pub fn dispatch_eval(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ic: &ImplicitCurveObj,
        view_bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<PendingImplicitEval> {
        let submit = self.plan_submit(ic, view_bounds, grid_size, variables)?;
        let grid_size = submit.sample_axis.saturating_sub(1);
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Implicit compute async dispatch (wait distributed over frames)");
        Some(PendingImplicitEval {
            grid_size,
            wait: PendingGpuReadback::submit(&map_ok),
        })
    }

    /// Resolve non-blocking de un dispatch previo. Solo copia si el poll ya
    /// reportó `Mapped`; en cualquier otro caso hace `unmap` y retorna `None`
    /// (el llamante usa el fallback CPU honesto). Nunca espera: el llamante
    /// debe haber hecho el `device.poll(Maintain::Poll)` no-bloqueante del
    /// frame antes de llamar.
    pub fn resolve_eval(&self, pending: PendingImplicitEval) -> Option<Vec<Vec<f64>>> {
        // Nota profiling: el path síncrono lee `gpu_timing::read_and_log` tras
        // el wait; aquí se omite a propósito porque su poll de 50 ms
        // reintroduciría bloqueo en el hilo UI.
        let PendingImplicitEval {
            grid_size,
            mut wait,
        } = pending;
        if wait.poll() != ReadbackPoll::Mapped {
            self.abort_eval();
            return None;
        }
        self.copy_mapped_rows(grid_size)
    }

    /// Evaluate the relation-normalized implicit scalar field on the GPU and
    /// return a grid of values. Returns `None` if the expression cannot be
    /// compiled to GPU bytecode (caller should fall back to CPU).
    ///
    /// Path síncrono legacy (bloquea hasta 250 ms): solo para callers sin slot
    /// background. El prepare 2D usa `dispatch_eval` + avance del job.
    pub fn evaluate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ic: &ImplicitCurveObj,
        view_bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> Option<Vec<Vec<f64>>> {
        let submit = self.plan_submit(ic, view_bounds, grid_size, variables)?;
        let grid_size = submit.sample_axis.saturating_sub(1);
        let map_ok = self.submit_buffers(device, queue, &submit);
        log::trace!("Implicit compute sync readback (bounded poll) — 1 intento por frame");
        let mapped = crate::sync_readback_with_timeout(device, &map_ok);
        crate::gpu_timing::read_and_log(&self.timing, device, "Implicit Compute");

        if !mapped {
            // `unmap` is idempotent: when `map_async` reported an
            // error the buffer was never mapped, so this is a no-op.
            self.values_readback.unmap();
            return None;
        }
        self.copy_mapped_rows(grid_size)
    }
}

/// Grid implícito por calidad de render (origen único sync/async).
fn implicit_grid_size_for_quality(view: &ViewTransform, quality: RenderQuality) -> usize {
    match quality {
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
        .min(grafito_core::implicit_curve::MAX_IMPLICIT_GRID_SIZE),
    }
}

/// Niveles de contorno del objeto (origen único sync/async).
fn implicit_contour_levels(ic: &ImplicitCurveObj) -> Vec<f64> {
    ic.contour_levels
        .as_ref()
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![0.0])
}

/// Escribe segmentos + key + región en el cache del objeto (origen único
/// sync/async).
fn populate_implicit_cache(
    ic: &ImplicitCurveObj,
    key: &ImplicitCurveCacheKey,
    padded_bounds: (f64, f64, f64, f64),
    segments: ImplicitCurveSegments,
) {
    *ic.cached_segments.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = segments;
    *ic.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key.clone());
    *ic.cached_region.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(padded_bounds);
}

/// Dispatch sin espera a nivel cache (frente B6): replica los chequeos de
/// [`maybe_compute_on_gpu`] y retorna el job en vuelo en vez de bloquear.
/// El llamante guarda el job en el slot (cap 1) y lo avanza con
/// [`advance_implicit_job`] en frames posteriores.
pub fn dispatch_implicit_on_gpu(
    compute: &ImplicitComputePipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ic: &ImplicitCurveObj,
    view: &ViewTransform,
    variables: &HashMap<String, f64>,
    quality: RenderQuality,
) -> ImplicitDispatchOutcome {
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(view.screen_size);
    let view_bounds = (
        world_tl.x.min(world_br.x),
        world_tl.x.max(world_br.x),
        world_br.y.min(world_tl.y),
        world_br.y.max(world_tl.y),
    );
    let padded_bounds = grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);
    let grid_size = implicit_grid_size_for_quality(view, quality);

    let key = ic.cache_key(padded_bounds, grid_size, variables);
    {
        let cached_key = ic.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return ImplicitDispatchOutcome::Cached;
        }
    }

    match compute.dispatch_eval(device, queue, ic, padded_bounds, grid_size, variables) {
        Some(eval) => ImplicitDispatchOutcome::Dispatched(Box::new(PendingImplicitJob {
            key,
            padded_bounds,
            grid_size,
            levels: implicit_contour_levels(ic),
            stage: ImplicitJobStage::AwaitingMap(eval),
        })),
        None => ImplicitDispatchOutcome::Unsupported,
    }
}

/// Avanza un job implícito sin bloquear jamás (llamar tras el
/// `device.poll(Maintain::Poll)` no-bloqueante del frame).
///
/// - `AwaitingMap` + poll `Pending` → `NotReady` (el job vuelve al slot).
/// - `AwaitingMap` + poll `Mapped` → copia rows, valida contra CPU y spawna
///   el marching-squares en background thread → `NotReady`.
/// - `AwaitingMap` + poll `Failed`/timeout → `unmap` + `Done(false)`.
/// - `Processing` + `try_recv` vacío → `NotReady`; con segmentos → popula
///   cache (tras re-chequeo de vigencia) → `Done(true)`; thread muerto →
///   `Done(false)` honesto.
pub fn advance_implicit_job(
    compute: &ImplicitComputePipeline,
    ic: &ImplicitCurveObj,
    variables: &HashMap<String, f64>,
    job: PendingImplicitJob,
) -> ImplicitResolveStep {
    let PendingImplicitJob {
        key,
        padded_bounds,
        grid_size,
        levels,
        stage,
    } = job;
    // Vigencia: la key incluye exprs, bounds con padding/snapping y variables
    // referenciadas; si el objeto cambió entre dispatch y avance, el resultado
    // es de otra escena y se descarta sin escribir.
    let fresh = ic.cache_key(padded_bounds, grid_size, variables);
    if fresh != key {
        log::debug!("Implicit GPU job obsoleto (key cambió); descartando sin escribir");
        compute.abort_eval();
        return ImplicitResolveStep::Done(false);
    }
    match stage {
        ImplicitJobStage::AwaitingMap(eval) => {
            let Some(rows) = compute.resolve_eval(eval) else {
                return ImplicitResolveStep::Done(false);
            };
            if !nonfinite_gpu_field_matches_cpu(ic, &rows, padded_bounds, grid_size, variables) {
                return ImplicitResolveStep::Done(false);
            }
            let segments_rx = spawn_marching_squares(rows, levels, padded_bounds);
            ImplicitResolveStep::NotReady(Box::new(PendingImplicitJob {
                key,
                padded_bounds,
                grid_size,
                levels: Vec::new(),
                stage: ImplicitJobStage::Processing(std::sync::Mutex::new(segments_rx)),
            }))
        }
        ImplicitJobStage::Processing(rx_mutex) => {
            let poll = match rx_mutex.lock() {
                Ok(rx) => poll_segments_thread(&rx),
                Err(_) => {
                    log::warn!("Implicit marching-squares lock envenenado; fallback CPU");
                    SegmentsPoll::Gone
                }
            };
            match poll {
                SegmentsPoll::Ready(segments) => {
                    populate_implicit_cache(ic, &key, padded_bounds, segments);
                    ImplicitResolveStep::Done(true)
                }
                SegmentsPoll::Pending => {
                    ImplicitResolveStep::NotReady(Box::new(PendingImplicitJob {
                        key,
                        padded_bounds,
                        grid_size,
                        levels,
                        stage: ImplicitJobStage::Processing(rx_mutex),
                    }))
                }
                SegmentsPoll::Gone => {
                    log::warn!("Implicit marching-squares thread murió; fallback CPU");
                    ImplicitResolveStep::Done(false)
                }
            }
        }
    }
}

/// Try to populate the implicit-curve cache using the GPU compute pipeline.
/// Returns `true` if the cache was populated (either already cached or freshly
/// computed on the GPU). Returns `false` if the GPU path is unavailable or the
/// expression is not supported by the bytecode machine.
///
/// Path síncrono legacy (bloquea hasta 250 ms + marching-squares en el hilo
/// UI). El prepare 2D usa `dispatch_implicit_on_gpu` + `advance_implicit_job`.
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
    let grid_size = implicit_grid_size_for_quality(view, quality);

    let key = ic.cache_key(padded_bounds, grid_size, variables);
    {
        let cached_key = ic.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return true;
        }
    }

    let Some(rows) = compute.evaluate(device, queue, ic, padded_bounds, grid_size, variables)
    else {
        return false;
    };
    if !nonfinite_gpu_field_matches_cpu(ic, &rows, padded_bounds, grid_size, variables) {
        return false;
    }

    let levels = implicit_contour_levels(ic);
    let segments = marching_squares_from_grid(
        &rows,
        &levels,
        padded_bounds.0,
        padded_bounds.2,
        padded_bounds.1,
        padded_bounds.3,
    );
    populate_implicit_cache(ic, &key, padded_bounds, segments);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::RelationOperator;

    #[test]
    fn greater_relations_prepare_the_cpu_field_for_nonzero_contours() {
        for operator in [RelationOperator::Greater, RelationOperator::GreaterEq] {
            let curve = ImplicitCurveObj::new("y", "3", operator);
            let field = prepare_implicit_field(&curve, &HashMap::new()).unwrap();

            assert_eq!(field.eval_2d("x", 0.0, "y", 1.0), 2.0);
            assert_eq!(field.eval_2d("x", 0.0, "y", 3.0), 0.0);

            let segments = marching_squares_from_grid(
                &[vec![2.0, 2.0], vec![0.0, 0.0]],
                &[1.0],
                0.0,
                1.0,
                1.0,
                3.0,
            );
            let (_, segments) = &segments[0];
            assert_eq!(segments.len(), 1);
            assert!((segments[0].0.y - 2.0).abs() <= f64::EPSILON);
            assert!((segments[0].1.y - 2.0).abs() <= f64::EPSILON);
        }
    }

    #[test]
    fn compiler_accepts_an_expression_using_exactly_32_stack_slots() {
        let expression = (1..32).fold("x".to_string(), |expression, _| {
            format!("min(x, {expression})")
        });
        let expression =
            grafito_geometry::expr::prepare_function_ast(&expression, &HashMap::new(), &["x"])
                .expect("the exact-stack expression must parse");
        let mut program = BytecodeProgram::default();

        compile_expr(&expression, &HashMap::new(), &mut program)
            .expect("the compiler must accept the shader's 32-slot stack limit");
        assert_eq!(program.code.len(), 63);
    }

    #[test]
    fn gpu_bytecode_rejects_bessel_expressions_for_cpu_domain_handling() {
        let expression = grafito_geometry::expr::prepare_function_ast(
            "besselj(n, x)",
            &HashMap::new(),
            &["x", "n"],
        )
        .unwrap();
        let mut program = BytecodeProgram::default();

        assert!(matches!(
            compile_expr(&expression, &HashMap::new(), &mut program),
            Err(CompileError::UnsupportedNode(_))
        ));
    }

    #[test]
    fn compiler_rejects_clamps_with_runtime_bounds() {
        let expression = grafito_geometry::expr::prepare_function_ast(
            "clamp(x, x + 1, x)",
            &HashMap::new(),
            &["x"],
        )
        .expect("runtime-bound clamp must parse");
        let mut program = BytecodeProgram::default();

        assert!(matches!(
            compile_expr(&expression, &HashMap::new(), &mut program),
            Err(CompileError::RuntimeClampBounds)
        ));
    }

    #[test]
    fn compiler_rejects_valid_clamp_bounds_that_collapse_to_f32() {
        let expression = grafito_geometry::expr::prepare_function_ast(
            "clamp(x, 1, 1.00000001)",
            &HashMap::new(),
            &["x"],
        )
        .expect("near-equal clamp must parse");
        let mut program = BytecodeProgram::default();

        assert!(matches!(
            compile_expr(&expression, &HashMap::new(), &mut program),
            Err(CompileError::PrecisionLoss)
        ));
    }
}
