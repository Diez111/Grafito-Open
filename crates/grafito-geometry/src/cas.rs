// Grafito CAS — Computer Algebra System (numeric methods for alpha)
//
// For alpha stage, uses numerical methods. Symbolic CAS planned for future
// integration with `symbolica` crate or SymEngine bindings.

const MAX_LEGACY_INTEGRAL_STEPS: usize = 100_000;
const ROOT_SCAN_INTERVALS: usize = 64;
const ROOT_LOCAL_RESIDUAL_TOLERANCE: f64 = 1.5e-8;
const NEWTON_RELATIVE_RESIDUAL_TOLERANCE: f64 = 128.0 * f64::EPSILON;
const NEWTON_RELATIVE_STEP_TOLERANCE: f64 = 1e-10;

/// Numeric derivative using central finite difference.
/// f'(x) ≈ (f(x+h) - f(x-h)) / (2h)
pub fn derivative<F: Fn(f64) -> f64>(f: F, x: f64, h: Option<f64>) -> f64 {
    let h = h.unwrap_or(1e-6);
    (f(x + h) - f(x - h)) / (2.0 * h)
}

/// Numeric integral using Simpson's rule.
///
/// Devuelve `NaN` si `n` supera el presupuesto fijo de 100 000 pasos.
pub fn integral<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, n: usize) -> f64 {
    let n = n.max(2);
    if n > MAX_LEGACY_INTEGRAL_STEPS {
        return f64::NAN;
    }
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
pub fn newton_root<F, G>(
    f: F,
    df: G,
    initial: f64,
    max_iter: usize,
    tol: f64,
) -> Result<f64, String>
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
        x -= fx / dfx;
    }
    Err("Newton did not converge".into())
}

/// Find root using Newton's method with auto-derivative (finite difference).
pub fn newton_root_auto<F: Fn(f64) -> f64>(f: &F, initial: f64) -> Result<f64, String> {
    if !initial.is_finite() {
        return Err("Initial value is not finite".into());
    }

    let mut x = initial;
    let mut fx = f(x);
    if !fx.is_finite() {
        return Err("Function returned a non-finite value".into());
    }
    let mut best_residual = fx.abs();
    let mut residual_scale = best_residual;
    let mut made_progress = false;

    for _ in 0..50 {
        let h = 1e-6 * x.abs().max(1.0);
        let left = f(x - h);
        let right = f(x + h);
        if !left.is_finite() || !right.is_finite() {
            return Err("Function returned a non-finite value".into());
        }
        residual_scale = residual_scale.max(left.abs()).max(right.abs());

        if fx == 0.0 && (left != 0.0 || right != 0.0) {
            return Ok(x);
        }

        let dfx = (right - left) / (2.0 * h);
        if !dfx.is_finite() || dfx == 0.0 {
            return Err("Derivative near zero".into());
        }
        let step = fx / dfx;
        let next = x - step;
        if !step.is_finite() || !next.is_finite() {
            return Err("Newton produced a non-finite iterate".into());
        }

        let next_fx = f(next);
        if !next_fx.is_finite() {
            return Err("Function returned a non-finite value".into());
        }
        let next_residual = next_fx.abs();
        residual_scale = residual_scale.max(next_residual);
        if next_residual < best_residual {
            best_residual = next_residual;
            made_progress = true;
        }
        if next_fx == 0.0 && made_progress {
            let next_h = 1e-6 * next.abs().max(1.0);
            let next_left = f(next - next_h);
            let next_right = f(next + next_h);
            if next_left.is_finite()
                && next_right.is_finite()
                && (next_left != 0.0 || next_right != 0.0)
            {
                return Ok(next);
            }
        }

        let residual_is_small = residual_scale > 0.0
            && next_residual <= NEWTON_RELATIVE_RESIDUAL_TOLERANCE * residual_scale;
        let step_is_small = step.abs() <= NEWTON_RELATIVE_STEP_TOLERANCE * next.abs().max(1.0);
        if made_progress && residual_is_small && step_is_small {
            let next_h = 1e-6 * next.abs().max(1.0);
            let next_left = f(next - next_h);
            let next_right = f(next + next_h);
            if next_left.is_finite()
                && next_right.is_finite()
                && opposite_signs(next_left, next_right)
            {
                return Ok(next);
            }
        }
        if next == x {
            break;
        }

        x = next;
        fx = next_fx;
    }
    Err("Newton did not converge".into())
}

fn opposite_signs(left: f64, right: f64) -> bool {
    left != 0.0 && right != 0.0 && left.is_sign_negative() != right.is_sign_negative()
}

fn has_scale_aware_root_residual<F: Fn(f64) -> f64>(f: &F, root: f64, a: f64, b: f64) -> bool {
    let residual = f(root);
    if !residual.is_finite() {
        return false;
    }
    if a == b {
        return residual == 0.0;
    }

    let interval_scale = (b - a).abs();
    let h = (interval_scale * 1e-6).max(root.abs().max(1.0) * 1.5e-8);
    let left_x = (root - h).max(a);
    let right_x = (root + h).min(b);
    let left_value = if left_x == root {
        None
    } else {
        Some(f(left_x))
    };
    let right_value = if right_x == root {
        None
    } else {
        Some(f(right_x))
    };
    if left_value.is_some_and(|value| !value.is_finite())
        || right_value.is_some_and(|value| !value.is_finite())
    {
        return false;
    }

    let local_scale = left_value
        .into_iter()
        .chain(right_value)
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    if residual == 0.0 {
        return local_scale > 0.0;
    }

    matches!((left_value, right_value), (Some(left), Some(right)) if opposite_signs(left, right))
        && residual.abs() <= ROOT_LOCAL_RESIDUAL_TOLERANCE * local_scale
}

