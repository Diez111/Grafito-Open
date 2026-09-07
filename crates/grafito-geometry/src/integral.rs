//! Integración numérica híbrida.
//!
//! Provee una ruta rápida para integrales definidas de funciones `y = f(x)`:
//! evaluar `f(x)` en una grilla fina (paralela en CPU o, opcionalmente, en GPU)
//! y reducir con una regla de cuadratura compuesta en CPU.

use rayon::prelude::*;
use std::fmt;

/// Máximo de muestras solicitables a cualquiera de los integradores híbridos.
pub const MAX_HYBRID_INTEGRAL_SAMPLES: usize = 100_000;

const HYBRID_RELATIVE_TOLERANCE: f64 = 1e-6;

/// Error estructurado de una integración híbrida no validada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HybridIntegralError {
    /// La cantidad solicitada excede el presupuesto fijo.
    SampleLimitExceeded { requested: usize, maximum: usize },
    /// No se pudo reservar una grilla acotada.
    AllocationFailed,
    /// El intervalo tiene un límite no finito o no es representable finitamente.
    InvalidInterval,
    /// Un evaluador externo devolvió una cantidad de valores distinta de la pedida.
    EvaluatorOutputLength { expected: usize, actual: usize },
    /// El integrando produjo un valor no finito en la grilla de validación.
    NonFiniteIntegrand { sample: usize },
    /// Las cuadraturas gruesa y refinada no coincidieron dentro de la tolerancia.
    NotConverged { samples: usize },
}

impl fmt::Display for HybridIntegralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleLimitExceeded { requested, maximum } => {
                write!(f, "se solicitaron {requested} muestras, máximo {maximum}")
            }
            Self::AllocationFailed => write!(f, "no se pudo reservar memoria para la grilla"),
            Self::InvalidInterval => write!(f, "el intervalo de integración no es finito"),
            Self::EvaluatorOutputLength { expected, actual } => write!(
                f,
                "el evaluador devolvió {actual} valores, se esperaban {expected}"
            ),
            Self::NonFiniteIntegrand { sample } => {
                write!(f, "el integrando no es finito en la muestra {sample}")
            }
            Self::NotConverged { samples } => {
                write!(f, "la cuadratura no convergió con {samples} muestras")
            }
        }
    }
}

impl std::error::Error for HybridIntegralError {}

fn validate_request(a: f64, b: f64, samples: usize) -> Result<usize, HybridIntegralError> {
    let samples = samples.max(2);
    if samples > MAX_HYBRID_INTEGRAL_SAMPLES {
        return Err(HybridIntegralError::SampleLimitExceeded {
            requested: samples,
            maximum: MAX_HYBRID_INTEGRAL_SAMPLES,
        });
    }
    if !a.is_finite() || !b.is_finite() || !(b - a).is_finite() {
        return Err(HybridIntegralError::InvalidInterval);
    }
    Ok(samples)
}

fn try_uniform_grid(
    a: f64,
    b: f64,
    samples: usize,
) -> Result<(Vec<f64>, f64), HybridIntegralError> {
    let dx = (b - a) / (samples - 1) as f64;
    if dx == 0.0 {
        return Err(HybridIntegralError::NotConverged { samples });
    }
    let mut xs = Vec::new();
    xs.try_reserve_exact(samples)
        .map_err(|_| HybridIntegralError::AllocationFailed)?;
    for i in 0..samples {
        let x = if i + 1 == samples {
            b
        } else {
            a + i as f64 * dx
        };
        if !x.is_finite() {
            return Err(HybridIntegralError::InvalidInterval);
        }
        xs.push(x);
    }
    Ok((xs, dx))
}

fn try_midpoint_grid(xs: &[f64], samples: usize) -> Result<Vec<f64>, HybridIntegralError> {
    let mut midpoints = Vec::new();
    midpoints
        .try_reserve_exact(xs.len() - 1)
        .map_err(|_| HybridIntegralError::AllocationFailed)?;
    for pair in xs.windows(2) {
        let midpoint = pair[0] + (pair[1] - pair[0]) * 0.5;
        if !midpoint.is_finite() || midpoint == pair[0] || midpoint == pair[1] {
            return Err(HybridIntegralError::NotConverged { samples });
        }
        midpoints.push(midpoint);
    }
    Ok(midpoints)
}

