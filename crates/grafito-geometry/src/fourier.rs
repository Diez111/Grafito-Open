//! Series de Fourier sobre la representación única `Expr`/bytecode de Grafito.
//!
//! Regla de diseño del repo: un solo AST (`crate::ast::Expr`) y un solo
//! bytecode (`crate::expr`). Toda evaluación caliente va por
//! [`compile_flat_ops`](crate::expr::compile_flat_ops) →
//! [`eval_opcodes_flat_raw`](crate::expr::eval_opcodes_flat_raw) (que ejecuta
//! `run_opcodes_flat`), y los coeficientes "simbólicos" salen de la
//! antiderivada del mismo `Expr` (nunca de una representación paralela).
//!
//! # Convención de período
//!
//! `period` es el **período completo** `T` de la extensión periódica; la
//! serie se desarrolla sobre el intervalo simétrico `[-T/2, T/2]` con
//! `L = T/2` y frecuencias `n·π/L = 2πn/T`:
//!
//! ```text
//! f(x) ≈ a0/2 + Σ_{n=1..N} [ an·cos(nπx/L) + bn·sin(nπx/L) ]
//! an = (1/L)·∫_{-L}^{L} f(x)·cos(nπx/L) dx
//! bn = (1/L)·∫_{-L}^{L} f(x)·sin(nπx/L) dx
//! ```
//!
//! Para `f(x) = x` en `[-π, π]` (`period = 2π`) esto da
//! `bn = 2(−1)^{n+1}/n`, la serie clásica `2·Σ (−1)^{n+1}·sin(nx)/n`.
//!
//! # Presupuestos
//!
//! - [`MAX_FOURIER_TERMS`] 128 términos.
//! - [`MAX_FOURIER_PANELS`] 512 paneles de cuadratura por coeficiente,
//!   Gauss–Legendre de orden [`GAUSS_LEGENDRE_ORDER`] 16 (+ 8 para la
//!   estimación de error) ⇒ ≤ 12 288 evaluaciones por coeficiente.
//! - La cuadratura que no converge (error estimado >
//!   [`QUAD_TOLERANCE`]·(1+|I|)) es error honesto, no un número inventado.

use crate::ast::{parse_ast, Expr};
use crate::expr::{
    compile_flat_ops, eval_opcodes_flat_raw, preprocess_expr, ValidatedOps, MAX_EXPR_LENGTH,
};
use num_complex::Complex64;
use std::sync::OnceLock;

/// Máximo de términos de la serie truncada (y de coeficientes por eje).
pub const MAX_FOURIER_TERMS: usize = 128;
/// Máximo de paneles de Gauss–Legendre compuesto por integral.
pub const MAX_FOURIER_PANELS: usize = 512;
/// Orden de Gauss–Legendre por panel (la estimación de error usa orden 8).
pub const GAUSS_LEGENDRE_ORDER: usize = 16;
/// Tolerancia relativa de cuadratura por coeficiente.
pub const QUAD_TOLERANCE: f64 = 1e-8;
/// Verificación simbólico-vs-numérico: tolerancia relativa de los sondeos.
const SYMBOLIC_VERIFY_TOLERANCE: f64 = 1e-6;

// ---------------------------------------------------------------------------
// Nodos de Gauss–Legendre (computados por Newton sobre P_n, sin tabla)
// ---------------------------------------------------------------------------

/// Nodos y pesos de Gauss–Legendre de orden [`GAUSS_LEGENDRE_ORDER`] en `[-1,1]`.
fn gauss_legendre() -> &'static [(f64, f64)] {
    static NODES: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    NODES.get_or_init(|| compute_gauss_legendre(GAUSS_LEGENDRE_ORDER))
}

/// Nodos y pesos de Gauss–Legendre de orden 8 en `[-1,1]` (estimación de error).
fn gauss_legendre_coarse() -> &'static [(f64, f64)] {
    static NODES: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    NODES.get_or_init(|| compute_gauss_legendre(8))
}

/// `(P_n(x), P_{n−1}(x))` por la recurrencia de Bonnet:
/// `(k+1)·P_{k+1} = (2k+1)·x·P_k − k·P_{k−1}`.
fn legendre_pair(order: usize, x: f64) -> (f64, f64) {
    let (mut p_k, mut p_km1) = (1.0, 0.0);
    for k in 0..order {
        let p_next = ((2 * k + 1) as f64 * x * p_k - k as f64 * p_km1) / (k + 1) as f64;
        p_km1 = p_k;
        p_k = p_next;
    }
    (p_k, p_km1)
}

