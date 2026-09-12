//! Frente B1 — `Solve` GENERAL (todas las raíces reales) + sistemas poli 2×2.
//!
//! - Lineal exacto vía Gauss (reúsa [`crate::matrices::solve_linear_system`]).
//! - Poli 1-var: lineal/cuadrática exactas + todas-reales por Sturm/bisección
//!   + Newton, intervalo por cota de Cauchy.
//! - Sistemas poli 2×2 por eliminación (resultante de Sylvester por
//!   interpolación) con verificación por residuo; la base de Groebner queda
//!   como derivación honesta (`Groebner`/`Eliminate`) cuando la eliminación
//!   falla.
//! - `solve_expression` (`cas.rs`) se mantiene como `NSolve` de 1 raíz.
//!
//! Imposible honesto: grado ≥ 5 sin fórmula cerrada (solo reales por
//! aislamiento numérico acotado), trascendentes mixtos → 1 raíz `NSolve`.
//!
//! Presupuestos: `MAX_SOLVE_BYTES` 2000 (igual que `MAX_EXPR_LENGTH`),
//! `MAX_SOLVE_DEGREE` 16, Newton/bisección `MAX_SOLVE_ITER` 100,
//! intervalos Sturm `MAX_STURM_INTERVALS` 1024, sistema grado total
//! `MAX_SYSTEM_DEGREE` 4, puntos `MAX_SYSTEM_POINTS` 64, lineal
//! `MAX_LINEAR_DIM` 32. Todo borde devuelve `Result<_, SolveError>`;
//! cero `unwrap` en producción.

use crate::ast::{parse_ast, Expr};
use crate::expr::preprocess_expr;
use crate::matrices::{solve_linear_system, Matrix};
use std::collections::HashMap;

/// Máximo de bytes por expresión de entrada (igual que `MAX_EXPR_LENGTH`).
pub const MAX_SOLVE_BYTES: usize = 2000;
/// Grado máximo aceptado para `Solve[expr, var]` directo.
pub const MAX_SOLVE_DEGREE: usize = 16;
/// Cota de sondeo para distinguir grado-excedido de no-polinómico.
const MAX_DEGREE_PROBE: usize = 64;
/// Iteraciones máximas de Newton/bisección por raíz.
pub const MAX_SOLVE_ITER: usize = 100;
/// Intervalos máximos en aislamiento Sturm.
pub const MAX_STURM_INTERVALS: usize = 1024;
/// Pasos máximos de Euclides polinómico en la secuencia Sturm.
const MAX_STURM_STEPS: usize = 512;
/// Grado total máximo por ecuación del sistema 2×2.
pub const MAX_SYSTEM_DEGREE: usize = 4;
/// Términos máximos por polinomio bivariado.
const MAX_SYSTEM_TERMS: usize = 128;
/// Puntos máximos devueltos por el sistema 2×2.
pub const MAX_SYSTEM_POINTS: usize = 64;
/// Dimensión máxima del sistema lineal exacto.
pub const MAX_LINEAR_DIM: usize = 32;
/// Iteraciones máximas del Newton 2D por candidato.
const MAX_NEWTON_2D_ITER: usize = 8;
/// Tolerancia relativa de residuo para aceptar raíces/candidatos.
const RESIDUAL_TOL: f64 = 1e-8;
/// Tolerancia de cruce para determinantes numéricos nulos.
const DET_TOL: f64 = 1e-12;

// ---------------------------------------------------------------------------
// Error y resultado
// ---------------------------------------------------------------------------

/// Error honesto del frente B1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveError {
    /// Entrada vacía, muy larga, variable inválida o sin incógnita.
    InvalidInput { detail: String },
    /// No es polinomio en la variable (trascendente/mixto) → `NSolve`.
    NotPolynomial { hint: String },
    /// Grado fuera de presupuesto.
    DegreeExceeded { degree: usize, maximum: usize },
    /// Singular, dependiente o fuera del subconjunto → derivación honesta.
    Unsupported { hint: String },
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput { detail } => write!(f, "entrada inválida: {detail}"),
            Self::NotPolynomial { hint } => write!(f, "no polinómico: {hint}"),
            Self::DegreeExceeded { degree, maximum } => write!(
                f,
                "grado {degree} excede el máximo {maximum}; reduce el grado o usa NSolve[..] / Eliminate[..]"
            ),
            Self::Unsupported { hint } => write!(f, "{hint}"),
        }
    }
}

/// Método que produjo las raíces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveMethod {
    /// Despeje `a·x + b = 0`.
    LinearExact,
    /// Fórmula cuadrática estable.
    QuadraticExact,
    /// Aislamiento Sturm + bisección + Newton.
    SturmBisection,
}

impl std::fmt::Display for SolveMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LinearExact => write!(f, "lineal exacta"),
            Self::QuadraticExact => write!(f, "cuadrática exacta"),
            Self::SturmBisection => write!(f, "Sturm+bisección+Newton"),
        }
    }
}

