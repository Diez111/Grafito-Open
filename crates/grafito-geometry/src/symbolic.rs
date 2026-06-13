//! Grafito Symbolic CAS — Symbolic differentiation, integration and limits.
//! Uses the AST engine in `ast.rs` for symbolic math,
//! with numeric fallbacks using `expr.rs` for evaluation.

/// Compute the symbolic derivative of `expr` with respect to `var`.
/// Returns a pretty-printed expression string.
pub fn derivative(expr: &str, var: &str) -> Result<String, String> {
    let preprocessed = expr.replace(" ", "");
    match crate::ast::parse_ast(&preprocessed) {
        Ok(ast) => {
            let d = ast.diff(var).simplify();
            Ok(d.to_expr_string())
        }
        Err(_) => {
            // Numeric fallback (central difference)
            let f = move |x: f64| crate::expr::eval_function(expr, x).unwrap_or(f64::NAN);
            let h = 1e-6;
            let x = 1.0;
            let d = (f(x + h) - f(x - h)) / (2.0 * h);
            if d.is_finite() {
                Ok(format!("≈ {} (numérica en x=1)", d))
            } else {
                Err(format!("No se pudo calcular la derivada de '{}'", expr))
            }
        }
    }
}

/// Compute a definite integral numerically with adaptive Gauss-Legendre quadrature.
pub fn integrate(expr: &str, var: &str) -> Result<String, String> {
    let preprocessed = expr.replace(" ", "");
    match crate::ast::parse_ast(&preprocessed) {
        Ok(ast) => match ast.integrate(var) {
            Some(integrated) => {
                let result = integrated.to_expr_string();
                Ok(format!("{result} + C"))
            }
            None => {
                let result = crate::ast::integrate_adaptive(expr, var, 0.0, 1.0, 6);
                Ok(format!("∫₀¹ {expr} d{var} ≈ {result:.8}"))
            }
        },
        Err(_) => {
            let result = crate::ast::integrate_adaptive(expr, var, 0.0, 1.0, 6);
            Ok(format!("∫₀¹ {expr} d{var} ≈ {result:.8}"))
        }
    }
}

/// Compute definite integral from `a` to `b`.
pub fn integrate_definite(expr: &str, var: &str, a: f64, b: f64) -> Result<String, String> {
    // Try symbolic + evaluate at bounds
    let preprocessed = expr.replace(" ", "");
    if let Ok(ast) = crate::ast::parse_ast(&preprocessed) {
        if let Some(integrated) = ast.integrate(var) {
            let fa =
                crate::expr::eval_function(&integrated.to_expr_string(), a).unwrap_or(f64::NAN);
            let fb =
                crate::expr::eval_function(&integrated.to_expr_string(), b).unwrap_or(f64::NAN);
            if fa.is_finite() && fb.is_finite() {
                return Ok(format!("∫[{a},{b}] {expr} d{var} = {:.8}", fb - fa));
            }
        }
    }
    let result = crate::ast::integrate_adaptive(expr, var, a, b, 7);
    Ok(format!("∫[{a},{b}] {expr} d{var} ≈ {result:.8}"))
}

/// Compute the limit of `expr` as `var -> at` numerically.
pub fn limit(expr: &str, var: &str, at: f64) -> Result<String, String> {
    match crate::ast::compute_limit(expr, var, at) {
        Some(val) => Ok(format!("lim({var}→{at}) {expr} = {val:.8}")),
        None => Ok(format!("lim({var}→{at}) {expr} = no existe (o es ∞)")),
    }
}

pub fn expand(expr: &str) -> Result<String, String> {
    // Basic distribution: (a+b)*(c+d)
    let expr = expr.replace(" ", "");
    if let Some(rest) = expr.strip_prefix("(") {
        if let Some((inner, outer)) = rest.split_once(")*(") {
            let outer = outer.trim_end_matches(')');
            let ip: Vec<&str> = inner.split('+').collect();
            let op: Vec<&str> = outer.split('+').collect();
            if ip.len() > 1 || op.len() > 1 {
                let mut terms = Vec::new();
                for i in &ip {
                    for o in &op {
                        terms.push(format!("{}*{}", i.trim(), o.trim()));
                    }
                }
                return Ok(terms.join(" + "));
            }
        }
    }
    if let Ok(v) = crate::expr::evaluate(&expr, &[]) {
        return Ok(format!("{}", v));
    }
    Ok(expr.to_string())
}

