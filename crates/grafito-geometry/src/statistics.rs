use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Máximo de clases que puede generar un histograma para limitar memoria y trabajo.
pub const MAX_HISTOGRAM_BINS: usize = 4_096;
/// Máximo grado admitido por la regresión polinómica densa.
pub const MAX_POLYNOMIAL_REGRESSION_DEGREE: usize = 16;
/// Máxima cantidad de pares que puede procesar un ajuste local persistente.
pub const MAX_FIT_DATA_POINTS: usize = 20_000;
/// Máximo de términos que puede recorrer una CDF discreta por suma directa.
pub const MAX_DISCRETE_CDF_ITERATIONS: usize = 10_001;

const MIN_SINUSOIDAL_SAMPLES: usize = 4;
const SINUSOIDAL_FREQUENCY_STEPS: usize = 256;
const FIT_EPSILON: f64 = 1e-12;

/// Familia de modelo usada por un ajuste local de datos `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitKind {
    Linear,
    Polynomial { degree: usize },
    Exponential,
    Logarithmic,
    Power,
    Sinusoidal,
}

impl FitKind {
    /// Cantidad de parámetros esperada en un resultado válido de este modelo.
    pub fn coefficient_count(self) -> Option<usize> {
        match self {
            Self::Linear | Self::Exponential | Self::Logarithmic | Self::Power => Some(2),
            Self::Polynomial { degree } => degree.checked_add(1),
            Self::Sinusoidal => Some(4),
        }
    }

    /// Nombre corto legible para el panel de datos y los mensajes de comando.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Linear => "lineal",
            Self::Polynomial { .. } => "polinómico",
            Self::Exponential => "exponencial",
            Self::Logarithmic => "logarítmico",
            Self::Power => "potencia",
            Self::Sinusoidal => "sinusoidal",
        }
    }
}

/// Diagnósticos de un ajuste expresados en las unidades originales de `y`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitDiagnostics {
    pub residuals: Vec<f64>,
    pub rmse: f64,
    pub r_squared: f64,
}

/// Resultado serializable y reproducible de un ajuste local.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitResult {
    pub kind: FitKind,
    pub coefficients: Vec<f64>,
    /// Centro aplicado a `x` antes de evaluar los modelos que lo requieren.
    #[serde(default)]
    pub x_offset: f64,
    /// Escala positiva aplicada a `x` antes de evaluar los modelos que lo requieren.
    #[serde(default = "default_fit_x_scale")]
    pub x_scale: f64,
    pub diagnostics: FitDiagnostics,
}

fn default_fit_x_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy)]
struct XNormalization {
    offset: f64,
    scale: f64,
}

impl XNormalization {
    const IDENTITY: Self = Self {
        offset: 0.0,
        scale: 1.0,
    };
}

impl FitResult {
    /// Evalúa el modelo ajustado en una coordenada `x`.
    pub fn predict(&self, x: f64) -> f64 {
        evaluate_fit(
            self.kind,
            &self.coefficients,
            x,
            XNormalization {
                offset: self.x_offset,
                scale: self.x_scale,
            },
        )
        .unwrap_or(f64::NAN)
    }

    /// Devuelve una expresión compatible con el motor de funciones de Grafito.
    pub fn expression(&self) -> String {
        fit_expression(
            self.kind,
            &self.coefficients,
            XNormalization {
                offset: self.x_offset,
                scale: self.x_scale,
            },
        )
        .unwrap_or_default()
    }
}

/// Ajusta un modelo acotado a pares de datos locales y devuelve sus diagnósticos.
///
/// Los residuales y RMSE siempre se calculan en unidades de `y`, incluso para
/// los modelos que usan una transformación logarítmica interna.
pub fn fit_xy(kind: FitKind, xs: &[f64], ys: &[f64]) -> Result<FitResult, String> {
    let minimum_samples = match kind {
        FitKind::Polynomial { degree } => {
            if degree == 0 || degree > MAX_POLYNOMIAL_REGRESSION_DEGREE {
                return Err(format!(
                    "el grado polinómico debe estar entre 1 y {MAX_POLYNOMIAL_REGRESSION_DEGREE}"
                ));
            }
            degree
                .checked_add(1)
                .ok_or_else(|| "el grado polinómico no es representable".to_string())?
        }
        FitKind::Sinusoidal => MIN_SINUSOIDAL_SAMPLES,
        _ => 2,
    };
    validate_fit_samples(xs, ys, minimum_samples)?;

    let normalizes_x = matches!(
        kind,
        FitKind::Linear | FitKind::Polynomial { .. } | FitKind::Exponential | FitKind::Sinusoidal
    );
    let normalization = if normalizes_x {
        x_normalization(xs)?
    } else {
        XNormalization::IDENTITY
    };
    let normalized_xs = normalize_xs(xs, normalization)?;
    let model_xs = if normalizes_x {
        normalized_xs.as_slice()
    } else {
        xs
    };

    let coefficients = match kind {
        FitKind::Linear => {
            let (slope, intercept, _) = linear_regression(model_xs, ys)
                .ok_or_else(|| "la variación de x no permite un ajuste lineal".to_string())?;
            vec![slope, intercept]
        }
        FitKind::Polynomial { degree } => polynomial_regression(model_xs, ys, degree)
            .ok_or_else(|| "los datos no permiten ese ajuste polinómico".to_string())?,
        FitKind::Exponential => {
            if ys.iter().any(|value| *value <= 0.0) {
                return Err("el ajuste exponencial requiere valores y positivos".to_string());
            }
            let (scale, rate, _) = exponential_regression(model_xs, ys)
                .ok_or_else(|| "los datos no permiten un ajuste exponencial".to_string())?;
            vec![scale, rate]
        }
        FitKind::Logarithmic => {
            if xs.iter().any(|value| *value <= 0.0) {
                return Err("el ajuste logarítmico requiere valores x positivos".to_string());
            }
            let (scale, intercept, _) = logarithmic_regression(xs, ys)
                .ok_or_else(|| "los datos no permiten un ajuste logarítmico".to_string())?;
            vec![scale, intercept]
        }
        FitKind::Power => {
            if xs.iter().any(|value| *value <= 0.0) || ys.iter().any(|value| *value <= 0.0) {
                return Err("el ajuste de potencia requiere valores x e y positivos".to_string());
            }
            let (scale, exponent, _) = power_regression(xs, ys)
                .ok_or_else(|| "los datos no permiten un ajuste de potencia".to_string())?;
            vec![scale, exponent]
        }
        FitKind::Sinusoidal => sinusoidal_regression(model_xs, ys)?,
    };

    if coefficients.iter().any(|value| !value.is_finite()) {
        return Err("el ajuste produjo parámetros no finitos".to_string());
    }
    let diagnostics = fit_diagnostics(kind, &coefficients, xs, ys, normalization)?;
    Ok(FitResult {
        kind,
        coefficients,
        x_offset: normalization.offset,
        x_scale: normalization.scale,
        diagnostics,
    })
}