/// Todas las raíces reales de un polinomio 1-var.
#[derive(Debug, Clone)]
pub struct RealRoots {
    /// Raíces reales ordenadas, sin duplicados.
    pub roots: Vec<f64>,
    /// Método aplicado.
    pub method: SolveMethod,
    /// Cantidad de raíces no reales (exacto en grado ≤ 2, estimado si no).
    pub complex_count: usize,
    /// Aviso honesto (grado ≥ 5, multiplicidad, etc.).
    pub notice: Option<String>,
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn valid_var(var: &str) -> bool {
    let mut chars = var.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn contains_var(ast: &Expr, var: &str) -> bool {
    let mut vars = std::collections::HashSet::new();
    ast.get_variables(&mut vars);
    vars.contains(var)
}

/// Horner ascendente (`coeffs[0] + coeffs[1]*x + ...`).
fn eval_poly(coeffs: &[f64], x: f64) -> f64 {
    let mut acc = 0.0_f64;
    for c in coeffs.iter().rev() {
        acc = acc.mul_add(x, *c);
    }
    acc
}

fn poly_deg(coeffs: &[f64]) -> usize {
    coeffs.iter().rposition(|c| *c != 0.0).unwrap_or(0)
}

/// Resto de división polinómica (`a mod b`), coeficientes ascendentes.
fn poly_remainder(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut rest: Vec<f64> = a.to_vec();
    let db = poly_deg(b);
    let lead_b = b.get(db).copied().unwrap_or(0.0);
    if lead_b == 0.0 {
        return Vec::new();
    }
    for _ in 0..MAX_STURM_STEPS {
        let dr = poly_deg(&rest);
        if dr < db || rest.iter().all(|c| *c == 0.0) {
            break;
        }
        let lead_r = rest.get(dr).copied().unwrap_or(0.0);
        let shift = dr.saturating_sub(db);
        let factor = lead_r / lead_b;
        if !factor.is_finite() {
            break;
        }
        for (i, c) in b.iter().enumerate().take(db + 1) {
            let idx = i + shift;
            if let Some(slot) = rest.get_mut(idx) {
                *slot -= factor * c;
            }
        }
    }
    while rest.len() > 1 && rest.last().is_some_and(|c| *c == 0.0) {
        rest.pop();
    }
    rest
}

/// Cota de Cauchy: todas las raíces cumplen `|x| ≤ 1 + max|aᵢ/aₙ|`.
fn cauchy_bound(coeffs: &[f64], degree: usize) -> f64 {
    let lead = coeffs.get(degree).copied().unwrap_or(0.0).abs();
    if lead == 0.0 || !lead.is_finite() {
        return 1.0;
    }
    let mut bound = 1.0_f64;
    for c in coeffs.iter().take(degree) {
        if c.is_finite() {
            bound = bound.max(1.0 + c.abs() / lead);
        }
    }
    if bound.is_finite() {
        bound
    } else {
        1.0
    }
}

fn poly_derivative(coeffs: &[f64]) -> Vec<f64> {
    if coeffs.len() <= 1 {
        return vec![0.0];
    }
    coeffs
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, c)| i as f64 * c)
        .collect()
}

/// Newton 1D acotado sobre coeficientes; `None` si no converge.
fn newton_polish(coeffs: &[f64], mut x: f64) -> Option<f64> {
    let deriv = poly_derivative(coeffs);
    for _ in 0..MAX_SOLVE_ITER {
        if !x.is_finite() {
            return None;
        }
        let fx = eval_poly(coeffs, x);
        let fpx = eval_poly(&deriv, x);
        if fx == 0.0 {
            return Some(x);
        }
        if fpx == 0.0 || !fpx.is_finite() || !fx.is_finite() {
            return None;
        }
        let step = fx / fpx;
        if !step.is_finite() {
            return None;
        }
        x -= step;
        if step.abs() <= 1e-13 * (1.0 + x.abs()) {
            return if x.is_finite() { Some(x) } else { None };
        }
    }
    if x.is_finite() {
        Some(x)
    } else {
        None
    }
}

fn residual_small(coeffs: &[f64], degree: usize, x: f64) -> bool {
    let mut scale = 0.0_f64;
    for c in coeffs.iter().take(degree + 1) {
        scale = scale.mul_add(x.abs(), c.abs());
    }
    let r = eval_poly(coeffs, x).abs();
    r.is_finite() && scale.is_finite() && (r == 0.0 || (scale > 0.0 && r <= RESIDUAL_TOL * scale))
}

// ---------------------------------------------------------------------------
// Sturm: secuencia, variaciones, aislamiento
// ---------------------------------------------------------------------------

fn sturm_sequence(p: &[f64]) -> Vec<Vec<f64>> {
    let mut seq = vec![p.to_vec(), poly_derivative(p)];
    for _ in 0..MAX_STURM_STEPS {
        let last = seq.len();
        let (Some(prev), Some(cur)) = (seq.get(last - 2).cloned(), seq.get(last - 1).cloned())
        else {
            break;
        };
        if cur.iter().all(|c| *c == 0.0) {
            break;
        }
        let mut rest = poly_remainder(&prev, &cur);
        for c in &mut rest {
            *c = -*c;
        }
        if rest.iter().all(|c| *c == 0.0) {
            break;
        }
        seq.push(rest);
    }
    seq
}

fn sign_variations(seq: &[Vec<f64>], x: f64) -> usize {
    let mut prev = 0_i8;
    let mut changes = 0_usize;
    for p in seq {
        let v = eval_poly(p, x);
        if !v.is_finite() || v == 0.0 {
            continue;
        }
        let s = if v > 0.0 { 1_i8 } else { -1_i8 };
        if prev != 0 && s != prev {
            changes += 1;
        }
        prev = s;
    }
    changes
}

fn bisect_root(coeffs: &[f64], mut lo: f64, mut hi: f64) -> f64 {
    let mut flo = eval_poly(coeffs, lo);
    for _ in 0..MAX_SOLVE_ITER {
        let mid = 0.5 * (lo + hi);
        if !mid.is_finite() || mid == lo || mid == hi {
            break;
        }
        let fmid = eval_poly(coeffs, mid);
        if !fmid.is_finite() {
            break;
        }
        if fmid == 0.0 || (hi - lo) <= 1e-13 * (1.0 + mid.abs()) {
            return mid;
        }
        if flo == 0.0 {
            return lo;
        }
        if (flo < 0.0) == (fmid < 0.0) {
            lo = mid;
            flo = fmid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn sturm_isolate(coeffs: &[f64], degree: usize, bound: f64) -> Vec<f64> {
    let seq = sturm_sequence(coeffs);
    let mut stack = vec![(-bound, bound)];
    let mut found = Vec::new();
    let mut guard = 0_usize;
    while let Some((lo, hi)) = stack.pop() {
        guard += 1;
        if guard > MAX_STURM_INTERVALS * 4 || found.len() > degree {
            break;
        }
        if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
            continue;
        }
        let count = sign_variations(&seq, lo).saturating_sub(sign_variations(&seq, hi));
        if count == 0 {
            continue;
        }
        if count == 1 || (hi - lo) <= 1e-12 * (1.0 + hi.abs().max(lo.abs())) {
            let mut x = bisect_root(coeffs, lo, hi);
            if let Some(polished) = newton_polish(coeffs, x) {
                x = polished;
            }
            if x.is_finite() && residual_small(coeffs, degree, x) {
                found.push(x);
            }
            continue;
        }
        let mid = 0.5 * (lo + hi);
        if !mid.is_finite() || mid == lo || mid == hi {
            continue;
        }
        stack.push((lo, mid));
        stack.push((mid, hi));
    }
    found.sort_by(f64::total_cmp);
    found.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs())));
    found
}

