//! Motor faltante de Álgebra/CAS (GeoGebra subset).
//!
//! Funciones puras sobre strings/`Vec` ya parseados para la fase de cableado.
//! Reusa `ast::parse_ast`, `Expr::diff/simplify/eval`, `cas::gruntz_limit`,
//! `symbolic::prime_factors`, `matrices::rank` y `list_ops::DeterministicRng`.
//! Sin I/O, sin `unwrap`/`expect` fuera de tests.

use std::collections::{BTreeMap, HashSet};

/// Entrada máxima por expresión (espeja `MAX_CAS_EXPR_BYTES` 2000).
pub const MAX_CAS_EXTRA_BYTES: usize = 2000;
/// Iteraciones máximas de [`iterate_function`].
pub const MAX_ITERATION_COUNT: usize = 1024;
/// Grado máximo de [`random_polynomial`].
pub const MAX_RANDOM_POLY_DEGREE: usize = 8;
/// Dimensión máxima de [`identity_matrix`].
pub const MAX_IDENTITY_DIM: usize = 64;
/// Dimensión máxima por lado en [`matrix_dimension`] y [`matrix_rank`].
pub const MAX_EXTRA_MATRIX_DIM: usize = 64;
/// Subintervalos máximos en las sumas de Riemann.
pub const MAX_RIEMANN_SUBINTERVALS: usize = 100_000;
/// Muestras internas por subintervalo para `Lower/Upper` (estimación honesta).
pub const RIEMANN_INNER_SAMPLES: usize = 8;
/// Muestras del barrido de [`inflection_points`].
pub const MAX_INFLECTION_SAMPLES: usize = 2000;
/// Puntos de inflexión máximos devueltos.
pub const MAX_INFLECTION_POINTS: usize = 64;
/// Periodos máximos en finanzas (100 años mensuales).
pub const MAX_FINANCE_PERIODS: u32 = 1200;
/// Monto absoluto máximo en finanzas.
pub const MAX_FINANCE_AMOUNT: f64 = 1e15;
/// Entero absoluto máximo en [`factors`]/[`factors_from_text`] (igual que `PrimeFactors`).
pub const MAX_FACTORS_ABS: i64 = 1_000_000_000_000;

fn check_bytes(text: &str) -> Result<String, String> {
    if text.is_empty() {
        return Err("expresión vacía".to_string());
    }
    if text.len() > MAX_CAS_EXTRA_BYTES {
        return Err(format!(
            "expresión de {} bytes excede el máximo {MAX_CAS_EXTRA_BYTES}",
            text.len()
        ));
    }
    Ok(text.replace(' ', ""))
}

fn check_var(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(format!("variable '{name}' no es un identificador válido"));
    }
    let mut chars = trimmed.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("variable '{name}' no es un identificador válido"));
    }
    Ok(trimmed.to_string())
}

fn parse_expr(clean: &str) -> Result<crate::ast::Expr, String> {
    crate::ast::parse_ast(clean).map_err(|reason| format!("no se pudo parsear: {reason}"))
}