fn validate_values(
    values: &[f64],
    expected: usize,
    midpoint_values: bool,
) -> Result<(), HybridIntegralError> {
    if values.len() != expected {
        return Err(HybridIntegralError::EvaluatorOutputLength {
            expected,
            actual: values.len(),
        });
    }
    if let Some(index) = values.iter().position(|value| !value.is_finite()) {
        return Err(HybridIntegralError::NonFiniteIntegrand {
            sample: if midpoint_values {
                2 * index + 1
            } else {
                2 * index
            },
        });
    }
    Ok(())
}

fn refined_simpson(ys: &[f64], midpoint_ys: &[f64], dx: f64) -> (f64, f64) {
    let max_value = ys
        .iter()
        .chain(midpoint_ys)
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if max_value == 0.0 {
        return (0.0, 0.0);
    }

    let mut signed_sum = ys[0] / max_value + ys[ys.len() - 1] / max_value;
    let mut absolute_sum = ys[0].abs() / max_value + ys[ys.len() - 1].abs() / max_value;
    for value in &ys[1..ys.len() - 1] {
        signed_sum += 2.0 * (*value / max_value);
        absolute_sum += 2.0 * (value.abs() / max_value);
    }
    for value in midpoint_ys {
        signed_sum += 4.0 * (*value / max_value);
        absolute_sum += 4.0 * (value.abs() / max_value);
    }

    let factor = (dx / 6.0) * max_value;
    (signed_sum * factor, absolute_sum * factor.abs())
}

fn finish_hybrid_integral(
    ys: &[f64],
    midpoint_ys: &[f64],
    dx: f64,
    samples: usize,
) -> Result<f64, HybridIntegralError> {
    let coarse = composite_simpson(ys, dx);
    let (refined, absolute_integral) = refined_simpson(ys, midpoint_ys, dx);
    if !coarse.is_finite() || !refined.is_finite() || !absolute_integral.is_finite() {
        return Err(HybridIntegralError::NotConverged { samples });
    }

    let error = (refined - coarse).abs();
    let scale = absolute_integral.max(coarse.abs()).max(refined.abs());
    if error > HYBRID_RELATIVE_TOLERANCE * scale {
        return Err(HybridIntegralError::NotConverged { samples });
    }
    Ok(refined)
}

/// Aplica la regla de Simpson compuesta a una serie de muestras uniformes.
///
/// `dx` es el espaciamiento entre muestras consecutivas. Si la cantidad de
/// intervalos es impar, se aplica Simpson a los primeros intervalos pares y se
/// cierra el último intervalo con la regla del trapecio.
pub fn composite_simpson(ys: &[f64], dx: f64) -> f64 {
    let n = ys.len();
    if n < 2 {
        return 0.0;
    }
    if n == 2 {
        return (ys[0] + ys[1]) * 0.5 * dx;
    }

    let intervals = n - 1;
    if intervals.is_multiple_of(2) {
        let mut sum = ys[0] + ys[n - 1];
        for i in (1..n - 1).step_by(2) {
            sum += 4.0 * ys[i];
        }
        for i in (2..n - 1).step_by(2) {
            sum += 2.0 * ys[i];
        }
        sum * dx / 3.0
    } else {
        // Simpson sobre los primeros intervalos-1 (cantidad par) y trapecio
        // en el intervalo final.
        let mut sum = ys[0] + ys[n - 2];
        for i in (1..n - 2).step_by(2) {
            sum += 4.0 * ys[i];
        }
        for i in (2..n - 2).step_by(2) {
            sum += 2.0 * ys[i];
        }
        let simpson_part = sum * dx / 3.0;
        let trapezoid_part = (ys[n - 2] + ys[n - 1]) * 0.5 * dx;
        simpson_part + trapezoid_part
    }
}

/// Calcula ∫ₐᵇ f(x) dx evaluando `f` en una grilla uniforme en paralelo (CPU)
/// y reduciendo con Simpson compuesto.
///
/// `samples` es la cantidad de puntos de evaluación (por defecto al menos 2).
/// Esta API de compatibilidad devuelve `NaN` ante cualquier error; use
/// [`try_eval_integral_hybrid`] para obtener el error estructurado.
pub fn eval_integral_hybrid<F>(f: F, a: f64, b: f64, samples: usize) -> f64
where
    F: Fn(f64) -> f64 + Sync,
{
    try_eval_integral_hybrid(f, a, b, samples).unwrap_or(f64::NAN)
}