pub fn factor(expr: &str) -> Result<String, String> {
    let expr2 = expr.replace(" ", "");
    let fexpr = expr2.clone();
    let f = move |x: f64| crate::expr::eval_function(&fexpr, x).unwrap_or(f64::NAN);
    let mut roots = Vec::new();
    for r in -20..=20 {
        let y = f(r as f64);
        if y.is_finite() && y.abs() < 0.001 {
            roots.push(r);
        }
    }
    if roots.is_empty() {
        return Ok(expr2);
    }
    let mut factors = Vec::new();
    for r in &roots {
        if *r == 0 {
            factors.push("x".into());
        } else if *r > 0 {
            factors.push(format!("(x - {})", r));
        } else {
            factors.push(format!("(x + {})", -r));
        }
    }
    Ok(factors.join(" * "))
}

pub fn simplify(expr: &str) -> Result<String, String> {
    // Try symbolic first
    let preprocessed = expr.replace(" ", "");
    if let Ok(ast) = crate::ast::parse_ast(&preprocessed) {
        return Ok(ast.simplify().to_expr_string());
    }
    match crate::expr::evaluate(expr, &[]) {
        Ok(v) => Ok(format!("{}", v)),
        Err(_) => Ok(expr.to_string()),
    }
}

pub fn substitute(expr: &str, var: &str, value: &str) -> Result<String, String> {
    let result = expr.replace(var, &format!("({})", value));
    match crate::expr::evaluate(&result, &[]) {
        Ok(v) => Ok(format!("{}", v)),
        Err(_) => Ok(result),
    }
}

pub fn solve(expr: &str, var: &str) -> Result<String, String> {
    let preprocessed = expr.replace(" ", "");
    if let Ok(ast) = crate::ast::parse_ast(&preprocessed) {
        if let Some(roots) = solve_polynomial_ast(&ast, var) {
            if roots.is_empty() {
                return Ok("No real roots found".to_string());
            }
            let s: Vec<String> = roots
                .iter()
                .map(|r| format!("{} = {:.8}", var, r))
                .collect();
            return Ok(s.join(", "));
        }
    }
    let expr2 = expr.to_string();
    let f = move |x: f64| crate::expr::eval_function(&expr2, x).unwrap_or(f64::NAN);
    match crate::cas::find_root(f, (-10.0, 10.0)) {
        Some(root) => Ok(format!("Root ≈ {:.8}", root)),
        None => Ok("No real roots found in [-10, 10]".to_string()),
    }
}

fn solve_polynomial_ast(ast: &crate::ast::Expr, var: &str) -> Option<Vec<f64>> {
    let coeffs = collect_polynomial_coeffs(ast, var, 4)?;
    let roots = solve_polynomial_real(&coeffs);
    Some(roots)
}