// ---------------------------------------------------------------------------
// Coeficientes 1-var (acotados, sin `unwrap`)
// ---------------------------------------------------------------------------

fn collect_coeffs(ast: &Expr, var: &str, max_deg: usize) -> Option<Vec<f64>> {
    fn add(a: Vec<f64>, b: Vec<f64>, sign: f64, max_deg: usize) -> Option<Vec<f64>> {
        let mut out = vec![0.0; max_deg + 1];
        for (i, (x, y)) in a.into_iter().zip(b).enumerate() {
            let v = x + sign * y;
            if !v.is_finite() {
                return None;
            }
            if let Some(slot) = out.get_mut(i) {
                *slot = v;
            }
        }
        Some(out)
    }
    fn mul(a: &[f64], b: &[f64], max_deg: usize) -> Option<Vec<f64>> {
        let mut out = vec![0.0; max_deg + 1];
        for (i, x) in a.iter().enumerate() {
            if *x == 0.0 {
                continue;
            }
            for (j, y) in b.iter().enumerate() {
                if *y == 0.0 {
                    continue;
                }
                let d = i.checked_add(j)?;
                if d > max_deg {
                    return None;
                }
                let v = out.get(d).copied().unwrap_or(0.0) + x * y;
                if !v.is_finite() {
                    return None;
                }
                if let Some(slot) = out.get_mut(d) {
                    *slot = v;
                }
            }
        }
        Some(out)
    }
    fn go(e: &Expr, var: &str, max_deg: usize) -> Option<Vec<f64>> {
        match e {
            Expr::Const(v) if v.is_finite() => {
                let mut c = vec![0.0; max_deg + 1];
                if let Some(slot) = c.first_mut() {
                    *slot = *v;
                }
                Some(c)
            }
            Expr::Var(name) if name == var => {
                if max_deg < 1 {
                    return None;
                }
                let mut c = vec![0.0; max_deg + 1];
                if let Some(slot) = c.get_mut(1) {
                    *slot = 1.0;
                }
                Some(c)
            }
            Expr::Neg(inner) => go(inner, var, max_deg)?
                .into_iter()
                .map(|c| (-c).is_finite().then_some(-c))
                .collect(),
            Expr::Add(l, r) => add(go(l, var, max_deg)?, go(r, var, max_deg)?, 1.0, max_deg),
            Expr::Sub(l, r) => add(go(l, var, max_deg)?, go(r, var, max_deg)?, -1.0, max_deg),
            Expr::Mul(l, r) => mul(&go(l, var, max_deg)?, &go(r, var, max_deg)?, max_deg),
            Expr::Div(n, d) => {
                let num = go(n, var, max_deg)?;
                let den = go(d, var, max_deg)?;
                if den.first().copied().unwrap_or(0.0) == 0.0
                    || den.iter().skip(1).any(|c| *c != 0.0)
                {
                    return None;
                }
                let q = den.first().copied().unwrap_or(0.0);
                num.into_iter()
                    .map(|c| {
                        let v = c / q;
                        v.is_finite().then_some(v)
                    })
                    .collect()
            }
            Expr::Pow(base, exp) => {
                let Expr::Const(n) = exp.as_ref() else {
                    return None;
                };
                if !n.is_finite() || *n < 0.0 || n.fract() != 0.0 || *n > max_deg as f64 {
                    return None;
                }
                let b = go(base, var, max_deg)?;
                let mut acc = vec![0.0; max_deg + 1];
                if let Some(slot) = acc.first_mut() {
                    *slot = 1.0;
                }
                for _ in 0..(*n as usize) {
                    acc = mul(&acc, &b, max_deg)?;
                }
                Some(acc)
            }
            _ => None,
        }
    }
    go(ast, var, max_deg)
}

// ---------------------------------------------------------------------------
// Raíces reales desde coeficientes (lineal/cuadrática exactas, resto Sturm)
// ---------------------------------------------------------------------------