/// Raíces de `P_n` por Newton (aproximación cosenoidal de Chebyshev) y pesos
/// `2/((1−x²)·P'_n(x)²)` con `P'_n(x) = n/(x²−1)·(x·P_n(x) − P_{n−1}(x))`.
/// Determinista: misma entrada ⇒ mismos nodos.
fn compute_gauss_legendre(order: usize) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(order);
    for i in 0..order {
        let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (order as f64 + 0.5)).cos();
        for _ in 0..64 {
            let (p, p_km1) = legendre_pair(order, x);
            let dp = order as f64 / (x * x - 1.0) * (x * p - p_km1);
            let dx = p / dp;
            x -= dx;
            if dx.abs() <= 1e-15 {
                break;
            }
        }
        let (p, p_km1) = legendre_pair(order, x);
        let dp = order as f64 / (x * x - 1.0) * (x * p - p_km1);
        let weight = 2.0 / ((1.0 - x * x) * dp * dp);
        out.push((x, weight));
    }
    out
}

// ---------------------------------------------------------------------------
// Evaluador compilado (bytecode plano de `crate::expr`)
// ---------------------------------------------------------------------------

/// Función univariada compilada una vez al bytecode plano de Grafito.
///
/// Es el mismo pipeline que `evaluate_cached` (`src/expr.rs:1019`):
/// `preprocess_expr` → `parse_ast` → `compile_flat_ops` (`src/expr.rs:2256`)
/// y evaluación con `run_opcodes_flat` (`src/expr.rs:1976`) vía
/// [`eval_opcodes_flat_raw`](crate::expr::eval_opcodes_flat_raw). Sin
/// re-parseo por muestra.
pub struct QuadFn {
    ops: ValidatedOps,
    var: String,
}

impl QuadFn {
    /// Compila `expr` como función de `var` (única variable admitida).
    pub fn new(expr: &str, var: &str) -> Result<Self, String> {
        let ast = parse_univariate(expr, var)?;
        let ops = compile_flat_ops(&ast, var, "", "")
            .ok_or_else(|| format!("no se pudo compilar '{expr}' a bytecode plano"))?;
        Ok(Self {
            ops,
            var: var.to_string(),
        })
    }

    /// Evalúa `f(x)` en el bytecode plano (`run_opcodes_flat`).
    pub fn eval(&self, x: f64) -> f64 {
        eval_opcodes_flat_raw(&self.ops, x, 0.0, 0.0)
    }

    /// Variable de barrido.
    pub fn var(&self) -> &str {
        &self.var
    }
}

/// Parsea `expr` exigiendo que su única variable sea `var`.
fn parse_univariate(expr: &str, var: &str) -> Result<Expr, String> {
    if expr.trim().is_empty() {
        return Err("expresión vacía".to_string());
    }
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "la expresión excede {MAX_EXPR_LENGTH} bytes (got {})",
            expr.len()
        ));
    }
    let clean = preprocess_expr(expr);
    let ast = parse_ast(&clean).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let mut vars = std::collections::HashSet::new();
    ast.get_variables(&mut vars);
    if let Some(other) = vars.iter().find(|v| v.as_str() != var) {
        return Err(format!(
            "la expresión '{expr}' tiene la variable ajena '{other}' (se esperaba solo '{var}')"
        ));
    }
    Ok(ast)
}

/// Función preparada: bytecode para muestreo + AST para la ruta simbólica.
pub(crate) struct PreparedFn {
    pub(crate) quad: QuadFn,
    pub(crate) ast: Expr,
    pub(crate) var: String,
}

