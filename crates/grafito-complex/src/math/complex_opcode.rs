use crate::math::complex_expr::ComplexExpr;
use num_complex::Complex64;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ComplexOp {
    // NOTA: los códigos 16–19 (ex Min/Max/Floor/Ceil) y 31–33 (ex Sec/Csc/Cot)
    // están retirados y se dejan sin asignar: el compilador jamás los emitió
    // (el parser desazucara sec/csc/cot a Div(1, …)) y su semántica divergía
    // del walk (Min/Max por norma, Floor/Ceil por componentes, gates 1e-15 vs
    // 1e-30). No reutilizar los números: el formato u32 lo comparte la GPU.
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
    Asin = 22,
    Acos = 23,
    Atan = 24,
    Sinh = 25,
    Cosh = 26,
    Tanh = 27,
    Asinh = 28,
    Acosh = 29,
    Atanh = 30,
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

#[derive(Debug)]
pub enum CompileError {
    UnsupportedNode(String),
    UnsupportedVariable(String),
    StackTooDeep,
    TooManyConstants,
}

pub fn compile_complex_expr(
    expr: &ComplexExpr,
    document_vars: &BTreeMap<String, f64>,
    var_map: &[(&str, u32)],
    prog: &mut ComplexBytecodeProgram,
) -> Result<(), CompileError> {
    // La validación de pila corre UNA vez sobre el programa completo (ver
    // `validate_complex_program`): hacerla por nodo recursivo rechaza
    // cualquier expresión binaria (tras compilar ambos hijos la pila queda
    // en 2 antes de emitir el opcode padre).
    compile_inner(expr, document_vars, var_map, prog)?;
    validate_complex_program(prog)
}

fn compile_inner(
    expr: &ComplexExpr,
    document_vars: &BTreeMap<String, f64>,
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
                if prog.constants.len() >= 254 {
                    return Err(CompileError::TooManyConstants);
                }
                let idx = prog.constants.len() as u32;
                prog.constants.push(0.0);
                prog.constants.push(1.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else if name == "e" {
                if prog.constants.len() >= 254 {
                    return Err(CompileError::TooManyConstants);
                }
                let idx = prog.constants.len() as u32;
                prog.constants.push(std::f64::consts::E);
                prog.constants.push(0.0);
                prog.code.push(ComplexOp::PushConst.encode(idx));
            } else if name == "pi" {
                if prog.constants.len() >= 254 {
                    return Err(CompileError::TooManyConstants);
                }
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
            compile_inner(a, document_vars, var_map, prog)?;
            compile_inner(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Add.encode(0));
        }
        ComplexExpr::Sub(a, b) => {
            compile_inner(a, document_vars, var_map, prog)?;
            compile_inner(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sub.encode(0));
        }
        ComplexExpr::Mul(a, b) => {
            compile_inner(a, document_vars, var_map, prog)?;
            compile_inner(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Mul.encode(0));
        }
        ComplexExpr::Div(a, b) => {
            compile_inner(a, document_vars, var_map, prog)?;
            compile_inner(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Div.encode(0));
        }
        ComplexExpr::Pow(a, b) => {
            compile_inner(a, document_vars, var_map, prog)?;
            compile_inner(b, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Pow.encode(0));
        }
        ComplexExpr::Neg(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Neg.encode(0));
        }
        ComplexExpr::Sin(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sin.encode(0));
        }
        ComplexExpr::Cos(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Cos.encode(0));
        }
        ComplexExpr::Tan(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Tan.encode(0));
        }
        ComplexExpr::Exp(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Exp.encode(0));
        }
        ComplexExpr::Ln(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Log.encode(0));
        }
        ComplexExpr::Sqrt(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sqrt.encode(0));
        }
        ComplexExpr::Abs(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Abs.encode(0));
        }
        ComplexExpr::Asin(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Asin.encode(0));
        }
        ComplexExpr::Acos(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Acos.encode(0));
        }
        ComplexExpr::Atan(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Atan.encode(0));
        }
        ComplexExpr::Sinh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Sinh.encode(0));
        }
        ComplexExpr::Cosh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Cosh.encode(0));
        }
        ComplexExpr::Tanh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Tanh.encode(0));
        }
        ComplexExpr::Asinh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Asinh.encode(0));
        }
        ComplexExpr::Acosh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Acosh.encode(0));
        }
        ComplexExpr::Atanh(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Atanh.encode(0));
        }
        ComplexExpr::Gamma(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Gamma.encode(0));
        }
        ComplexExpr::BesselJ(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::BesselJ.encode(0));
        }
        ComplexExpr::Conjugate(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Conjugate.encode(0));
        }
        ComplexExpr::RealPart(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::RealPart.encode(0));
        }
        ComplexExpr::ImagPart(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::ImagPart.encode(0));
        }
        ComplexExpr::Arg(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Arg.encode(0));
        }
        ComplexExpr::Erf(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Erf.encode(0));
        }
        ComplexExpr::LambertW(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::LambertW.encode(0));
        }
        ComplexExpr::Zeta(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::Zeta.encode(0));
        }
        ComplexExpr::BesselY(a) => {
            compile_inner(a, document_vars, var_map, prog)?;
            prog.code.push(ComplexOp::BesselY.encode(0));
        }
        ComplexExpr::DerivZ(_) | ComplexExpr::DerivZConj(_) => {
            return Err(CompileError::UnsupportedNode(format!("{expr:?}")));
        }
    }

    Ok(())
}