fn real_roots_of_coeffs(raw: &[f64]) -> RealRoots {
    let degree = poly_deg(raw);
    let coeffs: Vec<f64> = raw.iter().take(degree + 1).copied().collect();
    if degree == 0 {
        return RealRoots {
            roots: Vec::new(),
            method: SolveMethod::LinearExact,
            complex_count: 0,
            notice: None,
        };
    }
    // Cero es raíz: factoriza x^m y resuelve el resto.
    let zero_mult = coeffs.iter().take_while(|c| **c == 0.0).count();
    if zero_mult > 0 {
        let mut inner = real_roots_of_coeffs(&coeffs[zero_mult..]);
        if !inner.roots.contains(&0.0) {
            inner.roots.push(0.0);
            inner.roots.sort_by(f64::total_cmp);
        }
        return inner;
    }
    if degree == 1 {
        let a = coeffs.get(1).copied().unwrap_or(0.0);
        let b = coeffs.first().copied().unwrap_or(0.0);
        let mut roots = Vec::new();
        if a != 0.0 {
            let r = -b / a;
            if r.is_finite() {
                roots.push(r);
            }
        }
        return RealRoots {
            roots,
            method: SolveMethod::LinearExact,
            complex_count: 0,
            notice: None,
        };
    }
    if degree == 2 {
        let (a, b, c) = (
            coeffs.get(2).copied().unwrap_or(0.0),
            coeffs.get(1).copied().unwrap_or(0.0),
            coeffs.first().copied().unwrap_or(0.0),
        );
        if a == 0.0 {
            let mut low: Vec<f64> = coeffs.iter().take(2).copied().collect();
            if low.len() < 2 {
                low.resize(2, 0.0);
            }
            return real_roots_of_coeffs(&low);
        }
        let disc = b.mul_add(b, -4.0 * a * c);
        let round = 16.0 * f64::EPSILON * (b * b + (4.0 * a * c).abs());
        if disc < -round {
            return RealRoots {
                roots: Vec::new(),
                method: SolveMethod::QuadraticExact,
                complex_count: 2,
                notice: None,
            };
        }
        if disc.abs() <= round {
            let r = -b / (2.0 * a);
            return RealRoots {
                roots: if r.is_finite() { vec![r] } else { Vec::new() },
                method: SolveMethod::QuadraticExact,
                complex_count: 0,
                notice: Some("raíz doble".to_string()),
            };
        }
        let sqrt_d = disc.sqrt();
        let q = -0.5 * (b + sqrt_d.copysign(b));
        let mut roots = if q == 0.0 {
            vec![0.0]
        } else {
            vec![q / a, c / q]
        };
        roots.retain(|r| r.is_finite());
        roots.sort_by(f64::total_cmp);
        roots.dedup();
        return RealRoots {
            roots,
            method: SolveMethod::QuadraticExact,
            complex_count: 0,
            notice: None,
        };
    }
    let bound = cauchy_bound(&coeffs, degree);
    let mut roots: Vec<f64> = sturm_isolate(&coeffs, degree, bound)
        .into_iter()
        .filter(|r| r.is_finite() && residual_small(&coeffs, degree, *r))
        .collect();
    roots.sort_by(f64::total_cmp);
    roots.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs())));
    let complex_count = degree.saturating_sub(roots.len());
    let notice = if degree >= 5 {
        Some(format!(
            "grado {degree} ≥ 5: solo raíces reales por aislamiento numérico acotado (sin fórmula cerrada)"
        ))
    } else {
        None
    };
    RealRoots {
        roots,
        method: SolveMethod::SturmBisection,
        complex_count,
        notice,
    }
}

// ---------------------------------------------------------------------------
// API pública 1-var
// ---------------------------------------------------------------------------

/// `Solve[expr, var]` general: todas las raíces reales del polinomio.
///
/// `expr` admite `lhs = rhs` (se resta). Trascendentes/mixtos y grado >
/// `MAX_SOLVE_DEGREE` devuelven `Err` honesto (derivan a `NSolve`).
pub fn solve_all_real(expr: &str, var: &str) -> Result<RealRoots, SolveError> {
    if expr.len() > MAX_SOLVE_BYTES {
        return Err(SolveError::InvalidInput {
            detail: format!(
                "expresión de {} bytes excede el máximo {MAX_SOLVE_BYTES}",
                expr.len()
            ),
        });
    }
    if !valid_var(var) {
        return Err(SolveError::InvalidInput {
            detail: format!("variable '{var}' no es un identificador válido"),
        });
    }
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(SolveError::InvalidInput {
            detail: "expresión vacía".to_string(),
        });
    }
    // `lhs = rhs` → `lhs - rhs` (igual que el brazo `Solve` en comandos).
    let mut owned = trimmed.to_string();
    if let Some((lhs, rhs)) = split_top_eq(&owned) {
        owned = format!("({lhs}) - ({rhs})");
    }
    let parsed = parse_ast(&preprocess_expr(&owned)).map_err(|e| SolveError::InvalidInput {
        detail: format!("no se pudo parsear la expresión: {e}"),
    })?;
    if !contains_var(&parsed, var) {
        if crate::symbolic::is_identically_zero(&parsed) {
            return Ok(RealRoots {
                roots: Vec::new(),
                method: SolveMethod::LinearExact,
                complex_count: 0,
                notice: Some("identidad: se cumple para todo valor".to_string()),
            });
        }
        return Ok(RealRoots {
            roots: Vec::new(),
            method: SolveMethod::LinearExact,
            complex_count: 0,
            notice: Some("constante no nula en la variable: sin raíces".to_string()),
        });
    }
    if let Some(coeffs) = collect_coeffs(&parsed, var, MAX_SOLVE_DEGREE) {
        return Ok(real_roots_of_coeffs(&coeffs));
    }
    // Distingue grado-excedido (honesto con número) de no-polinómico.
    if let Some(wide) = collect_coeffs(&parsed, var, MAX_DEGREE_PROBE) {
        let degree = poly_deg(&wide);
        return Err(SolveError::DegreeExceeded {
            degree,
            maximum: MAX_SOLVE_DEGREE,
        });
    }
    Err(SolveError::NotPolynomial {
        hint: format!(
            "no es polinomio en '{var}' (trascendente o mixto); usa NSolve[expr, {var}, a, b] para 1 raíz numérica"
        ),
    })
}