impl PreparedFn {
    pub(crate) fn new(expr: &str, var: &str) -> Result<Self, String> {
        let ast = parse_univariate(expr, var)?;
        let quad = QuadFn::new(expr, var)?;
        Ok(Self {
            quad,
            ast,
            var: var.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Núcleo de cuadratura con kernel
// ---------------------------------------------------------------------------

/// Kernel del producto contra el que se proyecta `f`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kernel {
    /// `1` (modo constante).
    One,
    /// `cos(k·x)`.
    Cos,
    /// `sin(k·x)`.
    Sin,
}

/// Conjunto de coeficientes con su origen epistémico y error estimado.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoeffSet {
    pub(crate) values: Vec<f64>,
    pub(crate) source: CoeffSource,
    pub(crate) error_estimate: f64,
}

/// Origen de los coeficientes de Fourier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoeffSource {
    /// Derivados de una antiderivada simbólica exacta del `Expr` (verificados
    /// contra cuadratura numérica en sondeos).
    Symbolic,
    /// Cuadratura numérica Gauss–Legendre compuesta sobre el bytecode.
    Numeric,
}

/// `∫_a^b f(x)·kernel(k·(x − shift)) dx` con Gauss–Legendre compuesto
/// (orden 16) y estimación de error por comparación con orden 8 (cota
/// conservadora del error del orden 16).
///
/// `Err` honesto si alguna muestra no es finita o si el error estimado supera
/// [`QUAD_TOLERANCE`]·(1+|I|).
pub(crate) fn kernel_integral(
    f: &QuadFn,
    kernel: Kernel,
    k: f64,
    shift: f64,
    a: f64,
    b: f64,
) -> Result<(f64, f64), String> {
    let waves = k.abs() * (b - a) / (2.0 * std::f64::consts::PI);
    let panels = ((waves * 4.0).ceil() as usize).clamp(4, MAX_FOURIER_PANELS);
    let fine = gauss_legendre();
    let coarse = gauss_legendre_coarse();
    let h = (b - a) / panels as f64;
    let mut total_fine = 0.0;
    let mut total_coarse = 0.0;
    for p in 0..panels {
        let mid = a + (p as f64 + 0.5) * h;
        let half = h / 2.0;
        let mut s_fine = 0.0;
        let mut s_coarse = 0.0;
        for (xi, wi) in fine {
            let x = mid + half * xi;
            let fx = f.eval(x);
            if !fx.is_finite() {
                return Err(format!(
                    "la función no es integrable en la grilla de cuadratura (muestra no finita en x = {x})"
                ));
            }
            let w = kernel_weight(kernel, k, x - shift);
            s_fine += wi * fx * w;
        }
        for (xi, wi) in coarse {
            let x = mid + half * xi;
            let fx = f.eval(x);
            if !fx.is_finite() {
                return Err(format!(
                    "la función no es integrable en la grilla de cuadratura (muestra no finita en x = {x})"
                ));
            }
            let w = kernel_weight(kernel, k, x - shift);
            s_coarse += wi * fx * w;
        }
        total_fine += s_fine * half;
        total_coarse += s_coarse * half;
    }
    let error = (total_fine - total_coarse).abs();
    if error > QUAD_TOLERANCE * (1.0 + total_fine.abs()) {
        return Err(format!(
            "la cuadratura no convergió (error estimado {error:.3e} > tolerancia {QUAD_TOLERANCE:.1e}); posible discontinuidad interior no alineada con la grilla"
        ));
    }
    Ok((total_fine, error))
}

fn kernel_weight(kernel: Kernel, k: f64, x: f64) -> f64 {
    match kernel {
        Kernel::One => 1.0,
        Kernel::Cos => (k * x).cos(),
        Kernel::Sin => (k * x).sin(),
    }
}

/// Versión simbólica del mismo integral: antiderivada del `Expr` (misma
/// representación) evaluada en los extremos. `None` si no hay primitiva.
pub(crate) fn kernel_integral_symbolic(
    f: &PreparedFn,
    kernel: Kernel,
    k: f64,
    shift: f64,
    a: f64,
    b: f64,
) -> Option<f64> {
    if kernel == Kernel::One {
        let prim =
            crate::symbolic::integrate_expr(&f.ast, &f.var).or_else(|| f.ast.integrate(&f.var))?;
        let prim = crate::symbolic::simplify_expr(&prim);
        let fa = prim.eval_at(&f.var, a);
        let fb = prim.eval_at(&f.var, b);
        return (fa.is_finite() && fb.is_finite()).then_some(fb - fa);
    }
    let x = Expr::Var(f.var.clone());
    let arg = Expr::Mul(
        Box::new(Expr::Const(k)),
        Box::new(if shift == 0.0 {
            x.clone()
        } else {
            Expr::Sub(Box::new(x.clone()), Box::new(Expr::Const(shift)))
        }),
    );
    let kernel_expr = match kernel {
        Kernel::Cos => Expr::Cos(Box::new(arg)),
        Kernel::Sin => Expr::Sin(Box::new(arg)),
        Kernel::One => x.clone(),
    };
    let integrand = Expr::Mul(Box::new(f.ast.clone()), Box::new(kernel_expr));
    let prim = crate::symbolic::integrate_expr(&integrand, &f.var)
        .or_else(|| integrand.integrate(&f.var))?;
    let prim = crate::symbolic::simplify_expr(&prim);
    let fa = prim.eval_at(&f.var, a);
    let fb = prim.eval_at(&f.var, b);
    if fa.is_finite() && fb.is_finite() {
        Some(fb - fa)
    } else {
        None
    }
}

/// Coeficientes `norm·∫_lo^hi f(x)·kernel(freq_n·x) dx` para cada frecuencia
/// de `freqs` (o el modo constante si `kernel` es [`Kernel::One`]).
///
/// Intenta primero la ruta simbólica (antiderivada del `Expr`) y la acepta
/// solo si la verifica contra cuadratura numérica en hasta 3 sondeos; si no,
/// todo es numérico con estimación de error honesta.
pub(crate) fn expansion_coefficients(
    f: &PreparedFn,
    lo: f64,
    hi: f64,
    shift: f64,
    norm: f64,
    kernel: Kernel,
    freqs: &[f64],
) -> Result<CoeffSet, String> {
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return Err(format!("intervalo de integración inválido [{lo}, {hi}]"));
    }
    if !norm.is_finite() {
        return Err(format!("normalización no finita: {norm}"));
    }
    let count = if kernel == Kernel::One {
        1
    } else {
        freqs.len()
    };
    let probe_freq = |i: usize| if kernel == Kernel::One { 0.0 } else { freqs[i] };