/// Valida el programa COMPLETO una sola vez (cota 4096 ops, 254 elementos de
/// constantes = 127 constantes complejas (re, im intercalados), profundidad
/// máxima 64, exactamente un valor final en pila). El rechazo
/// fino a 32 slots WGSL vive en `gpu_program_is_supported` (render), no acá.
fn validate_complex_program(prog: &ComplexBytecodeProgram) -> Result<(), CompileError> {
    if prog.code.len() > 4096 {
        return Err(CompileError::StackTooDeep);
    }
    if prog.constants.len() > 254 {
        return Err(CompileError::TooManyConstants);
    }
    // Validación de profundidad de pila del programa compilado (GPU: 32).
    // El compilador no rastreaba stack, permitiendo código que luego fallaba en
    // gpu_program_is_supported. Rechazar aquí evita dispatch inútil.
    {
        let mut depth = 0usize;
        let mut max_depth = 0usize;
        for instr in &prog.code {
            match instr & 0xFF {
                0 => {}
                1 | 2 => depth += 1,
                3..=7 => {
                    if depth < 2 {
                        return Err(CompileError::StackTooDeep);
                    }
                    depth -= 1;
                }
                8..=15 | 22..=30 | 102..=105 => {
                    if depth == 0 {
                        return Err(CompileError::StackTooDeep);
                    }
                }
                100 | 101 | 106..=109 => {
                    if depth == 0 {
                        return Err(CompileError::StackTooDeep);
                    }
                }
                _ => {
                    return Err(CompileError::UnsupportedNode(format!(
                        "opcode {}",
                        instr & 0xFF
                    )))
                }
            }
            max_depth = max_depth.max(depth);
            if max_depth > 64 {
                return Err(CompileError::StackTooDeep);
            }
        }
        if depth != 1 {
            // Programa mal formado (no deja exactamente un valor en la pila)
            return Err(CompileError::StackTooDeep);
        }
    }
    Ok(())
}