/// Variante falible de [`eval_integral_hybrid`] con presupuesto explícito.
pub fn try_eval_integral_hybrid<F>(
    f: F,
    a: f64,
    b: f64,
    samples: usize,
) -> Result<f64, HybridIntegralError>
where
    F: Fn(f64) -> f64 + Sync,
{
    let samples = validate_request(a, b, samples)?;
    if a == b {
        return Ok(0.0);
    }
    let (xs, dx) = try_uniform_grid(a, b, samples)?;
    let mut ys = Vec::new();
    ys.try_reserve_exact(xs.len())
        .map_err(|_| HybridIntegralError::AllocationFailed)?;
    xs.par_iter().map(|&x| f(x)).collect_into_vec(&mut ys);
    validate_values(&ys, xs.len(), false)?;

    let midpoints = try_midpoint_grid(&xs, samples)?;
    let mut midpoint_ys = Vec::new();
    midpoint_ys
        .try_reserve_exact(midpoints.len())
        .map_err(|_| HybridIntegralError::AllocationFailed)?;
    midpoints
        .par_iter()
        .map(|&x| f(x))
        .collect_into_vec(&mut midpoint_ys);
    validate_values(&midpoint_ys, midpoints.len(), true)?;
    finish_hybrid_integral(&ys, &midpoint_ys, dx, samples)
}

/// Variante de [`eval_integral_hybrid`] que permite usar un evaluador externo
/// (por ejemplo, un pipeline GPU) para obtener `f(x)` en los puntos de la
/// grilla. El evaluador debe devolver exactamente un valor finito por punto y
/// se invoca también para los puntos medios usados por la validación. Devuelve
/// `NaN` ante errores; use [`try_eval_integral_hybrid_with_evaluator`] para
/// obtener el error estructurado.
pub fn eval_integral_hybrid_with_evaluator<G>(a: f64, b: f64, samples: usize, evaluator: G) -> f64
where
    G: Fn(&[f64]) -> Vec<f64>,
{
    try_eval_integral_hybrid_with_evaluator(a, b, samples, evaluator).unwrap_or(f64::NAN)
}

/// Variante falible de [`eval_integral_hybrid_with_evaluator`] con presupuesto explícito.
pub fn try_eval_integral_hybrid_with_evaluator<G>(
    a: f64,
    b: f64,
    samples: usize,
    evaluator: G,
) -> Result<f64, HybridIntegralError>
where
    G: Fn(&[f64]) -> Vec<f64>,
{
    let samples = validate_request(a, b, samples)?;
    if a == b {
        return Ok(0.0);
    }
    let (xs, dx) = try_uniform_grid(a, b, samples)?;
    let ys = evaluator(&xs);
    validate_values(&ys, xs.len(), false)?;

    let midpoints = try_midpoint_grid(&xs, samples)?;
    let midpoint_ys = evaluator(&midpoints);
    validate_values(&midpoint_ys, midpoints.len(), true)?;
    finish_hybrid_integral(&ys, &midpoint_ys, dx, samples)
}

// ---------------------------------------------------------------------------
// Frente G-A: Risch-Norman (polinomios, exponenciales, logaritmos).
//
// Subconjunto S/M de Risch: potencias `x^n`, `1/x → ln|x|`, linealidad,
// `exp(a·x+b)`, `ln(x)`/`log(x)`, `x·exp(a·x+b)` por partes y trig lineal
// heredada del motor existente. El resto devuelve `Err` honesto con la
// derivación a `symbolic::integrate` (Hermite/Rothstein) o cuadratura.
// Referencia GeoGebra: `Integral` simbólica.
//
// Presupuestos: entrada ≤ 2000 bytes, profundidad ≤ 32, términos ≤ 64.
// ---------------------------------------------------------------------------

/// Máximo de bytes del integrando (igual que `MAX_EXPR_LENGTH` 2000).
pub const MAX_RISCH_INPUT_BYTES: usize = 2000;
/// Profundidad máxima de recursión del integrador.
pub const MAX_RISCH_DEPTH: u32 = 32;
/// Máximo de términos visitados por integración.
pub const MAX_RISCH_TERMS: usize = 64;

