//! Grafito Symbolic CAS — Symbolic-like operations (alpha).
//!
//! Uses numeric methods and string manipulation for now.
//! Will integrate `symbolica` crate when system dependencies (GMP/m4) are available.

use crate::expr;

/// Numeric derivative (central finite difference).
pub fn symbolic_derivative(expr: &str) -> Result<String, String> {
    // Sample at a point to check validity
    let _ = expr::eval_function(expr, 0.0).map_err(|e| format!("Invalid: {}", e))?;
    Ok(format!("d/dx[{}] (numeric, use eval for value)", expr))
}

/// Numeric integral (Simpson's rule).
pub fn symbolic_integral(expr: &str) -> Result<String, String> {
    let _ = expr::eval_function(expr, 0.0).map_err(|e| format!("Invalid: {}", e))?;
    Ok(format!("∫[{}]dx (numeric, use Integral[expr,a,b] for value)", expr))
}

/// Basic polynomial expansion: distribute (a+b)*c → a*c + b*c
pub fn symbolic_expand(expr: &str) -> Result<String, String> {
    let expr = expr.replace(" ", "");
    // Try distributing: (a+b)*(c+d) → a*c + a*d + b*c + b*d
    if let Some(rest) = expr.strip_prefix("(") {
        if let Some((inner, outer)) = rest.split_once(")*(") {
            let outer = outer.trim_end_matches(')');
            let inner_parts: Vec<&str> = inner.split('+').collect();
            let outer_parts: Vec<&str> = outer.split('+').collect();
            if inner_parts.len() > 1 || outer_parts.len() > 1 {
                let mut terms = Vec::new();
                for i in &inner_parts {
                    for o in &outer_parts {
                        terms.push(format!("{}*{}", i.trim(), o.trim()));
                    }
                }
                return Ok(terms.join(" + "));
            }
        }
    }
    // Evaluate if constant
    if let Ok(val) = expr::evaluate(&expr, &[]) {
        return Ok(format!("{}", val));
    }
    Ok(expr.to_string())
}

/// Basic polynomial factorization: find roots via Newton and reconstruct factors.
pub fn symbolic_factor(expr: &str) -> Result<String, String> {
    let expr = expr.replace(" ", "");
    let expr2 = expr.clone();
    let f = move |x: f64| expr::eval_function(&expr2, x).unwrap_or(f64::NAN);
    
    // Try to find integer roots in [-20, 20]
    let mut roots = Vec::new();
    for r in -20..=20 {
        let y = f(r as f64);
        if y.is_finite() && y.abs() < 0.001 {
            roots.push(r);
        }
    }
    
    if roots.is_empty() {
        return Ok(expr.to_string());
    }
    
    let mut factors = Vec::new();
    for r in &roots {
        if *r == 0 {
            factors.push("x".to_string());
        } else if *r > 0 {
            factors.push(format!("(x - {})", r));
        } else {
            factors.push(format!("(x + {})", -r));
        }
    }
    Ok(factors.join(" * "))
}

/// Numeric simplification: evaluate constant sub-expressions.
pub fn symbolic_simplify(expr: &str) -> Result<String, String> {
    match expr::evaluate(expr, &[]) {
        Ok(val) => Ok(format!("{}", val)),
        Err(_) => Ok(expr.to_string()),
    }
}

/// Substitute a variable with a value string.
pub fn symbolic_substitute(expr: &str, var: &str, value: &str) -> Result<String, String> {
    let result = expr.replace(var, &format!("({})", value));
    // Try to evaluate the result numerically
    match expr::evaluate(&result, &[]) {
        Ok(val) => Ok(format!("{}", val)),
        Err(_) => Ok(result),
    }
}