/// Ejecuta un programa compilado sobre valores directos (sin `HashMap`).
///
/// `vars[slot]` = valor del slot (ver `var_map` de compilación; los
/// `document_vars` ya van horneados como constantes). Semántica idéntica al
/// walk (`ComplexExpr::eval`): mismas llamadas (`ComplexMatrix::add/sub/mul/div`,
/// `powc` con su gate, mismas trascendentes y especiales), así que el
/// resultado es bit a bit el del árbol.
///
/// `None` = opcode desconocido, slot/constante fuera de rango o pila
/// degenerada (el llamante conserva el walk como fallback honesto; con
/// programas de `compile_complex_expr` — ya validados — no ocurre).
pub fn exec_cpu(prog: &ComplexBytecodeProgram, vars: &[Complex64]) -> Option<Complex64> {
    use crate::math::complex_expr::{
        complex_bessel_j, complex_bessel_y, complex_erf, complex_gamma, complex_lambert_w,
        complex_zeta, ComplexMatrix,
    };
    use num_complex::Complex64;

    // Pila fija (el validador acota la profundidad a 64): sin allocs por
    // ejecución, sin pánicos por desborde (chequeos defensivos).
    let mut stack = [Complex64::new(0.0, 0.0); 64];
    let mut sp = 0usize;
    macro_rules! push {
        ($v:expr) => {{
            if sp >= stack.len() {
                return None;
            }
            stack[sp] = $v;
            sp += 1;
        }};
    }
    macro_rules! pop {
        () => {{
            if sp == 0 {
                return None;
            }
            sp -= 1;
            stack[sp]
        }};
    }
    macro_rules! unary {
        ($f:expr) => {{
            let a = pop!();
            // Mismas llamadas que el walk (round-trip matricial exacto;
            // num-complex 0.4 toma `self` por valor).
            push!(
                ComplexMatrix::from_complex($f(ComplexMatrix::from_complex(a).to_complex()))
                    .to_complex()
            );
        }};
    }
    for word in prog.code.iter() {
        let op = (word & 0xFF) as u8;
        let operand = word >> 8;
        match op {
            x if x == ComplexOp::Nop as u8 => {}
            x if x == ComplexOp::PushConst as u8 => {
                // Ojo: el operando ya es offset en ELEMENTOS (`idx` =
                // `constants.len()` antes del push del par), no índice de par.
                let i = operand as usize;
                let re = *prog.constants.get(i)?;
                let im = *prog.constants.get(i + 1)?;
                push!(Complex64::new(re, im));
            }
            x if x == ComplexOp::PushVar as u8 => {
                push!(*vars.get(operand as usize)?);
            }
            x if x == ComplexOp::Add as u8 => {
                let b = pop!();
                let a = pop!();
                let ma = ComplexMatrix::from_complex(a);
                let mb = ComplexMatrix::from_complex(b);
                push!(ma.add(&mb).to_complex());
            }
            x if x == ComplexOp::Sub as u8 => {
                let b = pop!();
                let a = pop!();
                push!(ComplexMatrix::from_complex(a)
                    .sub(&ComplexMatrix::from_complex(b))
                    .to_complex());
            }
            x if x == ComplexOp::Mul as u8 => {
                let b = pop!();
                let a = pop!();
                push!(ComplexMatrix::from_complex(a)
                    .mul(&ComplexMatrix::from_complex(b))
                    .to_complex());
            }
            x if x == ComplexOp::Div as u8 => {
                let b = pop!();
                let a = pop!();
                // Mismo gate que el walk (`det < 1e-30` → singular).
                let res = ComplexMatrix::from_complex(a).div(&ComplexMatrix::from_complex(b))?;
                push!(res.to_complex());
            }
            x if x == ComplexOp::Pow as u8 => {
                let exp = pop!();
                let base = pop!();
                if base.norm() < 1e-300 && exp.re < 0.0 && exp.im.abs() < 1e-12 {
                    return None;
                }
                push!(base.powc(exp));
            }
            x if x == ComplexOp::Neg as u8 => {
                let a = pop!();
                push!(-a);
            }
            x if x == ComplexOp::Sin as u8 => unary!(Complex64::sin),
            x if x == ComplexOp::Cos as u8 => unary!(Complex64::cos),
            x if x == ComplexOp::Tan as u8 => unary!(Complex64::tan),
            x if x == ComplexOp::Exp as u8 => unary!(Complex64::exp),
            // Ojo: el opcode se llama `Log` pero el compilador lo emite
            // para `Ln` (logaritmo principal, no log10).
            x if x == ComplexOp::Log as u8 => unary!(Complex64::ln),
            x if x == ComplexOp::Sqrt as u8 => unary!(Complex64::sqrt),
            x if x == ComplexOp::Abs as u8 => {
                let a = pop!();
                push!(Complex64::new(a.norm(), 0.0));
            }
            x if x == ComplexOp::Asin as u8 => unary!(Complex64::asin),
            x if x == ComplexOp::Acos as u8 => unary!(Complex64::acos),
            x if x == ComplexOp::Atan as u8 => unary!(Complex64::atan),
            x if x == ComplexOp::Sinh as u8 => unary!(Complex64::sinh),
            x if x == ComplexOp::Cosh as u8 => unary!(Complex64::cosh),
            x if x == ComplexOp::Tanh as u8 => unary!(Complex64::tanh),
            x if x == ComplexOp::Asinh as u8 => unary!(Complex64::asinh),
            x if x == ComplexOp::Acosh as u8 => unary!(Complex64::acosh),
            x if x == ComplexOp::Atanh as u8 => unary!(Complex64::atanh),
            x if x == ComplexOp::Gamma as u8 => unary!(complex_gamma),
            x if x == ComplexOp::BesselJ as u8 => {
                let a = pop!();
                push!(complex_bessel_j(0.0, a));
            }
            x if x == ComplexOp::Conjugate as u8 => {
                let a = pop!();
                // `conj` es el único que toma `&self` en num-complex 0.4.
                push!(ComplexMatrix::from_complex(a.conj()).to_complex());
            }
            x if x == ComplexOp::RealPart as u8 => {
                let a = pop!();
                push!(Complex64::new(a.re, 0.0));
            }
            x if x == ComplexOp::ImagPart as u8 => {
                let a = pop!();
                push!(Complex64::new(a.im, 0.0));
            }
            x if x == ComplexOp::Arg as u8 => {
                let a = pop!();
                push!(Complex64::new(a.arg(), 0.0));
            }
            x if x == ComplexOp::Erf as u8 => unary!(complex_erf),
            x if x == ComplexOp::LambertW as u8 => unary!(complex_lambert_w),
            x if x == ComplexOp::Zeta as u8 => unary!(complex_zeta),
            x if x == ComplexOp::BesselY as u8 => {
                let a = pop!();
                push!(complex_bessel_y(0.0, a));
            }
            _ => return None,
        }
    }
    if sp == 1 {
        Some(stack[0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::complex_expr::parse;
    use num_complex::Complex64;
    use std::collections::HashMap;

    /// Paridad Ola 2 B13: `exec_cpu` acuerda con el walk en gaps
    /// (finito/no-finito: lo que decide celdas dibujadas) y en valores
    /// cuando ambos son finitos. NOTA: no se exige bit a bit en NaN porque
    /// los payloads dependen del codegen del call-site (destino vs fuente
    /// en MULSD/ADDSD): el mismo `add` da payloads distintos según el
    /// contexto de inlineado. Los renderers solo miran `is_finite`, así
    /// que la paridad que importa es la de gaps + valores finitos.
    ///
    /// Casos: polos (`1/(z^2+1)` en ±i), divisiones casi-singulares (gate
    /// 1e-30), gates de `Pow`, entradas no finitas.
    #[test]
    fn exec_cpu_coincide_con_walk_en_malla_adversarial() {
        let casos = [
            "z",
            "1/z",
            "z^2+2*z+1",
            "sin(z)/cos(z)",
            "exp(z)/z",
            "sqrt(z-1)",
            "z^z",
            "z^(-1)",
            "gamma(z)",
            "conj(z)*z",
            "1/(z^2+1)",
            "log(z)",
            "(z-1)/(z+1)",
            "z^0.5",
        ];
        let coords = [
            -2.0,
            -1.0,
            -0.5,
            0.0,
            0.5,
            1.0,
            2.0,
            1e-16,
            -1e-16,
            1e-300,
            1e300,
            f64::INFINITY,
            f64::NAN,
        ];
        for expr_str in casos {
            let expr = parse(expr_str).unwrap_or_else(|e| panic!("parsea: {expr_str}: {e}"));
            let mut prog = ComplexBytecodeProgram::default();
            compile_complex_expr(&expr, &BTreeMap::new(), &[("z", 0)], &mut prog)
                .unwrap_or_else(|e| panic!("compila: {expr_str}: {e:?}"));
            for &x in &coords {
                for &y in &coords {
                    let z = Complex64::new(x, y);
                    let mut vars = HashMap::new();
                    vars.insert("z".to_string(), z);
                    // Efectivo para render: `Some` solo si finito (igual que
                    // los call-sites filtran). Así la paridad es sobre lo que
                    // el usuario ve (celda dibujada o no + valor).
                    let walk = expr
                        .eval(&vars)
                        .ok()
                        .filter(|v| v.re.is_finite() && v.im.is_finite());
                    let flat =
                        exec_cpu(&prog, &[z]).filter(|v| v.re.is_finite() && v.im.is_finite());
                    match (walk, flat) {
                        (Some(w), Some(f)) => {
                            let tol = 1e-12 * 1.0f64.max(w.norm()).max(f.norm());
                            assert!(
                                (w - f).norm() <= tol,
                                "deriva en {expr_str} z=({x},{y}): walk={w:?} flat={f:?}"
                            );
                        }
                        (None, None) => {}
                        (w, f) => {
                            panic!("gap en {expr_str} z=({x},{y}): walk={w:?} flat={f:?}")
                        }
                    }
                }
            }
        }
        // DerivZ no compila (fallback al walk en los llamantes).
        let deriv = parse("deriv_z(z^2)").expect("parse deriv");
        let mut prog = ComplexBytecodeProgram::default();
        assert!(
            compile_complex_expr(&deriv, &BTreeMap::new(), &[("z", 0)], &mut prog).is_err(),
            "deriv_z debe rechazar compilación"
        );
    }

    fn right_nested_sum(terms: usize) -> String {
        (1..terms).fold("z".to_owned(), |expr, _| format!("z + ({expr})"))
    }

    #[test]
    fn binary_nodes_compile_with_exact_final_stack_accounting() {
        // Regresión: la validación corría por nodo recursivo y rechazaba
        // cualquier binaria (pila intermedia en 2 antes del opcode padre).
        for terms in [2, 3, 32] {
            let expr = parse(&right_nested_sum(terms)).expect("debe parsear");
            let mut prog = ComplexBytecodeProgram::default();
            compile_complex_expr(&expr, &BTreeMap::new(), &[("z", 0)], &mut prog)
                .expect("programa bien formado debe compilar");
            assert_eq!(prog.code.len(), 2 * terms - 1);
        }
        // 33 compila (cota compilador: 64); el rechazo a 32 slots WGSL
        // lo hace `gpu_program_is_supported` en render, no el compilador.
        let expr = parse(&right_nested_sum(33)).expect("debe parsear");
        let mut prog = ComplexBytecodeProgram::default();
        compile_complex_expr(&expr, &BTreeMap::new(), &[("z", 0)], &mut prog)
            .expect("33 compila, el gate GPU lo rechaza después");
    }
}