/// Error honesto del integrador Risch-Norman.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RischError {
    /// Entrada vacía o mayor a 2000 bytes.
    InputTooLong { provided: usize, maximum: usize },
    /// Variable no es identificador válido.
    InvalidVariable { variable: String },
    /// No parsea con el AST de Grafito.
    Parse { reason: String },
    /// Fuera del subconjunto S/M; Risch completo es L (Tasks.md F10.W5).
    Unsupported { hint: String },
    /// Profundidad o términos excedidos.
    ResourceLimit { detail: String },
    /// Intervalo no finito o con polo en los extremos.
    BadInterval { detail: String },
}

impl std::fmt::Display for RischError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLong { provided, maximum } => {
                write!(
                    f,
                    "integrando de {provided} bytes excede el máximo {maximum}"
                )
            }
            Self::InvalidVariable { variable } => {
                write!(f, "variable '{variable}' no es un identificador válido")
            }
            Self::Parse { reason } => write!(f, "no se pudo parsear el integrando: {reason}"),
            Self::Unsupported { hint } => write!(f, "Risch-Norman no cubre este caso: {hint}"),
            Self::ResourceLimit { detail } => write!(f, "presupuesto agotado: {detail}"),
            Self::BadInterval { detail } => write!(f, "intervalo inválido: {detail}"),
        }
    }
}

impl std::error::Error for RischError {}

fn validate_risch_input(expr: &str, var: &str) -> Result<(String, String), RischError> {
    if expr.is_empty() || expr.len() > MAX_RISCH_INPUT_BYTES {
        return Err(RischError::InputTooLong {
            provided: expr.len(),
            maximum: MAX_RISCH_INPUT_BYTES,
        });
    }
    let mut chars = var.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(RischError::InvalidVariable {
            variable: var.to_string(),
        });
    }
    Ok((expr.replace(' ', ""), var.to_string()))
}

/// Antiderivada simbólica del subconjunto Risch-Norman.
///
/// Devuelve la primitiva como string (`to_expr_string`). Casos con `Err`
/// honesto: `exp(x^2)`, `sin(x)/x`, racionales propios (derivan a
/// Hermite/Rothstein en `symbolic::integrate`).
pub fn risch_norman_integrate(expr: &str, var: &str) -> Result<String, RischError> {
    let (clean, var) = validate_risch_input(expr, var)?;
    let ast = crate::ast::parse_ast(&clean).map_err(|reason| RischError::Parse { reason })?;
    let mut terms = 0_usize;
    let prim = risch_expr(&ast, &var, 0, &mut terms)?;
    Ok(prim.to_expr_string())
}

/// Definida por FTC sobre la primitiva Risch-Norman.
///
/// Extremos no finitos o primitiva no finita en ellos → `Err` honesto
/// (posible polo interior; usa la cuadratura híbrida).
pub fn risch_norman_definite(expr: &str, var: &str, a: f64, b: f64) -> Result<f64, RischError> {
    if !a.is_finite() || !b.is_finite() {
        return Err(RischError::BadInterval {
            detail: "los extremos deben ser finitos".to_string(),
        });
    }
    if a == b {
        return Ok(0.0);
    }
    let (clean, var) = validate_risch_input(expr, var)?;
    let ast = crate::ast::parse_ast(&clean).map_err(|reason| RischError::Parse { reason })?;
    let mut terms = 0_usize;
    let prim = risch_expr(&ast, &var, 0, &mut terms)?;
    let (fa, fb) = (prim.eval_at(&var, a), prim.eval_at(&var, b));
    if !fa.is_finite() || !fb.is_finite() {
        return Err(RischError::BadInterval {
            detail: "la primitiva no es finita en los extremos (posible polo interior)".to_string(),
        });
    }
    let value = fb - fa;
    if !value.is_finite() {
        return Err(RischError::BadInterval {
            detail: "la diferencia FTC no es finita".to_string(),
        });
    }
    Ok(value)
}

fn risch_unsupported(hint: String) -> RischError {
    RischError::Unsupported { hint }
}

