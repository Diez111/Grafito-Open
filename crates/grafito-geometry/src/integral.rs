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
}