/// Parte `lhs = rhs` en el `=` de nivel superior (sin paréntesis).
fn split_top_eq(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 => {
                if bytes.get(i + 1) == Some(&b'=') || bytes.get(i + 1) == Some(&b'>') {
                    continue;
                }
                if i > 0
                    && matches!(
                        bytes.get(i.saturating_sub(1)),
                        Some(b'=') | Some(b'!') | Some(b'<') | Some(b'>')
                    )
                {
                    continue;
                }
                return Some((text[..i].trim(), text[i + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

/// Formatea una raíz: entero si está a `1e-9`, si no 6 decimales.
pub fn format_root(root: f64) -> String {
    let rounded = root.round();
    if (root - rounded).abs() <= 1e-9 && rounded.abs() < 1e15 {
        format!("{rounded}")
    } else {
        format!("{root:.6}")
    }
}

/// `{r1, r2} (método) + avisos` para el brazo `Solve` y la puerta CAS.
pub fn format_real_roots(roots: &RealRoots) -> String {
    let body = roots
        .roots
        .iter()
        .map(|r| format_root(*r))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!("{{{body}}}");
    if roots.roots.is_empty() {
        out.push_str(" (sin raíces reales");
        if roots.complex_count > 0 {
            out.push_str(&format!("; {} complejas no reales", roots.complex_count));
        }
        out.push(')');
    } else {
        out.push_str(&format!(" ({})", roots.method));
    }
    if let Some(notice) = &roots.notice {
        out.push_str(&format!("; aviso: {notice}"));
    }
    out
}

// ---------------------------------------------------------------------------
// Lineal exacto vía Gauss (`matrices.rs`)
// ---------------------------------------------------------------------------

/// Sistema lineal cuadrado `A·x = b` por Gauss con pivoteo (`matrices.rs`).
///
/// `None` del motor (singular) → `Unsupported` honesto, no pánico.
pub fn solve_linear_system_exact(a: &[Vec<f64>], b: &[f64]) -> Result<Vec<f64>, SolveError> {
    let n = b.len();
    if n == 0 || n > MAX_LINEAR_DIM {
        return Err(SolveError::InvalidInput {
            detail: format!("dimensión {n} fuera de 1..={MAX_LINEAR_DIM}"),
        });
    }
    if a.len() != n || a.iter().any(|row| row.len() != n) {
        return Err(SolveError::InvalidInput {
            detail: "la matriz debe ser cuadrada n×n con b de largo n".to_string(),
        });
    }
    if !b.iter().all(|v| v.is_finite()) || !a.iter().flatten().all(|v| v.is_finite()) {
        return Err(SolveError::InvalidInput {
            detail: "coeficientes no finitos".to_string(),
        });
    }
    let mat_a = Matrix::from_rows(a.to_vec()).ok_or(SolveError::InvalidInput {
        detail: "matriz fuera de presupuesto".to_string(),
    })?;
    let rows_b: Vec<Vec<f64>> = b.iter().map(|v| vec![*v]).collect();
    let mat_b = Matrix::from_rows(rows_b).ok_or(SolveError::InvalidInput {
        detail: "vector fuera de presupuesto".to_string(),
    })?;
    let sol = solve_linear_system(&mat_a, &mat_b).ok_or(SolveError::Unsupported {
        hint: "sistema singular o degenerado; sin solución única (verifica rango con Rank[...])"
            .to_string(),
    })?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let v = sol.get(i, 0);
        if !v.is_finite() {
            return Err(SolveError::Unsupported {
                hint: "solución no finita; sistema mal condicionado".to_string(),
            });
        }
        out.push(v);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Sistema poli 2×2 por eliminación (resultante de Sylvester interpolada)
// ---------------------------------------------------------------------------

type BiPoly = HashMap<(u32, u32), f64>;

fn collect_bi(ast: &Expr, x: &str, y: &str) -> Option<BiPoly> {
    fn add(a: BiPoly, b: &BiPoly, sign: f64) -> Option<BiPoly> {
        let mut out = a;
        for (k, v) in b {
            let entry = out.entry(*k).or_insert(0.0);
            *entry += sign * v;
            if !entry.is_finite() {
                return None;
            }
            if *entry == 0.0 {
                out.remove(k);
            }
        }
        if out.len() > MAX_SYSTEM_TERMS {
            return None;
        }
        Some(out)
    }
    fn mul(a: &BiPoly, b: &BiPoly) -> Option<BiPoly> {
        let mut out: BiPoly = HashMap::new();
        for ((ix1, iy1), c1) in a {
            for ((ix2, iy2), c2) in b {
                let kx = ix1.checked_add(*ix2)?;
                let ky = iy1.checked_add(*iy2)?;
                if kx + ky > MAX_SYSTEM_DEGREE as u32 {
                    return None;
                }
                let entry = out.entry((kx, ky)).or_insert(0.0);
                *entry += c1 * c2;
                if !entry.is_finite() {
                    return None;
                }
            }
        }
        if out.len() > MAX_SYSTEM_TERMS {
            return None;
        }
        Some(out)
    }
    fn go(e: &Expr, x: &str, y: &str) -> Option<BiPoly> {
        match e {
            Expr::Const(v) if v.is_finite() => {
                let mut m = BiPoly::new();
                if *v != 0.0 {
                    m.insert((0, 0), *v);
                }
                Some(m)
            }
            Expr::Var(name) if name == x => {
                let mut m = BiPoly::new();
                m.insert((1, 0), 1.0);
                Some(m)
            }
            Expr::Var(name) if name == y => {
                let mut m = BiPoly::new();
                m.insert((0, 1), 1.0);
                Some(m)
            }
            Expr::Var(_) => None,
            Expr::Neg(inner) => {
                let m = go(inner, x, y)?;
                m.into_iter()
                    .map(|(k, c)| (-c).is_finite().then_some((k, -c)))
                    .collect()
            }
            Expr::Add(l, r) => add(go(l, x, y)?, &go(r, x, y)?, 1.0),
            Expr::Sub(l, r) => add(go(l, x, y)?, &go(r, x, y)?, -1.0),
            Expr::Mul(l, r) => mul(&go(l, x, y)?, &go(r, x, y)?),
            Expr::Div(n, d) => {
                let num = go(n, x, y)?;
                let den = go(d, x, y)?;
                if den.len() != 1 {
                    return None;
                }
                let ((ex, ey), q) = den.into_iter().next()?;
                if ex != 0 || ey != 0 || q == 0.0 || !q.is_finite() {
                    return None;
                }
                num.into_iter()
                    .map(|(k, c)| {
                        let v = c / q;
                        v.is_finite().then_some((k, v))
                    })
                    .collect()
            }
            Expr::Pow(base, exp) => {
                let Expr::Const(n) = exp.as_ref() else {
                    return None;
                };
                if !n.is_finite() || *n < 0.0 || n.fract() != 0.0 || *n > MAX_SYSTEM_DEGREE as f64 {
                    return None;
                }
                let b = go(base, x, y)?;
                let mut acc: BiPoly = HashMap::new();
                acc.insert((0, 0), 1.0);
                for _ in 0..(*n as usize) {
                    acc = mul(&acc, &b)?;
                }
                acc.remove(&(0, 0));
                if acc.is_empty() {
                    let mut one = BiPoly::new();
                    one.insert((0, 0), 1.0);
                    return Some(one);
                }
                Some(acc)
            }
            _ => None,
        }
    }
    go(ast, x, y)
}

fn eval_bi(p: &BiPoly, x: f64, y: f64) -> f64 {
    let mut acc = 0.0_f64;
    for ((ix, iy), c) in p {
        acc += c * x.powi(*ix as i32) * y.powi(*iy as i32);
    }
    acc
}

fn bi_scale(p: &BiPoly) -> f64 {
    p.values().fold(0.0_f64, |s, c| s + c.abs())
}

/// Coeficientes en `y` evaluados en `x`: `c[j] = Σᵢ p(i,j)·xⁱ`.
fn y_coeffs_at(p: &BiPoly, x: f64) -> Vec<f64> {
    let max_j = p.keys().map(|(_, j)| *j).max().unwrap_or(0) as usize;
    let mut out = vec![0.0; max_j + 1];
    for ((ix, iy), c) in p {
        if let Some(slot) = out.get_mut(*iy as usize) {
            *slot += c * x.powi(*ix as i32);
        }
    }
    while out.len() > 1 && out.last().is_some_and(|c| *c == 0.0) {
        out.pop();
    }
    out
}

fn y_degree(p: &BiPoly) -> usize {
    p.keys().map(|(_, j)| *j as usize).max().unwrap_or(0)
}

fn x_degree(p: &BiPoly) -> usize {
    p.keys().map(|(i, _)| *i as usize).max().unwrap_or(0)
}

/// Determinante numérico por Gauss con pivoteo; `None` si excede presupuesto.
fn numeric_det(mat: &[Vec<f64>]) -> Option<f64> {
    let n = mat.len();
    if n == 0 || n > 8 || mat.iter().any(|r| r.len() != n) {
        return None;
    }
    let mut a = mat.to_vec();
    let mut det = 1.0_f64;
    for col in 0..n {
        let mut piv = col;
        for row in (col + 1)..n {
            if a[row][col].abs() > a[piv][col].abs() {
                piv = row;
            }
        }
        if piv != col {
            a.swap(piv, col);
            det = -det;
        }
        let d = a[col][col];
        if !d.is_finite() || d.abs() <= DET_TOL {
            return Some(0.0);
        }
        det *= d;
        for row in (col + 1)..n {
            let factor = a[row][col] / d;
            if !factor.is_finite() {
                return None;
            }
            let (head, tail) = a.split_at_mut(row);
            let pivot_row = &head[col];
            let target = &mut tail[0];
            for (slot, pivot) in target.iter_mut().skip(col).zip(pivot_row.iter().skip(col)) {
                *slot -= factor * pivot;
            }
        }
    }
    if det.is_finite() {
        Some(det)
    } else {
        None
    }
}

/// Entrada `(fila, col)` de Sylvester en `y` evaluada en `x`.
fn sylvester_entry(
    f1: &BiPoly,
    f2: &BiPoly,
    m: usize,
    n: usize,
    row: usize,
    col: usize,
    x: f64,
) -> f64 {
    // Filas 0..n: `y^shift * f1`; filas n..: `y^shift * f2`.
    let (poly, shift, deg) = if row < n {
        (f1, n - 1 - row, m)
    } else {
        (f2, m + n - 1 - row, n)
    };
    if col < shift {
        return 0.0;
    }
    let pow = col - shift;
    if pow > deg {
        return 0.0;
    }
    let mut acc = 0.0_f64;
    for ((ix, iy), c) in poly {
        if *iy as usize == pow {
            acc += c * x.powi(*ix as i32);
        }
    }
    acc
}

/// Resultante `Res_y(f1, f2)(x)` por interpolación de determinantes.
fn sylvester_resultant(f1: &BiPoly, f2: &BiPoly, m: usize, n: usize) -> Option<Vec<f64>> {
    let size = m + n;
    if size == 0 || size > 8 {
        return None;
    }
    let deg_bound = m
        .checked_mul(x_degree(f2))?
        .checked_add(n.checked_mul(x_degree(f1))?)?;
    if deg_bound > 32 {
        return None;
    }
    // Puntos de muestreo: 0, 1, -1, 2, -2, ...
    let mut xs = Vec::with_capacity(deg_bound + 1);
    let mut k = 0_i32;
    while xs.len() <= deg_bound {
        xs.push(f64::from(k));
        if k <= 0 {
            k = -k + 1;
        } else {
            k = -k;
        }
    }
    let mut vals = Vec::with_capacity(xs.len());
    let mut peak = 0.0_f64;
    for x in &xs {
        let mut mat = vec![vec![0.0; size]; size];
        for (r, row) in mat.iter_mut().enumerate() {
            for (c, slot) in row.iter_mut().enumerate() {
                *slot = sylvester_entry(f1, f2, m, n, r, c, *x);
            }
        }
        let d = numeric_det(&mat)?;
        if d.is_finite() {
            peak = peak.max(d.abs());
        }
        vals.push(d);
    }
    if peak <= DET_TOL {
        return None; // resultante idénticamente nula: dependientes.
    }
    // Lagrange → base monomial (O(D³), D ≤ 32).
    let d = deg_bound;
    let mut coeffs = vec![0.0; d + 1];
    for (k, xk) in xs.iter().enumerate() {
        let yk = vals.get(k).copied().unwrap_or(0.0);
        if yk == 0.0 {
            continue;
        }
        let mut basis = vec![0.0; d + 1];
        basis[0] = 1.0;
        let mut denom = 1.0_f64;
        let mut blen = 1_usize;
        let mut bad = false;
        for (j, xj) in xs.iter().enumerate() {
            if j == k {
                continue;
            }
            denom *= xk - xj;
            let mut next = vec![0.0; d + 1];
            for (i, b) in basis.iter().enumerate().take(blen) {
                if let Some(slot) = next.get_mut(i + 1) {
                    *slot += b;
                }
                if let Some(slot) = next.get_mut(i) {
                    *slot -= b * xj;
                }
            }
            basis = next;
            blen = (blen + 1).min(d + 1);
            if !denom.is_finite() {
                bad = true;
                break;
            }
        }
        if bad || !denom.is_finite() || denom == 0.0 {
            continue;
        }
        for (i, b) in basis.iter().enumerate() {
            if let Some(slot) = coeffs.get_mut(i) {
                *slot += yk * b / denom;
            }
        }
    }
    if coeffs.iter().all(|c| !c.is_finite()) {
        return None;
    }
    Some(coeffs)
}

fn verify_point(f1: &BiPoly, f2: &BiPoly, x: f64, y: f64) -> bool {
    if !(x.is_finite() && y.is_finite()) {
        return false;
    }
    let scale = 1.0 + bi_scale(f1).max(bi_scale(f2)) * (1.0 + x.abs().powi(4) + y.abs().powi(4));
    eval_bi(f1, x, y).abs() <= RESIDUAL_TOL * scale
        && eval_bi(f2, x, y).abs() <= RESIDUAL_TOL * scale
}

/// Un paso Newton 2D con jacobiano numérico.
fn newton_2d_step(f1: &BiPoly, f2: &BiPoly, x: f64, y: f64) -> Option<(f64, f64)> {
    let h = 1e-7 * (1.0 + x.abs().max(y.abs()));
    let f1v = eval_bi(f1, x, y);
    let f2v = eval_bi(f2, x, y);
    if !(f1v.is_finite() && f2v.is_finite()) {
        return None;
    }
    let j11 = (eval_bi(f1, x + h, y) - eval_bi(f1, x - h, y)) / (2.0 * h);
    let j12 = (eval_bi(f1, x, y + h) - eval_bi(f1, x, y - h)) / (2.0 * h);
    let j21 = (eval_bi(f2, x + h, y) - eval_bi(f2, x - h, y)) / (2.0 * h);
    let j22 = (eval_bi(f2, x, y + h) - eval_bi(f2, x, y - h)) / (2.0 * h);
    let det = j11 * j22 - j12 * j21;
    if !det.is_finite() || det.abs() <= 1e-14 * (j11.abs() + j12.abs() + j21.abs() + j22.abs()) {
        return None;
    }
    let dx = (j22 * f1v - j12 * f2v) / det;
    let dy = (j11 * f2v - j21 * f1v) / det;
    if !(dx.is_finite() && dy.is_finite()) {
        return None;
    }
    Some((x - dx, y - dy))
}

fn polish_point(f1: &BiPoly, f2: &BiPoly, mut x: f64, mut y: f64) -> Option<(f64, f64)> {
    // Refina aunque el candidato inicial ya verifique: la resultante
    // interpolada deja errores ~1e-6 que Newton 2D lleva a ~1e-12.
    let mut best = verify_point(f1, f2, x, y).then_some((x, y));
    for _ in 0..MAX_NEWTON_2D_ITER {
        let (nx, ny) = newton_2d_step(f1, f2, x, y)?;
        let step = (nx - x).abs() + (ny - y).abs();
        x = nx;
        y = ny;
        if verify_point(f1, f2, x, y) {
            best = Some((x, y));
        }
        if step <= 1e-13 * (1.0 + x.abs() + y.abs()) {
            break;
        }
    }
    best
}

fn parse_system_eq(eq: &str, x: &str, y: &str) -> Result<BiPoly, SolveError> {
    let trimmed = eq.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_SOLVE_BYTES {
        return Err(SolveError::InvalidInput {
            detail: "ecuación vacía o mayor a 2000 bytes".to_string(),
        });
    }
    let mut owned = trimmed.to_string();
    if let Some((lhs, rhs)) = split_top_eq(&owned) {
        owned = format!("({lhs}) - ({rhs})");
    }
    let ast = parse_ast(&preprocess_expr(&owned)).map_err(|e| SolveError::InvalidInput {
        detail: format!("no se pudo parsear '{trimmed}': {e}"),
    })?;
    collect_bi(&ast, x, y).ok_or(SolveError::Unsupported {
        hint: format!(
            "ecuación '{trimmed}' no polinómica o de grado total > {MAX_SYSTEM_DEGREE}; usa NSolve[..] o Eliminate[..]"
        ),
    })
}

/// Sistema polinómico 2×2 → puntos reales verificados por residuo.
///
/// Eliminación por resultante de Sylvester interpolada + sustitución +
/// Newton 2D. Fallos honestos derivan a `Groebner`/`Eliminate`.
pub fn solve_system_2x2(
    eq1: &str,
    eq2: &str,
    x: &str,
    y: &str,
) -> Result<Vec<(f64, f64)>, SolveError> {
    for v in [x, y] {
        if !valid_var(v) {
            return Err(SolveError::InvalidInput {
                detail: format!("variable '{v}' no es un identificador válido"),
            });
        }
    }
    if x == y {
        return Err(SolveError::InvalidInput {
            detail: "las dos incógnitas deben ser distintas".to_string(),
        });
    }
    let f1 = parse_system_eq(eq1, x, y)?;
    let f2 = parse_system_eq(eq2, x, y)?;
    if f1.is_empty() || f2.is_empty() {
        return Err(SolveError::Unsupported {
            hint: "sistema nulo o vacío; nada que resolver".to_string(),
        });
    }
    let (m, n) = (y_degree(&f1), y_degree(&f2));
    // Caso triangular: una ecuación sin `y` → 1-var + sustitución.
    if m == 0 || n == 0 {
        let uni = if m == 0 { &f1 } else { &f2 };
        let mut coeffs = vec![0.0; x_degree(uni) + 1];
        for ((ix, iy), c) in uni {
            if *iy == 0 {
                if let Some(slot) = coeffs.get_mut(*ix as usize) {
                    *slot += c;
                }
            }
        }
        let xs = real_roots_of_coeffs(&coeffs);
        return finish_system_points(&f1, &f2, &xs.roots);
    }
    let res = sylvester_resultant(&f1, &f2, m, n).ok_or(SolveError::Unsupported {
        hint: "eliminación degenerada (ecuaciones dependientes o resultante nula); usa Groebner[..] o Eliminate[..]".to_string(),
    })?;
    let xs = real_roots_of_coeffs(&res);
    finish_system_points(&f1, &f2, &xs.roots)
}

fn finish_system_points(
    f1: &BiPoly,
    f2: &BiPoly,
    xs: &[f64],
) -> Result<Vec<(f64, f64)>, SolveError> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for x in xs.iter().take(MAX_SYSTEM_POINTS) {
        if !x.is_finite() {
            continue;
        }
        // Coeficientes en y de ambas ecuaciones en x; usa la de mayor grado
        // no degenerada primero y une candidatas de la otra.
        let c1 = y_coeffs_at(f1, *x);
        let c2 = y_coeffs_at(f2, *x);
        let (first, second) = if poly_deg(&c1) >= poly_deg(&c2) {
            (&c1, &c2)
        } else {
            (&c2, &c1)
        };
        let mut ys: Vec<f64> = real_roots_of_coeffs(first).roots;
        for r in real_roots_of_coeffs(second).roots {
            if !ys.iter().any(|v| (*v - r).abs() <= 1e-7 * (1.0 + v.abs())) {
                ys.push(r);
            }
        }
        for y in ys {
            if !y.is_finite() {
                continue;
            }
            let polished = polish_point(f1, f2, *x, y).unwrap_or((*x, y));
            if verify_point(f1, f2, polished.0, polished.1)
                && !points.iter().any(|(px, py)| {
                    (px - polished.0).abs() <= 1e-6 * (1.0 + px.abs())
                        && (py - polished.1).abs() <= 1e-6 * (1.0 + py.abs())
                })
            {
                points.push(polished);
                if points.len() >= MAX_SYSTEM_POINTS {
                    break;
                }
            }
        }
        if points.len() >= MAX_SYSTEM_POINTS {
            break;
        }
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    Ok(points)
}

/// `{(x1, y1), (x2, y2)}` con enteros compactos; `{}` si vacío.
pub fn format_system_points(points: &[(f64, f64)]) -> String {
    if points.is_empty() {
        return "{} (sin puntos reales verificados)".to_string();
    }
    let body = points
        .iter()
        .map(|(x, y)| format!("({}, {})", format_root(*x), format_root(*y)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{body}}}")
}

// ---------------------------------------------------------------------------
// Tests (permiten `unwrap`/`expect` solo aquí)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_acceptance_all_three_roots() {
        let r = solve_all_real("x^3-6x^2+11x-6", "x").expect("cúbica B1");
        assert_eq!(r.roots.len(), 3);
        for (got, want) in r.roots.iter().zip([1.0, 2.0, 3.0]) {
            assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
        }
        assert_eq!(format_real_roots(&r), "{1, 2, 3} (Sturm+bisección+Newton)");
    }

    #[test]
    fn quadratic_complex_is_empty_with_notice() {
        let r = solve_all_real("x^2+1", "x").expect("cuadrática");
        assert!(r.roots.is_empty());
        assert_eq!(r.complex_count, 2);
        let msg = format_real_roots(&r);
        assert!(msg.contains("{}"), "got {msg}");
        assert!(msg.contains("complej"), "got {msg}");
    }

    #[test]
    fn linear_and_quadratic_exact() {
        let l = solve_all_real("2*x-6", "x").expect("lineal");
        assert_eq!(l.method, SolveMethod::LinearExact);
        assert!((l.roots[0] - 3.0).abs() < 1e-12);
        let q = solve_all_real("x^2-5*x+6", "x").expect("cuadrática");
        assert_eq!(q.method, SolveMethod::QuadraticExact);
        assert_eq!(q.roots.len(), 2);
    }

    #[test]
    fn quintic_bounded_numeric_with_notice() {
        let r = solve_all_real("x^5-x-1", "x").expect("quíntica acotada");
        assert_eq!(r.roots.len(), 1);
        assert!((r.roots[0] - 1.167_303_978).abs() < 1e-6);
        assert!(r.notice.is_some_and(|n| n.contains("≥ 5")));
    }

    #[test]
    fn transcendental_is_honest_error() {
        let err = solve_all_real("sin(x)+x^2", "x").expect_err("trascendente");
        assert!(matches!(err, SolveError::NotPolynomial { .. }), "got {err}");
        assert!(format!("{err}").contains("NSolve"));
    }

    #[test]
    fn degree_exceeded_is_honest() {
        let err = solve_all_real("x^17-1", "x").expect_err("grado 17");
        assert!(
            matches!(err, SolveError::DegreeExceeded { degree: 17, .. }),
            "got {err}"
        );
    }

    #[test]
    fn linear_system_via_gauss() {
        let sol = solve_linear_system_exact(&[vec![2.0, 1.0], vec![1.0, 3.0]], &[5.0, 10.0])
            .expect("Gauss B1");
        assert!((sol[0] - 1.0).abs() < 1e-12);
        assert!((sol[1] - 3.0).abs() < 1e-12);
        let err = solve_linear_system_exact(&[vec![1.0, 1.0], vec![2.0, 2.0]], &[1.0, 3.0])
            .expect_err("singular");
        assert!(matches!(err, SolveError::Unsupported { .. }));
    }

    #[test]
    fn nonlinear_system_circle_line() {
        let pts = solve_system_2x2("x^2+y^2-25", "x-y-1", "x", "y").expect("círculo+recta");
        assert_eq!(pts.len(), 2, "got {pts:?}");
        assert!(pts
            .iter()
            .any(|(x, y)| (x - 4.0).abs() < 1e-6 && (y - 3.0).abs() < 1e-6));
        assert!(pts
            .iter()
            .any(|(x, y)| (x + 3.0).abs() < 1e-6 && (y + 4.0).abs() < 1e-6));
        assert_eq!(format_system_points(&pts), "{(-3, -4), (4, 3)}");
    }

    #[test]
    fn system_triangular_path() {
        let pts = solve_system_2x2("x^2-4", "y-x-1", "x", "y").expect("triangular");
        assert_eq!(pts.len(), 2, "got {pts:?}");
    }

    #[test]
    fn system_dependent_is_honest() {
        let err = solve_system_2x2("x+y-1", "2*x+2*y-2", "x", "y").expect_err("dependiente");
        assert!(matches!(err, SolveError::Unsupported { .. }), "got {err}");
    }

    #[test]
    fn invalid_inputs_rejected() {
        assert!(solve_all_real("", "x").is_err());
        assert!(solve_all_real("x+1", "1x").is_err());
        assert!(solve_all_real("x+1", "x").is_ok());
        assert!(solve_system_2x2("x", "y", "x", "x").is_err());
    }
}
