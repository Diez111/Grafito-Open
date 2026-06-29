use crate::math::complex_expr::ComplexExpr;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ComplexOp {
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
    Gamma = 100,
    BesselJ = 101,
    Conjugate = 102,
    RealPart = 103,
    ImagPart = 104,
    Arg = 105,
    Erf = 106,
    LambertW = 107,
    Zeta = 108,
    BesselY = 109,
}

impl ComplexOp {
    pub fn encode(self, operand: u32) -> u32 {
        (self as u32) | (operand << 8)
    }
}

#[derive(Debug, Default)]
pub struct ComplexBytecodeProgram {
    pub code: Vec<u32>,
    pub constants: Vec<f64>, // Pares (re, im) intercalados: constants[2*i]=re, constants[2*i+1]=im
}

pub enum CompileError {
    UnsupportedNode(String),
    UnsupportedVariable(String),
    StackTooDeep,
    TooManyConstants,
}

pub fn compile_complex_expr(
    expr: &ComplexExpr,
    document_vars: &HashMap<String, f64>,
    var_map: &[(&str, u32)],
    prog: &mut ComplexBytecodeProgram,
) -> Result<(), CompileError> {
    match expr {
        ComplexExpr::Const(c) => {
            if prog.constants.len() >= 254 {
                return Err(CompileError::TooManyConstants);
            }
            let idx = prog.constants.len() as u32;
            prog.constants.push(c.re);
            prog.constants.push(c.im);
            prog.code.push(ComplexOp::PushConst.encode(idx));
        }
        ComplexExpr::Var(name) => {
            let name = name.as_str();
            if let Some((_, operand)) = var_map.iter().find(|(n, _)| *n == name) {
                prog.code.push(ComplexOp::PushVar.encode(*operand));
            } else if name == "i" {
                let idx = prog.constants.len() as u32;
                prog.constants.push(0.0);
                prog.constants.push(1.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else if name == "e" {
                let idx = prog.constants.len() as u32;
                prog.constants.push(std::f64::consts::E);
                prog.constants.push(0.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else if name == "pi" {
                let idx = prog.constants.len() as u32;
                prog.constants.push(std::f64::consts::PI);
                prog.constants.push(0.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else if let Some(v) = document_vars.get(name) {
                if prog.constants.len() >= 254 {
                    return Err(CompileError::TooManyConstants);
                }
                let idx = prog.constants.len() as u32;
                prog.constants.push(*v);
                prog.constants.push(0.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else {
                return Err(CompileError::UnsupportedVariable(name.to_string()));
            }
        }
        ComplexExpr::Add(a, b) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            compile_complex_expr(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Add.encode(0));
        }
        ComplexExpr::Sub(a, b) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            compile_complex_expr(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sub.encode(0));
        }
        ComplexExpr::Mul(a, b) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            compile_complex_expr(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Mul.encode(0));
        }
        ComplexExpr::Div(a, b) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            compile_complex_expr(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Div.encode(0));
        }
        ComplexExpr::Pow(a, b) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            compile_complex_expr(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Pow.encode(0));
        }
        ComplexExpr::Neg(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Neg.encode(0));
        }
        ComplexExpr::Sin(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sin.encode(0));
        }
        ComplexExpr::Cos(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Cos.encode(0));
        }
        ComplexExpr::Tan(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Tan.encode(0));
        }
        ComplexExpr::Exp(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Exp.encode(0));
        }
        ComplexExpr::Ln(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Log.encode(0));
        }
        ComplexExpr::Sqrt(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sqrt.encode(0));
        }
        ComplexExpr::Abs(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Abs.encode(0));
        }
        ComplexExpr::Asin(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Asin.encode(0));
        }
        ComplexExpr::Acos(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Acos.encode(0));
        }
        ComplexExpr::Atan(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Atan.encode(0));
        }
        ComplexExpr::Sinh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sinh.encode(0));
        }
        ComplexExpr::Cosh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Cosh.encode(0));
        }
        ComplexExpr::Tanh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Tanh.encode(0));
        }
        ComplexExpr::Asinh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Asinh.encode(0));
        }
        ComplexExpr::Acosh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Acosh.encode(0));
        }
        ComplexExpr::Atanh(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Atanh.encode(0));
        }
        ComplexExpr::Gamma(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Gamma.encode(0));
        }
        ComplexExpr::BesselJ(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::BesselJ.encode(0));
        }
        ComplexExpr::Conjugate(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Conjugate.encode(0));
        }
        ComplexExpr::RealPart(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::RealPart.encode(0));
        }
        ComplexExpr::ImagPart(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::ImagPart.encode(0));
        }
        ComplexExpr::Arg(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Arg.encode(0));
        }
        ComplexExpr::Erf(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Erf.encode(0));
        }
        ComplexExpr::LambertW(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::LambertW.encode(0));
        }
        ComplexExpr::Zeta(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Zeta.encode(0));
        }
        ComplexExpr::BesselY(a) => {
            compile_complex_expr(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::BesselY.encode(0));
        }
    }

    if prog.code.len() > 4096 {
        return Err(CompileError::StackTooDeep);
    }
    Ok(())
}
