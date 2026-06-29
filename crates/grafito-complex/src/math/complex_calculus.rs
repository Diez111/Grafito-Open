use num_complex::Complex64;
use std::collections::HashMap;

use crate::math::complex_expr::ComplexExpr;

/// Numerical integration of a complex function over a contour (path).
/// Approximates the integral \oint_C f(z) dz using the trapezoidal rule.
pub fn contour_integral(
    expr: &ComplexExpr,
    path: &[Complex64],
    vars: &HashMap<String, Complex64>,
    symbol: &str,
) -> Result<Complex64, String> {
    if path.len() < 2 {
        return Ok(Complex64::new(0.0, 0.0));
    }

    let mut integral = Complex64::new(0.0, 0.0);
    let mut local_vars = vars.clone();

    for i in 0..path.len() - 1 {
        let z0 = path[i];
        let z1 = path[i + 1];

        // Evaluate at z0
        local_vars.insert(symbol.to_string(), z0);
        let f0 = expr.eval(&local_vars)?;

        // Evaluate at z1
        local_vars.insert(symbol.to_string(), z1);
        let f1 = expr.eval(&local_vars)?;

        // Trapezoidal rule: (f(z0) + f(z1)) / 2 * (z1 - z0)
        let dz = z1 - z0;
        let avg_f = (f0 + f1) * 0.5;
        integral += avg_f * dz;
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
    let mut local_vars = vars.clone();
    local_vars.insert(symbol.to_string(), Complex64::new(x, y));
    let result = expr.eval(&local_vars)?;
    Ok((result.re, result.im))
}
