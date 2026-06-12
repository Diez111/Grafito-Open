//! Special mathematical curves with parametric equations.
//!
//! This module provides functions to generate points for various special curves
//! commonly used in mathematics and physics.

use crate::Point2;

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
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
        let r = a * (1.0 + theta.cos());
        let x = r * theta.cos();
        let y = r * theta.sin();
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    let k = n as f64 / d as f64;
    let max_theta = if (n * d) % 2 == 0 {
        2.0 * std::f64::consts::PI * d as f64
    } else {
        std::f64::consts::PI * d as f64
    };

    for i in 0..steps {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a * (k * theta).cos();
        let x = r * theta.cos();
        let y = r * theta.sin();
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a + b * theta;
        let x = r * theta.cos();
        let y = r * theta.sin();
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let theta = max_theta * (i as f64) / (steps as f64);
        let r = a * (b * theta).exp();
        let x = r * theta.cos();
        let y = r * theta.sin();
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    let max_t = 2.0 * std::f64::consts::PI;

    for i in 0..steps {
        let t = max_t * (i as f64) / (steps as f64);
        let x = a * (freq_x * t + delta).sin();
        let y = b * (freq_y * t).sin();
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    let max_theta = 2.0 * std::f64::consts::PI * k.ceil();

    for i in 0..steps {
        let theta = max_theta * (i as f64) / (steps as f64);
        let x = r * ((1.0 + k) * theta.cos() - k * ((1.0 + k) * theta / k).cos());
        let y = r * ((1.0 + k) * theta.sin() - k * ((1.0 + k) * theta / k).sin());
        points.push(Point2::new(x, y));
    }
    points
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
    let mut points = Vec::with_capacity(steps);
    let max_theta = 2.0 * std::f64::consts::PI * k.ceil();

    for i in 0..steps {
        let theta = max_theta * (i as f64) / (steps as f64);
        let x = r * ((k - 1.0) * theta.cos() + ((k - 1.0) * theta).cos());
        let y = r * ((k - 1.0) * theta.sin() - ((k - 1.0) * theta).sin());
        points.push(Point2::new(x, y));
    }
    points
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
}
