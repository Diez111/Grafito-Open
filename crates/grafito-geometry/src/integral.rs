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
// Frente F3c: trigonométricas (tan, sec²), racionales por fracciones
// parciales sobre lineales reales (grado ≤ 4), `1/(x²+1)`-style (arctan +
// log de cuadrática irreducible), potencias `x^n` racionales (`Sqrt`/`Cbrt`
// y exponente fraccionario). El resto devuelve `Err` honesto con la
// derivación a `symbolic::integrate` (Hermite/Rothstein) o cuadratura.
// Referencia GeoGebra: `Integral` simbólica.
//
// Presupuestos: entrada ≤ 2000 bytes, profundidad ≤ 32, términos ≤ 64,
// grado parcial ≤ 4 (`MAX_RISCH_PARTIAL_DEGREE`, muy por debajo de
// `MAX_BUCHBERGER_DEGREE` 64), raíces candidatas ≤ 32.
// ---------------------------------------------------------------------------

/// Máximo de bytes del integrando (igual que `MAX_EXPR_LENGTH` 2000).
pub const MAX_RISCH_INPUT_BYTES: usize = 2000;
/// Profundidad máxima de recursión del integrador.
pub const MAX_RISCH_DEPTH: u32 = 32;
/// Máximo de términos visitados por integración.
pub const MAX_RISCH_TERMS: usize = 64;
/// Grado máximo del denominador en fracciones parciales.
///
/// Acota la explosión combinatoria del `cover-up`/sistema lineal muy por
/// debajo de `MAX_BUCHBERGER_DEGREE` 64.
pub const MAX_RISCH_PARTIAL_DEGREE: usize = 4;
/// Máximo de raíces racionales candidatas probadas al factorizar.
pub const MAX_RISCH_ROOT_TRIALS: usize = 32;

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
            // F3c: `sec²(a·x+b)` como `Pow(Sec, 2)` → `tan/a`.
            if let Some(n) = crate::cas::cas_const_value(exp) {
                if (n - 2.0).abs() < 1e-12 {
                    if let Expr::Sec(u) = base.as_ref() {
                        let (a, _) = crate::cas::cas_linear_coeff(u, var).ok_or_else(|| {
                            risch_unsupported(
                                "sec²(f(x)) con f no lineal; Risch completo pendiente"
                                    .to_string(),
                            )
                        })?;
                        if a.abs() < 1e-12 {
                            return Err(risch_unsupported(
                                "sec²(constante) degenerada".to_string(),
                            ));
                        }
                        return Ok(Expr::Mul(
                            Box::new(Expr::Const(1.0 / a)),
                            Box::new(Expr::Tan(u.clone())),
                        ));
                    }
                }
                // F3c: `1/cos²(a·x+b)` = sec² → `tan/a`.
                if (n + 2.0).abs() < 1e-12 {
                    if let Expr::Cos(u) = base.as_ref() {
                        let (a, _) = crate::cas::cas_linear_coeff(u, var).ok_or_else(|| {
                            risch_unsupported(
                                "cos(f(x))^-2 con f no lineal; Risch completo pendiente"
                                    .to_string(),
                            )
                        })?;
                        if a.abs() < 1e-12 {
                            return Err(risch_unsupported(
                                "cos(constante)^-2 degenerado".to_string(),
                            ));
                        }
                        return Ok(Expr::Mul(
                            Box::new(Expr::Const(1.0 / a)),
                            Box::new(Expr::Tan(u.clone())),
                        ));
                    }
                }
            }
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
                        // F3c: vale para `n` racional (`x^(1/2)`, `x^(-3/2)`…).
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
            // F3c: `(a·x+b)^n` con `n` entero ≥ 0 acotado → expansión binomial.
            if let Some(n) = crate::cas::cas_const_value(exp) {
                if n.is_finite() && n >= 0.0 && (n - n.round()).abs() < 1e-9 {
                    let ni = n.round() as usize;
                    if ni <= MAX_RISCH_PARTIAL_DEGREE * 2 {
                        if let Some((a, b)) = crate::cas::cas_linear_coeff(base, var) {
                            if a.is_finite() && b.is_finite() {
                                let poly = expand_linear_pow(a, b, ni, var);
                                return rec(&poly);
                            }
                        }
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
            // F3c: `K/cos²(a·x+b)` = `K·sec²` → `K·tan/a`.
            if let Expr::Pow(cos_base, cos_exp) = den.as_ref() {
                if let Expr::Cos(u) = cos_base.as_ref() {
                    if let Some(n) = crate::cas::cas_const_value(cos_exp) {
                        if (n - 2.0).abs() < 1e-12 {
                            if let Some(k) = crate::cas::cas_const_value(num) {
                                let (a, _) =
                                    crate::cas::cas_linear_coeff(u, var).ok_or_else(|| {
                                        risch_unsupported(
                                            "sec²(f(x)) con f no lineal; Risch completo pendiente"
                                                .to_string(),
                                        )
                                    })?;
                                if a.abs() < 1e-12 {
                                    return Err(risch_unsupported(
                                        "sec²(constante) degenerada".to_string(),
                                    ));
                                }
                                return Ok(Expr::Mul(
                                    Box::new(Expr::Const(k / a)),
                                    Box::new(Expr::Tan(u.clone())),
                                ));
                            }
                        }
                    }
                }
            }
            // F3c: racionales `P(x)/Q(x)` por fracciones parciales (grado ≤ 4);
            // resto → `Err` honesto con la factorización parcial lograda.
            match risch_rational(num, den, var, depth, terms) {
                Ok(prim) => Ok(prim),
                Err(RischError::Unsupported { hint }) => Err(risch_unsupported(format!(
                    "cociente no trivial ({hint}); racional propio general → Hermite/Rothstein en symbolic::integrate, resto Risch completo pendiente"
                ))),
                Err(other) => Err(other),
            }
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
        // F3c: `∫tan(a·x+b) dx = −ln|cos(a·x+b)|/a`.
        Expr::Tan(arg) => {
            let (a, _) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported("tan(f(x)) con f no lineal; Risch completo pendiente".to_string())
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("tan(constante) degenerado".to_string()));
            }
            Ok(Expr::Mul(
                Box::new(Expr::Const(-1.0 / a)),
                Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Expr::Cos(
                    arg.clone(),
                )))))),
            ))
        }
        // F3c: `∫sqrt(a·x+b) dx = 2·(a·x+b)^(3/2)/(3·a)`.
        Expr::Sqrt(arg) => {
            let (a, b) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported("sqrt(f(x)) con f no lineal; Risch completo pendiente".to_string())
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("sqrt(constante) degenerada".to_string()));
            }
            Ok(Expr::Mul(
                Box::new(Expr::Const(2.0 / (3.0 * a))),
                Box::new(Expr::Pow(
                    Box::new(Expr::Add(
                        Box::new(Expr::Mul(
                            Box::new(Expr::Const(a)),
                            Box::new(var_expr),
                        )),
                        Box::new(Expr::Const(b)),
                    )),
                    Box::new(Expr::Const(1.5)),
                )),
            ))
        }
        // F3c: `∫cbrt(a·x+b) dx = 3·(a·x+b)^(4/3)/(4·a)`.
        Expr::Cbrt(arg) => {
            let (a, b) = crate::cas::cas_linear_coeff(arg, var).ok_or_else(|| {
                risch_unsupported("cbrt(f(x)) con f no lineal; Risch completo pendiente".to_string())
            })?;
            if a.abs() < 1e-12 {
                return Err(risch_unsupported("cbrt(constante) degenerada".to_string()));
            }
            Ok(Expr::Mul(
                Box::new(Expr::Const(3.0 / (4.0 * a))),
                Box::new(Expr::Pow(
                    Box::new(Expr::Add(
                        Box::new(Expr::Mul(
                            Box::new(Expr::Const(a)),
                            Box::new(var_expr),
                        )),
                        Box::new(Expr::Const(b)),
                    )),
                    Box::new(Expr::Const(4.0 / 3.0)),
                )),
            ))
        }
        _ => Err(risch_unsupported(format!(
            "nodo {} fuera del subconjunto F3c (polinomios, x^(p/q), exp/log, sin/cos/tan/sec², racionales con lineales reales, 1/(x²+1)-style); usa symbolic::integrate o cuadratura",
            e.to_expr_string()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Frente F3c: polinomios y fracciones parciales (grado acotado).
// ---------------------------------------------------------------------------

/// Tolerancia de cero para coeficientes polinómicos.
const POLY_EPS: f64 = 1e-12;

/// Coeficientes ascendentes de `e` como polinomio en `var` (`c[0] + c[1]·x + …`).
///
/// `None` si no es polinómico (transcendentes, otras variables, cocientes no
/// constantes) o si el grado excede `max_deg`. `pub(crate)` para reutilizar
/// en `ode` (clasificación del RHS de 2º orden) sin reparsear.
pub(crate) fn poly_coeffs_bounded(
    e: &crate::ast::Expr,
    var: &str,
    max_deg: usize,
) -> Option<Vec<f64>> {
    use crate::ast::Expr;
    let bounded = |mut c: Vec<f64>| -> Option<Vec<f64>> {
        while c.len() > 1 && c.last().is_some_and(|v| v.abs() < POLY_EPS) {
            c.pop();
        }
        if c.len() - 1 > max_deg {
            None
        } else {
            Some(c)
        }
    };
    match e {
        Expr::Const(c) if c.is_finite() => bounded(vec![*c]),
        Expr::Var(name) if name == var => bounded(vec![0.0, 1.0]),
        Expr::Var(_) => None,
        Expr::Neg(a) => {
            let mut c = poly_coeffs_bounded(a, var, max_deg)?;
            for v in &mut c {
                *v = -*v;
            }
            bounded(c)
        }
        Expr::Add(a, b) => {
            let (mut x, y) = (
                poly_coeffs_bounded(a, var, max_deg)?,
                poly_coeffs_bounded(b, var, max_deg)?,
            );
            if x.len() < y.len() {
                x.resize(y.len(), 0.0);
            }
            for (xi, yi) in x.iter_mut().zip(y.iter()) {
                *xi += yi;
                if !xi.is_finite() {
                    return None;
                }
            }
            bounded(x)
        }
        Expr::Sub(a, b) => {
            let (mut x, y) = (
                poly_coeffs_bounded(a, var, max_deg)?,
                poly_coeffs_bounded(b, var, max_deg)?,
            );
            if x.len() < y.len() {
                x.resize(y.len(), 0.0);
            }
            for (xi, yi) in x.iter_mut().zip(y.iter()) {
                *xi -= yi;
                if !xi.is_finite() {
                    return None;
                }
            }
            bounded(x)
        }
        Expr::Mul(a, b) => {
            let (x, y) = (
                poly_coeffs_bounded(a, var, max_deg)?,
                poly_coeffs_bounded(b, var, max_deg)?,
            );
            if x.len() + y.len() < 2 || x.len() - 1 + (y.len() - 1) > max_deg {
                return None;
            }
            let mut out = vec![0.0; x.len() + y.len() - 1];
            for (i, xi) in x.iter().enumerate() {
                for (j, yj) in y.iter().enumerate() {
                    let v = xi * yj;
                    if !v.is_finite() {
                        return None;
                    }
                    out[i + j] += v;
                }
            }
            if !out.iter().all(|v| v.is_finite()) {
                return None;
            }
            bounded(out)
        }
        Expr::Pow(base, exp) => {
            let n = crate::cas::cas_const_value(exp)?;
            if !n.is_finite() || n < 0.0 || (n - n.round()).abs() > 1e-9 {
                return None;
            }
            let ni = n.round() as usize;
            if ni > max_deg {
                return None;
            }
            let (a, b) = crate::cas::cas_linear_coeff(base, var)?;
            if !a.is_finite() || !b.is_finite() {
                return None;
            }
            // Expansión binomial de `(a·x+b)^n`.
            let mut out = vec![0.0; ni + 1];
            let mut binom = 1.0;
            for (k, slot) in out.iter_mut().enumerate() {
                if k > 0 {
                    binom = binom * (ni - k + 1) as f64 / k as f64;
                }
                let v = binom * a.powi(k as i32) * b.powi((ni - k) as i32);
                if !v.is_finite() {
                    return None;
                }
                *slot = v;
            }
            bounded(out)
        }
        _ => None,
    }
}

/// Expande `(a·x+b)^n` como AST suma de monomios (binomio, `n` acotado).
fn expand_linear_pow(a: f64, b: f64, n: usize, var: &str) -> crate::ast::Expr {
    use crate::ast::Expr;
    let mut acc = Expr::Const(b.powi(n as i32));
    let mut binom = 1.0;
    for k in 1..=n {
        binom = binom * (n - k + 1) as f64 / k as f64;
        let coeff = binom * a.powi(k as i32) * b.powi((n - k) as i32);
        if coeff.abs() < POLY_EPS {
            continue;
        }
        let term = Expr::Mul(
            Box::new(Expr::Const(coeff)),
            Box::new(Expr::Pow(
                Box::new(Expr::Var(var.to_string())),
                Box::new(Expr::Const(k as f64)),
            )),
        );
        acc = Expr::Add(Box::new(acc), Box::new(term));
    }
    acc
}

/// Evalúa un polinomio ascendente en `x`.
fn eval_poly_asc(c: &[f64], x: f64) -> f64 {
    let mut acc = 0.0;
    for (i, ci) in c.iter().enumerate() {
        acc += ci * x.powi(i as i32);
    }
    acc
}

/// Divide `num / den` (ascendentes, `den` no nulo): `(cociente, resto)`.
fn poly_div_asc(num: &[f64], den: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut rem = num.to_vec();
    while rem.len() > 1 && rem.last().is_some_and(|v| v.abs() < POLY_EPS) {
        rem.pop();
    }
    let mut dn = den.len();
    while dn > 1 && den.get(dn - 1).is_some_and(|v| v.abs() < POLY_EPS) {
        dn -= 1;
    }
    if dn == 0 || rem.len() < dn {
        return (vec![0.0], rem);
    }
    let lead = den[dn - 1];
    let mut quot = vec![0.0; rem.len() - dn + 1];
    while rem.len() >= dn {
        let k = rem.len() - dn;
        let Some(last) = rem.last().copied() else {
            break;
        };
        let q = last / lead;
        quot[k] = q;
        for i in 0..dn {
            rem[k + i] -= q * den[i];
        }
        rem.pop();
        while rem.len() > 1 && rem.last().is_some_and(|v| v.abs() < POLY_EPS) {
            rem.pop();
        }
        if rem.is_empty() {
            rem.push(0.0);
            break;
        }
    }
    (quot, rem)
}

/// Resuelve `M·c = rhs` (cuadrada, `n ≤ 4`) por eliminación con pivoteo.
///
/// `None` si es singular: el llamador emite `Err` honesto en vez de ruido.
fn solve_small_system(m: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rhs.len();
    if n == 0 || n > MAX_RISCH_PARTIAL_DEGREE || m.len() != n {
        return None;
    }
    for row in m {
        if row.len() != n {
            return None;
        }
    }
    let mut aug: Vec<Vec<f64>> = m
        .iter()
        .zip(rhs.iter())
        .map(|(row, r)| {
            let mut v = row.clone();
            v.push(*r);
            v
        })
        .collect();
    for col in 0..n {
        let mut piv = col;
        for r in col..n {
            if aug[r][col].abs() > aug[piv][col].abs() {
                piv = r;
            }
        }
        if !aug[piv][col].is_finite() || aug[piv][col].abs() < 1e-12 {
            return None;
        }
        aug.swap(col, piv);
        let diag = aug[col][col];
        let pivot_row: Vec<f64> = aug[col][col..=n].to_vec();
        for (r, row) in aug.iter_mut().enumerate().take(n) {
            if r == col {
                continue;
            }
            let factor = row[col] / diag;
            if !factor.is_finite() {
                return None;
            }
            for (cell, pivot) in row.iter_mut().skip(col).zip(pivot_row.iter()) {
                *cell -= factor * pivot;
            }
        }
    }
    let mut out = vec![0.0; n];
    for (i, row) in aug.iter().enumerate() {
        if row[i].abs() < 1e-12 {
            return None;
        }
        out[i] = row[n] / row[i];
        if !out[i].is_finite() {
            return None;
        }
    }
    Some(out)
}

/// Raíces reales (con multiplicidad) de un polinomio ascendente.
///
/// Solo pela factores lineales de raíz racional entera acotada (candidatos
/// `p|ct, q|cl`, ≤ 32 pruebas): determinista y sin explosión. Devuelve
/// `(raíces, resto)`: si `resto` es grado ≥ 1, el llamador emite `Err` con
/// la factorización parcial lograda.
fn peel_rational_roots(coeffs: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut rest = coeffs.to_vec();
    while rest.len() > 1 && rest.last().is_some_and(|v| v.abs() < POLY_EPS) {
        rest.pop();
    }
    let mut roots = Vec::new();
    loop {
        let deg = rest.len().saturating_sub(1);
        if deg < 1 {
            break;
        }
        // Candidatos enteros solo si los coeficientes son (casi) enteros.
        let rounded: Vec<f64> = rest.iter().map(|v| v.round()).collect();
        if rounded
            .iter()
            .zip(rest.iter())
            .any(|(r, v)| (r - v).abs() > 1e-9 || !v.is_finite())
        {
            break;
        }
        let ct = rounded.first().copied().unwrap_or(0.0).abs() as i64;
        let cl = rounded.last().copied().unwrap_or(0.0).abs() as i64;
        if ct == 0 {
            // Raíz x = 0 con multiplicidad: pela directamente.
            roots.push(0.0);
            rest.remove(0);
            continue;
        }
        if cl == 0 {
            break;
        }
        let divisors = |m: i64| -> Vec<i64> {
            let mut d = Vec::new();
            let mut k = 1_i64;
            while k * k <= m {
                if m % k == 0 {
                    d.push(k);
                    if k * k != m {
                        d.push(m / k);
                    }
                }
                k += 1;
            }
            d
        };
        let (dp, dq) = (divisors(ct), divisors(cl.max(1)));
        let mut found: Option<f64> = None;
        let mut trials = 0_usize;
        'search: for p in &dp {
            for q in &dq {
                for sign in [-1.0, 1.0] {
                    if trials >= MAX_RISCH_ROOT_TRIALS {
                        break 'search;
                    }
                    trials += 1;
                    let cand = sign * (*p as f64) / (*q as f64);
                    if eval_poly_asc(&rest, cand).abs() < 1e-9 {
                        found = Some(cand);
                        break 'search;
                    }
                }
            }
        }
        let Some(r) = found else {
            break;
        };
        // División sintética por `(x − r)`.
        let mut next = vec![0.0; rest.len() - 1];
        let mut carry = 0.0;
        for i in (0..rest.len()).rev() {
            let v = rest[i] + carry;
            if i > 0 {
                next[i - 1] = v;
                carry = v * r;
            } else if v.abs() > 1e-6 {
                // Residuo no nulo: falso positivo numérico, no pela.
                return (roots, rest);
            }
        }
        roots.push(r);
        rest = next;
        if roots.len() > MAX_RISCH_PARTIAL_DEGREE {
            break;
        }
    }
    (roots, rest)
}

/// Primitiva de `P(x)/Q(x)` propios con `Q` cuadrática irreducible.
///
/// `∫P/Q = (p1/2c2)·ln|Q| + K·(2·σ/s)·atan(σ·(2c2·x+c1)/s)` con
/// `s = √(4c2c0−c1²)`, `σ = signo(c2)`, `K = p0 − p1·c1/2c2`.
fn primitive_irreducible_quadratic(p: &[f64], q: &[f64], var: &str) -> Option<crate::ast::Expr> {
    use crate::ast::Expr;
    if q.len() != 3 {
        return None;
    }
    let (c0, c1, c2) = (q[0], q[1], q[2]);
    if c2.abs() < POLY_EPS {
        return None;
    }
    let disc4 = 4.0 * c2 * c0 - c1 * c1;
    if !disc4.is_finite() || disc4 <= 0.0 {
        return None;
    }
    let (p0, p1) = (
        p.first().copied().unwrap_or(0.0),
        p.get(1).copied().unwrap_or(0.0),
    );
    let x = Expr::Var(var.to_string());
    let q_expr = Expr::Add(
        Box::new(Expr::Add(
            Box::new(Expr::Mul(
                Box::new(Expr::Const(c2)),
                Box::new(Expr::Pow(Box::new(x.clone()), Box::new(Expr::Const(2.0)))),
            )),
            Box::new(Expr::Mul(Box::new(Expr::Const(c1)), Box::new(x.clone()))),
        )),
        Box::new(Expr::Const(c0)),
    );
    let s = disc4.sqrt();
    let sigma = c2.signum();
    let mut prim: Option<Expr> = None;
    if (p1 / (2.0 * c2)).abs() > POLY_EPS {
        prim = Some(Expr::Mul(
            Box::new(Expr::Const(p1 / (2.0 * c2))),
            Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(q_expr.clone()))))),
        ));
    }
    let k = p0 - p1 * c1 / (2.0 * c2);
    if k.abs() > POLY_EPS {
        let arg = Expr::Mul(
            Box::new(Expr::Const(sigma / s)),
            Box::new(Expr::Add(
                Box::new(Expr::Mul(Box::new(Expr::Const(2.0 * c2)), Box::new(x))),
                Box::new(Expr::Const(c1)),
            )),
        );
        let atan_term = Expr::Mul(
            Box::new(Expr::Const(k * 2.0 * sigma / s)),
            Box::new(Expr::Atan(Box::new(arg))),
        );
        prim = Some(match prim {
            Some(prev) => Expr::Add(Box::new(prev), Box::new(atan_term)),
            None => atan_term,
        });
    }
    prim
}