    // Ruta simbólica: todos los coeficientes por antiderivada + verificación.
    let mut symbolic_values = Vec::with_capacity(count);
    let mut symbolic_ok = true;
    for i in 0..count {
        match kernel_integral_symbolic(f, kernel, probe_freq(i), shift, lo, hi) {
            Some(v) => symbolic_values.push(norm * v),
            None => {
                symbolic_ok = false;
                break;
            }
        }
    }
    let mut verify_error = 0.0_f64;
    if symbolic_ok {
        let probes: Vec<usize> = match count {
            0 => Vec::new(),
            1 => vec![0],
            2 => vec![0, 1],
            n => vec![0, n / 2, n - 1],
        };
        for i in probes {
            let (raw, err) = kernel_integral(&f.quad, kernel, probe_freq(i), shift, lo, hi)?;
            let num = norm * raw;
            verify_error = verify_error.max(err * norm.abs());
            let sym = symbolic_values[i];
            if (sym - num).abs() > SYMBOLIC_VERIFY_TOLERANCE * (1.0 + sym.abs()) {
                symbolic_ok = false;
                break;
            }
            verify_error = verify_error.max((sym - num).abs());
        }
    }
    if symbolic_ok {
        return Ok(CoeffSet {
            values: symbolic_values,
            source: CoeffSource::Symbolic,
            error_estimate: verify_error,
        });
    }

    // Ruta numérica: todo por Gauss–Legendre compuesto.
    let mut values = Vec::with_capacity(count);
    let mut error_estimate = 0.0_f64;
    for i in 0..count {
        let (v, err) = kernel_integral(&f.quad, kernel, probe_freq(i), shift, lo, hi)?;
        values.push(norm * v);
        error_estimate = error_estimate.max(err * norm.abs());
    }
    Ok(CoeffSet {
        values,
        source: CoeffSource::Numeric,
        error_estimate,
    })
}

// ---------------------------------------------------------------------------
// API pública de Fourier
// ---------------------------------------------------------------------------

/// Coeficientes de la serie de Fourier de `f` con período `period` (= `T`).
///
/// El desarrollo es sobre `[-T/2, T/2]` (ver la convención del módulo):
/// `a0`, `an[n-1] = a_n`, `bn[n-1] = b_n` para `n = 1..=terms`.
#[derive(Clone, Debug, PartialEq)]
pub struct FourierCoeffs {
    /// `a0` (el término constante de la serie es `a0/2`).
    pub a0: f64,
    /// `a_n` para `n = 1..=terms`.
    pub an: Vec<f64>,
    /// `b_n` para `n = 1..=terms`.
    pub bn: Vec<f64>,
    /// Período completo `T`.
    pub period: f64,
    /// Cantidad de armónicos `terms`.
    pub terms: usize,
    /// Origen de los coeficientes (simbólico verificado o numérico).
    pub source: CoeffSource,
    /// Estimación absoluta del error de cuadratura/verificación.
    pub error_estimate: f64,
}

impl FourierCoeffs {
    /// Semi-período `L = T/2`.
    pub fn half_period(&self) -> f64 {
        self.period / 2.0
    }

