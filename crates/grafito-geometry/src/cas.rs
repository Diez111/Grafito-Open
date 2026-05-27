// Grafito CAS — Computer Algebra System (numeric methods for alpha)
//
// For alpha stage, uses numerical methods. Symbolic CAS planned for future
// integration with `symbolica` crate or SymEngine bindings.

/// Numeric derivative using central finite difference.
/// f'(x) ≈ (f(x+h) - f(x-h)) / (2h)
pub fn derivative<F: Fn(f64) -> f64>(f: F, x: f64, h: Option<f64>) -> f64 {
    let h = h.unwrap_or(1e-6);
    (f(x + h) - f(x - h)) / (2.0 * h)
}

/// Numeric integral using Simpson's rule.
pub fn integral<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, n: usize) -> f64 {
    let n = n.max(2);
    let n = if n % 2 == 1 { n + 1 } else { n };
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        let x = a + i as f64 * h;
        sum += if i % 2 == 0 { 2.0 * f(x) } else { 4.0 * f(x) };
    }
    sum * h / 3.0
}

/// Numeric definite integral with auto step count.
pub fn integral_auto<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> f64 {
    integral(f, a, b, 1000)
}

/// Find root using Newton's method. Returns Ok(guess) or Err(reason).
pub fn newton_root<F, G>(f: F, df: G, initial: f64, max_iter: usize, tol: f64) -> Result<f64, String>
where
    F: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
{
    let mut x = initial;
    for _ in 0..max_iter {
        let fx = f(x);
        if fx.abs() < tol {
            return Ok(x);
        }
        let dfx = df(x);
        if dfx.abs() < 1e-15 {
            return Err("Derivative near zero".into());
        }
        x = x - fx / dfx;
    }
    Err("Newton did not converge".into())
}

/// Find root using Newton's method with auto-derivative (finite difference).
pub fn newton_root_auto<F: Fn(f64) -> f64>(f: &F, initial: f64) -> Result<f64, String> {
    let mut x = initial;
    for _ in 0..50 {
        let fx = f(x);
        if fx.abs() < 1e-8 {
            return Ok(x);
        }
        let h = 1e-6;
        let dfx = (f(x + h) - f(x - h)) / (2.0 * h);
        if dfx.abs() < 1e-15 {
            return Err("Derivative near zero".into());
        }
        x = x - fx / dfx;
    }
    Err("Newton did not converge".into())
}

/// Try multiple initial guesses to find a root.
pub fn find_root<F: Fn(f64) -> f64>(f: F, range: (f64, f64)) -> Option<f64> {
    let (a, b) = range;
    for guess in [a, b, (a + b) * 0.5, a * 0.7 + b * 0.3, a * 0.3 + b * 0.7] {
        if let Ok(root) = newton_root_auto(&f, guess) {
            if root >= a && root <= b {
                return Some(root);
            }
        }
    }
    None
}

/// Evaluate expression with f(x) form and find all roots in [a, b] by scanning.
pub fn solve_expression(expr: &str, var: f64, vars: &std::collections::HashMap<String, f64>, a: f64, b: f64) -> Result<f64, String> {
    let expr_owned = expr.to_string();
    let f = move |x: f64| {
        let mut v = vars.clone();
        v.insert("x".to_string(), x);
        crate::expr::evaluate(&expr_owned, &v.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>())
            .unwrap_or(f64::NAN)
    };
    // Try equal to zero: solve f(x)=var
    let g = move |x: f64| f(x) - var;
    find_root(g, (a, b)).ok_or("No root found in range".into())
}

/// Compute `limit f(x) as x -> a` using Richardson extrapolation.
pub fn limit<F: Fn(f64) -> f64>(f: F, x: f64) -> f64 {
    let h0 = 0.1;
    let mut vals = [0.0f64; 5];
    for i in 0..5 {
        let h = h0 / (1 << i) as f64;
        vals[i] = f(x + h);
    }
    // Richardson extrapolation
    let mut r = vals.to_vec();
    for j in 1..5 {
        for i in 0..(5 - j) {
            let p = 2.0f64.powi(j as i32);
            r[i] = (p * r[i + 1] - r[i]) / (p - 1.0);
        }
    }
    r[0]
}