/// Integra `num/den` racionales: división + fracciones parciales.
///
/// Grado del denominador ≤ 4 con factores lineales reales, o cuadrática
/// irreducible (`ln` + `atan`). Resto → `Err::Unsupported` con la
/// factorización parcial para que el mensaje diga el límite.
#[allow(clippy::too_many_lines)]
fn risch_rational(
    num: &crate::ast::Expr,
    den: &crate::ast::Expr,
    var: &str,
    depth: u32,
    terms: &mut usize,
) -> Result<crate::ast::Expr, RischError> {
    use crate::ast::Expr;
    let p_full = poly_coeffs_bounded(num, var, MAX_RISCH_PARTIAL_DEGREE * 2)
        .ok_or_else(|| risch_unsupported("numerador no polinómico en la variable".to_string()))?;
    let mut q_full = poly_coeffs_bounded(den, var, MAX_RISCH_PARTIAL_DEGREE).ok_or_else(|| {
        risch_unsupported(format!(
            "denominador no polinómico o de grado > {MAX_RISCH_PARTIAL_DEGREE}"
        ))
    })?;
    while q_full.len() > 1 && q_full.last().is_some_and(|v| v.abs() < POLY_EPS) {
        q_full.pop();
    }
    if q_full.iter().all(|v| v.abs() < POLY_EPS) {
        return Err(risch_unsupported(
            "denominador idénticamente nulo".to_string(),
        ));
    }
    if q_full.len() - 1 > MAX_RISCH_PARTIAL_DEGREE {
        return Err(risch_unsupported(format!(
            "denominador de grado {} excede {MAX_RISCH_PARTIAL_DEGREE}",
            q_full.len() - 1
        )));
    }
    if depth > MAX_RISCH_DEPTH {
        return Err(RischError::ResourceLimit {
            detail: "profundidad en fracciones parciales".to_string(),
        });
    }
    let x = Expr::Var(var.to_string());
    let mut prim_parts: Vec<Expr> = Vec::new();
    // Parte polinómica si `grado(num) ≥ grado(den)`.
    let (quot, rem) = poly_div_asc(&p_full, &q_full);
    for (k, qk) in quot.iter().enumerate() {
        if qk.abs() < POLY_EPS {
            continue;
        }
        *terms += 1;
        if *terms > MAX_RISCH_TERMS {
            return Err(RischError::ResourceLimit {
                detail: format!("más de {MAX_RISCH_TERMS} términos"),
            });
        }
        if k == 0 {
            prim_parts.push(Expr::Mul(Box::new(Expr::Const(*qk)), Box::new(x.clone())));
        } else {
            prim_parts.push(Expr::Mul(
                Box::new(Expr::Const(qk / (k as f64 + 1.0))),
                Box::new(Expr::Pow(
                    Box::new(x.clone()),
                    Box::new(Expr::Const(k as f64 + 1.0)),
                )),
            ));
        }
    }
    let mut rem_c = rem;
    while rem_c.len() > 1 && rem_c.last().is_some_and(|v| v.abs() < POLY_EPS) {
        rem_c.pop();
    }
    if rem_c.iter().all(|v| v.abs() < POLY_EPS) {
        let acc = prim_parts
            .into_iter()
            .reduce(|a, b| Expr::Add(Box::new(a), Box::new(b)));
        return acc.ok_or_else(|| risch_unsupported("cociente vacío".to_string()));
    }
    let deg_q = q_full.len() - 1;
    // Denominador lineal: `K/(d1·x+d0)` → `(K/d1)·ln|x−r|`.
    if deg_q == 1 {
        let (d0, d1) = (q_full[0], q_full[1]);
        if d1.abs() < POLY_EPS {
            return Err(risch_unsupported(
                "denominador lineal degenerado".to_string(),
            ));
        }
        let k = rem_c.first().copied().unwrap_or(0.0);
        let r = -d0 / d1;
        *terms += 1;
        if *terms > MAX_RISCH_TERMS {
            return Err(RischError::ResourceLimit {
                detail: format!("más de {MAX_RISCH_TERMS} términos"),
            });
        }
        let log_term = Expr::Mul(
            Box::new(Expr::Const(k / d1)),
            Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Expr::Sub(
                Box::new(x.clone()),
                Box::new(Expr::Const(r)),
            )))))),
        );
        prim_parts.push(log_term);
        let acc = prim_parts
            .into_iter()
            .reduce(|a, b| Expr::Add(Box::new(a), Box::new(b)));
        return acc.ok_or_else(|| risch_unsupported("cociente vacío".to_string()));
    }
    // Denominador cuadrático.
    if deg_q == 2 {
        let (c0, c1, c2) = (q_full[0], q_full[1], q_full[2]);
        let disc = c1 * c1 - 4.0 * c2 * c0;
        if disc > POLY_EPS {
            // Raíces reales distintas: cover-up `A = P(r)/Q'(r)`.
            let s = disc.sqrt();
            let (r1, r2) = ((-c1 + s) / (2.0 * c2), (-c1 - s) / (2.0 * c2));
            for r in [r1, r2] {
                let qp = 2.0 * c2 * r + c1;
                if qp.abs() < POLY_EPS {
                    return Err(risch_unsupported(
                        "raíz múltiple aparente en cuadrática".to_string(),
                    ));
                }
                let coef = eval_poly_asc(&rem_c, r) / qp;
                *terms += 1;
                if *terms > MAX_RISCH_TERMS {
                    return Err(RischError::ResourceLimit {
                        detail: format!("más de {MAX_RISCH_TERMS} términos"),
                    });
                }
                prim_parts.push(Expr::Mul(
                    Box::new(Expr::Const(coef)),
                    Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Expr::Sub(
                        Box::new(x.clone()),
                        Box::new(Expr::Const(r)),
                    )))))),
                ));
            }
        } else if disc >= -POLY_EPS {
            // Raíz doble `r`: `A·ln|x−r| − B/(x−r)`.
            let r = -c1 / (2.0 * c2);
            let (p0, p1) = (
                rem_c.first().copied().unwrap_or(0.0),
                rem_c.get(1).copied().unwrap_or(0.0),
            );
            let (big_a, big_b) = (p1, p0 + p1 * r);
            *terms += 2;
            if *terms > MAX_RISCH_TERMS {
                return Err(RischError::ResourceLimit {
                    detail: format!("más de {MAX_RISCH_TERMS} términos"),
                });
            }
            prim_parts.push(Expr::Mul(
                Box::new(Expr::Const(big_a / c2)),
                Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Expr::Sub(
                    Box::new(x.clone()),
                    Box::new(Expr::Const(r)),
                )))))),
            ));
            prim_parts.push(Expr::Div(
                Box::new(Expr::Const(-big_b / c2)),
                Box::new(Expr::Sub(Box::new(x), Box::new(Expr::Const(r)))),
            ));
        } else {
            // Irreducible: `ln(Q)` + `atan` (incluye `1/(x²+1)`-style).
            match primitive_irreducible_quadratic(&rem_c, &q_full, var) {
                Some(qprim) => prim_parts.push(qprim),
                None => {
                    return Err(risch_unsupported(
                        "cuadrática irreducible degenerada".to_string(),
                    ));
                }
            }
        }
        let acc = prim_parts
            .into_iter()
            .reduce(|a, b| Expr::Add(Box::new(a), Box::new(b)));
        return acc.ok_or_else(|| risch_unsupported("cociente vacío".to_string()));
    }
    // Grado 3–4: pela lineales reales; resto irreducible → `Err` parcial.
    let (roots, rest) = peel_rational_roots(&q_full);
    if rest.len() > 1 {
        let partial: Vec<String> = roots.iter().map(|r| format!("(x−{r:.6})")).collect();
        return Err(risch_unsupported(format!(
            "denominador con factor irreducible de grado {} (parcial: {}); factoriza el resto en lineales reales o usa Hermite/Rothstein",
            rest.len() - 1,
            if partial.is_empty() {
                "sin lineales".to_string()
            } else {
                partial.join("·")
            }
        )));
    }
    if roots.is_empty() {
        return Err(risch_unsupported(
            "sin raíces lineales reales; usa Hermite/Rothstein".to_string(),
        ));
    }
    // Fracciones `Σ C[j]/(x−r_j)^k` por identidad polinómica en puntos enteros.
    let lead_q = q_full.last().copied().unwrap_or(1.0);
    let mut run: Vec<(f64, usize)> = Vec::new();
    let mut sorted = roots.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    for r in sorted {
        match run.last_mut() {
            Some(last) if (last.0 - r).abs() < 1e-9 => last.1 += 1,
            _ => run.push((r, 1)),
        }
    }
    let m: usize = run.iter().map(|(_, k)| k).sum();
    // Puntos de colocación enteros que evitan las raíces.
    let mut xs: Vec<f64> = Vec::new();
    let mut cand: i64 = 0;
    while xs.len() < m && cand < 64 {
        for v in [cand as f64, -(cand as f64)] {
            if xs.len() >= m {
                break;
            }
            if run.iter().all(|(r, _)| (v - r).abs() > 1e-9) && !xs.contains(&v) {
                xs.push(v);
            }
        }
        cand += 1;
    }
    xs.truncate(m);
    if xs.len() < m {
        return Err(risch_unsupported(
            "sin puntos de colocación para el sistema parcial".to_string(),
        ));
    }
    // Base `Q(x)/(x−r)^k` por incógnita (orden: por raíz, k = 1..mult).
    let mut mat: Vec<Vec<f64>> = Vec::new();
    let mut rhs_v: Vec<f64> = Vec::new();
    for xv in &xs {
        let mut row = Vec::with_capacity(m);
        for (r, mult) in &run {
            for k in 1..=*mult {
                let mut basis = lead_q;
                for (r2, mult2) in &run {
                    for k2 in 1..=*mult2 {
                        if r2 == r && k2 == k {
                            continue;
                        }
                        basis *= (xv - r2).powi(k2 as i32);
                    }
                }
                // Potencia restante de la propia raíz.
                let own = if mult - k > 0 {
                    (xv - r).powi((mult - k) as i32)
                } else {
                    1.0
                };
                row.push(basis * own);
            }
        }
        mat.push(row);
        rhs_v.push(eval_poly_asc(&rem_c, *xv));
    }
    let coeffs = solve_small_system(&mat, &rhs_v).ok_or_else(|| {
        risch_unsupported(
            "sistema de fracciones parciales singular; reduce el grado o usa Hermite/Rothstein"
                .to_string(),
        )
    })?;
    let mut idx = 0_usize;
    for (r, mult) in &run {
        for k in 1..=*mult {
            let Some(c) = coeffs.get(idx).copied() else {
                return Err(risch_unsupported(
                    "coeficiente parcial faltante".to_string(),
                ));
            };
            idx += 1;
            if c.abs() < POLY_EPS {
                continue;
            }
            *terms += 1;
            if *terms > MAX_RISCH_TERMS {
                return Err(RischError::ResourceLimit {
                    detail: format!("más de {MAX_RISCH_TERMS} términos"),
                });
            }
            let base = Expr::Sub(Box::new(x.clone()), Box::new(Expr::Const(*r)));
            if k == 1 {
                prim_parts.push(Expr::Mul(
                    Box::new(Expr::Const(c)),
                    Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(base))))),
                ));
            } else {
                prim_parts.push(Expr::Div(
                    Box::new(Expr::Const(c / (1.0 - k as f64))),
                    Box::new(Expr::Pow(
                        Box::new(base),
                        Box::new(Expr::Const(k as f64 - 1.0)),
                    )),
                ));
            }
        }
    }
    let acc = prim_parts
        .into_iter()
        .reduce(|a, b| Expr::Add(Box::new(a), Box::new(b)));
    acc.ok_or_else(|| risch_unsupported("cociente vacío".to_string()))
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
        // F3c: `1/(x^2+1)` ahora es `atan` (ver `risch_arctan_style`).
        for expr in ["exp(x^2)", "sin(x)/x", "x^x", "tan(x^2)", "1/(x^3+x+1)"] {
            let err = risch_norman_integrate(expr, "x").expect_err("fuera de S/M");
            assert!(
                matches!(err, RischError::Unsupported { .. }),
                "{expr}: got {err}"
            );
        }
    }

    // --- Frente F3c: Risch extendido (derivada-verificación por regla) ---

    /// Verifica que `d(prim)/dx = integrando` en puntos de muestra finitos.
    fn check_prim_by_derivative(expr: &str, var: &str, prim: &str, points: &[f64]) {
        let integrand =
            crate::ast::parse_ast(&expr.replace(' ', "")).expect("parse integrando F3c");
        let prim_ast = crate::ast::parse_ast(&prim.replace(' ', "")).expect("parse primitiva F3c");
        let deriv = prim_ast.diff(var);
        for at in points {
            let f = integrand.eval_at(var, *at);
            let d = deriv.eval_at(var, *at);
            assert!(
                f.is_finite() && d.is_finite(),
                "punto no finito en {at}: f={f} d={d} ({expr} → {prim})"
            );
            assert!(
                (f - d).abs() < 1e-6,
                "derivada no recupera integrando en {at}: f={f} d={d} ({expr} → {prim})"
            );
        }
    }

    const F3C_POINTS: [f64; 4] = [0.37, 1.13, 2.71, -0.53];

    #[test]
    fn risch_tan_linear_arg() {
        let prim = risch_norman_integrate("tan(x)", "x").expect("tan");
        assert!(prim.contains("ln"), "got {prim}");
        check_prim_by_derivative("tan(x)", "x", &prim, &F3C_POINTS);
        let prim2 = risch_norman_integrate("tan(2*x+1)", "x").expect("tan lineal");
        check_prim_by_derivative("tan(2*x+1)", "x", &prim2, &F3C_POINTS);
    }

    #[test]
    fn risch_sec_squared() {
        for expr in ["sec(x)^2", "1/cos(x)^2"] {
            let prim = risch_norman_integrate(expr, "x").expect("sec²");
            assert!(prim.contains("tan"), "got {prim} para {expr}");
            check_prim_by_derivative(expr, "x", &prim, &F3C_POINTS);
        }
    }

    #[test]
    fn risch_rational_distinct_linear_roots() {
        // 1/((x−1)(x+2)) → logs por cover-up.
        let prim = risch_norman_integrate("1/(x^2+x-2)", "x").expect("parciales distintas");
        assert!(prim.contains("ln"), "got {prim}");
        check_prim_by_derivative("1/(x^2+x-2)", "x", &prim, &[0.37, -0.53, 2.71, 3.5]);
    }

    #[test]
    fn risch_rational_repeated_root() {
        // 1/(x−1)² → −1/(x−1).
        let prim = risch_norman_integrate("1/(x^2-2*x+1)", "x").expect("raíz doble");
        check_prim_by_derivative("1/(x^2-2*x+1)", "x", &prim, &[0.37, 2.71, -0.53, 3.5]);
    }

    #[test]
    fn risch_rational_linear_den_with_poly_num() {
        // (2x+3)/(x+1) = 2 + 1/(x+1).
        let prim = risch_norman_integrate("(2*x+3)/(x+1)", "x").expect("división + ln");
        check_prim_by_derivative("(2*x+3)/(x+1)", "x", &prim, &[0.37, 2.71, -0.53, 1.5]);
    }

    #[test]
    fn risch_rational_improper() {
        // x²/(x+1) = x − 1 + 1/(x+1).
        let prim = risch_norman_integrate("x^2/(x+1)", "x").expect("impropia");
        check_prim_by_derivative("x^2/(x+1)", "x", &prim, &[0.37, 2.71, -0.53, 1.5]);
    }

    #[test]
    fn risch_arctan_style() {
        let prim = risch_norman_integrate("1/(x^2+1)", "x").expect("atan");
        assert!(prim.contains("atan"), "got {prim}");
        check_prim_by_derivative("1/(x^2+1)", "x", &prim, &F3C_POINTS);
    }

    #[test]
    fn risch_arctan_scaled() {
        let prim = risch_norman_integrate("1/(4*x^2+9)", "x").expect("atan escalado");
        assert!(prim.contains("atan"), "got {prim}");
        check_prim_by_derivative("1/(4*x^2+9)", "x", &prim, &F3C_POINTS);
    }

    #[test]
    fn risch_irreducible_quadratic_with_linear_num() {
        // x/(x²+1) → ½·ln(x²+1).
        let prim = risch_norman_integrate("x/(x^2+1)", "x").expect("ln cuadrática");
        assert!(prim.contains("ln"), "got {prim}");
        check_prim_by_derivative("x/(x^2+1)", "x", &prim, &F3C_POINTS);
    }

    #[test]
    fn risch_sqrt_cbrt_and_fractional_power() {
        let prim = risch_norman_integrate("sqrt(x)", "x").expect("sqrt");
        check_prim_by_derivative("sqrt(x)", "x", &prim, &[0.37, 1.13, 2.71, 4.0]);
        let prim2 = risch_norman_integrate("cbrt(x)", "x").expect("cbrt");
        check_prim_by_derivative("cbrt(x)", "x", &prim2, &[0.37, 1.13, 2.71, 4.0]);
        let prim3 = risch_norman_integrate("x^(3/2)", "x").expect("x^3/2");
        check_prim_by_derivative("x^(3/2)", "x", &prim3, &[0.37, 1.13, 2.71, 4.0]);
    }

    #[test]
    fn risch_cubic_fully_split() {
        // 1/((x−1)(x−2)(x−3)) → tres logs por sistema 3×3.
        let prim = risch_norman_integrate("1/(x^3-6*x^2+11*x-6)", "x").expect("cúbica");
        assert!(prim.contains("ln"), "got {prim}");
        check_prim_by_derivative(
            "1/(x^3-6*x^2+11*x-6)",
            "x",
            &prim,
            &[0.37, -0.53, 1.5, 3.71],
        );
    }

    #[test]
    fn risch_binomial_power_expands() {
        let prim = risch_norman_integrate("(x+1)^3", "x").expect("binomio");
        check_prim_by_derivative("(x+1)^3", "x", &prim, &F3C_POINTS);
    }

    #[test]
    fn risch_partial_beyond_subset_is_honest() {
        // Cúbica con cuadrática irreducible: `Err` + factorización parcial.
        let err =
            risch_norman_integrate("1/(x^3-3*x^2+3*x-2)", "x").expect_err("resto irreducible");
        let msg = format!("{err}");
        assert!(matches!(err, RischError::Unsupported { .. }), "got {msg}");
        assert!(msg.contains("(x−2"), "debe traer la parcial, got {msg}");
        // Grado 5 > cota 4: `Err` sin explosión.
        let err5 = risch_norman_integrate("1/(x^5+2*x+1)", "x").expect_err("grado 5");
        assert!(matches!(err5, RischError::Unsupported { .. }), "got {err5}");
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