fn trim_float(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let text = format!("{value:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Ecuaciones: LeftSide / RightSide
// ---------------------------------------------------------------------------

fn split_equation(equation: &str) -> Result<(String, String), String> {
    let clean = check_bytes(equation)?;
    let parts: Vec<&str> = clean.split('=').collect();
    if parts.len() != 2 {
        if !clean.contains('=') {
            return Err("se esperaba una ecuación con '='".to_string());
        }
        return Err("la ecuación tiene más de un '='".to_string());
    }
    let left = parts.first().map_or("", |s| s.trim());
    let right = parts.get(1).map_or("", |s| s.trim());
    if left.is_empty() || right.is_empty() {
        return Err("ecuación con lado vacío".to_string());
    }
    Ok((left.to_string(), right.to_string()))
}

/// Lado izquierdo de una ecuación `izq = der`.
pub fn left_side(equation: &str) -> Result<String, String> {
    split_equation(equation).map(|pair| pair.0)
}

/// Lado derecho de una ecuación `izq = der`.
pub fn right_side(equation: &str) -> Result<String, String> {
    split_equation(equation).map(|pair| pair.1)
}

// ---------------------------------------------------------------------------
// Derivada implícita F(x,y)=0
// ---------------------------------------------------------------------------

/// Derivada implícita `dy/dx = -Fx/Fy` como expresión simplificada.
pub fn implicit_derivative(expr: &str, x_var: &str, y_var: &str) -> Result<String, String> {
    let clean = check_bytes(expr)?;
    let x = check_var(x_var)?;
    let y = check_var(y_var)?;
    if x == y {
        return Err("las variables x e y deben ser distintas".to_string());
    }
    let ast = parse_expr(&clean)?;
    let fx = ast.diff(&x);
    let fy = ast.diff(&y);
    let num = crate::ast::Expr::Neg(Box::new(fx));
    let dydx = crate::ast::Expr::Div(Box::new(num), Box::new(fy));
    let out = dydx.simplify().to_expr_string();
    if out.len() > MAX_CAS_EXTRA_BYTES * 4 {
        return Err("derivada implícita excede el presupuesto".to_string());
    }
    Ok(out)
}

/// Derivada implícita evaluada en `(x0, y0)`.
pub fn implicit_derivative_at(
    expr: &str,
    x_var: &str,
    y_var: &str,
    x0: f64,
    y0: f64,
) -> Result<f64, String> {
    if !x0.is_finite() || !y0.is_finite() {
        return Err("el punto debe ser finito".to_string());
    }
    let clean = check_bytes(expr)?;
    let x = check_var(x_var)?;
    let y = check_var(y_var)?;
    if x == y {
        return Err("las variables x e y deben ser distintas".to_string());
    }
    let ast = parse_expr(&clean)?;
    let fx = ast.diff(&x).eval_2d(&x, x0, &y, y0);
    let fy = ast.diff(&y).eval_2d(&x, x0, &y, y0);
    if !fx.is_finite() || !fy.is_finite() {
        return Err("parciales no finitas en el punto".to_string());
    }
    if fy == 0.0 {
        return Err("tangente vertical (Fy=0), derivada no definida".to_string());
    }
    let out = -fx / fy;
    if !out.is_finite() {
        return Err("derivada no finita en el punto".to_string());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Iteración / Numeric
// ---------------------------------------------------------------------------

/// Itera `x_{k+1} = f(x_k)` desde `x0`; devuelve `[x1, .., xn]`.
pub fn iterate_function(expr: &str, var: &str, x0: f64, count: usize) -> Result<Vec<f64>, String> {
    if !x0.is_finite() {
        return Err("x0 debe ser finito".to_string());
    }
    if count > MAX_ITERATION_COUNT {
        return Err(format!(
            "iteraciones {count} exceden el máximo {MAX_ITERATION_COUNT}"
        ));
    }
    let clean = check_bytes(expr)?;
    let variable = check_var(var)?;
    let ast = parse_expr(&clean)?;
    let mut out = Vec::new();
    if out.try_reserve(count).is_err() {
        return Err("no se pudo reservar la lista iterada".to_string());
    }
    let mut current = x0;
    for step in 0..count {
        let next = ast.eval_at(&variable, current);
        if !next.is_finite() {
            return Err(format!(
                "la iteración divergió (valor no finito en el paso {})",
                step + 1
            ));
        }
        out.push(next);
        current = next;
    }
    Ok(out)
}

/// Evalúa una expresión constante a decimal.
pub fn numeric(expr: &str) -> Result<f64, String> {
    let clean = check_bytes(expr)?;
    let value = crate::expr::evaluate(&clean, &[])
        .map_err(|reason| format!("no se pudo evaluar: {reason}"))?;
    if !value.is_finite() {
        return Err("resultado no finito".to_string());
    }
    Ok(value)
}

/// Evalúa `expr` en `var = value`.
pub fn numeric_at(expr: &str, var: &str, value: f64) -> Result<f64, String> {
    if !value.is_finite() {
        return Err("el valor debe ser finito".to_string());
    }
    let clean = check_bytes(expr)?;
    let variable = check_var(var)?;
    let got = crate::expr::evaluate(&clean, &[(variable, value)])
        .map_err(|reason| format!("no se pudo evaluar: {reason}"))?;
    if !got.is_finite() {
        return Err("resultado no finito".to_string());
    }
    Ok(got)
}

// ---------------------------------------------------------------------------
// Complejo a exponencial r·e^(iθ)
// ---------------------------------------------------------------------------

/// Complejo `(re, im)` a forma `r·e^(iθ)` como string (mínimo local, sin `grafito-complex`).
pub fn to_exponential(re: f64, im: f64) -> Result<String, String> {
    if !re.is_finite() || !im.is_finite() {
        return Err("el complejo debe ser finito".to_string());
    }
    let radius = re.hypot(im);
    if radius == 0.0 {
        return Ok("0".to_string());
    }
    let theta = im.atan2(re);
    Ok(format!(
        "{}*e^(i*{})",
        trim_float(radius),
        trim_float(theta)
    ))
}

// ---------------------------------------------------------------------------
// Factores primos con multiplicidad
// ---------------------------------------------------------------------------

/// Factores primos de `n` con multiplicidad `[(primo, exp)]` (reusa la cota de `PrimeFactors`).
pub fn factors(n: i64) -> Result<Vec<(i64, u32)>, String> {
    if n == 0 {
        return Err("no se puede factorizar 0".to_string());
    }
    let abs = n.unsigned_abs() as i128;
    if abs > i128::from(MAX_FACTORS_ABS) {
        return Err(format!("valor excede el máximo {MAX_FACTORS_ABS}"));
    }
    if abs <= 1 {
        return Ok(Vec::new());
    }
    let mut rest = abs as u64;
    let mut out: Vec<(i64, u32)> = Vec::new();
    let mut count = 0u32;
    while rest.is_multiple_of(2) {
        rest /= 2;
        count += 1;
    }
    if count > 0 {
        out.push((2, count));
    }
    let mut prime: u64 = 3;
    while prime * prime <= rest {
        if rest.is_multiple_of(prime) {
            let mut exp = 0u32;
            while rest.is_multiple_of(prime) {
                rest /= prime;
                exp += 1;
            }
            let converted = i64::try_from(prime).map_err(|_| "primo fuera de rango".to_string())?;
            out.push((converted, exp));
        }
        prime += 2;
        if prime > 1_000_002 {
            break;
        }
    }
    if rest > 1 {
        let converted = i64::try_from(rest).map_err(|_| "primo fuera de rango".to_string())?;
        out.push((converted, 1));
    }
    Ok(out)
}

/// Parsea un entero en texto y lo factoriza.
pub fn factors_from_text(text: &str) -> Result<Vec<(i64, u32)>, String> {
    let clean = check_bytes(text)?;
    let compact: String = clean.chars().filter(|c| *c != '_').collect();
    compact
        .parse::<i64>()
        .map_err(|_| "no se pudo interpretar como entero".to_string())
        .and_then(factors)
}

/// Formatea factores estilo `2^2 * 3`; vacío → `1`.
pub fn format_factors(pairs: &[(i64, u32)]) -> String {
    if pairs.is_empty() {
        return "1".to_string();
    }
    pairs
        .iter()
        .map(|(prime, exp)| {
            if *exp == 1 {
                format!("{prime}")
            } else {
                format!("{prime}^{exp}")
            }
        })
        .collect::<Vec<_>>()
        .join(" * ")
}

// ---------------------------------------------------------------------------
// AreEqual honesto
// ---------------------------------------------------------------------------

/// Veredicto de igualdad demostrable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualityVerdict {
    /// Diferencia idénticamente nula.
    Equal,
    /// Un contraejemplo numérico las distingue.
    NotEqual,
    /// Coinciden en el sondeo pero sin prueba simbólica.
    Unknown,
}

impl EqualityVerdict {
    /// Etiqueta corta en español.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Equal => "iguales",
            Self::NotEqual => "distintas",
            Self::Unknown => "indeterminado",
        }
    }
}

const ARE_EQUAL_SAMPLES: [f64; 8] = [-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 3.0];

/// Igualdad demostrable: simplifica la diferencia; solo `true` simbólico, si no sondea.
pub fn are_equal(left: &str, right: &str) -> Result<EqualityVerdict, String> {
    let clean_left = check_bytes(left)?;
    let clean_right = check_bytes(right)?;
    let left_ast = parse_expr(&clean_left)?;
    let right_ast = parse_expr(&clean_right)?;
    let diff = crate::ast::Expr::Sub(Box::new(left_ast.clone()), Box::new(right_ast.clone()));
    let simplified = diff.simplify();
    if let crate::ast::Expr::Const(c) = simplified {
        if !c.is_finite() {
            return Ok(EqualityVerdict::Unknown);
        }
        if c == 0.0 {
            return Ok(EqualityVerdict::Equal);
        }
        return Ok(EqualityVerdict::NotEqual);
    }
    let mut variables = HashSet::new();
    left_ast.get_variables(&mut variables);
    right_ast.get_variables(&mut variables);
    if variables.is_empty() {
        return Ok(EqualityVerdict::Unknown);
    }
    let names: Vec<String> = variables.into_iter().collect();
    for sample in ARE_EQUAL_SAMPLES {
        let mut bindings = BTreeMap::new();
        for name in &names {
            bindings.insert(name.clone(), sample);
        }
        let left_value = left_ast
            .substitute_vars(&bindings, &[])
            .simplify()
            .to_expr_string();
        let _ = left_value;
        let diff_value = simplified.substitute_vars(&bindings, &[]).simplify();
        if let crate::ast::Expr::Const(c) = diff_value {
            if c.is_finite() && c.abs() > 1e-9 {
                return Ok(EqualityVerdict::NotEqual);
            }
        }
    }
    Ok(EqualityVerdict::Unknown)
}

// ---------------------------------------------------------------------------
// Discontinuidad evitable / Inflexiones
// ---------------------------------------------------------------------------

/// Resultado de [`removable_discontinuity`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemovableOutcome {
    /// `true` si el límite existe pero `f` es indefinida o distinta.
    pub is_removable: bool,
    /// Límite bilateral si existe y es finito.
    pub limit: Option<f64>,
    /// Valor puntual (`None` si indefinido).
    pub value_at: Option<f64>,
}