/// Núcleo Risch-Norman sobre AST (sin pasar por strings).
///
/// `pub(crate)` para que `ode` integre el factor `μ·q` sin reparsear
/// literales negativos (el parser produce `Neg(Const)` y el motor
/// `symbolic` solo acepta linealidad con lado `Const`).
pub(crate) fn risch_ast(e: &crate::ast::Expr, var: &str) -> Result<crate::ast::Expr, RischError> {
    let mut terms = 0_usize;
    risch_expr(e, var, 0, &mut terms)
}

fn risch_expr(
    e: &crate::ast::Expr,
    var: &str,
    depth: u32,
    terms: &mut usize,
) -> Result<crate::ast::Expr, RischError> {
    use crate::ast::Expr;
    if depth > MAX_RISCH_DEPTH {
        return Err(RischError::ResourceLimit {
            detail: format!("profundidad {depth} excede {MAX_RISCH_DEPTH}"),
        });
    }
    *terms += 1;
    if *terms > MAX_RISCH_TERMS {
        return Err(RischError::ResourceLimit {
            detail: format!("más de {MAX_RISCH_TERMS} términos"),
        });
    }
    let var_expr = Expr::Var(var.to_string());
    let mut rec = |x: &Expr| risch_expr(x, var, depth + 1, terms);

    if !crate::cas::cas_contains_var(e, var) {
        return Ok(Expr::Mul(Box::new(e.clone()), Box::new(var_expr)));
    }
    match e {
        Expr::Const(_) => Ok(Expr::Mul(Box::new(e.clone()), Box::new(var_expr))),
        Expr::Var(name) if name == var => Ok(Expr::Mul(
            Box::new(Expr::Pow(Box::new(var_expr.clone()), Box::new(Expr::Const(2.0)))),
            Box::new(Expr::Const(0.5)),
        )),
        Expr::Neg(a) => Ok(Expr::Neg(Box::new(rec(a)?))),
        Expr::Add(a, b) => Ok(Expr::Add(Box::new(rec(a)?), Box::new(rec(b)?))),
        Expr::Sub(a, b) => Ok(Expr::Sub(Box::new(rec(a)?), Box::new(rec(b)?))),
        Expr::Mul(a, b) => {
            if !crate::cas::cas_contains_var(a, var) {
                return Ok(Expr::Mul(a.clone(), Box::new(rec(b)?)));
            }
            if !crate::cas::cas_contains_var(b, var) {
                return Ok(Expr::Mul(Box::new(rec(a)?), b.clone()));
            }
            if let Some(parts) = risch_parts_x_exp(a, b, var, depth, terms)? {
                return Ok(parts);
            }
            Err(risch_unsupported(
                "producto de dos funciones de x (fuera de x·exp(a·x+b)); Risch completo pendiente en Tasks.md F10.W5".to_string(),
            ))
        }
        Expr::Pow(base, exp) => {
            if let Expr::Var(name) = base.as_ref() {
                if name == var {
                    if let Some(n) = crate::cas::cas_const_value(exp) {
                        if (n + 1.0).abs() < 1e-12 {
                            return Ok(Expr::Ln(Box::new(Expr::Abs(Box::new(var_expr)))));
                        }
                        let next = n + 1.0;
                        if !next.is_finite() || next == 0.0 {
                            return Err(risch_unsupported(
                                "exponente degenerado en potencia".to_string(),
                            ));
                        }
                        return Ok(Expr::Mul(
                            Box::new(Expr::Const(1.0 / next)),
                            Box::new(Expr::Pow(
                                Box::new(var_expr),
                                Box::new(Expr::Const(next)),
                            )),
                        ));
                    }
                }
            }
            Err(risch_unsupported(
                "potencia no monomial (p. ej. x^x o (f(x))^g(x)); Risch completo pendiente".to_string(),
            ))
        }
        Expr::Div(num, den) => {
            if let Expr::Var(name) = den.as_ref() {
                if name == var && !crate::cas::cas_contains_var(num, var) {
                    return Ok(Expr::Mul(
                        num.clone(),
                        Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(var_expr))))),
                    ));
                }
            }
            Err(risch_unsupported(
                "cociente no trivial (racional propio → Hermite/Rothstein en symbolic::integrate; resto Risch completo pendiente)".to_string(),
            ))
        }
        Expr::Exp(arg) => {
            let (a, _) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported(
                    "exp(f(x)) con f no lineal (p. ej. exp(x^2)); Risch completo pendiente"
                        .to_string(),
                )
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("exp(constante) degenerada".to_string()));
            }
            if (a - 1.0).abs() < 1e-12 {
                Ok(Expr::Exp(arg.clone()))
            } else {
                Ok(Expr::Mul(
                    Box::new(Expr::Const(1.0 / a)),
                    Box::new(Expr::Exp(arg.clone())),
                ))
            }
        }
        Expr::Ln(arg) => {
            if matches!(arg.as_ref(), Expr::Var(name) if name == var) {
                return Ok(Expr::Sub(
                    Box::new(Expr::Mul(
                        Box::new(var_expr.clone()),
                        Box::new(Expr::Ln(Box::new(var_expr.clone()))),
                    )),
                    Box::new(var_expr),
                ));
            }
            Err(risch_unsupported(
                "ln(f(x)) con f no trivial; Risch completo pendiente".to_string(),
            ))
        }
        Expr::Log(arg) => {
            if matches!(arg.as_ref(), Expr::Var(name) if name == var) {
                return Ok(Expr::Sub(
                    Box::new(Expr::Mul(
                        Box::new(var_expr.clone()),
                        Box::new(Expr::Log(Box::new(var_expr.clone()))),
                    )),
                    Box::new(Expr::Div(
                        Box::new(var_expr),
                        Box::new(Expr::Const(std::f64::consts::LN_10)),
                    )),
                ));
            }
            Err(risch_unsupported(
                "log(f(x)) con f no trivial; Risch completo pendiente".to_string(),
            ))
        }
        Expr::Sin(arg) => {
            let (a, _) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported("sin(f(x)) con f no lineal".to_string())
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("sin(constante) degenerado".to_string()));
            }
            Ok(Expr::Mul(
                Box::new(Expr::Const(-1.0 / a)),
                Box::new(Expr::Cos(arg.clone())),
            ))
        }
        Expr::Cos(arg) => {
            let (a, _) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported("cos(f(x)) con f no lineal".to_string())
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("cos(constante) degenerado".to_string()));
            }
            Ok(Expr::Mul(
                Box::new(Expr::Const(1.0 / a)),
                Box::new(Expr::Sin(arg.clone())),
            ))
        }
        _ => Err(risch_unsupported(format!(
            "nodo {} fuera del subconjunto polinomios/exponenciales/logaritmos; usa symbolic::integrate o cuadratura",
            e.to_expr_string()
        ))),
    }
}

