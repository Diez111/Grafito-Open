//! Special mathematical curves with parametric equations.
//!
//! This module provides functions to generate points for various special curves
//! commonly used in mathematics and physics.

use crate::Point2;
use std::fmt;

/// Maximum number of samples accepted by a public special-curve sampler.
pub const MAX_SPECIAL_CURVE_STEPS: usize = 100_000;
/// Backwards-compatible name for the shared special-curve sampling budget.
pub const MAX_ROSE_STEPS: usize = MAX_SPECIAL_CURVE_STEPS;

/// Failure returned when special-curve sampling cannot safely produce points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialCurveError {
    /// The frequency ratio has a zero denominator.
    ZeroFrequencyDenominator,
    /// The requested point count exceeds the bounded sampling budget.
    StepLimitExceeded { requested: usize, maximum: usize },
    /// Reserving the bounded point buffer failed.
    AllocationFailed,
}

impl fmt::Display for SpecialCurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFrequencyDenominator => {
                write!(f, "el denominador de frecuencia no puede ser cero")
            }
            Self::StepLimitExceeded { requested, maximum } => {
                write!(f, "se solicitaron {requested} muestras, máximo {maximum}")
            }
            Self::AllocationFailed => write!(f, "no se pudo reservar memoria para las muestras"),
        }
    }
}

/// Backwards-compatible name for the error returned by [`try_rose`].
pub type RoseError = SpecialCurveError;

fn try_sample_curve<F>(steps: usize, mut sample: F) -> Result<Vec<Point2>, SpecialCurveError>
where
    F: FnMut(usize) -> Point2,
{
    if steps > MAX_SPECIAL_CURVE_STEPS {
        return Err(SpecialCurveError::StepLimitExceeded {
            requested: steps,
            maximum: MAX_SPECIAL_CURVE_STEPS,
        });
    }

    let mut points = Vec::new();
    points
        .try_reserve_exact(steps)
        .map_err(|_| SpecialCurveError::AllocationFailed)?;
    for i in 0..steps {
        points.push(sample(i));
    }
    Ok(points)
}

/// Generate points for a cardioid curve.
///
/// A cardioid is a plane curve traced by a point on the perimeter of a circle
/// that is rolling around a fixed circle of the same radius.
///
/// # Arguments
/// * `a` - The size parameter (radius of the rolling circle)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn cardioid(a: f64, steps: usize) -> Vec<Point2> {
    try_cardioid(a, steps).unwrap_or_default()
}

/// Intenta generar una cardioide dentro del presupuesto de muestreo público.
pub fn try_cardioid(a: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    try_sample_curve(steps, |i| {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        let r = a * (1.0 + theta.cos());
        Point2::new(r * theta.cos(), r * theta.sin())
    })
}

/// Generate points for a rose curve (rhodonea curve).
///
/// A rose curve is a sinusoidal curve plotted in polar coordinates.
///
/// # Arguments
/// * `a` - The amplitude parameter
/// * `n` - Numerator of the frequency ratio
/// * `d` - Denominator of the frequency ratio
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn rose(a: f64, n: i32, d: i32, steps: usize) -> Vec<Point2> {
    try_rose(a, n, d, steps).unwrap_or_default()
}

/// Intenta generar una rosa con un presupuesto de muestras y reserva falible.
pub fn try_rose(a: f64, n: i32, d: i32, steps: usize) -> Result<Vec<Point2>, RoseError> {
    if d == 0 {
        return Err(RoseError::ZeroFrequencyDenominator);
    }
    let k = n as f64 / d as f64;
    let max_theta = if n % 2 == 0 || d % 2 == 0 {
        2.0 * std::f64::consts::PI * d as f64
    } else {
        std::f64::consts::PI * d as f64
    };

    try_sample_curve(steps, |i| {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a * (k * theta).cos();
        Point2::new(r * theta.cos(), r * theta.sin())
    })
}

/// Generate points for an Archimedean spiral.
///
/// An Archimedean spiral is a curve traced by a point moving away from a fixed
/// point at a constant rate while rotating around it.
///
/// # Arguments
/// * `a` - Initial radius
/// * `b` - Growth rate (distance between successive turnings)
/// * `max_theta` - Maximum angle (in radians)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn archimedean_spiral(a: f64, b: f64, max_theta: f64, steps: usize) -> Vec<Point2> {
    try_archimedean_spiral(a, b, max_theta, steps).unwrap_or_default()
}