fn validate_fit_samples(xs: &[f64], ys: &[f64], minimum_samples: usize) -> Result<(), String> {
    if xs.len() != ys.len() {
        return Err("las listas x e y deben tener la misma longitud".to_string());
    }
    if xs.len() < minimum_samples {
        return Err(format!(
            "se necesitan al menos {minimum_samples} pares de datos para este ajuste"
        ));
    }
    if xs.len() > MAX_FIT_DATA_POINTS {
        return Err(format!(
            "la tabla supera el máximo de {MAX_FIT_DATA_POINTS} pares para un ajuste"
        ));
    }
    if xs.iter().chain(ys.iter()).any(|value| !value.is_finite()) {
        return Err("todos los pares de datos deben ser finitos".to_string());
    }
    Ok(())
}

fn x_normalization(xs: &[f64]) -> Result<XNormalization, String> {
    let minimum = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // Halving both finite endpoints avoids overflowing when they share a sign.
    let offset = minimum * 0.5 + maximum * 0.5;
    let scale = xs
        .iter()
        .map(|value| (value - offset).abs())
        .fold(0.0_f64, f64::max);
    if !offset.is_finite() || !scale.is_finite() || scale == 0.0 {
        return Err("la variación de x no permite un ajuste estable".to_string());
    }
    Ok(XNormalization { offset, scale })
}

fn normalize_xs(xs: &[f64], normalization: XNormalization) -> Result<Vec<f64>, String> {
    let normalized = xs
        .iter()
        .map(|value| (value - normalization.offset) / normalization.scale)
        .collect::<Vec<_>>();
    if normalized.iter().any(|value| !value.is_finite()) {
        return Err("la normalización de x no es representable".to_string());
    }
    Ok(normalized)
}

fn fit_diagnostics(
    kind: FitKind,
    coefficients: &[f64],
    xs: &[f64],
    ys: &[f64],
    normalization: XNormalization,
) -> Result<FitDiagnostics, String> {
    let mut residuals = Vec::with_capacity(xs.len());
    for (&x, &y) in xs.iter().zip(ys) {
        let prediction = evaluate_fit(kind, coefficients, x, normalization)?;
        if !prediction.is_finite() {
            return Err("el ajuste produjo predicciones no finitas".to_string());
        }
        let residual = y - prediction;
        if !residual.is_finite() {
            return Err("el ajuste produjo residuales no finitos".to_string());
        }
        residuals.push(residual);
    }

    let rmse = root_mean_square(&residuals)
        .ok_or_else(|| "el RMSE del ajuste no es representable".to_string())?;
    let y_mean = stable_mean(ys).ok_or_else(|| "la media de y no es representable".to_string())?;
    let (residual_scale, residual_sum) = scaled_sum_squares(&residuals, 0.0);
    let (total_scale, total_sum) = scaled_sum_squares(ys, y_mean);
    let r_squared = if total_scale == 0.0 || total_sum == 0.0 {
        if rmse <= FIT_EPSILON * y_mean.abs().max(1.0) {
            1.0
        } else {
            0.0
        }
    } else {
        let ratio = residual_scale / total_scale;
        1.0 - ratio * ratio * residual_sum / total_sum
    };
    if !r_squared.is_finite() {
        return Err("R² del ajuste no es representable".to_string());
    }

    Ok(FitDiagnostics {
        residuals,
        rmse,
        r_squared,
    })
}

fn stable_mean(values: &[f64]) -> Option<f64> {
    let mut mean = 0.0;
    for (index, &value) in values.iter().enumerate() {
        mean += (value - mean) / (index + 1) as f64;
        if !mean.is_finite() {
            return None;
        }
    }
    (!values.is_empty()).then_some(mean)
}

fn root_mean_square(values: &[f64]) -> Option<f64> {
    let (scale, sum) = scaled_sum_squares(values, 0.0);
    if scale == 0.0 {
        return Some(0.0);
    }
    let result = scale * (sum / values.len() as f64).sqrt();
    result.is_finite().then_some(result)
}

fn scaled_sum_squares(values: &[f64], center: f64) -> (f64, f64) {
    let mut scale = 0.0;
    let mut sum = 0.0;
    for &value in values {
        let magnitude = (value - center).abs();
        if magnitude == 0.0 {
            continue;
        }
        if scale < magnitude {
            let ratio = if scale == 0.0 { 0.0 } else { scale / magnitude };
            sum = 1.0 + sum * ratio * ratio;
            scale = magnitude;
        } else {
            let ratio = magnitude / scale;
            sum += ratio * ratio;
        }
    }
    (scale, sum)
}

fn sinusoidal_regression(xs: &[f64], ys: &[f64]) -> Result<Vec<f64>, String> {
    let x_min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = x_max - x_min;
    if !span.is_finite() || span <= FIT_EPSILON * x_max.abs().max(x_min.abs()).max(1.0) {
        return Err("el ajuste sinusoidal requiere variación finita de x".to_string());
    }

    let base_frequency = std::f64::consts::TAU / span;
    let mut best: Option<(f64, Vec<f64>)> = None;
    for step in 1..=SINUSOIDAL_FREQUENCY_STEPS {
        let frequency = base_frequency * step as f64 / 4.0;
        let Some([sin_coefficient, cos_coefficient, offset]) =
            solve_sinusoidal_basis(xs, ys, frequency)
        else {
            continue;
        };
        let amplitude = sin_coefficient.hypot(cos_coefficient);
        let phase = cos_coefficient.atan2(sin_coefficient);
        let coefficients = vec![amplitude, frequency, phase, offset];
        let diagnostics = match fit_diagnostics(
            FitKind::Sinusoidal,
            &coefficients,
            xs,
            ys,
            XNormalization::IDENTITY,
        ) {
            Ok(diagnostics) => diagnostics,
            Err(_) => continue,
        };
        let improves_best = match best.as_ref() {
            Some((best_rmse, _)) => diagnostics.rmse < *best_rmse,
            None => true,
        };
        if improves_best {
            best = Some((diagnostics.rmse, coefficients));
        }
    }

    best.map(|(_, coefficients)| coefficients)
        .ok_or_else(|| "los datos no permiten un ajuste sinusoidal estable".to_string())
}

fn solve_sinusoidal_basis(xs: &[f64], ys: &[f64], frequency: f64) -> Option<[f64; 3]> {
    let mut matrix = [[0.0; 3]; 3];
    let mut rhs = [0.0; 3];
    for (&x, &y) in xs.iter().zip(ys) {
        let basis = [(frequency * x).sin(), (frequency * x).cos(), 1.0];
        for row in 0..3 {
            rhs[row] += basis[row] * y;
            for column in 0..3 {
                matrix[row][column] += basis[row] * basis[column];
            }
        }
    }
    solve_three_by_three(matrix, rhs)
}

