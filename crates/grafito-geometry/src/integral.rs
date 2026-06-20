//! Integración numérica híbrida.
//!
//! Provee una ruta rápida para integrales definidas de funciones `y = f(x)`:
//! evaluar `f(x)` en una grilla fina (paralela en CPU o, opcionalmente, en GPU)
//! y reducir con una regla de cuadratura compuesta en CPU.

use rayon::prelude::*;

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
    if intervals % 2 == 0 {
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
pub fn eval_integral_hybrid<F>(f: F, a: f64, b: f64, samples: usize) -> f64
where
    F: Fn(f64) -> f64 + Sync,
{
    if (b - a).abs() < 1e-15 {
        return 0.0;
    }
    let n = samples.max(2);
    let dx = (b - a) / (n - 1) as f64;
    let xs: Vec<f64> = (0..n).map(|i| a + i as f64 * dx).collect();
    let ys: Vec<f64> = xs.par_iter().map(|&x| f(x)).collect();
    composite_simpson(&ys, dx)
}

/// Variante de [`eval_integral_hybrid`] que permite usar un evaluador externo
/// (por ejemplo, un pipeline GPU) para obtener `f(x)` en los puntos de la
/// grilla. Si el evaluador devuelve menos valores que los solicitados, se
/// reduce con los valores disponibles.
pub fn eval_integral_hybrid_with_evaluator<G>(a: f64, b: f64, samples: usize, evaluator: G) -> f64
where
    G: Fn(&[f64]) -> Vec<f64>,
{
    if (b - a).abs() < 1e-15 {
        return 0.0;
    }
    let n = samples.max(2);
    let dx = (b - a) / (n - 1) as f64;
    let xs: Vec<f64> = (0..n).map(|i| a + i as f64 * dx).collect();
    let ys = evaluator(&xs);
    composite_simpson(&ys, dx)
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
}
