//! Grafito Interval Arithmetic — Guaranteed bounds for function plotting.
//! Uses DoubleDouble arithmetic for extended precision interval evaluation.

use crate::dd::DD;

/// Dígitos decimales máximos con sentido para el redondeo outward de
/// [`Interval::new`]: más allá de 15 un `f64` ya no distingue el paso de
/// cuantización y se conserva la precisión DD completa.
pub const MAX_INTERVAL_PREC_DIGITS: u32 = 15;

#[derive(Debug, Clone)]
pub struct Interval {
    pub lo: DD,
    pub hi: DD,
}

impl Interval {
    /// Construye `[lo, hi]` con redondeo outward a `prec` dígitos decimales:
    /// `lo` se redondea hacia abajo y `hi` hacia arriba (más 1 ulp f64 de
    /// resguardo en cada extremo), de modo que el intervalo resultante
    /// contiene al `[lo, hi]` pedido.
    ///
    /// - `prec = 0` → sin cuantizar: se guardan los extremos tal cual en DD.
    /// - `1 <= prec <= 15` → cuantización decimal outward real.
    /// - `prec > 15` → se satura a 15 (documentado, no silencioso: más
    ///   dígitos no son representables en `f64`).
    ///
    /// Honestidad: el respaldo es double-double de precisión FIJA (~106 bits
    /// de mantisa), NO aritmética de intervalos rigurosa de precisión
    /// arbitraria (MPFR/etc.). `prec` NO aumenta la precisión interna: solo
    /// ensancha (outward) los extremos a la granularidad pedida. Los extremos
    /// se normalizan (`lo <= hi`, se reordenan si vienen invertidos); los no
    /// finitos (`NaN`/`Inf`) se guardan tal cual sin cuantizar, igual que
    /// `prec = 0`.
    pub fn new(prec: u32, lo: f64, hi: f64) -> Self {
        let (mut lo, mut hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        if prec > 0 && lo.is_finite() && hi.is_finite() {
            let digits = prec.min(MAX_INTERVAL_PREC_DIGITS);
            let scale = 10_f64.powi(digits as i32);
            if scale.is_finite() && scale > 0.0 {
                // `floor`/`ceil` dirigen el redondeo hacia afuera; el
                // `next_down`/`next_up` cubre el error de la
                // multiplicación/división en f64.
                let qlo = (lo * scale).floor() / scale;
                let qhi = (hi * scale).ceil() / scale;
                if qlo.is_finite() && qhi.is_finite() && qlo <= qhi {
                    lo = qlo.next_down();
                    hi = qhi.next_up();
                }
            }
        }
        Self {
            lo: DD::from_f64(lo),
            hi: DD::from_f64(hi),
        }
    }

    /// Intervalo punto `[val, val]` con el mismo redondeo outward de
    /// [`Interval::new`] (antes ignoraba `prec`, ver `new`).
    pub fn point(prec: u32, val: f64) -> Self {
        Self::new(prec, val, val)
    }

    pub fn crosses_zero(&self) -> bool {
        // Comparación en DD directo: `to_f64` redondea y un intervalo DD
        // diminuto alrededor de 0 podría clasificarse mal tras el redondeo.
        self.lo <= DD::from_f64(0.0) && DD::from_f64(0.0) <= self.hi
    }

    pub fn contains(&self, val: f64) -> bool {
        let v = DD::from_f64(val);
        self.lo <= v && v <= self.hi
    }

    pub fn is_definitely_positive(&self) -> bool {
        self.lo > DD::from_f64(0.0)
    }

    pub fn is_definitely_negative(&self) -> bool {
        self.hi < DD::from_f64(0.0)
    }

    /// Punto medio overflow-safe: `lo + (hi - lo) / 2`.
    ///
    /// Nunca se usa `(lo + hi) / 2`, que desborda con extremos grandes
    /// (`hi = f64::MAX`) o pierde precisión con rangos asimétricos.
    pub fn midpoint(&self) -> f64 {
        // Aritmética en DD y una sola conversión final: evita el desborde
        // de `(lo+hi)/2` y conserva precisión con rangos asimétricos.
        (self.lo + (self.hi - self.lo) * DD::from_f64(0.5)).to_f64()
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

    #[test]
    fn prec_cero_no_cuantiza() {
        // `prec = 0` conserva el comportamiento previo: extremos tal cual.
        let i = Interval::new(0, 0.123_456_789, 0.987_654_321);
        assert!((i.lo.to_f64() - 0.123_456_789).abs() < 1e-15);
        assert!((i.hi.to_f64() - 0.987_654_321).abs() < 1e-15);
    }

    #[test]
    fn prec_hace_outward_real() {
        // `prec = 2` ensancha hacia afuera: contiene al [lo, hi] pedido y
        // toca la grilla de 0.01 por fuera (más 1 ulp de resguardo).
        let (lo, hi) = (0.123_456, 0.987_654);
        let i = Interval::new(2, lo, hi);
        assert!(i.lo.to_f64() <= 0.12, "lo = {}", i.lo.to_f64());
        assert!(i.hi.to_f64() >= 0.99, "hi = {}", i.hi.to_f64());
        assert!(i.lo.to_f64() <= lo && hi <= i.hi.to_f64());
        assert!(i.contains(lo) && i.contains(hi));
    }

    #[test]
    fn prec_normaliza_extremos_invertidos() {
        let i = Interval::new(0, 2.0, -2.0);
        assert!(i.lo.to_f64() <= i.hi.to_f64());
        assert!(i.contains(0.0));
    }

    #[test]
    fn prec_mayor_a_15_satura_sin_panico() {
        let i = Interval::new(99, 0.1, 0.2);
        assert!(i.lo.to_f64() <= 0.1);
        assert!(i.hi.to_f64() >= 0.2);
    }

    #[test]
    fn point_respeta_prec() {
        // Antes `point` ignoraba su `prec` (delegaba con 0): ahora ensancha.
        let a = Interval::point(0, 0.123_456);
        let b = Interval::point(2, 0.123_456);
        assert!((a.lo.to_f64() - 0.123_456).abs() < 1e-15);
        assert!(b.lo.to_f64() <= 0.123_456 && 0.123_456 <= b.hi.to_f64());
        assert!(b.lo.to_f64() < a.lo.to_f64() || b.hi.to_f64() > a.hi.to_f64());
    }
}