    /// Evalúa la serie truncada en `x`.
    pub fn eval(&self, x: f64) -> f64 {
        let l = self.half_period();
        let mut acc = self.a0 / 2.0;
        for n in 1..=self.terms {
            let k = n as f64 * std::f64::consts::PI / l;
            if let Some(an) = self.an.get(n - 1) {
                acc += an * (k * x).cos();
            }
            if let Some(bn) = self.bn.get(n - 1) {
                acc += bn * (k * x).sin();
            }
        }
        acc
    }

    /// Coeficientes exponenciales `c_n` para `n = −terms..=terms`:
    /// `c_0 = a0/2`, `c_n = (a_n − i·b_n)/2` y `c_{−n} = conj(c_n)`.
    pub fn exponential(&self) -> Vec<Complex64> {
        let mut out = Vec::with_capacity(self.terms * 2 + 1);
        for n in -(self.terms as i32)..=self.terms as i32 {
            if n == 0 {
                out.push(Complex64::new(self.a0 / 2.0, 0.0));
                continue;
            }
            let idx = n.unsigned_abs() as usize - 1;
            let a = self.an.get(idx).copied().unwrap_or(0.0) / 2.0;
            let b = self.bn.get(idx).copied().unwrap_or(0.0) / 2.0;
            let im = if n > 0 { -b } else { b };
            out.push(Complex64::new(a, im));
        }
        out
    }

    /// Serie truncada como `Expr` (representación única de Grafito).
    pub fn to_series_expr(&self, var: &str) -> Expr {
        let l = self.half_period();
        let x = Expr::Var(var.to_string());
        let mut acc: Option<Expr> = Some(Expr::Const(self.a0 / 2.0));
        for n in 1..=self.terms {
            let k = n as f64 * std::f64::consts::PI / l;
            for (coeff, kernel) in [
                (self.an.get(n - 1).copied().unwrap_or(0.0), Kernel::Cos),
                (self.bn.get(n - 1).copied().unwrap_or(0.0), Kernel::Sin),
            ] {
                if coeff == 0.0 {
                    continue;
                }
                let trig = match kernel {
                    Kernel::Cos => Expr::Cos(Box::new(Expr::Mul(
                        Box::new(Expr::Const(k)),
                        Box::new(x.clone()),
                    ))),
                    _ => Expr::Sin(Box::new(Expr::Mul(
                        Box::new(Expr::Const(k)),
                        Box::new(x.clone()),
                    ))),
                };
                let term = Expr::Mul(Box::new(Expr::Const(coeff)), Box::new(trig));
                acc = Some(match acc {
                    Some(prev) => Expr::Add(Box::new(prev), Box::new(term)),
                    None => term,
                });
            }
        }
        acc.unwrap_or(Expr::Const(0.0))
    }

    /// Serie truncada como texto (`to_expr_string` del `Expr` de arriba).
    pub fn series_string(&self, var: &str) -> String {
        self.to_series_expr(var).to_expr_string()
    }
}

/// Coeficientes de Fourier `{a0, an[], bn[]}` de `f` con período `period`.
///
/// `period` es el período completo `T` (intervalo `[-T/2, T/2]`); `terms` es
/// la cantidad de armónicos (`≤ 128`). Integración numérica Gauss–Legendre
/// compuesta sobre el bytecode, o ruta simbólica verificada si `f` es
/// polinómica/trigonométrica simple.
pub fn fourier_coefficients(
    expr: &str,
    var: &str,
    period: f64,
    terms: usize,
) -> Result<FourierCoeffs, String> {
    validate_terms(terms)?;
    if !period.is_finite() || period <= 0.0 {
        return Err(format!(
            "período inválido: {period} (debe ser finito y > 0)"
        ));
    }
    let f = PreparedFn::new(expr, var)?;
    let l = period / 2.0;
    let norm = 1.0 / l;
    let freqs: Vec<f64> = (1..=terms)
        .map(|n| n as f64 * std::f64::consts::PI / l)
        .collect();
    let a0 = expansion_coefficients(&f, -l, l, 0.0, norm, Kernel::One, &[])?;
    let an = expansion_coefficients(&f, -l, l, 0.0, norm, Kernel::Cos, &freqs)?;
    let bn = expansion_coefficients(&f, -l, l, 0.0, norm, Kernel::Sin, &freqs)?;

    let source = if [a0.source, an.source, bn.source]
        .iter()
        .all(|s| *s == CoeffSource::Symbolic)
    {
        CoeffSource::Symbolic
    } else {
        CoeffSource::Numeric
    };
    let error_estimate = a0
        .error_estimate
        .max(an.error_estimate)
        .max(bn.error_estimate);
    let a0_value = a0.values.first().copied().unwrap_or(0.0);
    Ok(FourierCoeffs {
        a0: a0_value,
        an: an.values,
        bn: bn.values,
        period,
        terms,
        source,
        error_estimate,
    })
}

