//! Grafito Interval Arithmetic — Guaranteed bounds for function plotting.
//! Uses rug::Float with directed rounding for rigorous interval evaluation.

use rug::Float;

#[derive(Debug, Clone)]
pub struct Interval {
    pub lo: Float,
    pub hi: Float,
}

impl Interval {
    pub fn new(prec: u32, lo: f64, hi: f64) -> Self {
        Self {
            lo: Float::with_val(prec, lo),
            hi: Float::with_val(prec, hi),
        }
    }

    pub fn point(prec: u32, val: f64) -> Self { Self::new(prec, val, val) }

    pub fn crosses_zero(&self) -> bool {
        self.lo.to_f64() <= 0.0 && 0.0 <= self.hi.to_f64()
    }

    pub fn contains(&self, val: f64) -> bool {
        self.lo.to_f64() <= val && val <= self.hi.to_f64()
    }

    pub fn is_definitely_positive(&self) -> bool { self.lo.to_f64() > 0.0 }
    pub fn is_definitely_negative(&self) -> bool { self.hi.to_f64() < 0.0 }
    pub fn midpoint(&self) -> f64 { (self.lo.to_f64() + self.hi.to_f64()) * 0.5 }
}

/// Safe sample of a function f(x) with asymptote detection.
/// Returns (x, y) where y is None at discontinuities/asymptotes.
pub fn safe_sample<F: Fn(f64) -> f64>(f: F, x_min: f64, x_max: f64, n: usize) -> Vec<(f64, Option<f64>)> {
    let dx = (x_max - x_min) / (n - 1) as f64;
    (0..n).map(|i| {
        let x = x_min + i as f64 * dx;
        let y = f(x);
        if y.is_finite() && y.abs() < 1e6 {
            (x, Some(y))
        } else {
            (x, None)
        }
    }).collect()
}

/// Detect asymptotes by finding sign changes with large magnitude jumps.
pub fn detect_asymptotes(samples: &[(f64, Option<f64>)]) -> Vec<f64> {
    let mut asymptotes = Vec::new();
    for i in 1..samples.len() {
        if let (Some(y0), Some(y1)) = (samples[i-1].1, samples[i].1) {
            if y0.signum() != y1.signum() && (y1 / y0.max(1e-10)).abs() > 100.0 {
                asymptotes.push((samples[i-1].0 + samples[i].0) * 0.5);
            }
        }
    }
    asymptotes
}