/// Punto evitable: el límite existe y es finito pero `f` es indefinida o distinta.
pub fn removable_discontinuity(expr: &str, var: &str, at: f64) -> Result<RemovableOutcome, String> {
    if !at.is_finite() {
        return Err("el punto debe ser finito".to_string());
    }
    let clean = check_bytes(expr)?;
    let variable = check_var(var)?;
    let ast = parse_expr(&clean)?;
    let raw = ast.eval_at(&variable, at);
    let value_at = if raw.is_finite() { Some(raw) } else { None };
    let limit = match crate::cas::gruntz_limit(&clean, &variable, at) {
        Ok(outcome) if outcome.value.is_finite() => Some(outcome.value),
        _ => crate::ast::compute_limit(&clean, &variable, at).filter(|v| v.is_finite()),
    };
    let Some(limit_value) = limit else {
        return Ok(RemovableOutcome {
            is_removable: false,
            limit: None,
            value_at,
        });
    };
    match value_at {
        None => Ok(RemovableOutcome {
            is_removable: true,
            limit: Some(limit_value),
            value_at: None,
        }),
        Some(current) => {
            if (current - limit_value).abs() <= 1e-6 * (1.0 + limit_value.abs()) {
                Ok(RemovableOutcome {
                    is_removable: false,
                    limit: Some(limit_value),
                    value_at: Some(current),
                })
            } else {
                Ok(RemovableOutcome {
                    is_removable: true,
                    limit: Some(limit_value),
                    value_at: Some(current),
                })
            }
        }
    }
}