fn solve_three_by_three(mut matrix: [[f64; 3]; 3], mut rhs: [f64; 3]) -> Option<[f64; 3]> {
    for column in 0..3 {
        let pivot = (column..3).max_by(|&left, &right| {
            matrix[left][column]
                .abs()
                .partial_cmp(&matrix[right][column].abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let pivot_value = matrix[column][column];
        if !pivot_value.is_finite() || pivot_value.abs() <= FIT_EPSILON {
            return None;
        }
        let pivot_row = matrix[column];
        for (row_index, row) in matrix.iter_mut().enumerate().skip(column + 1) {
            let factor = row[column] / pivot_value;
            for (entry, pivot_entry) in row.iter_mut().zip(pivot_row.iter()).skip(column) {
                *entry -= factor * pivot_entry;
            }
            rhs[row_index] -= factor * rhs[column];
        }
    }

    let mut result = [0.0; 3];
    for row in (0..3).rev() {
        let mut value = rhs[row];
        for (column, result_value) in result.iter().enumerate().skip(row + 1) {
            value -= matrix[row][column] * result_value;
        }
        result[row] = value / matrix[row][row];
    }
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}

fn evaluate_fit(
    kind: FitKind,
    coefficients: &[f64],
    x: f64,
    normalization: XNormalization,
) -> Result<f64, String> {
    if !x.is_finite() {
        return Err("x debe ser finito".to_string());
    }
    let expected = kind
        .coefficient_count()
        .ok_or_else(|| "la cantidad de parámetros del ajuste no es representable".to_string())?;
    if coefficients.len() != expected || coefficients.iter().any(|value| !value.is_finite()) {
        return Err("los parámetros del ajuste no son válidos".to_string());
    }
    if !normalization.offset.is_finite()
        || !normalization.scale.is_finite()
        || normalization.scale <= 0.0
    {
        return Err("la normalización del ajuste no es válida".to_string());
    }
    let normalized_x = (x - normalization.offset) / normalization.scale;
    if !normalized_x.is_finite() {
        return Err("la coordenada x normalizada no es representable".to_string());
    }
    let value = match kind {
        FitKind::Linear => coefficients[0].mul_add(normalized_x, coefficients[1]),
        FitKind::Polynomial { .. } => coefficients
            .iter()
            .rev()
            .fold(0.0_f64, |accumulator, coefficient| {
                accumulator.mul_add(normalized_x, *coefficient)
            }),
        FitKind::Exponential => coefficients[0] * (coefficients[1] * normalized_x).exp(),
        FitKind::Logarithmic => {
            if x <= 0.0 {
                return Err("el ajuste logarítmico no está definido para x <= 0".to_string());
            }
            coefficients[0].mul_add(x.ln(), coefficients[1])
        }
        FitKind::Power => {
            if x <= 0.0 {
                return Err("el ajuste de potencia no está definido para x <= 0".to_string());
            }
            coefficients[0] * x.powf(coefficients[1])
        }
        FitKind::Sinusoidal => {
            coefficients[0] * (coefficients[1] * normalized_x + coefficients[2]).sin()
                + coefficients[3]
        }
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| "el ajuste produjo una predicción no finita".to_string())
}

fn fit_expression(
    kind: FitKind,
    coefficients: &[f64],
    normalization: XNormalization,
) -> Result<String, String> {
    let scalar = |index: usize| {
        coefficients
            .get(index)
            .filter(|value| value.is_finite())
            .map(|value| format_fit_scalar(*value))
            .ok_or_else(|| "los parámetros del ajuste no son válidos".to_string())
    };
    let normalized_x = normalized_x_expression(normalization)?;
    match kind {
        FitKind::Linear => Ok(format!(
            "({})*{}+({})",
            scalar(0)?,
            normalized_x,
            scalar(1)?
        )),
        FitKind::Polynomial { degree } => {
            if degree.checked_add(1) != Some(coefficients.len()) {
                return Err("los parámetros polinómicos no son válidos".to_string());
            }
            Ok(coefficients
                .iter()
                .enumerate()
                .map(|(index, coefficient)| match index {
                    0 => format!("({})", format_fit_scalar(*coefficient)),
                    1 => format!("({})*{}", format_fit_scalar(*coefficient), normalized_x),
                    _ => format!(
                        "({})*({})^{index}",
                        format_fit_scalar(*coefficient),
                        normalized_x
                    ),
                })
                .collect::<Vec<_>>()
                .join("+"))
        }
        FitKind::Exponential => Ok(format!(
            "({})*exp(({})*{})",
            scalar(0)?,
            scalar(1)?,
            normalized_x
        )),
        FitKind::Logarithmic => Ok(format!("({})*ln(x)+({})", scalar(0)?, scalar(1)?)),
        FitKind::Power => Ok(format!("({})*x^({})", scalar(0)?, scalar(1)?)),
        FitKind::Sinusoidal => Ok(format!(
            "({})*sin(({})*{}+({}))+({})",
            scalar(0)?,
            scalar(1)?,
            normalized_x,
            scalar(2)?,
            scalar(3)?
        )),
    }
}

fn format_fit_scalar(value: f64) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}

fn normalized_x_expression(normalization: XNormalization) -> Result<String, String> {
    if !normalization.offset.is_finite()
        || !normalization.scale.is_finite()
        || normalization.scale <= 0.0
    {
        return Err("la normalización del ajuste no es válida".to_string());
    }
    if normalization.offset == 0.0 && normalization.scale == 1.0 {
        Ok("x".to_string())
    } else {
        Ok(format!(
            "((x-({}))/({}))",
            format_fit_scalar(normalization.offset),
            format_fit_scalar(normalization.scale)
        ))
    }
}

fn discrete_cdf_within_budget(k: u32) -> bool {
    usize::try_from(k)
        .ok()
        .and_then(|value| value.checked_add(1))
        .is_some_and(|iterations| iterations <= MAX_DISCRETE_CDF_ITERATIONS)
}

pub fn mean(data: &[f64]) -> Option<f64> {
    if data.is_empty() || data.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(data.iter().sum::<f64>() / data.len() as f64)
}

pub fn median(data: &[f64]) -> Option<f64> {
    if data.is_empty() || data.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n.is_multiple_of(2) {
        Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2.0)
    } else {
        Some(sorted[n / 2])
    }
}

pub fn mode(data: &[f64]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let mut counts: HashMap<String, (f64, usize)> = HashMap::new();
    for &v in data {
        let key = format!("{:.10}", v);
        let entry = counts.entry(key).or_insert((v, 0));
        entry.1 += 1;
    }
    counts.values().max_by_key(|(_, c)| *c).map(|(v, _)| *v)
}

pub fn variance(data: &[f64]) -> Option<f64> {
    let m = mean(data)?;
    let n = data.len() as f64;
    if n < 2.0 {
        return None;
    }
    Some(data.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0))
}

pub fn std_dev(data: &[f64]) -> Option<f64> {
    variance(data).map(|v| v.sqrt())
}

pub fn min(data: &[f64]) -> Option<f64> {
    data.iter().cloned().reduce(f64::min)
}