/// Intenta generar una espiral de Arquímedes dentro del presupuesto público.
pub fn try_archimedean_spiral(
    a: f64,
    b: f64,
    max_theta: f64,
    steps: usize,
) -> Result<Vec<Point2>, SpecialCurveError> {
    try_sample_curve(steps, |i| {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a + b * theta;
        Point2::new(r * theta.cos(), r * theta.sin())
    })
}

/// Generate points for a logarithmic spiral.
///
/// A logarithmic spiral is a self-similar spiral curve that often appears in nature.
///
/// # Arguments
/// * `a` - Initial radius
/// * `b` - Growth factor (controls how tightly the spiral is wound)
/// * `max_theta` - Maximum angle (in radians)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn logarithmic_spiral(a: f64, b: f64, max_theta: f64, steps: usize) -> Vec<Point2> {
    try_logarithmic_spiral(a, b, max_theta, steps).unwrap_or_default()
}

/// Intenta generar una espiral logarítmica dentro del presupuesto público.
pub fn try_logarithmic_spiral(
    a: f64,
    b: f64,
    max_theta: f64,
    steps: usize,
) -> Result<Vec<Point2>, SpecialCurveError> {
    try_sample_curve(steps, |i| {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a * (b * theta).exp();
        Point2::new(r * theta.cos(), r * theta.sin())
    })
}

/// Generate points for a Lissajous curve.
///
/// A Lissajous curve is the graph of a system of parametric equations
/// describing complex harmonic motion.
///
/// # Arguments
/// * `a` - Amplitude in x direction
/// * `b` - Amplitude in y direction
/// * `freq_x` - Frequency in x direction
/// * `freq_y` - Frequency in y direction
/// * `delta` - Phase difference (in radians)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn lissajous(
    a: f64,
    b: f64,
    freq_x: f64,
    freq_y: f64,
    delta: f64,
    steps: usize,
) -> Vec<Point2> {
    try_lissajous(a, b, freq_x, freq_y, delta, steps).unwrap_or_default()
}

/// Intenta generar una curva de Lissajous dentro del presupuesto público.
pub fn try_lissajous(
    a: f64,
    b: f64,
    freq_x: f64,
    freq_y: f64,
    delta: f64,
    steps: usize,
) -> Result<Vec<Point2>, SpecialCurveError> {
    let max_t = 2.0 * std::f64::consts::PI;

    try_sample_curve(steps, |i| {
        let t = max_t * (i as f64) / (steps as f64);
        Point2::new(a * (freq_x * t + delta).sin(), b * (freq_y * t).sin())
    })
}

/// Generate points for an epicycloid.
///
/// An epicycloid is a plane curve produced by tracing the path of a point on
/// the circumference of a circle rolling around the outside of another circle.
///
/// # Arguments
/// * `r` - Radius of the fixed circle
/// * `k` - Ratio of the rolling circle radius to the fixed circle radius
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn epicycloid(r: f64, k: f64, steps: usize) -> Vec<Point2> {
    try_epicycloid(r, k, steps).unwrap_or_default()
}

/// Intenta generar una epicicloide dentro del presupuesto público.
pub fn try_epicycloid(r: f64, k: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    let max_theta = 2.0 * std::f64::consts::PI * k.ceil();

    try_sample_curve(steps, |i| {
        let theta = max_theta * (i as f64) / (steps as f64);
        let x = r * ((1.0 + k) * theta.cos() - k * ((1.0 + k) * theta / k).cos());
        let y = r * ((1.0 + k) * theta.sin() - k * ((1.0 + k) * theta / k).sin());
        Point2::new(x, y)
    })
}

/// Generate points for a hypocycloid.
///
/// A hypocycloid is a curve traced by a point on a circle rolling inside another circle.
///
/// # Arguments
/// * `r` - Radius of the fixed circle
/// * `k` - Ratio of the fixed circle radius to the rolling circle radius
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn hypocycloid(r: f64, k: f64, steps: usize) -> Vec<Point2> {
    try_hypocycloid(r, k, steps).unwrap_or_default()
}

/// Intenta generar una hipocicloide dentro del presupuesto público.
pub fn try_hypocycloid(r: f64, k: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    let max_theta = 2.0 * std::f64::consts::PI * k.ceil();

    try_sample_curve(steps, |i| {
        let theta = max_theta * (i as f64) / (steps as f64);
        let x = r * ((k - 1.0) * theta.cos() + ((k - 1.0) * theta).cos());
        let y = r * ((k - 1.0) * theta.sin() - ((k - 1.0) * theta).sin());
        Point2::new(x, y)
    })
}