/// Serie de Fourier truncada como `Expr` (ver [`fourier_coefficients`]).
pub fn fourier_series(expr: &str, var: &str, period: f64, terms: usize) -> Result<Expr, String> {
    let coeffs = fourier_coefficients(expr, var, period, terms)?;
    Ok(coeffs.to_series_expr(var))
}

/// Serie de cosenos (extensión par) de `f` definida en `[0, T/2]`:
/// `a0/2 + Σ a_n·cos(2πnx/T)` con `a_n = (4/T)·∫_0^{T/2} f·cos(2πnx/T) dx`.
pub fn fourier_cosine_series(
    expr: &str,
    var: &str,
    period: f64,
    terms: usize,
) -> Result<Expr, String> {
    let coeffs = fourier_coefficients_on(expr, var, 0.0, period / 2.0, terms, true)?;
    Ok(coeffs.to_series_expr(var))
}

/// Serie de senos (extensión impar) de `f` definida en `[0, T/2]`:
/// `Σ b_n·sin(2πnx/T)` con `b_n = (4/T)·∫_0^{T/2} f·sin(2πnx/T) dx`.
pub fn fourier_sine_series(
    expr: &str,
    var: &str,
    period: f64,
    terms: usize,
) -> Result<Expr, String> {
    let coeffs = fourier_coefficients_on(expr, var, 0.0, period / 2.0, terms, false)?;
    Ok(coeffs.to_series_expr(var))
}

/// Coeficientes exponenciales `c_n` (`n = −terms..=terms`) de la serie
/// `Σ c_n·e^{i·2πnx/T}`. Para `f` real, `c_{−n} = conj(c_n)`.
pub fn fourier_exponential_coefficients(
    expr: &str,
    var: &str,
    period: f64,
    terms: usize,
) -> Result<Vec<Complex64>, String> {
    Ok(fourier_coefficients(expr, var, period, terms)?.exponential())
}

/// Media/expansión sobre `[lo, hi]` re-expandida a período `2·(hi−lo)`:
/// si `cosine` la serie usa cosenos (extensión par), si no senos (impar).
///
/// Expuesta para que `crate::pde` proyecte datos de frontera sobre las bases
/// propias de Dirichlet (`sin(nπx/L)`) y Neumann (`cos(nπx/L)`) sin duplicar
/// la cuadratura. Frecuencias `nπ/(hi−lo)`.
pub fn fourier_coefficients_on(
    expr: &str,
    var: &str,
    lo: f64,
    hi: f64,
    terms: usize,
    cosine: bool,
) -> Result<FourierCoeffs, String> {
    validate_terms(terms)?;
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return Err(format!("intervalo inválido [{lo}, {hi}]"));
    }
    let f = PreparedFn::new(expr, var)?;
    let s = hi - lo;
    let norm = 2.0 / s;
    let freqs: Vec<f64> = (1..=terms)
        .map(|n| n as f64 * std::f64::consts::PI / s)
        .collect();
    let kernel = if cosine { Kernel::Cos } else { Kernel::Sin };
    let active = expansion_coefficients(&f, lo, hi, 0.0, norm, kernel, &freqs)?;
    let mut an = vec![0.0; terms];
    let mut bn = vec![0.0; terms];
    if cosine {
        an.copy_from_slice(&active.values);
    } else {
        bn.copy_from_slice(&active.values);
    }
    let (a0, a0_source, a0_err) = if cosine {
        let set = expansion_coefficients(&f, lo, hi, 0.0, norm, Kernel::One, &[])?;
        (
            set.values.first().copied().unwrap_or(0.0),
            set.source,
            set.error_estimate,
        )
    } else {
        (0.0, active.source, 0.0)
    };
    let source = if a0_source == active.source {
        active.source
    } else {
        CoeffSource::Numeric
    };
    Ok(FourierCoeffs {
        a0,
        an,
        bn,
        period: 2.0 * s,
        terms,
        source,
        error_estimate: active.error_estimate.max(a0_err),
    })
}

