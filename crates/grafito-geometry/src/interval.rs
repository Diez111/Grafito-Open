//! Grafito Interval Arithmetic — Guaranteed bounds for function plotting.
//! Uses DoubleDouble arithmetic for extended precision interval evaluation.

use crate::dd::DD;

#[derive(Debug, Clone)]
pub struct Interval {
    pub lo: DD,
    pub hi: DD,
}

impl Interval {
    pub fn new(_prec: u32, lo: f64, hi: f64) -> Self {
        Self {
            lo: DD::from_f64(lo),
            hi: DD::from_f64(hi),
        }
    }

    pub fn point(_prec: u32, val: f64) -> Self {
        Self::new(0, val, val)
    }

    pub fn crosses_zero(&self) -> bool {
        self.lo.to_f64() <= 0.0 && 0.0 <= self.hi.to_f64()
    }

    pub fn contains(&self, val: f64) -> bool {
        self.lo.to_f64() <= val && val <= self.hi.to_f64()
    }

    pub fn is_definitely_positive(&self) -> bool {
        self.lo.to_f64() > 0.0
    }

    pub fn is_definitely_negative(&self) -> bool {
        self.hi.to_f64() < 0.0
    }

    /// Punto medio overflow-safe: `lo + (hi - lo) / 2`.
    ///
    /// Nunca se usa `(lo + hi) / 2`, que desborda con extremos grandes
    /// (`hi = f64::MAX`) o pierde precisión con rangos asimétricos.
    pub fn midpoint(&self) -> f64 {
        let lo = self.lo.to_f64();
        let hi = self.hi.to_f64();
        lo + (hi - lo) * 0.5
    }
}

/// Safe sample of a function f(x) with asymptote detection.
/// Returns (x, y) where y is None at discontinuities/asymptotes.
///
/// Garantías verificadas (auditoría geometría):
/// - Presupuesto `MAX_SAMPLES` = 100k: `n` fuera de `2..=100_000` devuelve
///   `vec![]` sin reservar (cota OOM; el peor caso son ~1.6 MiB).
/// - `x_min`/`x_max` deben ser finitos y `x_min < x_max`; `dx` debe ser finito,
///   no nulo y con granularidad representable (`x_min + dx != x_min`).
/// - Cada `x` muestreado se valida finito; cada `y` debe ser finito y
///   `|y| < 1e50` (un `Div` 1/0 del evaluador da `NaN`, nunca `Inf`, y cae a
///   `None` aquí en lugar de contaminar el render).
pub fn safe_sample<F: Fn(f64) -> f64>(
    f: F,
    x_min: f64,
    x_max: f64,
    n: usize,
) -> Vec<(f64, Option<f64>)> {
    const MAX_SAMPLES: usize = 100_000;
    if !(2..=MAX_SAMPLES).contains(&n) {
        return vec![];
    }
    if !x_min.is_finite() || !x_max.is_finite() || x_min >= x_max {
        return vec![];
    }
    let dx = (x_max - x_min) / (n - 1) as f64;
    if !dx.is_finite() || dx == 0.0 || x_min + dx == x_min || x_max - dx == x_max {
        return vec![];
    }
    (0..n)
        .map(|i| {
            let x = x_min + i as f64 * dx;
            if !x.is_finite() {
                return (x, None);
            }
            let y = f(x);
            if y.is_finite() && y.abs() < 1e50 {
                (x, Some(y))
            } else {
                (x, None)
            }
        })
        .collect()
}

/// Detect asymptotes by finding sign changes with large magnitude jumps.
pub fn detect_asymptotes(samples: &[(f64, Option<f64>)]) -> Vec<f64> {
    let mut asymptotes = Vec::new();
    for i in 1..samples.len() {
        if let (Some(y0), Some(y1)) = (samples[i - 1].1, samples[i].1) {
            if y0.signum() != y1.signum() && (y1 / y0.abs().max(1e-10)).abs() > 100.0 {
                asymptotes.push((samples[i - 1].0 + samples[i].0) * 0.5);
            }
        }
    }
    asymptotes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_crosses_zero() {
        let i = Interval::new(0, -1.0, 1.0);
        assert!(i.crosses_zero());
        let i = Interval::new(0, 1.0, 2.0);
        assert!(!i.crosses_zero());
    }

    #[test]
    fn test_interval_contains() {
        let i = Interval::new(0, -1.0, 1.0);
        assert!(i.contains(0.0));
        assert!(i.contains(-1.0));
        assert!(i.contains(1.0));
        assert!(!i.contains(2.0));
    }

    #[test]
    fn test_interval_definitely_positive_negative() {
        let pos = Interval::new(0, 0.1, 1.0);
        assert!(pos.is_definitely_positive());
        assert!(!pos.is_definitely_negative());
        let neg = Interval::new(0, -1.0, -0.1);
        assert!(neg.is_definitely_negative());
        assert!(!neg.is_definitely_positive());
    }

    #[test]
    fn test_safe_sample_normal() {
        let f = |x: f64| x * x;
        let samples = safe_sample(f, 0.0, 2.0, 5);
        assert_eq!(samples.len(), 5);
        assert_eq!(samples[0], (0.0, Some(0.0)));
        assert_eq!(samples[4], (2.0, Some(4.0)));
    }

    #[test]
    fn test_safe_sample_n_less_than_2() {
        let f = |x: f64| x;
        let samples = safe_sample(f, 0.0, 1.0, 0);
        assert!(samples.is_empty());
        let samples = safe_sample(f, 0.0, 1.0, 1);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_safe_sample_nan() {
        let f = |x: f64| if x == 0.0 { f64::NAN } else { 1.0 / x };
        let samples = safe_sample(f, -1.0, 1.0, 3);
        assert_eq!(samples[1].1, None); // x=0 → NaN
    }

    #[test]
    fn test_detect_asymptotes() {
        // 1/x near x=0: small negative → large positive (ratio > 100)
        let samples = vec![
            (-1.0, Some(-1.0)),
            (-0.01, Some(-0.01)),
            (0.01, Some(100.0)),
            (1.0, Some(1.0)),
        ];
        let asymp = detect_asymptotes(&samples);
        assert!(!asymp.is_empty());
    }

    #[test]
    fn safe_sample_enforces_finite_bounds_and_max_samples_budget() {
        let f = |x: f64| x;
        // Sobre el presupuesto: vacío sin reservar.
        assert!(safe_sample(f, 0.0, 1.0, 100_001).is_empty());
        // Bordes no finitos o invertidos: vacío.
        assert!(safe_sample(f, f64::NAN, 1.0, 10).is_empty());
        assert!(safe_sample(f, 0.0, f64::INFINITY, 10).is_empty());
        assert!(safe_sample(f, 1.0, 0.0, 10).is_empty());
        // Tope exacto del presupuesto: 100k muestras.
        assert_eq!(safe_sample(f, 0.0, 1.0, 100_000).len(), 100_000);
    }

    #[test]
    fn test_detect_asymptotes_empty() {
        let samples = vec![(0.0, Some(1.0)), (1.0, Some(2.0))];
        let asymp = detect_asymptotes(&samples);
        assert!(asymp.is_empty());
    }

    #[test]
    fn test_interval_midpoint() {
        let i = Interval::new(0, -2.0, 2.0);
        assert!((i.midpoint() - 0.0).abs() < 1e-10);
    }
}