pub fn max(data: &[f64]) -> Option<f64> {
    data.iter().cloned().reduce(f64::max)
}

pub fn range(data: &[f64]) -> Option<f64> {
    Some(max(data)? - min(data)?)
}

pub fn quantile(data: &[f64], q: f64) -> Option<f64> {
    if data.is_empty() || !(0.0..=1.0).contains(&q) {
        return None;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Some(sorted[lo] * (1.0 - frac) + sorted[hi.min(sorted.len() - 1)] * frac)
}

pub fn q1(data: &[f64]) -> Option<f64> {
    quantile(data, 0.25)
}
pub fn q3(data: &[f64]) -> Option<f64> {
    quantile(data, 0.75)
}
pub fn iqr(data: &[f64]) -> Option<f64> {
    Some(q3(data)? - q1(data)?)
}

pub fn covariance(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let mx = mean(xs)?;
    let my = mean(ys)?;
    let n = xs.len() as f64;
    Some(
        xs.iter()
            .zip(ys.iter())
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>()
            / (n - 1.0),
    )
}

pub fn pearson_correlation(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let cov = covariance(xs, ys)?;
    let sx = std_dev(xs)?;
    let sy = std_dev(ys)?;
    if sx == 0.0 || sy == 0.0 {
        return None;
    }
    Some(cov / (sx * sy))
}

pub fn linear_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    if xs.len() != ys.len()
        || xs.len() < 2
        || xs.iter().chain(ys.iter()).any(|value| !value.is_finite())
    {
        return None;
    }
    let mx = stable_mean(xs)?;
    let my = stable_mean(ys)?;
    let x_scale = xs.iter().map(|x| (*x - mx).abs()).fold(0.0_f64, f64::max);
    if !x_scale.is_finite() || x_scale == 0.0 {
        return None;
    }
    let y_scale = ys.iter().map(|y| (*y - my).abs()).fold(0.0_f64, f64::max);
    if !y_scale.is_finite() {
        return None;
    }
    if y_scale == 0.0 {
        return Some((0.0, my, 1.0));
    }

    let mut ss_xx = 0.0;
    let mut ss_xy = 0.0;
    let mut ss_yy = 0.0;
    for (&x, &y) in xs.iter().zip(ys) {
        let normalized_x = (x - mx) / x_scale;
        let normalized_y = (y - my) / y_scale;
        ss_xx += normalized_x * normalized_x;
        ss_xy += normalized_x * normalized_y;
        ss_yy += normalized_y * normalized_y;
    }
    if ss_xx == 0.0 || ss_yy == 0.0 {
        return None;
    }
    let slope = y_scale / x_scale * (ss_xy / ss_xx);
    let intercept = my - slope * mx;
    let r_squared = ss_xy * ss_xy / (ss_xx * ss_yy);
    (slope.is_finite() && intercept.is_finite() && r_squared.is_finite())
        .then_some((slope, intercept, r_squared))
}

pub fn polynomial_regression(xs: &[f64], ys: &[f64], degree: usize) -> Option<Vec<f64>> {
    if degree > MAX_POLYNOMIAL_REGRESSION_DEGREE || xs.len() != ys.len() || xs.len() <= degree {
        return None;
    }
    let m = degree.checked_add(1)?;
    let mut a = vec![vec![0.0f64; m + 1]; m];
    #[allow(clippy::needless_range_loop)]
    for (i, row) in a.iter_mut().enumerate() {
        for j in 0..m {
            row[j] = xs.iter().map(|x| x.powi((i + j) as i32)).sum();
        }
        row[m] = xs
            .iter()
            .zip(ys.iter())
            .map(|(x, y)| y * x.powi(i as i32))
            .sum();
    }
    gauss_elimination(&mut a, m)
}

fn gauss_elimination(a: &mut [Vec<f64>], n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let mut max_row = col;
        for row in (col + 1)..n {
            if a[row][col].abs() > a[max_row][col].abs() {
                max_row = row;
            }
        }
        a.swap(col, max_row);
        if a[col][col].abs() < 1e-15 {
            return None;
        }
        #[allow(clippy::needless_range_loop)]
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            for j in col..=n {
                a[row][j] -= factor * a[col][j];
            }
        }
    }
    let mut result = vec![0.0; n];
    for i in (0..n).rev() {
        result[i] = a[i][n];
        for j in (i + 1)..n {
            result[i] -= a[i][j] * result[j];
        }
        result[i] /= a[i][i];
    }
    Some(result)
}

pub fn exponential_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_ys: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
    if log_ys.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    let (slope, intercept, r2) = linear_regression(xs, &log_ys)?;
    Some((intercept.exp(), slope, r2))
}

pub fn logarithmic_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_xs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
    if log_xs.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    linear_regression(&log_xs, ys)
}

pub fn power_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_xs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
    let log_ys: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
    if log_xs.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    if log_ys.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    let (slope, intercept, r2) = linear_regression(&log_xs, &log_ys)?;
    Some((intercept.exp(), slope, r2))
}

pub fn histogram(data: &[f64], bins: usize) -> Vec<(f64, f64, f64)> {
    if data.is_empty() || bins == 0 {
        return vec![];
    }
    let finite_data: Vec<f64> = data
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite_data.is_empty() {
        return vec![];
    }
    let bins = bins.min(MAX_HISTOGRAM_BINS);
    let (Some(lo), Some(hi)) = (min(&finite_data), max(&finite_data)) else {
        return vec![];
    };
    let width = if (hi - lo).abs() < 1e-15 {
        1.0
    } else {
        (hi - lo) / bins as f64
    };
    let mut counts = vec![0usize; bins];
    for v in finite_data {
        let idx = ((v - lo) / width).floor() as usize;
        let idx = idx.min(bins - 1);
        counts[idx] += 1;
    }
    counts
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let left = lo + i as f64 * width;
            (left, left + width, c as f64)
        })
        .collect()
}

pub fn frequency_table(data: &[f64]) -> Vec<(f64, usize, f64, f64)> {
    let mut counts: HashMap<String, (f64, usize)> = HashMap::new();
    for &v in data {
        let key = format!("{:.10}", v);
        let entry = counts.entry(key).or_insert((v, 0));
        entry.1 += 1;
    }
    let n = data.len() as f64;
    let mut table: Vec<_> = counts
        .values()
        .map(|(v, c)| (*v, *c, *c as f64 / n, *c as f64 / n))
        .collect();
    table.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut cum = 0.0;
    for entry in &mut table {
        cum += entry.3;
        entry.3 = cum;
    }
    table
}