/// Expansión sobre `[lo, hi]` con las frecuencias **periódicas** del período
/// `s = hi − lo`: `cos(2πnx/s)` y `sin(2πnx/s)` (a diferencia de
/// [`fourier_coefficients_on`], que usa `nπx/s`).
///
/// Expuesta para que `crate::pde` resuelva condiciones de contorno periódicas
/// sobre `[0, L]` sin duplicar la cuadratura. La convención de
/// [`FourierCoeffs`] se mantiene: `period = s`, término constante `a0/2`.
pub fn fourier_coefficients_periodic_on(
    expr: &str,
    var: &str,
    lo: f64,
    hi: f64,
    terms: usize,
) -> Result<FourierCoeffs, String> {
    validate_terms(terms)?;
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return Err(format!("intervalo inválido [{lo}, {hi}]"));
    }
    let f = PreparedFn::new(expr, var)?;
    let s = hi - lo;
    let norm = 2.0 / s;
    let freqs: Vec<f64> = (1..=terms)
        .map(|n| n as f64 * 2.0 * std::f64::consts::PI / s)
        .collect();
    let a0 = expansion_coefficients(&f, lo, hi, 0.0, norm, Kernel::One, &[])?;
    let an = expansion_coefficients(&f, lo, hi, 0.0, norm, Kernel::Cos, &freqs)?;
    let bn = expansion_coefficients(&f, lo, hi, 0.0, norm, Kernel::Sin, &freqs)?;
    let source = if [a0.source, an.source, bn.source]
        .iter()
        .all(|s| *s == CoeffSource::Symbolic)
    {
        CoeffSource::Symbolic
    } else {
        CoeffSource::Numeric
    };
    let error_estimate = a0
        .error_estimate
        .max(an.error_estimate)
        .max(bn.error_estimate);
    Ok(FourierCoeffs {
        a0: a0.values.first().copied().unwrap_or(0.0),
        an: an.values,
        bn: bn.values,
        period: s,
        terms,
        source,
        error_estimate,
    })
}