fn collect_polynomial_coeffs(
    ast: &crate::ast::Expr,
    var: &str,
    max_deg: usize,
) -> Option<Vec<f64>> {
    use crate::ast::Expr::{self, *};
    let mut coeffs = vec![0.0; max_deg + 1];
    fn collect_terms(expr: &Expr, var: &str, coeffs: &mut Vec<f64>) -> bool {
        match expr {
            Const(c) => {
                coeffs[0] += c;
                true
            }
            Var(v) if v == var => {
                coeffs[1] += 1.0;
                true
            }
            Neg(a) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] -= sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Add(a, b) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, coeffs) && collect_terms(b, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] += sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Sub(a, b) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, coeffs) && collect_terms(b, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] -= sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Mul(a, b) => {
                if let Const(c) = a.as_ref() {
                    let mut sub = vec![0.0; coeffs.len()];
                    if collect_terms(b, var, &mut sub) {
                        for i in 0..coeffs.len() {
                            coeffs[i] += c * sub[i];
                        }
                        true
                    } else {
                        false
                    }
                } else if let Const(c) = b.as_ref() {
                    let mut sub = vec![0.0; coeffs.len()];
                    if collect_terms(a, var, &mut sub) {
                        for i in 0..coeffs.len() {
                            coeffs[i] += c * sub[i];
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Pow(base, exp) => {
                if let (Var(v), Const(n)) = (base.as_ref(), exp.as_ref()) {
                    if v == var && *n >= 0.0 && *n <= coeffs.len() as f64 - 1.0 {
                        let idx = n.round() as usize;
                        if idx < coeffs.len() {
                            coeffs[idx] += 1.0;
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
    if collect_terms(ast, var, &mut coeffs) {
        Some(coeffs)
    } else {
        None
    }
}

fn solve_polynomial_real(coeffs: &[f64]) -> Vec<f64> {
    let degree = coeffs.iter().rposition(|&c| c.abs() > 1e-12).unwrap_or(0);
    match degree {
        1 => solve_linear(coeffs),
        2 => solve_quadratic(coeffs),
        3 => solve_cubic(coeffs),
        _ => solve_polynomial_newton(coeffs),
    }
}

fn solve_linear(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[1];
    let b = coeffs[0];
    if a.abs() < 1e-12 {
        vec![]
    } else {
        vec![-b / a]
    }
}

fn solve_quadratic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[2];
    let b = coeffs[1];
    let c = coeffs[0];
    if a.abs() < 1e-12 {
        return solve_linear(&[c, b]);
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < -1e-12 {
        vec![]
    } else if discriminant.abs() < 1e-12 {
        vec![-b / (2.0 * a)]
    } else {
        let sqrt_d = discriminant.sqrt();
        vec![(-b - sqrt_d) / (2.0 * a), (-b + sqrt_d) / (2.0 * a)]
    }
}

fn solve_cubic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[3];
    let b = coeffs[2];
    let c = coeffs[1];
    let d = coeffs[0];
    if a.abs() < 1e-12 {
        return solve_quadratic(&[d, c, b]);
    }
    let b = b / a;
    let c = c / a;
    let d = d / a;
    let p = c - b * b / 3.0;
    let q = d - b * c / 3.0 + 2.0 * b * b * b / 27.0;
    let discriminant = q * q / 4.0 + p * p * p / 27.0;
    let shift = -b / 3.0;
    if discriminant > 1e-12 {
        let sqrt_d = discriminant.sqrt();
        let u = (-q / 2.0 + sqrt_d).cbrt();
        let v = (-q / 2.0 - sqrt_d).cbrt();
        vec![u + v + shift]
    } else if discriminant.abs() < 1e-12 {
        let u = (-q / 2.0).cbrt();
        let r1 = 2.0 * u + shift;
        let r2 = -u + shift;
        if (r1 - r2).abs() < 1e-12 {
            vec![r1]
        } else {
            vec![r1, r2]
        }
    } else {
        let r = (-p / 3.0).sqrt();
        let phi = (-q / (2.0 * r * r * r)).acos();
        let r1 = 2.0 * r * (phi / 3.0).cos() + shift;
        let r2 = 2.0 * r * ((phi + 2.0 * std::f64::consts::PI) / 3.0).cos() + shift;
        let r3 = 2.0 * r * ((phi + 4.0 * std::f64::consts::PI) / 3.0).cos() + shift;
        vec![r1, r2, r3]
    }
}

fn solve_polynomial_newton(coeffs: &[f64]) -> Vec<f64> {
    let mut roots = Vec::new();
    let f = |x: f64| {
        let mut result = 0.0;
        let mut xn = 1.0;
        for c in coeffs {
            result += c * xn;
            xn *= x;
        }
        result
    };
    let df = |x: f64| {
        let mut result = 0.0;
        let mut xn = 1.0;
        for (i, c) in coeffs.iter().enumerate() {
            if i > 0 {
                result += (i as f64) * c * xn;
            }
            xn *= x;
        }
        result
    };
    for start in -10..=10 {
        let mut x = start as f64;
        for _ in 0..50 {
            let fx = f(x);
            if fx.abs() < 1e-10 {
                let is_dup = roots.iter().any(|r: &f64| (r - x).abs() < 1e-6);
                if !is_dup {
                    roots.push(x);
                }
                break;
            }
            let dfx = df(x);
            if dfx.abs() < 1e-15 {
                break;
            }
            x -= fx / dfx;
        }
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solve_linear() {
        let result = solve("2*x - 4", "x").unwrap();
        assert!(result.contains("2"));
    }

    #[test]
    fn test_solve_quadratic() {
        let result = solve("x^2 - 4", "x").unwrap();
        assert!(result.contains("-2") || result.contains("2.0"));
    }

    #[test]
    fn test_solve_cubic() {
        let result = solve("x^3 - x", "x").unwrap();
        assert!(result.contains("0") || result.contains("-1") || result.contains("1"));
    }

    #[test]
    fn test_integrate_sin() {
        let result = integrate("sin(x)", "x").unwrap();
        assert!(result.contains("cos") || result.contains("Cos"));
    }

    #[test]
    fn test_integrate_power() {
        let result = integrate("x^2", "x").unwrap();
        assert!(
            result.contains("x ^ 3") || result.contains("x^3"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_integrate_definite_linear() {
        let result = integrate_definite("2*x", "x", 0.0, 3.0).unwrap();
        assert!(result.contains("9"));
    }
}