/// Puntos de inflexión en `[lo, hi]` por cambio de signo de `f''`.
pub fn inflection_points(expr: &str, var: &str, lo: f64, hi: f64) -> Result<Vec<f64>, String> {
    if !lo.is_finite() || !hi.is_finite() {
        return Err("el intervalo debe ser finito".to_string());
    }
    if lo >= hi {
        return Err("se esperaba lo < hi".to_string());
    }
    if hi - lo > 1e12 {
        return Err("intervalo demasiado amplio".to_string());
    }
    let clean = check_bytes(expr)?;
    let variable = check_var(var)?;
    let ast = parse_expr(&clean)?;
    let second = ast.diff(&variable).diff(&variable);
    let samples = MAX_INFLECTION_SAMPLES;
    let width = hi - lo;
    let mut previous_x = lo;
    let mut previous_v = second.eval_at(&variable, lo);
    let mut out: Vec<f64> = Vec::new();
    for index in 1..=samples {
        let current_x = lo + width * (index as f64) / (samples as f64);
        let current_v = second.eval_at(&variable, current_x);
        if previous_v.is_finite() && current_v.is_finite() {
            let sign_change =
                (previous_v < 0.0 && current_v > 0.0) || (previous_v > 0.0 && current_v < 0.0);
            let touches_zero = previous_v == 0.0 || current_v == 0.0 || sign_change;
            if touches_zero && out.len() < MAX_INFLECTION_POINTS {
                let mut low = previous_x;
                let mut high = current_x;
                let mut low_v = previous_v;
                let mut candidate = (low + high) / 2.0;
                for _ in 0..50 {
                    let mid = (low + high) / 2.0;
                    if !mid.is_finite() {
                        break;
                    }
                    candidate = mid;
                    let mid_v = second.eval_at(&variable, mid);
                    if !mid_v.is_finite() {
                        break;
                    }
                    if mid_v == 0.0 {
                        break;
                    }
                    if (low_v < 0.0) == (mid_v < 0.0) {
                        low = mid;
                        low_v = mid_v;
                    } else {
                        high = mid;
                    }
                    if high - low < 1e-9 {
                        break;
                    }
                }
                let function_value = ast.eval_at(&variable, candidate);
                let second_value = second.eval_at(&variable, candidate);
                if function_value.is_finite()
                    && second_value.is_finite()
                    && second_value.abs() < 1e-4
                    && !out.iter().any(|seen| (seen - candidate).abs() < 1e-7)
                {
                    out.push(candidate);
                }
            }
        }
        previous_x = current_x;
        previous_v = current_v;
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Matrices sobre Vec<Vec<f64>> ya parseados
// ---------------------------------------------------------------------------

/// Dimensión `(filas, columnas)` validando rectangularidad y cotas.
pub fn matrix_dimension(matrix: &[Vec<f64>]) -> Result<(usize, usize), String> {
    if matrix.is_empty() {
        return Err("matriz vacía".to_string());
    }
    if matrix.len() > MAX_EXTRA_MATRIX_DIM {
        return Err(format!(
            "filas {} exceden el máximo {MAX_EXTRA_MATRIX_DIM}",
            matrix.len()
        ));
    }
    let Some(first) = matrix.first() else {
        return Err("matriz vacía".to_string());
    };
    if first.is_empty() {
        return Err("matriz con columnas vacías".to_string());
    }
    if first.len() > MAX_EXTRA_MATRIX_DIM {
        return Err(format!(
            "columnas {} exceden el máximo {MAX_EXTRA_MATRIX_DIM}",
            first.len()
        ));
    }
    for row in matrix {
        if row.len() != first.len() {
            return Err("matriz no rectangular".to_string());
        }
    }
    Ok((matrix.len(), first.len()))
}

/// Identidad `n×n` (`1 ≤ n ≤ 64`).
pub fn identity_matrix(n: usize) -> Result<Vec<Vec<f64>>, String> {
    if n == 0 {
        return Err("n debe ser ≥ 1".to_string());
    }
    if n > MAX_IDENTITY_DIM {
        return Err(format!("n={n} excede el máximo {MAX_IDENTITY_DIM}"));
    }
    let mut out = vec![vec![0.0; n]; n];
    for (index, row) in out.iter_mut().enumerate() {
        if let Some(cell) = row.get_mut(index) {
            *cell = 1.0;
        }
    }
    Ok(out)
}

fn bareiss_rank_integers(values: &[Vec<i128>]) -> Option<usize> {
    let rows = values.len();
    let cols = values.first()?.len();
    if rows == 0 || cols == 0 {
        return None;
    }
    let mut work = values.to_vec();
    let mut rank = 0_usize;
    let mut previous: i128 = 1;
    for col in 0..cols {
        let mut pivot: Option<usize> = None;
        for row in rank..rows {
            if let Some(current) = work.get(row)?.get(col) {
                if *current != 0 {
                    pivot = Some(row);
                    break;
                }
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        work.swap(rank, pivot_row);
        let pivot_value = *work.get(rank)?.get(col)?;
        for row in (rank + 1)..rows {
            for inner in (col + 1)..cols {
                let numerator = work
                    .get(row)?
                    .get(col)?
                    .checked_mul(*work.get(rank)?.get(inner)?)?
                    .checked_sub(work.get(row)?.get(inner)?.checked_mul(pivot_value)?)?;
                let updated = if previous == 1 || previous == -1 {
                    numerator.checked_mul(previous)?
                } else {
                    if numerator % previous != 0 {
                        return None;
                    }
                    numerator.checked_div(previous)?
                };
                if let Some(cell) = work.get_mut(row)?.get_mut(inner) {
                    *cell = updated;
                }
            }
        }
        for row in (rank + 1)..rows {
            if let Some(cell) = work.get_mut(row)?.get_mut(col) {
                *cell = 0;
            }
        }
        previous = pivot_value;
        rank += 1;
        if rank >= rows.min(cols) && rank == rows.min(cols) {
            let all_zero = (rank..rows).all(|row| {
                (col + 1..cols).all(|inner| {
                    work.get(row)
                        .and_then(|r| r.get(inner))
                        .is_some_and(|v| *v == 0)
                })
            });
            if all_zero {
                break;
            }
        }
    }
    Some(rank)
}

/// Rango por Bareiss exacto si es entera, si no por SVD (`matrices::rank`).
///
/// Tolerancia documentada: vía entera exacta sin tolerancia; vía flotante la de
/// `matrices::rank` (SVD, umbral escalado por norma espectral).
pub fn matrix_rank(matrix: &[Vec<f64>]) -> Result<usize, String> {
    let (rows, cols) = matrix_dimension(matrix)?;
    let mut integers: Vec<Vec<i128>> = Vec::new();
    let mut all_integer = true;
    for row in matrix {
        let mut converted = Vec::new();
        for value in row {
            if !value.is_finite() {
                return Err("matriz con valores no finitos".to_string());
            }
            if value.abs() > 1_000_000.0 || (value - value.round()).abs() > 1e-9 {
                all_integer = false;
                break;
            }
            converted.push(value.round() as i128);
        }
        if !all_integer {
            break;
        }
        integers.push(converted);
    }
    if all_integer && integers.len() == rows {
        if let Some(rank) = bareiss_rank_integers(&integers) {
            return Ok(rank);
        }
    }
    let owned: Vec<Vec<f64>> = matrix.to_vec();
    let dense = crate::matrices::Matrix::from_rows(owned)
        .ok_or_else(|| "matriz inválida para rango".to_string())?;
    if dense.rows != rows || dense.cols != cols {
        return Err("matriz inválida para rango".to_string());
    }
    crate::matrices::rank(&dense).ok_or_else(|| "no se pudo calcular el rango".to_string())
}

// ---------------------------------------------------------------------------
// Azar determinista (reusa DeterministicRng)
// ---------------------------------------------------------------------------

/// Entero uniforme en `[min, max]` determinista por `seed`.
pub fn random_between(min: i64, max: i64, seed: u64) -> Result<i64, String> {
    if min > max {
        return Err("mínimo mayor que máximo".to_string());
    }
    let span = i128::from(max) - i128::from(min) + 1;
    if span <= 0 || span > i128::from(usize::MAX as u64) {
        return Err("rango demasiado amplio".to_string());
    }
    let span_usize = span as usize;
    let mut rng = crate::list_ops::DeterministicRng::seed_from_parts(
        seed,
        "RandomBetween",
        &[&min.to_string(), &max.to_string()],
    );
    let offset = rng
        .below(span_usize)
        .ok_or_else(|| "rango vacío".to_string())?;
    min.checked_add(offset as i64)
        .ok_or_else(|| "desborde en el sorteo".to_string())
}

/// Polinomio aleatorio determinista grado `≤ 8`, coeficientes `[-9, 9]` ascendentes.
pub fn random_polynomial(degree: usize, seed: u64) -> Result<Vec<f64>, String> {
    if degree > MAX_RANDOM_POLY_DEGREE {
        return Err(format!(
            "grado {degree} excede el máximo {MAX_RANDOM_POLY_DEGREE}"
        ));
    }
    let mut rng = crate::list_ops::DeterministicRng::seed_from_parts(
        seed,
        "RandomPolynomial",
        &[&degree.to_string()],
    );
    let mut coeffs = Vec::new();
    if coeffs.try_reserve(degree + 1).is_err() {
        return Err("no se pudo reservar el polinomio".to_string());
    }
    for _ in 0..=degree {
        let slot = rng.below(19).ok_or_else(|| "sorteo vacío".to_string())?;
        coeffs.push(slot as f64 - 9.0);
    }
    if degree > 0 {
        let leading = coeffs.get(degree).copied().unwrap_or(0.0);
        if leading == 0.0 {
            if let Some(cell) = coeffs.get_mut(degree) {
                *cell = 1.0;
            }
        }
    }
    Ok(coeffs)
}

// ---------------------------------------------------------------------------
// Finanzas estándar (pagos vencidos)
// ---------------------------------------------------------------------------
//
// Núcleo canónico de las formas cerradas (ver `FINANCE_ZERO_RATE`): este
// módulo es la casa de la matemática financiera y `stats_extra` delega en
// estos núcleos (`crate::cas_extra::finance_*`) para no duplicar lógica.
// Lo que NO se unifica, a propósito:
// - orden de parámetros (`cas_extra`: `(tasa, n, cuota, capital)`;
//   `stats_extra` (convención de aula): `(tasa, n, capital, cuota)`) y
// - `payment` de 4 args (cuota que lleva `presente → futuro`, con signo) vs
//   `payment` de 3 args de aula (amortización, `futuro = 0`, siempre ≥ 0),
// - cota de períodos (1200 acá = 100 años mensuales; 100_000 en aula).
// Cada módulo conserva su validación, sus cotas y sus mensajes.

/// Umbral bajo el cual la tasa se trata como cero en las formas cerradas:
/// evita cancelación catastrófica en `(factor − 1) / tasa` con tasas
/// diminutas (para `tasa = 0.0` el resultado es bit-idéntico al lineal).
pub(crate) const FINANCE_ZERO_RATE: f64 = 1e-12;

/// Factor `(1 + tasa)^n`, o `None` si la tasa es inválida o el factor no es
/// finito positivo. No valida `periods`: cada caller aplica su propia cota.
pub(crate) fn finance_factor(rate: f64, periods: u32) -> Option<f64> {
    if !rate.is_finite() {
        return None;
    }
    let growth = 1.0 + rate;
    if !growth.is_finite() || growth <= 0.0 {
        return None;
    }
    let exponent = i32::try_from(periods).ok()?;
    let factor = growth.powi(exponent);
    (factor.is_finite() && factor > 0.0).then_some(factor)
}

/// Valor futuro con pagos vencidos: `presente·factor + cuota·((factor−1)/tasa)`.
/// Requiere `factor = finance_factor(tasa, n)`; pura, sin validación.
pub(crate) fn finance_future_value(
    factor: f64,
    rate: f64,
    periods: u32,
    payment: f64,
    present: f64,
) -> f64 {
    if rate.abs() < FINANCE_ZERO_RATE {
        present + payment * f64::from(periods)
    } else {
        present * factor + payment * (factor - 1.0) / rate
    }
}

/// Valor presente (inversa del futuro). Requiere `factor = finance_factor`.
/// Pura, sin validación.
pub(crate) fn finance_present_value(
    factor: f64,
    rate: f64,
    periods: u32,
    payment: f64,
    future: f64,
) -> f64 {
    if rate.abs() < FINANCE_ZERO_RATE {
        future - payment * f64::from(periods)
    } else {
        (future - payment * (factor - 1.0) / rate) / factor
    }
}

/// Cuota que lleva `present` a `future` en `n` períodos (convención de signos
/// del flujo). `None` si el denominador es nulo; la validación de entradas
/// la hace el caller. Pura, sin validación.
pub(crate) fn finance_target_payment(
    factor: f64,
    rate: f64,
    periods: u32,
    present: f64,
    future: f64,
) -> Option<f64> {
    if rate.abs() < FINANCE_ZERO_RATE {
        let n = f64::from(periods);
        if n == 0.0 {
            return None;
        }
        return Some((future - present) / n);
    }
    let denominator = factor - 1.0;
    if denominator == 0.0 {
        return None;
    }
    Some((future - present * factor) * rate / denominator)
}

fn check_finance(rate: f64, periods: u32, first: f64, second: f64) -> Result<f64, String> {
    if !rate.is_finite() || !first.is_finite() || !second.is_finite() {
        return Err("parámetros no finitos".to_string());
    }
    if first.abs() > MAX_FINANCE_AMOUNT || second.abs() > MAX_FINANCE_AMOUNT {
        return Err("monto excede el máximo".to_string());
    }
    if periods == 0 || periods > MAX_FINANCE_PERIODS {
        return Err(format!("periodos debe estar en 1..={MAX_FINANCE_PERIODS}"));
    }
    if !(1.0 + rate).is_finite() || 1.0 + rate <= 0.0 {
        return Err("tasa inválida (se exige 1+tasa > 0)".to_string());
    }
    finance_factor(rate, periods).ok_or_else(|| "factor fuera de rango".to_string())
}

/// Valor futuro `PV*(1+r)^n + PMT*[((1+r)^n-1)/r]`.
pub fn future_value(rate: f64, n_periods: u32, payment: f64, present: f64) -> Result<f64, String> {
    let factor = check_finance(rate, n_periods, payment, present)?;
    let out = finance_future_value(factor, rate, n_periods, payment, present);
    if !out.is_finite() {
        return Err("resultado no finito".to_string());
    }
    Ok(out)
}

/// Valor presente `FV/(1+r)^n - PMT*[1-(1+r)^-n]/r`.
pub fn present_value(rate: f64, n_periods: u32, payment: f64, future: f64) -> Result<f64, String> {
    let factor = check_finance(rate, n_periods, payment, future)?;
    let out = finance_present_value(factor, rate, n_periods, payment, future);
    if !out.is_finite() {
        return Err("resultado no finito".to_string());
    }
    Ok(out)
}

/// Cuota `PMT` que lleva `present` a `future` en `n` periodos.
pub fn payment(rate: f64, n_periods: u32, present: f64, future: f64) -> Result<f64, String> {
    let factor = check_finance(rate, n_periods, present, future)?;
    let out = finance_target_payment(factor, rate, n_periods, present, future)
        .ok_or_else(|| "denominador nulo en la cuota".to_string())?;
    if !out.is_finite() {
        return Err("resultado no finito".to_string());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Sumas de Riemann (5 métodos, n acotado)
// ---------------------------------------------------------------------------

/// Método de suma de Riemann.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiemannMethod {
    /// Extremo izquierdo.
    Left,
    /// Mínimo por subintervalo (estimado con 8 muestras).
    Lower,
    /// Máximo por subintervalo (estimado con 8 muestras).
    Upper,
    /// Trapecio `(f(x_i)+f(x_{i+1}))/2`.
    Trapezoidal,
    /// Punto medio.
    Midpoint,
}

fn check_riemann(
    expr: &str,
    var: &str,
    a: f64,
    b: f64,
    n: usize,
) -> Result<(crate::ast::Expr, String, f64), String> {
    if !a.is_finite() || !b.is_finite() {
        return Err("extremos no finitos".to_string());
    }
    if a >= b {
        return Err("se esperaba a < b".to_string());
    }
    if !((b - a).is_finite()) {
        return Err("ancho no finito".to_string());
    }
    if n == 0 || n > MAX_RIEMANN_SUBINTERVALS {
        return Err(format!("n debe estar en 1..={MAX_RIEMANN_SUBINTERVALS}"));
    }
    let clean = check_bytes(expr)?;
    let variable = check_var(var)?;
    let ast = parse_expr(&clean)?;
    Ok((ast, variable, (b - a) / (n as f64)))
}

fn eval_finite(ast: &crate::ast::Expr, var: &str, at: f64) -> Result<f64, String> {
    let value = ast.eval_at(var, at);
    if !value.is_finite() {
        return Err("integrando no finito en la partición (posible polo)".to_string());
    }
    Ok(value)
}

/// Suma genérica por `method`.
pub fn riemann_sum(
    expr: &str,
    var: &str,
    a: f64,
    b: f64,
    n: usize,
    method: RiemannMethod,
) -> Result<f64, String> {
    let (ast, variable, dx) = check_riemann(expr, var, a, b, n)?;
    if !dx.is_finite() || dx <= 0.0 {
        return Err("paso no finito".to_string());
    }
    let mut total = 0.0;
    for index in 0..n {
        let left = a + (index as f64) * dx;
        let right = left + dx;
        let contribution = match method {
            RiemannMethod::Left => eval_finite(&ast, &variable, left)?,
            RiemannMethod::Midpoint => eval_finite(&ast, &variable, (left + right) / 2.0)?,
            RiemannMethod::Trapezoidal => {
                let first = eval_finite(&ast, &variable, left)?;
                let second = eval_finite(&ast, &variable, right)?;
                (first + second) / 2.0
            }
            RiemannMethod::Lower | RiemannMethod::Upper => {
                let mut best: Option<f64> = None;
                for inner in 0..RIEMANN_INNER_SAMPLES {
                    let sample =
                        left + dx * ((inner as f64) + 0.5) / (RIEMANN_INNER_SAMPLES as f64);
                    let value = eval_finite(&ast, &variable, sample)?;
                    best = Some(match best {
                        None => value,
                        Some(current) if method == RiemannMethod::Lower => current.min(value),
                        Some(current) => current.max(value),
                    });
                }
                let edge_left = eval_finite(&ast, &variable, left)?;
                let edge_right = eval_finite(&ast, &variable, right)?;
                let mut current = best.unwrap_or(edge_left);
                if method == RiemannMethod::Lower {
                    current = current.min(edge_left).min(edge_right);
                } else {
                    current = current.max(edge_left).max(edge_right);
                }
                current
            }
        };
        total += contribution * dx;
        if !total.is_finite() {
            return Err("suma no finita".to_string());
        }
    }
    Ok(total)
}

/// Suma izquierda.
pub fn left_sum(expr: &str, var: &str, a: f64, b: f64, n: usize) -> Result<f64, String> {
    riemann_sum(expr, var, a, b, n, RiemannMethod::Left)
}

/// Suma inferior (mínimo estimado por subintervalo).
pub fn lower_sum(expr: &str, var: &str, a: f64, b: f64, n: usize) -> Result<f64, String> {
    riemann_sum(expr, var, a, b, n, RiemannMethod::Lower)
}

/// Suma superior (máximo estimado por subintervalo).
pub fn upper_sum(expr: &str, var: &str, a: f64, b: f64, n: usize) -> Result<f64, String> {
    riemann_sum(expr, var, a, b, n, RiemannMethod::Upper)
}

/// Suma trapezoidal.
pub fn trapezoidal_sum(expr: &str, var: &str, a: f64, b: f64, n: usize) -> Result<f64, String> {
    riemann_sum(expr, var, a, b, n, RiemannMethod::Trapezoidal)
}

/// Suma de rectángulos (punto medio).
pub fn rectangle_sum(expr: &str, var: &str, a: f64, b: f64, n: usize) -> Result<f64, String> {
    riemann_sum(expr, var, a, b, n, RiemannMethod::Midpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_and_right_side_split_equation() {
        assert_eq!(left_side("y = x^2").expect("izq"), "y");
        assert_eq!(right_side("y = x^2").expect("der"), "x^2");
    }

    #[test]
    fn sides_reject_honest_errors_and_bounds() {
        assert!(left_side("x+1").is_err());
        assert!(right_side("a=b=c").is_err());
        assert!(left_side("").is_err());
        assert!(left_side(&"x".repeat(2001)).is_err());
    }

    #[test]
    fn implicit_circle_gives_minus_x_over_y() {
        let out = implicit_derivative("x^2+y^2-1", "x", "y").expect("implícita");
        assert!(out.contains('x') && out.contains('y'), "got {out}");
        let at = implicit_derivative_at("x^2+y^2-1", "x", "y", 0.0, 1.0).expect("punto");
        assert!((at - 0.0).abs() < 1e-9, "got {at}");
    }

    #[test]
    fn implicit_errors_are_honest() {
        assert!(implicit_derivative("x+y", "x", "x").is_err());
        assert!(implicit_derivative_at("x^2+y^2-1", "x", "y", 1.0, 0.0).is_err());
        assert!(implicit_derivative("", "x", "y").is_err());
    }

    #[test]
    fn iteration_counts_and_bounds() {
        let values = iterate_function("x+1", "x", 0.0, 3).expect("itera");
        assert_eq!(values.len(), 3);
        assert!((values[2] - 3.0).abs() < 1e-12);
        assert!(iterate_function("x", "x", 0.0, MAX_ITERATION_COUNT + 1).is_err());
        assert!(
            iterate_function("1/0", "x", 0.0, 2).is_err()
                || iterate_function("x/0", "x", 1.0, 1).is_err()
        );
    }

    #[test]
    fn numeric_evaluates_and_rejects() {
        let pi = numeric("pi").expect("pi");
        assert!((pi - std::f64::consts::PI).abs() < 1e-9);
        assert!((numeric_at("x^2", "x", 3.0).expect("cuadrado") - 9.0).abs() < 1e-9);
        assert!(numeric("").is_err());
        assert!(numeric_at("x", "x", f64::NAN).is_err());
    }

    #[test]
    fn exponential_formats_and_rejects() {
        let zero = to_exponential(0.0, 0.0).expect("cero");
        assert_eq!(zero, "0");
        let one = to_exponential(1.0, 0.0).expect("uno");
        assert!(one.contains("e^(i*"), "got {one}");
        assert!(to_exponential(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn factors_cover_happy_error_bounds() {
        assert_eq!(factors(12).expect("12"), vec![(2, 2), (3, 1)]);
        assert_eq!(format_factors(&[(2, 2), (3, 1)]), "2^2 * 3");
        assert!(factors(0).is_err());
        assert!(factors(MAX_FACTORS_ABS + 1).is_err());
        assert_eq!(factors(1).expect("1").len(), 0);
        assert_eq!(
            factors_from_text("12").expect("texto"),
            vec![(2, 2), (3, 1)]
        );
        assert!(factors_from_text("hola").is_err());
    }

    #[test]
    fn are_equal_proves_and_stays_honest() {
        assert_eq!(
            are_equal("2+2", "4").expect("iguales"),
            EqualityVerdict::Equal
        );
        assert_eq!(
            are_equal("x", "x+1").expect("distintas"),
            EqualityVerdict::NotEqual
        );
        assert_eq!(
            are_equal("sin(x)^2+cos(x)^2", "1").expect("trig"),
            EqualityVerdict::Unknown
        );
        assert!(are_equal("", "x").is_err());
    }

    #[test]
    fn removable_detects_hole_and_continuous() {
        let hole = removable_discontinuity("(x^2-1)/(x-1)", "x", 1.0).expect("hueco");
        assert!(hole.is_removable);
        assert!(hole.limit.is_some_and(|v| (v - 2.0).abs() < 1e-4));
        let smooth = removable_discontinuity("x^2", "x", 1.0).expect("lisa");
        assert!(!smooth.is_removable);
        assert!(removable_discontinuity("x", "x", f64::INFINITY).is_err());
    }

    #[test]
    fn inflections_find_cubic_and_reject() {
        let points = inflection_points("x^3", "x", -2.0, 2.0).expect("cúbica");
        assert!(!points.is_empty());
        assert!(points.iter().any(|p| p.abs() < 0.2), "got {points:?}");
        let none = inflection_points("x^2", "x", -2.0, 2.0).expect("parábola");
        assert!(none.is_empty());
        assert!(inflection_points("x", "x", 1.0, 0.0).is_err());
    }

    #[test]
    fn dimension_and_identity_bounds() {
        assert_eq!(
            matrix_dimension(&[vec![1.0, 2.0], vec![3.0, 4.0]]).expect("2x2"),
            (2, 2)
        );
        assert!(matrix_dimension(&[]).is_err());
        assert!(matrix_dimension(&[vec![1.0], vec![1.0, 2.0]]).is_err());
        let identity = identity_matrix(2).expect("identidad");
        assert_eq!(identity, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert!(identity_matrix(0).is_err());
        assert!(identity_matrix(MAX_IDENTITY_DIM + 1).is_err());
    }

    #[test]
    fn rank_uses_exact_and_float_paths() {
        let full = matrix_rank(&[vec![1.0, 0.0], vec![0.0, 1.0]]).expect("rango 2");
        assert_eq!(full, 2);
        let singular = matrix_rank(&[vec![1.0, 2.0], vec![2.0, 4.0]]).expect("rango 1");
        assert_eq!(singular, 1);
        assert!(matrix_rank(&[]).is_err());
        assert!(matrix_rank(&[vec![f64::NAN]]).is_err());
    }

    #[test]
    fn random_is_deterministic_and_bounded() {
        let first = random_between(1, 10, 7).expect("sorteo");
        let second = random_between(1, 10, 7).expect("repite");
        assert_eq!(first, second);
        assert!((1..=10).contains(&first));
        assert!(random_between(10, 1, 0).is_err());
        let poly = random_polynomial(3, 11).expect("poli");
        assert_eq!(poly.len(), 4);
        assert!(random_polynomial(MAX_RANDOM_POLY_DEGREE + 1, 0).is_err());
        let again = random_polynomial(3, 11).expect("repite poli");
        assert_eq!(poly, again);
    }

    #[test]
    fn finance_matches_closed_forms() {
        let future = future_value(0.0, 12, 100.0, 1000.0).expect("vf tasa 0");
        assert!((future - 2200.0).abs() < 1e-9);
        let present = present_value(0.0, 12, 100.0, 2200.0).expect("va tasa 0");
        assert!((present - 1000.0).abs() < 1e-9);
        let fee = payment(0.0, 12, 1000.0, 2200.0).expect("cuota tasa 0");
        assert!((fee - 100.0).abs() < 1e-9);
        assert!(future_value(-2.0, 12, 0.0, 1.0).is_err());
        assert!(future_value(0.05, 0, 0.0, 1.0).is_err());
    }

    #[test]
    fn riemann_sums_converge_and_bound() {
        let left = left_sum("x", "x", 0.0, 1.0, 1000).expect("izq");
        assert!((left - 0.5).abs() < 0.01, "got {left}");
        let trap = trapezoidal_sum("x", "x", 0.0, 1.0, 1000).expect("trap");
        assert!((trap - 0.5).abs() < 1e-6, "got {trap}");
        let rect = rectangle_sum("1", "x", 0.0, 2.0, 10).expect("rect");
        assert!((rect - 2.0).abs() < 1e-9, "got {rect}");
        let lower = lower_sum("x", "x", 0.0, 1.0, 100).expect("inf");
        let upper = upper_sum("x", "x", 0.0, 1.0, 100).expect("sup");
        assert!(lower <= 0.5 + 0.02 && upper >= 0.5 - 0.02);
        assert!(left_sum("x", "x", 0.0, 1.0, 0).is_err());
        assert!(left_sum("x", "x", 1.0, 0.0, 10).is_err());
        assert!(left_sum("1/(x-0.5)", "x", 0.0, 1.0, 10).is_err());
    }
}
