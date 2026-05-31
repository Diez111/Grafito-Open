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
pub fn integrate(expr: &str, _var: &str) -> Result<String, String> {
    let result = crate::ast::integrate_adaptive(expr, "x", 0.0, 1.0, 6);
    Ok(format!("∫₀¹ {} dx ≈ {:.8}", expr, result))
}

/// Compute definite integral from `a` to `b`.
pub fn integrate_definite(expr: &str, var: &str, a: f64, b: f64) -> Result<String, String> {
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
                for i in &ip { for o in &op { terms.push(format!("{}*{}", i.trim(), o.trim())); } }
                return Ok(terms.join(" + "));
            }
        }
    }
    if let Ok(v) = crate::expr::evaluate(&expr, &[]) { return Ok(format!("{}", v)); }
    Ok(expr.to_string())
}

pub fn factor(expr: &str) -> Result<String, String> {
    let expr2 = expr.replace(" ", "");
    let fexpr = expr2.clone();
    let f = move |x: f64| crate::expr::eval_function(&fexpr, x).unwrap_or(f64::NAN);
    let mut roots = Vec::new();
    for r in -20..=20 {
        let y = f(r as f64);
        if y.is_finite() && y.abs() < 0.001 { roots.push(r); }
    }
    if roots.is_empty() { return Ok(expr2); }
    let mut factors = Vec::new();
    for r in &roots {
        if *r == 0 { factors.push("x".into()); }
        else if *r > 0 { factors.push(format!("(x - {})", r)); }
        else { factors.push(format!("(x + {})", -r)); }
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

pub fn solve(expr: &str, _var: &str) -> Result<String, String> {
    let expr2 = expr.to_string();
    let f = move |x: f64| crate::expr::eval_function(&expr2, x).unwrap_or(f64::NAN);
    match crate::cas::find_root(f, (-10.0, 10.0)) {
        Some(root) => Ok(format!("Root ≈ {:.8}", root)),
        None => Ok("No real roots found in [-10, 10]".to_string()),
    }
}