/// Generate points for an astroid curve.
///
/// An astroid is a hypocycloid with four cusps:
/// x = a * cos³(t), y = a * sin³(t)
///
/// # Arguments
/// * `a` - The size parameter
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn astroid(a: f64, steps: usize) -> Vec<Point2> {
    try_astroid(a, steps).unwrap_or_default()
}

/// Intenta generar una astroide dentro del presupuesto público.
pub fn try_astroid(a: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    try_sample_curve(steps, |i| {
        let t = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        Point2::new(a * t.cos().powi(3), a * t.sin().powi(3))
    })
}

/// Generate points for a deltoid curve.
///
/// A deltoid (tricuspid) is a hypocycloid with three cusps:
/// x = 2a*cos(t) + a*cos(2t), y = 2a*sin(t) - a*sin(2t)
///
/// # Arguments
/// * `a` - The size parameter (radius of the rolling circle)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn deltoid(a: f64, steps: usize) -> Vec<Point2> {
    try_deltoid(a, steps).unwrap_or_default()
}

/// Intenta generar una deltoide dentro del presupuesto público.
pub fn try_deltoid(a: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    try_sample_curve(steps, |i| {
        let t = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        Point2::new(
            2.0 * a * t.cos() + a * (2.0 * t).cos(),
            2.0 * a * t.sin() - a * (2.0 * t).sin(),
        )
    })
}

/// Generate points for a tractrix curve.
///
/// A tractrix is the curve along which an object moves when pulled by a string
/// of constant length. Parametrized for t > 0:
/// x = a / cosh(t), y = a * (t - tanh(t))
///
/// # Arguments
/// * `a` - The asymptotic distance (string length)
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn tractrix(a: f64, steps: usize) -> Vec<Point2> {
    try_tractrix(a, steps).unwrap_or_default()
}

/// Intenta generar una tractriz dentro del presupuesto público.
pub fn try_tractrix(a: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    let t_max = 5.0;
    try_sample_curve(steps, |i| {
        let t = t_max * (i as f64) / (steps as f64) + 1e-6;
        Point2::new(a / t.cosh(), a * (t - t.tanh()))
    })
}

/// Generate points for a brachistochrone curve.
///
/// The brachistochrone is the curve of fastest descent under gravity,
/// which is a cycloid:
/// x = a * (t - sin(t)), y = a * (1 - cos(t))
///
/// # Arguments
/// * `a` - The radius of the generating circle
/// * `steps` - Number of points to generate
///
/// # Returns
/// Vector of Point2 representing the curve
pub fn brachistochrone(a: f64, steps: usize) -> Vec<Point2> {
    try_brachistochrone(a, steps).unwrap_or_default()
}