pub fn boxplot_stats(data: &[f64]) -> Option<(f64, f64, f64, f64, f64, Vec<f64>)> {
    let q1 = q1(data)?;
    let med = median(data)?;
    let q3 = q3(data)?;
    let iqr = q3 - q1;
    let lower_fence = q1 - 1.5 * iqr;
    let upper_fence = q3 + 1.5 * iqr;
    let outliers: Vec<f64> = data
        .iter()
        .cloned()
        .filter(|&v| v < lower_fence || v > upper_fence)
        .collect();
    let whisker_lo = data
        .iter()
        .cloned()
        .filter(|&v| v >= lower_fence)
        .reduce(f64::min)
        .unwrap_or(q1);
    let whisker_hi = data
        .iter()
        .cloned()
        .filter(|&v| v <= upper_fence)
        .reduce(f64::max)
        .unwrap_or(q3);
    Some((whisker_lo, q1, med, q3, whisker_hi, outliers))
}

fn erf(x: f64) -> f64 {
    crate::special_functions::erf(x)
}

fn gamma_ln(x: f64) -> f64 {
    crate::special_functions::ln_gamma(x)
}

pub fn normal_pdf(x: f64, mu: f64, sigma: f64) -> f64 {
    if x.is_nan() || !mu.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
        return f64::NAN;
    }
    let z = (x - mu) / sigma;
    (-0.5 * z * z).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt())
}

pub fn normal_cdf(x: f64, mu: f64, sigma: f64) -> f64 {
    if x.is_nan() || !mu.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
        return f64::NAN;
    }
    (0.5 * (1.0 + erf((x - mu) / (sigma * std::f64::consts::SQRT_2)))).clamp(0.0, 1.0)
}

// The published rational approximation coefficients need their original literals.
#[allow(clippy::excessive_precision)]
pub fn normal_quantile(p: f64, mu: f64, sigma: f64) -> f64 {
    if !p.is_finite()
        || !(0.0..1.0).contains(&p)
        || !mu.is_finite()
        || !sigma.is_finite()
        || sigma <= 0.0
    {
        return f64::NAN;
    }
    let a = [
        -3.969_683_028_665_376e+01,
        2.209_460_984_245_205e+02,
        -2.759_285_104_469_687e+02,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e+01,
        2.506_628_277_459_239e+00,
    ];
    let b = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    let c = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    let d = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let p_low = 0.02425;
    let p_high = 1.0 - p_low;
    let q;
    let r;
    if p < p_low {
        q = (-2.0 * p.ln()).sqrt();
        let z = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
        mu + sigma * z
    } else if p <= p_high {
        q = p - 0.5;
        r = q * q;
        let z = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0);
        mu + sigma * z
    } else {
        q = (-2.0 * (1.0 - p).ln()).sqrt();
        let z = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
        mu + sigma * z
    }
}

pub fn binomial_pmf(n: u32, p: f64, k: u32) -> f64 {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if k > n {
        return 0.0;
    }
    if p == 0.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    if p == 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    let ln_coeff =
        gamma_ln(n as f64 + 1.0) - gamma_ln(k as f64 + 1.0) - gamma_ln((n - k) as f64 + 1.0);
    (ln_coeff + k as f64 * p.ln() + (n - k) as f64 * (1.0 - p).ln()).exp()
}

pub fn binomial_cdf(n: u32, p: f64, k: u32) -> f64 {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if k >= n {
        return 1.0;
    }
    if !discrete_cdf_within_budget(k) {
        return f64::NAN;
    }
    (0..=k).map(|i| binomial_pmf(n, p, i)).sum()
}

pub fn poisson_pmf(lambda: f64, k: u32) -> f64 {
    if !lambda.is_finite() || lambda < 0.0 {
        return f64::NAN;
    }
    if lambda == 0.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    (k as f64 * lambda.ln() - lambda - gamma_ln(k as f64 + 1.0)).exp()
}

pub fn poisson_cdf(lambda: f64, k: u32) -> f64 {
    if !lambda.is_finite() || lambda < 0.0 {
        return f64::NAN;
    }
    if !discrete_cdf_within_budget(k) {
        return f64::NAN;
    }
    (0..=k).map(|i| poisson_pmf(lambda, i)).sum()
}

pub fn student_t_pdf(x: f64, nu: f64) -> f64 {
    let coeff =
        (gamma_ln((nu + 1.0) / 2.0) - gamma_ln(nu / 2.0) - 0.5 * (nu * std::f64::consts::PI).ln())
            .exp();
    coeff * (1.0 + x * x / nu).powf(-(nu + 1.0) / 2.0)
}

pub fn student_t_cdf(x: f64, nu: f64) -> f64 {
    if x.is_nan() || !nu.is_finite() || nu <= 0.0 {
        return f64::NAN;
    }
    if nu == 1.0 {
        return if x < 0.0 {
            (-x.recip()).atan() / std::f64::consts::PI
        } else if x > 0.0 {
            1.0 - x.recip().atan() / std::f64::consts::PI
        } else {
            0.5
        };
    }

    let scaled_x = x.abs() / nu.sqrt();
    let t = if scaled_x <= 1.0 {
        1.0 / (1.0 + scaled_x * scaled_x)
    } else {
        let reciprocal = scaled_x.recip();
        let reciprocal_squared = reciprocal * reciprocal;
        reciprocal_squared / (1.0 + reciprocal_squared)
    };
    let i = regularized_incomplete_beta(nu / 2.0, 0.5, t);
    if x >= 0.0 {
        1.0 - 0.5 * i
    } else {
        0.5 * i
    }
}

/// Cuantil (inversa de la CDF) de la distribución t-Student.
/// Amplía el intervalo adaptativamente y usa bisección sobre `student_t_cdf`.
pub fn student_t_quantile(p: f64, nu: f64) -> f64 {
    if !p.is_finite() || p <= 0.0 || p >= 1.0 || !nu.is_finite() || nu <= 0.0 {
        return f64::NAN;
    }

    if p == 0.5 {
        return 0.0;
    }
    if nu == 1.0 {
        if !(0.25..=0.75).contains(&p) {
            let tail_probability = if p < 0.5 { p } else { 1.0 - p };
            let angle = std::f64::consts::PI * tail_probability;
            let magnitude = if angle < f64::EPSILON.sqrt() {
                angle.recip()
            } else {
                angle.tan().recip()
            };
            return if p < 0.5 { -magnitude } else { magnitude };
        }
        return (std::f64::consts::PI * (p - 0.5)).tan();
    }

    let (mut lo, mut hi) = if p < 0.5 { (-1.0, 0.0) } else { (0.0, 1.0) };
    if p < 0.5 {
        loop {
            let cdf = student_t_cdf(lo, nu);
            if !cdf.is_finite() {
                return f64::NAN;
            }
            if cdf <= p {
                break;
            }
            hi = lo;
            if lo <= -f64::MAX / 2.0 {
                lo = -f64::MAX;
                let edge_cdf = student_t_cdf(lo, nu);
                if !edge_cdf.is_finite() {
                    return f64::NAN;
                }
                if edge_cdf > p {
                    return f64::NEG_INFINITY;
                }
                break;
            }
            lo *= 2.0;
        }
    } else {
        loop {
            let cdf = student_t_cdf(hi, nu);
            if !cdf.is_finite() {
                return f64::NAN;
            }
            if cdf >= p {
                break;
            }
            lo = hi;
            if hi >= f64::MAX / 2.0 {
                hi = f64::MAX;
                let edge_cdf = student_t_cdf(hi, nu);
                if !edge_cdf.is_finite() {
                    return f64::NAN;
                }
                if edge_cdf < p {
                    return f64::INFINITY;
                }
                break;
            }
            hi *= 2.0;
        }
    }

    for _ in 0..128 {
        let mid = lo + (hi - lo) / 2.0;
        if mid == lo || mid == hi {
            break;
        }
        let cdf = student_t_cdf(mid, nu);
        if !cdf.is_finite() {
            return f64::NAN;
        }
        if cdf < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo + (hi - lo) / 2.0
}

fn regularized_incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_beta = gamma_ln(a) + gamma_ln(b) - gamma_ln(a + b);
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * continued_fraction_beta(a, b, x) / a
    } else {
        1.0 - front * continued_fraction_beta(b, a, 1.0 - x) / b
    }
}