fn bisect_root<F: Fn(f64) -> f64>(
    f: &F,
    mut left: f64,
    mut right: f64,
    mut left_value: f64,
    mut right_value: f64,
    range: (f64, f64),
) -> Option<f64> {
    for _ in 0..100 {
        let middle = left + (right - left) * 0.5;
        if middle == left || middle == right {
            break;
        }
        let middle_value = f(middle);
        if !middle_value.is_finite() {
            return None;
        }
        if middle_value == 0.0 {
            return Some(middle);
        }
        if opposite_signs(left_value, middle_value) {
            right = middle;
            right_value = middle_value;
        } else {
            left = middle;
            left_value = middle_value;
        }
    }

    let candidate = if left_value.abs() <= right_value.abs() {
        left
    } else {
        right
    };
    has_scale_aware_root_residual(f, candidate, range.0, range.1).then_some(candidate)
}

/// Try multiple initial guesses to find a root.
pub fn find_root<F: Fn(f64) -> f64>(f: F, range: (f64, f64)) -> Option<f64> {
    let (a, b) = range;
    if !a.is_finite() || !b.is_finite() || a > b {
        return None;
    }
    if a == b {
        return (f(a) == 0.0).then_some(a);
    }

    let span = b - a;
    if !span.is_finite() {
        return None;
    }

    let mut previous_x = a;
    let mut previous_value = f(a);
    if previous_value.is_finite()
        && previous_value == 0.0
        && has_scale_aware_root_residual(&f, a, a, b)
    {
        return Some(a);
    }
    for index in 1..=ROOT_SCAN_INTERVALS {
        let x = if index == ROOT_SCAN_INTERVALS {
            b
        } else {
            a + span * index as f64 / ROOT_SCAN_INTERVALS as f64
        };
        let value = f(x);
        if value.is_finite() {
            if value == 0.0 && has_scale_aware_root_residual(&f, x, a, b) {
                return Some(x);
            }
            if previous_value.is_finite() && opposite_signs(previous_value, value) {
                if let Some(root) = bisect_root(&f, previous_x, x, previous_value, value, range) {
                    return Some(root);
                }
            }
        }
        previous_x = x;
        previous_value = value;
    }

    for guess in [a, b, (a + b) * 0.5, a * 0.7 + b * 0.3, a * 0.3 + b * 0.7] {
        if let Ok(root) = newton_root_auto(&f, guess) {
            if root >= a && root <= b && has_scale_aware_root_residual(&f, root, a, b) {
                return Some(root);
            }
        }
    }
    None
}

/// Evaluate expression with f(x) form and find all roots in [a, b] by scanning.
pub fn solve_expression(
    expr: &str,
    var: f64,
    vars: &std::collections::HashMap<String, f64>,
    a: f64,
    b: f64,
) -> Result<f64, String> {
    let expr_owned = expr.to_string();
    let f = move |x: f64| {
        let mut v = vars.clone();
        v.insert("x".to_string(), x);
        crate::expr::evaluate(
            &expr_owned,
            &v.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>(),
        )
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
    for (i, val) in vals.iter_mut().enumerate() {
        let h = h0 / (1 << i) as f64;
        *val = f(x + h);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn hardening_find_root_rejects_small_nonzero_constant() {
        assert_eq!(find_root(|_| 1e-9, (-10.0, 10.0)), None);
        assert!(newton_root_auto(&|_| 1e-9, 0.0).is_err());
    }

    #[test]
    fn hardening_find_root_rejects_small_positive_minimum() {
        assert_eq!(find_root(|x| x * x + 1e-30, (-1.0, 1.0)), None);
    }

    #[test]
    fn hardening_find_root_preserves_bracketed_root() {
        let root = find_root(|x| x * x - 2.0, (0.0, 2.0)).expect("root should exist");

        assert!((root - 2.0_f64.sqrt()).abs() < 1e-10);
    }

    #[test]
    fn hardening_find_root_is_scale_aware_for_small_linear_function() {
        let root = find_root(|x| 1e-20 * (x - 1.0), (0.0, 2.0)).expect("root should exist");

        assert!((root - 1.0).abs() < 1e-10);
    }

    #[test]
    fn hardening_legacy_integral_preserves_normal_simpson_result() {
        let result = integral(|x| x * x, 0.0, 1.0, 1_000);

        assert!((result - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn hardening_legacy_integral_rejects_unbounded_steps_without_evaluating() {
        let evaluations = AtomicUsize::new(0);
        let result = integral(
            |x| {
                evaluations.fetch_add(1, Ordering::Relaxed);
                x
            },
            0.0,
            1.0,
            usize::MAX,
        );

        assert!(result.is_nan());
        assert_eq!(evaluations.load(Ordering::Relaxed), 0);
    }
}
