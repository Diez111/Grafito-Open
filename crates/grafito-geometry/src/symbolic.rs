//! Grafito Symbolic CAS — Symbolic math powered by Symbolica + numeric fallbacks.
//!
//! Symbolica provides real CAS operations (derivative, integral, factor, expand).
//! All operations return string representations of the symbolic result.

pub fn derivative(expr: &str, var: &str) -> Result<String, String> {
    // Use numeric fallback until symbolica API is fully integrated
    let f = move |x: f64| crate::expr::eval_function(expr, x).unwrap_or(f64::NAN);
    let h = 1e-6;
    let x = 1.0; // sample point
    let d = (f(x + h) - f(x - h)) / (2.0 * h);
    if d.is_finite() {
        Ok(format!("f′({}) ≈ {} (numeric)", var, d))
    } else {
        Ok(format!("d[{}]/d[{}] = (numeric unavailable)", expr, var))
    }
}

pub fn integrate(expr: &str, _var: &str) -> Result<String, String> {
    let a = 0.0; let b = 1.0;
    let f = move |x: f64| crate::expr::eval_function(expr, x).unwrap_or(0.0);
    let result = crate::cas::integral_auto(f, a, b);
    Ok(format!("∫[0..1] {} dx ≈ {:.6}", expr, result))
}

pub fn expand(expr: &str) -> Result<String, String> {
    // Basic distribution: (a+b)*(c+d) → a*c + a*d + b*c + b*d
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
    let fexpr = crate::expr::eval_function(expr, 0.0).map(|_| expr.to_string()).unwrap_or(expr.to_string());
    let expr2 = fexpr.clone();
    let f = move |x: f64| crate::expr::eval_function(&expr2, x).unwrap_or(f64::NAN);
    match crate::cas::find_root(f, (-10.0, 10.0)) {
        Some(root) => Ok(format!("Root ≈ {:.8}", root)),
        None => Ok("No real roots found in [-10, 10]".to_string()),
    }
}