fn continued_fraction_beta(a: f64, b: f64, x: f64) -> f64 {
    let max_iter = 200;
    let eps = 1e-14;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < 1e-30 {
        d = 1e-30;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=max_iter {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < eps {
            break;
        }
    }
    h
}

pub fn chi_squared_pdf(x: f64, k: f64) -> f64 {
    if x.is_nan() || !k.is_finite() || k <= 0.0 {
        return f64::NAN;
    }
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return if k < 2.0 {
            f64::INFINITY
        } else if k == 2.0 {
            0.5
        } else {
            0.0
        };
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    let half_k = k / 2.0;
    ((half_k - 1.0) * x.ln() - x / 2.0 - half_k * (2.0_f64.ln()) - gamma_ln(half_k)).exp()
}

pub fn chi_squared_cdf(x: f64, k: f64) -> f64 {
    if x.is_nan() || !k.is_finite() || k <= 0.0 {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    if x == f64::INFINITY {
        return 1.0;
    }
    regularized_gamma_lower(k / 2.0, x / 2.0).clamp(0.0, 1.0)
}

fn regularized_gamma_lower(a: f64, x: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        let mut sum = 1.0 / a;
        let mut term = 1.0 / a;
        for n in 1..200 {
            term *= x / (a + n as f64);
            sum += term;
            if term.abs() < 1e-14 * sum.abs() {
                break;
            }
        }
        sum * (-x + a * x.ln() - gamma_ln(a)).exp()
    } else {
        1.0 - regularized_gamma_upper(a, x)
    }
}

fn regularized_gamma_upper(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1e30;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..200 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = b + an / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    h * (-x + a * x.ln() - gamma_ln(a)).exp()
}

pub fn f_distribution_pdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let half_d1 = d1 / 2.0;
    let half_d2 = d2 / 2.0;
    let ln_coeff = half_d1 * d1.ln() + half_d2 * d2.ln() - gamma_ln(half_d1) - gamma_ln(half_d2)
        + gamma_ln(half_d1 + half_d2);
    let ln_val = ln_coeff + (half_d1 - 1.0) * x.ln() - (half_d1 + half_d2) * (d1 * x + d2).ln();
    ln_val.exp()
}

pub fn exponential_pdf(x: f64, lambda: f64) -> f64 {
    if x.is_nan() || !lambda.is_finite() || lambda <= 0.0 {
        return f64::NAN;
    }
    if x < 0.0 {
        return 0.0;
    }
    lambda * (-lambda * x).exp()
}

pub fn exponential_cdf(x: f64, lambda: f64) -> f64 {
    if x.is_nan() || !lambda.is_finite() || lambda <= 0.0 {
        return f64::NAN;
    }
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-lambda * x).exp()
}

pub fn geometric_pmf(p: f64, k: u32) -> f64 {
    (1.0 - p).powi(k as i32) * p
}

pub fn geometric_cdf(p: f64, k: u32) -> f64 {
    1.0 - (1.0 - p).powi(k as i32 + 1)
}

pub fn hypergeometric_pmf(n_pop: u32, k_success: u32, n_draw: u32, k_observed: u32) -> f64 {
    if k_success > n_pop || n_draw > n_pop || k_observed > k_success || k_observed > n_draw {
        return 0.0;
    }
    if n_draw - k_observed > n_pop - k_success {
        return 0.0;
    }
    let unobserved_successes = k_success - k_observed;
    let failures = n_pop - k_success;
    let unobserved_draws = n_draw - k_observed;
    let remaining_failures = failures - unobserved_draws;
    let remaining_population = n_pop - n_draw;
    let ln_num = gamma_ln(k_success as f64 + 1.0)
        - gamma_ln(k_observed as f64 + 1.0)
        - gamma_ln(unobserved_successes as f64 + 1.0)
        + gamma_ln(failures as f64 + 1.0)
        - gamma_ln(unobserved_draws as f64 + 1.0)
        - gamma_ln(remaining_failures as f64 + 1.0);
    let ln_den = gamma_ln(n_pop as f64 + 1.0)
        - gamma_ln(n_draw as f64 + 1.0)
        - gamma_ln(remaining_population as f64 + 1.0);
    (ln_num - ln_den).exp()
}

pub fn logistic_pdf(x: f64, mu: f64, s: f64) -> f64 {
    let z = (x - mu) / s;
    let ez = (-z).exp();
    ez / (s * (1.0 + ez).powi(2))
}

pub fn logistic_cdf(x: f64, mu: f64, s: f64) -> f64 {
    1.0 / (1.0 + (-(x - mu) / s).exp())
}

pub fn weibull_pdf(x: f64, k: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    (k / lambda) * (x / lambda).powf(k - 1.0) * (-(x / lambda).powf(k)).exp()
}

pub fn weibull_cdf(x: f64, k: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-(x / lambda).powf(k)).exp()
}

pub fn uniform_pdf(x: f64, a: f64, b: f64) -> f64 {
    if x.is_nan() || !a.is_finite() || !b.is_finite() || a >= b {
        return f64::NAN;
    }
    if x < a || x > b {
        return 0.0;
    }
    1.0 / (b - a)
}

pub fn uniform_cdf(x: f64, a: f64, b: f64) -> f64 {
    if x.is_nan() || !a.is_finite() || !b.is_finite() || a >= b {
        return f64::NAN;
    }
    if x < a {
        return 0.0;
    }
    if x > b {
        return 1.0;
    }
    (x - a) / (b - a)
}

pub fn gamma_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let coef = beta.powf(alpha) / super::special_functions::gamma(alpha);
    coef * x.powf(alpha - 1.0) * (-beta * x).exp()
}

pub fn beta_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) {
        return 0.0;
    }
    let coef = super::special_functions::gamma(alpha + beta)
        / (super::special_functions::gamma(alpha) * super::special_functions::gamma(beta));
    coef * x.powf(alpha - 1.0) * (1.0 - x).powf(beta - 1.0)
}

