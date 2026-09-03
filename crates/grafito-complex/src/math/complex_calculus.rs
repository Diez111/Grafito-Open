use num_complex::Complex64;
use std::collections::HashMap;

use crate::math::complex_expr::ComplexExpr;

const MAX_CONTOUR_POINTS: usize = 10_000;

/// Numerical integration of a complex function over a contour (path).
/// Approximates the integral \oint_C f(z) dz using the trapezoidal rule.
///
/// Retorna error si algún `f(z)` no es finito (polo/NaN en el contorno) o
/// si el contorno excede `MAX_CONTOUR_POINTS` (presupuesto anti-DoS).
pub fn contour_integral(
    expr: &ComplexExpr,
    path: &[Complex64],
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    if path.len() < 2 {
        return Ok(Complex64::new(0.0, 0.0));
    }
    if path.len() > MAX_CONTOUR_POINTS {
        return Err(format!(
            "contour too large: {} > {MAX_CONTOUR_POINTS}",
            path.len()
        ));
    }

    let mut integral = Complex64::new(0.0, 0.0);
    let mut local_vars = vars.clone();

    for i in 0..path.len() - 1 {
        let z0 = path[i];
        let z1 = path[i + 1];

        // Evaluate at z0
        local_vars.insert(symbol.to_string(), z0);
        let f0 = expr.eval(&local_vars)?;
        if !f0.re.is_finite() || !f0.im.is_finite() {
            return Err(format!("non-finite value at contour point {i}: {f0}"));
        }

        // Evaluate at z1
        local_vars.insert(symbol.to_string(), z1);
        let f1 = expr.eval(&local_vars)?;
        if !f1.re.is_finite() || !f1.im.is_finite() {
            return Err(format!("non-finite value at contour point {}: {f1}", i + 1));
        }

        // Trapezoidal rule: (f(z0) + f(z1)) / 2 * (z1 - z0)
        let dz = z1 - z0;
        if !dz.re.is_finite() || !dz.im.is_finite() {
            return Err(format!("non-finite segment dz at {i}: {dz}"));
        }
        let avg_f = (f0 + f1) * 0.5;
        let contrib = avg_f * dz;
        if !contrib.re.is_finite() || !contrib.im.is_finite() {
            return Err(format!("non-finite contribution at segment {i}"));
        }
        integral += contrib;
        if !integral.re.is_finite() || !integral.im.is_finite() {
            return Err(format!("integral overflow at segment {i}"));
        }
    }

    Ok(integral)
}

/// Detects the sum of residues (and poles) enclosed by a closed contour.
/// By Cauchy's Residue Theorem: \oint_C f(z) dz = 2 * pi * i * Sum(Residues)
pub fn sum_of_residues(
    expr: &ComplexExpr,
    closed_path: &[Complex64],
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    let integral = contour_integral(expr, closed_path, vars, symbol)?;

    // Sum(Res) = 1/(2 * pi * i) * \oint_C f(z) dz
    // 1 / (2 * pi * i) = -i / (2 * pi)
    let inv_2pi_i = Complex64::new(0.0, -1.0 / (2.0 * std::f64::consts::PI));
    Ok(integral * inv_2pi_i)
}

/// Converts a complex function f(z) into a 2D vector field (Flow).
/// For a complex function f(z) = u(x,y) + i v(x,y), the flow can be interpreted as
/// the vector field F(x,y) = (u(x,y), v(x,y)).
/// Another common interpretation (conjugate flow) is F(x,y) = (u(x,y), -v(x,y)).
/// Here we return the standard velocity vector (u, v).
pub fn evaluate_flow(
    expr: &ComplexExpr,
    x: f64,
    y: f64,
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<(f64, f64), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("non-finite flow coordinates".to_string());
    }
    let mut local_vars = vars.clone();
    local_vars.insert(symbol.to_string(), Complex64::new(x, y));
    let result = expr.eval(&local_vars)?;
    if !result.re.is_finite() || !result.im.is_finite() {
        return Err(format!("non-finite flow result: {result}"));
    }
    Ok((result.re, result.im))
}