fn validate_terms(terms: usize) -> Result<(), String> {
    if terms == 0 || terms > MAX_FOURIER_TERMS {
        return Err(format!(
            "cantidad de términos inválida: {terms} (debe estar en 1..={MAX_FOURIER_TERMS})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `f(x) = x` en `[-π, π]` ⇒ `b_n = 2(−1)^{n+1}/n`.
    #[test]
    fn sawtooth_coefficients_match_closed_form() {
        let c = fourier_coefficients("x", "x", 2.0 * std::f64::consts::PI, 8)
            .expect("coeficientes de x");
        assert!(c.a0.abs() < 1e-9, "a0 = {}", c.a0);
        for n in 1..=8 {
            let expected_bn = 2.0 * if n % 2 == 1 { 1.0 } else { -1.0 } / n as f64;
            assert!(
                (c.bn[n - 1] - expected_bn).abs() < 1e-9,
                "b{n} = {} (esperado {expected_bn})",
                c.bn[n - 1]
            );
            assert!(c.an[n - 1].abs() < 1e-9, "a{n} = {}", c.an[n - 1]);
        }
        assert_eq!(
            c.source,
            CoeffSource::Symbolic,
            "x es polinómica: simbólico"
        );
    }

    /// Onda cuadrada impar (`sign(x)`) ⇒ `b_n = 4/(nπ)` para n impar, 0 par.
    #[test]
    fn square_wave_coefficients() {
        let c = fourier_coefficients("sign(x)", "x", 2.0 * std::f64::consts::PI, 7)
            .expect("coeficientes de onda cuadrada");
        for n in 1..=7 {
            let expected_bn = if n % 2 == 1 {
                4.0 / (n as f64 * std::f64::consts::PI)
            } else {
                0.0
            };
            assert!(
                (c.bn[n - 1] - expected_bn).abs() < 1e-9,
                "b{n} = {} (esperado {expected_bn})",
                c.bn[n - 1]
            );
        }
    }

    /// `f(x) = cos(x)` en `[-π, π]` ⇒ `a_1 = 1`, resto 0.
    #[test]
    fn cosine_picks_single_harmonic() {
        let c = fourier_coefficients("cos(x)", "x", 2.0 * std::f64::consts::PI, 4)
            .expect("coeficientes de cos(x)");
        assert!((c.a0).abs() < 1e-9, "a0 = {}", c.a0);
        assert!((c.an[0] - 1.0).abs() < 1e-9, "a1 = {}", c.an[0]);
        for n in 2..=4 {
            assert!(c.an[n - 1].abs() < 1e-9, "a{n} = {}", c.an[n - 1]);
            assert!(c.bn[n - 1].abs() < 1e-9, "b{n} = {}", c.bn[n - 1]);
        }
    }

    /// La serie truncada reconstruye la función (error de truncamiento O(1/N)
    /// para la sierra; un armónico se reconstruye exacto).
    #[test]
    fn truncated_series_evaluates() {
        let c =
            fourier_coefficients("x", "x", 2.0 * std::f64::consts::PI, 64).expect("coeficientes");
        for x in [-2.0, -0.5, 0.7, 2.5] {
            assert!((c.eval(x) - x).abs() < 0.1, "x = {x} → {}", c.eval(x));
        }
        // Un solo armónico: reconstrucción exacta.
        let g = fourier_coefficients("cos(2*x)", "x", 2.0 * std::f64::consts::PI, 4)
            .expect("coeficientes de cos(2x)");
        for x in [-2.0, -0.5, 0.7, 2.5] {
            assert!(
                (g.eval(x) - (2.0 * x).cos()).abs() < 1e-9,
                "x = {x} → {}",
                g.eval(x)
            );
        }
    }

    /// Coeficientes exponenciales: `c_n = conj(c_{-n})`, `c_0 = a0/2`.
    #[test]
    fn exponential_coefficients_are_conjugate_symmetric() {
        let cs =
            fourier_exponential_coefficients("x", "x", 2.0 * std::f64::consts::PI, 4).expect("c_n");
        assert_eq!(cs.len(), 9);
        assert!(cs[4].re.abs() < 1e-9 && cs[4].im.abs() < 1e-9, "c0");
        // c_n = (a_n − i b_n)/2; para f = x, b_n = 2(−1)^{n+1}/n ⇒
        // c_1 = −i·(1) = (0, −1).
        assert!(cs[5].re.abs() < 1e-9, "c1 re");
        assert!((cs[5].im + 1.0).abs() < 1e-9, "c1 im = {}", cs[5].im);
        for n in 1..=4 {
            let cp = cs[4 + n];
            let cm = cs[4 - n];
            assert!((cp.re - cm.re).abs() < 1e-9);
            assert!((cp.im + cm.im).abs() < 1e-9);
        }
    }

    #[test]
    fn budgets_are_enforced() {
        assert!(fourier_coefficients("x", "x", 1.0, 0).is_err());
        assert!(fourier_coefficients("x", "x", 1.0, MAX_FOURIER_TERMS + 1).is_err());
        assert!(fourier_coefficients("x", "x", -1.0, 4).is_err());
        assert!(fourier_coefficients("x", "x", f64::NAN, 4).is_err());
        // Variable ajena: error honesto.
        assert!(fourier_coefficients("x*y", "x", 2.0, 4).is_err());
    }

    #[test]
    fn half_range_sine_series_of_one() {
        // f = 1 en [0, π]: extensión impar ⇒ b_n = 2(1−(−1)^n)/(nπ) = 4/(nπ) n impar.
        let s =
            fourier_sine_series("1", "x", 2.0 * std::f64::consts::PI, 5).expect("serie de senos");
        let f = s.eval_at("x", 1.0);
        // Σ 4/(nπ)·sin(n) para n impar ≤ 5 ≈ 1.0 (Gibbs incluido).
        assert!((f - 1.0).abs() < 0.2, "u(1) = {f}");
    }

    #[test]
    fn gauss_legendre_nodes_are_symmetric_and_integrate_exact_polynomials() {
        let nodes = gauss_legendre();
        assert_eq!(nodes.len(), GAUSS_LEGENDRE_ORDER);
        let total: f64 = nodes.iter().map(|(_, w)| w).sum();
        assert!((total - 2.0).abs() < 1e-12, "pesos suman {total}");
        for (i, (x, w)) in nodes.iter().enumerate() {
            let mirror = nodes[GAUSS_LEGENDRE_ORDER - 1 - i];
            assert!((x + mirror.0).abs() < 1e-12);
            assert!((w - mirror.1).abs() < 1e-12);
        }
        // ∫_{-1}^{1} x^14 dx = 2/15 exacto para GL16.
        let f = QuadFn::new("x^14", "x").expect("compila");
        let mut s = 0.0;
        for (x, w) in nodes {
            s += w * f.eval(*x);
        }
        assert!((s - 2.0 / 15.0).abs() < 1e-12, "∫x^14 = {s}");
    }
}