pub fn cauchy_pdf(x: f64, x0: f64, gamma: f64) -> f64 {
    1.0 / (std::f64::consts::PI * gamma * (1.0 + ((x - x0) / gamma).powi(2)))
}

pub fn cauchy_cdf(x: f64, x0: f64, gamma: f64) -> f64 {
    0.5 + ((x - x0) / gamma).atan() / std::f64::consts::PI
}

pub fn pareto_pdf(x: f64, xm: f64, alpha: f64) -> f64 {
    if x < xm {
        return 0.0;
    }
    alpha * xm.powf(alpha) / x.powf(alpha + 1.0)
}

pub fn pareto_cdf(x: f64, xm: f64, alpha: f64) -> f64 {
    if x < xm {
        return 0.0;
    }
    1.0 - (xm / x).powf(alpha)
}

pub fn rayleigh_pdf(x: f64, sigma: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    (x / sigma.powi(2)) * (-x.powi(2) / (2.0 * sigma.powi(2))).exp()
}

pub fn rayleigh_cdf(x: f64, sigma: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-x.powi(2) / (2.0 * sigma.powi(2))).exp()
}

pub fn laplace_pdf(x: f64, mu: f64, b: f64) -> f64 {
    (1.0 / (2.0 * b)) * (-(x - mu).abs() / b).exp()
}

pub fn laplace_cdf(x: f64, mu: f64, b: f64) -> f64 {
    if x < mu {
        0.5 * ((x - mu) / b).exp()
    } else {
        1.0 - 0.5 * (-(x - mu) / b).exp()
    }
}

pub fn negative_binomial_pmf(r: u32, p: f64, k: u32) -> f64 {
    if r == 0 || !p.is_finite() || p <= 0.0 || p > 1.0 {
        return f64::NAN;
    }
    if p == 1.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    let Some(total) = k.checked_add(r) else {
        return f64::NAN;
    };
    let Some(k_plus_one) = k.checked_add(1) else {
        return f64::NAN;
    };
    let log_pmf = gamma_ln(total as f64) - gamma_ln(r as f64) - gamma_ln(k_plus_one as f64)
        + r as f64 * p.ln()
        + k as f64 * (-p).ln_1p();
    log_pmf.exp()
}

pub fn negative_binomial_cdf(r: u32, p: f64, k: u32) -> f64 {
    if r == 0 || !p.is_finite() || p <= 0.0 || p > 1.0 || !discrete_cdf_within_budget(k) {
        return f64::NAN;
    }
    if p == 1.0 {
        return 1.0;
    }
    if k.checked_add(r).is_none() {
        return f64::NAN;
    }

    let log_failure_probability = (-p).ln_1p();
    let mut log_term = r as f64 * p.ln();
    let mut log_sum = log_term;
    for i in 0..k {
        log_term += (i as f64 + r as f64).ln() - (i as f64 + 1.0).ln() + log_failure_probability;
        if log_term > log_sum {
            log_sum = log_term + (log_sum - log_term).exp().ln_1p();
        } else {
            log_sum += (log_term - log_sum).exp().ln_1p();
        }
    }
    log_sum.exp().clamp(0.0, 1.0)
}

pub fn t_test_one_sample(data: &[f64], mu0: f64) -> Option<(f64, f64)> {
    let n = data.len();
    if n < 2 {
        return None;
    }

    let sample_mean = mean(data)?;
    let sample_std = std_dev(data)?;

    let t_stat = (sample_mean - mu0) / (sample_std / (n as f64).sqrt());
    let df = (n - 1) as f64;

    let p_value = 2.0 * (1.0 - student_t_cdf(t_stat.abs(), df));

    Some((t_stat, p_value))
}

pub fn t_test_two_sample(data1: &[f64], data2: &[f64]) -> Option<(f64, f64)> {
    let n1 = data1.len();
    let n2 = data2.len();
    if n1 < 2 || n2 < 2 {
        return None;
    }

    let mean1 = mean(data1)?;
    let mean2 = mean(data2)?;
    let var1 = variance(data1)?;
    let var2 = variance(data2)?;

    let se = (var1 / n1 as f64 + var2 / n2 as f64).sqrt();
    let t_stat = (mean1 - mean2) / se;

    let df_num = (var1 / n1 as f64 + var2 / n2 as f64).powi(2);
    let df_den =
        (var1 / n1 as f64).powi(2) / (n1 - 1) as f64 + (var2 / n2 as f64).powi(2) / (n2 - 1) as f64;
    let df = df_num / df_den;

    let p_value = 2.0 * (1.0 - student_t_cdf(t_stat.abs(), df));

    Some((t_stat, p_value))
}

pub fn z_test_one_sample(data: &[f64], mu0: f64, sigma: f64) -> Option<(f64, f64)> {
    let n = data.len();
    if n < 1 {
        return None;
    }

    let sample_mean = mean(data)?;
    let z_stat = (sample_mean - mu0) / (sigma / (n as f64).sqrt());
    let p_value = 2.0 * (1.0 - normal_cdf(z_stat.abs(), 0.0, 1.0));

    Some((z_stat, p_value))
}

pub fn chi_squared_test(observed: &[f64], expected: &[f64]) -> Option<(f64, f64)> {
    if observed.len() != expected.len() || observed.len() < 2 {
        return None;
    }

    let mut chi2 = 0.0;
    for (o, e) in observed.iter().zip(expected.iter()) {
        if *e <= 0.0 {
            return None;
        }
        chi2 += (o - e).powi(2) / e;
    }

    let df = (observed.len() - 1) as f64;
    let p_value = 1.0 - chi_squared_cdf(chi2, df);

    Some((chi2, p_value))
}

pub fn anova_one_way(groups: &[&[f64]]) -> Option<(f64, f64)> {
    if groups.len() < 2 {
        return None;
    }

    let k = groups.len();
    let mut n_total = 0;
    let mut grand_sum = 0.0;

    for group in groups {
        n_total += group.len();
        grand_sum += group.iter().sum::<f64>();
    }

    if n_total < k + 1 {
        return None;
    }

    let grand_mean = grand_sum / n_total as f64;

    let mut ss_between = 0.0;
    let mut ss_within = 0.0;

    for group in groups {
        let group_mean = mean(group)?;
        let n_i = group.len() as f64;
        ss_between += n_i * (group_mean - grand_mean).powi(2);

        for &x in *group {
            ss_within += (x - group_mean).powi(2);
        }
    }

    let df_between = (k - 1) as f64;
    let df_within = (n_total - k) as f64;

    let ms_between = ss_between / df_between;
    let ms_within = ss_within / df_within;

    let f_stat = ms_between / ms_within;
    let p_value = 1.0 - f_distribution_cdf(f_stat, df_between, df_within);

    Some((f_stat, p_value))
}

pub fn f_distribution_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }

    let z = d1 * x / (d1 * x + d2);
    regularized_incomplete_beta(d1 / 2.0, d2 / 2.0, z)
}