/// Intenta generar una braquistócrona dentro del presupuesto público.
pub fn try_brachistochrone(a: f64, steps: usize) -> Result<Vec<Point2>, SpecialCurveError> {
    let t_max = 2.0 * std::f64::consts::PI;
    try_sample_curve(steps, |i| {
        let t = t_max * (i as f64) / (steps as f64);
        Point2::new(a * (t - t.sin()), a * (1.0 - t.cos()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardioid() {
        let points = cardioid(1.0, 100);
        assert_eq!(points.len(), 100);
        // First point should be at (2, 0) when theta = 0
        assert!((points[0].x - 2.0).abs() < 0.01);
        assert!(points[0].y.abs() < 0.01);
    }

    #[test]
    fn test_rose() {
        let points = rose(1.0, 3, 1, 100);
        assert_eq!(points.len(), 100);
    }

    #[test]
    fn rose_rejects_zero_denominators_and_avoids_frequency_overflow() {
        assert!(rose(1.0, 3, 0, 100).is_empty());
        assert_eq!(
            try_rose(1.0, 3, 0, 100),
            Err(RoseError::ZeroFrequencyDenominator)
        );

        let points = rose(1.0, i32::MAX, 2, 3);
        assert_eq!(points.len(), 3);
        assert!(points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite()));
    }

    #[test]
    fn try_rose_rejects_step_budgets_before_allocating() {
        assert_eq!(
            try_rose(1.0, 3, 1, MAX_ROSE_STEPS + 1),
            Err(RoseError::StepLimitExceeded {
                requested: MAX_ROSE_STEPS + 1,
                maximum: MAX_ROSE_STEPS,
            })
        );
    }

    #[test]
    fn every_public_curve_sampler_rejects_unbounded_step_counts_before_allocation() {
        let expected = Err(SpecialCurveError::StepLimitExceeded {
            requested: usize::MAX,
            maximum: MAX_SPECIAL_CURVE_STEPS,
        });

        assert_eq!(try_cardioid(1.0, usize::MAX), expected);
        assert_eq!(try_rose(1.0, 3, 1, usize::MAX), expected);
        assert_eq!(
            try_archimedean_spiral(0.0, 1.0, std::f64::consts::TAU, usize::MAX),
            expected
        );
        assert_eq!(
            try_logarithmic_spiral(1.0, 0.1, std::f64::consts::TAU, usize::MAX),
            expected
        );
        assert_eq!(try_lissajous(1.0, 1.0, 3.0, 2.0, 0.0, usize::MAX), expected);
        assert_eq!(try_epicycloid(1.0, 3.0, usize::MAX), expected);
        assert_eq!(try_hypocycloid(1.0, 4.0, usize::MAX), expected);
        assert_eq!(try_astroid(1.0, usize::MAX), expected);
        assert_eq!(try_deltoid(1.0, usize::MAX), expected);
        assert_eq!(try_tractrix(1.0, usize::MAX), expected);
        assert_eq!(try_brachistochrone(1.0, usize::MAX), expected);

        assert!(cardioid(1.0, usize::MAX).is_empty());
        assert!(archimedean_spiral(0.0, 1.0, 1.0, usize::MAX).is_empty());
    }

    #[test]
    fn test_archimedean_spiral() {
        let points = archimedean_spiral(0.0, 1.0, 4.0 * std::f64::consts::PI, 100);
        assert_eq!(points.len(), 100);
        // Spiral should grow outward
        let r_first = (points[10].x.powi(2) + points[10].y.powi(2)).sqrt();
        let r_last = (points[90].x.powi(2) + points[90].y.powi(2)).sqrt();
        assert!(r_last > r_first);
    }

    #[test]
    fn test_logarithmic_spiral() {
        let points = logarithmic_spiral(1.0, 0.1, 4.0 * std::f64::consts::PI, 100);
        assert_eq!(points.len(), 100);
    }

    #[test]
    fn test_lissajous() {
        let points = lissajous(1.0, 1.0, 3.0, 2.0, 0.0, 100);
        assert_eq!(points.len(), 100);
    }

    #[test]
    fn test_epicycloid() {
        let points = epicycloid(1.0, 3.0, 100);
        assert_eq!(points.len(), 100);
    }

    #[test]
    fn test_hypocycloid() {
        let points = hypocycloid(1.0, 4.0, 100);
        assert_eq!(points.len(), 100);
    }

    #[test]
    fn test_astroid() {
        let a = 2.0;
        let points = astroid(a, 200);
        assert_eq!(points.len(), 200);
        // Verify |x|^(2/3) + |y|^(2/3) = a^(2/3)
        let rhs = a.powf(2.0 / 3.0);
        for p in points.iter().take(50) {
            let lhs = p.x.abs().powf(2.0 / 3.0) + p.y.abs().powf(2.0 / 3.0);
            assert!((lhs - rhs).abs() < 1e-6, "lhs={lhs}, rhs={rhs}");
        }
    }

    #[test]
    fn test_deltoid() {
        let points = deltoid(1.0, 100);
        assert_eq!(points.len(), 100);
        // All points should be finite
        for p in &points {
            assert!(p.x.is_finite());
            assert!(p.y.is_finite());
        }
    }

    #[test]
    fn test_tractrix() {
        let points = tractrix(1.0, 100);
        assert_eq!(points.len(), 100);
        // x = a/cosh(t) should always be positive
        for p in &points {
            assert!(p.x > 0.0, "tractrix x should be positive, got {}", p.x);
        }
    }

    #[test]
    fn test_brachistochrone() {
        let a = 1.0;
        let points = brachistochrone(a, 100);
        assert_eq!(points.len(), 100);
        // At t=0: x=0, y=0 (start of cycloid)
        assert!(points[0].x.abs() < 1e-10);
        assert!(points[0].y.abs() < 1e-10);
        // At t=π (middle, i=50): x = a*π, y = 2a
        let mid = &points[50];
        assert!((mid.x - a * std::f64::consts::PI).abs() < 0.01);
        assert!((mid.y - 2.0 * a).abs() < 0.01);
        // At t≈2π (end, i=99): y ≈ 0, x ≈ 2π*a
        let last = &points[99];
        assert!(last.y.abs() < 0.01);
        assert!((last.x - 2.0 * std::f64::consts::PI * a).abs() < 0.1);
    }
}