/// Partes para `x·exp(a·x+b)`: `e^{ax+b}·(a·x−1)/a²`.
fn risch_parts_x_exp(
    a: &crate::ast::Expr,
    b: &crate::ast::Expr,
    var: &str,
    depth: u32,
    terms: &mut usize,
) -> Result<Option<crate::ast::Expr>, RischError> {
    use crate::ast::Expr;
    if depth > MAX_RISCH_DEPTH {
        return Err(RischError::ResourceLimit {
            detail: "profundidad en partes".to_string(),
        });
    }
    let _ = terms;
    for (poly_side, exp_side) in [(a, b), (b, a)] {
        if !matches!(exp_side, Expr::Exp(_)) {
            continue;
        }
        let (pa, pb) = match crate::cas::cas_linear_coeff(poly_side, var) {
            Some(v) => v,
            None => continue,
        };
        if pa.abs() < 1e-12 {
            continue;
        }
        if let Expr::Exp(arg) = exp_side {
            let (ea, _) = match crate::cas::cas_linear_coeff(arg, var) {
                Some(v) => v,
                None => continue,
            };
            if ea.abs() < 1e-12 {
                continue;
            }
            // ∫(pa·x+pb)·e^{ea·x+eb} = e^{ea·x+eb}·(pa·(ea·x−1)/ea² + pb/ea)
            let x = Expr::Var(var.to_string());
            let eax_minus_1 = Expr::Sub(
                Box::new(Expr::Mul(Box::new(Expr::Const(ea)), Box::new(x.clone()))),
                Box::new(Expr::Const(1.0)),
            );
            let bracket = Expr::Add(
                Box::new(Expr::Mul(
                    Box::new(Expr::Const(pa / (ea * ea))),
                    Box::new(eax_minus_1),
                )),
                Box::new(Expr::Const(pb / ea)),
            );
            return Ok(Some(Expr::Mul(
                Box::new(Expr::Exp(arg.clone())),
                Box::new(bracket),
            )));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integral_hybrid_polynomial() {
        // ∫₀¹ x² dx = 1/3
        let result = eval_integral_hybrid(|x| x * x, 0.0, 1.0, 1024);
        assert!((result - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_integral_hybrid_sine() {
        // ∫₀^π sin(x) dx = 2
        let result = eval_integral_hybrid(|x| x.sin(), 0.0, std::f64::consts::PI, 2048);
        assert!((result - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_integral_hybrid_exponential() {
        // ∫₀¹ e^x dx = e - 1
        let result = eval_integral_hybrid(|x| x.exp(), 0.0, 1.0, 1024);
        assert!((result - (std::f64::consts::E - 1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_composite_simpson_even_intervals() {
        // ∫₀¹ x³ dx = 1/4; Simpson es exacto para cúbicos.
        let ys: Vec<f64> = (0..=100).map(|i| (i as f64 / 100.0).powi(3)).collect();
        let dx = 1.0 / 100.0;
        let result = composite_simpson(&ys, dx);
        assert!((result - 0.25).abs() < 1e-12);
    }

    #[test]
    fn hybrid_integrators_reject_unbounded_samples_before_allocating() {
        let expected = Err(HybridIntegralError::SampleLimitExceeded {
            requested: usize::MAX,
            maximum: MAX_HYBRID_INTEGRAL_SAMPLES,
        });

        assert_eq!(
            try_eval_integral_hybrid(|x| x, 0.0, 1.0, usize::MAX),
            expected
        );
        assert_eq!(
            try_eval_integral_hybrid_with_evaluator(0.0, 1.0, usize::MAX, |_| vec![]),
            expected
        );
        assert!(eval_integral_hybrid(|x| x, 0.0, 1.0, usize::MAX).is_nan());
    }

    #[test]
    fn hardening_hybrid_integral_preserves_tiny_nonzero_span() {
        let result = try_eval_integral_hybrid(|_| 1e16, 0.0, 1e-16, 101)
            .expect("finite constant should integrate");

        assert!((result - 1.0).abs() < 1e-12);
    }

    #[test]
    fn hardening_hybrid_integrators_preserve_normal_results() {
        let cpu = try_eval_integral_hybrid(|x| x * x, 0.0, 1.0, 101)
            .expect("polynomial should integrate");
        let external = try_eval_integral_hybrid_with_evaluator(0.0, 1.0, 101, |xs| {
            xs.iter().map(|x| x * x).collect()
        })
        .expect("matching evaluator output should integrate");

        assert!((cpu - 1.0 / 3.0).abs() < 1e-12);
        assert!((external - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn hardening_hybrid_evaluator_rejects_empty_short_and_long_outputs() {
        for actual in [0, 100, 101, 102] {
            let result =
                try_eval_integral_hybrid_with_evaluator(0.0, 1.0, 101, |_| vec![1.0; actual]);

            assert!(
                matches!(
                    result,
                    Err(HybridIntegralError::EvaluatorOutputLength { .. })
                ),
                "accepted evaluator output length {actual}"
            );
        }
    }

    #[test]
    fn hardening_hybrid_integrators_reject_nonfinite_samples() {
        assert_eq!(
            try_eval_integral_hybrid(|_| f64::NAN, 0.0, 1.0, 101),
            Err(HybridIntegralError::NonFiniteIntegrand { sample: 0 })
        );
        assert_eq!(
            try_eval_integral_hybrid_with_evaluator(0.0, 1.0, 101, |xs| {
                vec![f64::INFINITY; xs.len()]
            }),
            Err(HybridIntegralError::NonFiniteIntegrand { sample: 0 })
        );
    }

    #[test]
    fn hardening_hybrid_integrator_rejects_nonfinite_bounds() {
        assert_eq!(
            try_eval_integral_hybrid(|x| x, f64::NAN, 1.0, 101),
            Err(HybridIntegralError::InvalidInterval)
        );
        assert_eq!(
            try_eval_integral_hybrid_with_evaluator(0.0, f64::INFINITY, 101, |xs| {
                vec![1.0; xs.len()]
            }),
            Err(HybridIntegralError::InvalidInterval)
        );
    }

    #[test]
    fn hardening_hybrid_integrator_rejects_finite_quadrature_across_interior_pole() {
        let result = try_eval_integral_hybrid(|x| 1.0 / (x * x), -1.0, 1.0, 1_000);

        assert!(matches!(
            result,
            Err(HybridIntegralError::NonFiniteIntegrand { .. })
                | Err(HybridIntegralError::NotConverged { .. })
        ));
    }

    #[test]
    fn hardening_hybrid_integrator_reports_nonconvergence_for_off_grid_pole() {
        let result =
            try_eval_integral_hybrid(|x| 1.0 / (x - 0.123_456_789).powi(2), -1.0, 1.0, 1_000);

        assert_eq!(
            result,
            Err(HybridIntegralError::NotConverged { samples: 1_000 })
        );
    }

    // --- Frente G-A: Risch-Norman ---

    fn eval_prim_contains(prim: &str, var: &str, at: f64, expected: f64) -> bool {
        let ast = crate::ast::parse_ast(&prim.replace(' ', "")).unwrap();
        (ast.eval_at(var, at) - expected).abs() < 1e-9
    }

    #[test]
    fn risch_polynomial_power_rule() {
        let prim = risch_norman_integrate("x^2", "x").expect("potencia");
        assert!(eval_prim_contains(&prim, "x", 2.0, 8.0 / 3.0), "got {prim}");
        let prim2 = risch_norman_integrate("3*x^2 + 2*x + 1", "x").expect("polinomio");
        assert!(eval_prim_contains(&prim2, "x", 1.0, 3.0), "got {prim2}");
    }

    #[test]
    fn risch_inverse_gives_log() {
        let prim = risch_norman_integrate("1/x", "x").expect("1/x");
        assert!(prim.contains("ln"), "got {prim}");
        assert!(
            eval_prim_contains(&prim, "x", 1.0_f64.exp(), 1.0),
            "got {prim}"
        );
    }

    #[test]
    fn risch_exponential_linear_arg() {
        let prim = risch_norman_integrate("exp(2*x)", "x").expect("exp lineal");
        assert!(
            eval_prim_contains(&prim, "x", 1.0, 2.0_f64.exp() / 2.0),
            "got {prim}"
        );
        let prim2 = risch_norman_integrate("exp(x)", "x").expect("exp");
        assert!(eval_prim_contains(&prim2, "x", 0.0, 1.0), "got {prim2}");
    }

    #[test]
    fn risch_logarithm() {
        let prim = risch_norman_integrate("ln(x)", "x").expect("ln");
        assert!(eval_prim_contains(&prim, "x", 1.0, -1.0), "got {prim}");
    }

    #[test]
    fn risch_x_times_exp_by_parts() {
        let prim = risch_norman_integrate("x*exp(x)", "x").expect("partes");
        assert!(eval_prim_contains(&prim, "x", 0.0, -1.0), "got {prim}");
        assert!(eval_prim_contains(&prim, "x", 1.0, 0.0), "got {prim}");
    }

    #[test]
    fn risch_definite_via_ftc() {
        let v = risch_norman_definite("x^2", "x", 0.0, 1.0).expect("FTC");
        assert!((v - 1.0 / 3.0).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn risch_honest_err_beyond_subset() {
        for expr in ["exp(x^2)", "sin(x)/x", "x^x", "1/(x^2+1)"] {
            let err = risch_norman_integrate(expr, "x").expect_err("fuera de S/M");
            assert!(
                matches!(err, RischError::Unsupported { .. }),
                "{expr}: got {err}"
            );
        }
    }

    #[test]
    fn risch_rejects_bad_input() {
        assert!(matches!(
            risch_norman_integrate(&"x".repeat(2001), "x"),
            Err(RischError::InputTooLong { .. })
        ));
        assert!(matches!(
            risch_norman_integrate("x", "2y"),
            Err(RischError::InvalidVariable { .. })
        ));
        assert!(matches!(
            risch_norman_definite("1/x", "x", 0.0, 1.0),
            Err(RischError::BadInterval { .. })
        ));
    }
}