pub fn confidence_interval_mean(data: &[f64], confidence: f64) -> Option<(f64, f64, f64)> {
    let n = data.len();
    if n < 2 {
        return None;
    }

    let sample_mean = mean(data)?;
    let sample_std = std_dev(data)?;
    let se = sample_std / (n as f64).sqrt();

    let alpha = 1.0 - confidence;
    let t_crit = if n < 30 {
        student_t_quantile(1.0 - alpha / 2.0, (n - 1) as f64)
    } else {
        normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0)
    };

    let margin = t_crit * se;
    let lower = sample_mean - margin;
    let upper = sample_mean + margin;

    Some((lower, sample_mean, upper))
}

pub fn confidence_interval_proportion(
    successes: u32,
    n: u32,
    confidence: f64,
) -> Option<(f64, f64, f64)> {
    if n == 0 || successes > n {
        return None;
    }

    let p_hat = successes as f64 / n as f64;
    let se = (p_hat * (1.0 - p_hat) / n as f64).sqrt();

    let alpha = 1.0 - confidence;
    let z_crit = normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0);

    let margin = z_crit * se;
    let lower = (p_hat - margin).max(0.0);
    let upper = (p_hat + margin).min(1.0);

    Some((lower, p_hat, upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polynomial_regression_rejects_degrees_above_the_safe_allocation_limit() {
        let xs: Vec<f64> = (0..=MAX_POLYNOMIAL_REGRESSION_DEGREE + 1)
            .map(|index| index as f64 / MAX_POLYNOMIAL_REGRESSION_DEGREE as f64)
            .collect();
        let ys: Vec<f64> = xs.iter().map(|x| 1.0 + 2.0 * x).collect();

        assert!(polynomial_regression(&xs, &ys, MAX_POLYNOMIAL_REGRESSION_DEGREE - 1).is_some());
        assert!(polynomial_regression(&xs, &ys, MAX_POLYNOMIAL_REGRESSION_DEGREE).is_some());
        assert_eq!(
            polynomial_regression(&xs, &ys, MAX_POLYNOMIAL_REGRESSION_DEGREE + 1),
            None
        );
    }

    #[test]
    fn discrete_cdfs_stop_at_the_shared_iteration_budget() {
        let last_allowed_k = (MAX_DISCRETE_CDF_ITERATIONS - 1) as u32;

        assert!(poisson_cdf(1.0, last_allowed_k - 1).is_finite());
        assert!(poisson_cdf(1.0, last_allowed_k).is_finite());
        assert!(poisson_cdf(1.0, last_allowed_k + 1).is_nan());
        assert!(binomial_cdf(last_allowed_k + 1, 0.5, last_allowed_k).is_finite());
        assert!(binomial_cdf(last_allowed_k + 2, 0.5, last_allowed_k + 1).is_nan());
        assert!(negative_binomial_cdf(2, 0.5, last_allowed_k + 1).is_nan());
        assert!(negative_binomial_pmf(2, 0.5, u32::MAX).is_nan());
    }

    #[test]
    fn test_mean() {
        assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(3.0));
        assert_eq!(mean(&[]), None);
    }

    #[test]
    fn test_median() {
        assert_eq!(median(&[1.0, 3.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn summary_statistics_reject_non_finite_observations() {
        assert_eq!(mean(&[1.0, f64::NAN]), None);
        assert_eq!(median(&[1.0, f64::INFINITY]), None);
    }

    #[test]
    fn test_std_dev() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let sd = std_dev(&data).unwrap();
        assert!((sd - 2.138).abs() < 0.01);
    }

    #[test]
    fn test_linear_regression() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [2.0, 4.0, 6.0, 8.0, 10.0];
        let (slope, intercept, r2) = linear_regression(&xs, &ys).unwrap();
        assert!((slope - 2.0).abs() < 1e-10);
        assert!((intercept - 0.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_normal_cdf() {
        assert!((normal_cdf(0.0, 0.0, 1.0) - 0.5).abs() < 1e-7);
        assert!((normal_cdf(1.96, 0.0, 1.0) - 0.975).abs() < 0.001);
    }

    #[test]
    fn probability_functions_reject_invalid_parameters_and_handle_mass_boundaries() {
        assert!(normal_pdf(0.0, 0.0, 0.0).is_nan());
        assert!(normal_cdf(0.0, 0.0, -1.0).is_nan());
        assert!(normal_quantile(0.0, 0.0, 1.0).is_nan());
        assert_eq!(binomial_pmf(4, 0.0, 0), 1.0);
        assert_eq!(binomial_pmf(4, 1.0, 4), 1.0);
        assert_eq!(poisson_pmf(0.0, 0), 1.0);
        assert!(exponential_pdf(1.0, 0.0).is_nan());
        assert!(uniform_cdf(0.0, 1.0, 1.0).is_nan());
    }

    #[test]
    fn hypergeometric_validates_population_constraints_before_subtraction() {
        assert_eq!(hypergeometric_pmf(3, 4, 1, 0), 0.0);
        assert_eq!(hypergeometric_pmf(3, 1, 4, 0), 0.0);
    }

    #[test]
    fn hypergeometric_evaluates_valid_lower_support_without_unsigned_underflow() {
        let probability = hypergeometric_pmf(10, 8, 5, 3);
        assert!(probability.is_finite());
        assert!((0.0..=1.0).contains(&probability));
    }

    #[test]
    fn test_binomial() {
        assert!((binomial_pmf(10, 0.5, 5) - 0.2461).abs() < 0.001);
    }

    #[test]
    fn f_distribution_pdf_uses_beta_normalization() {
        // Para F(4, 2), f(1) = 8 / 27.
        assert!((f_distribution_pdf(1.0, 4.0, 2.0) - 8.0 / 27.0).abs() < 1e-12);
    }

    #[test]
    fn test_histogram() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let h = histogram(&data, 5);
        assert_eq!(h.len(), 5);
        let total: f64 = h.iter().map(|(_, _, c)| c).sum();
        assert_eq!(total, 10.0);
    }

    #[test]
    fn histogram_caps_bin_count_before_allocation() {
        let histogram = histogram(&[1.0, 2.0, 3.0], 4_097);
        assert_eq!(histogram.len(), 4_096);
    }

    #[test]
    fn histogram_ignores_non_finite_observations_before_computing_bounds() {
        let mixed = histogram(&[1.0, f64::NAN, 3.0, f64::INFINITY], 2);
        assert_eq!(mixed.len(), 2);
        assert!(mixed
            .iter()
            .all(|(left, right, _)| left.is_finite() && right.is_finite()));
        assert_eq!(mixed.iter().map(|(_, _, count)| count).sum::<f64>(), 2.0);

        assert!(histogram(&[f64::NAN, f64::NEG_INFINITY], 2).is_empty());
    }

    #[test]
    fn test_boxplot() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0];
        let (_wl, _q1, med, _q3, _wh, outliers) = boxplot_stats(&data).unwrap();
        assert!((med - 5.5).abs() < 0.1);
        assert!(!outliers.is_empty());
    }
}
