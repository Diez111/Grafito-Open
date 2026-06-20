//! Ordinary Differential Equation (ODE) solvers.
//!
//! This module provides numerical methods for solving initial value problems
//! of the form dy/dt = f(t, y) with y(t0) = y0.

use crate::Point2;

/// Solve an ODE using Euler's method.
///
/// Euler's method is the simplest numerical integration technique:
/// y_{n+1} = y_n + h * f(t_n, y_n)
///
/// # Arguments
/// * `f` - The derivative function f(t, y) -> dy/dt
/// * `t0` - Initial time
/// * `y0` - Initial value
/// * `t_end` - Final time
/// * `steps` - Number of integration steps
///
/// # Returns
/// Vector of (t, y) points representing the solution
pub fn euler<F>(f: F, t0: f64, y0: f64, t_end: f64, steps: usize) -> Vec<(f64, f64)>
where
    F: Fn(f64, f64) -> f64,
{
    if steps == 0 {
        return vec![(t0, y0)];
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    for _ in 0..steps {
        let dydt = f(t, y);
        y += h * dydt;
        t += h;
        points.push((t, y));
    }

    points
}

/// Solve an ODE using the 4th-order Runge-Kutta method.
///
/// RK4 is a widely-used method that provides good accuracy:
/// k1 = h * f(t_n, y_n)
/// k2 = h * f(t_n + h/2, y_n + k1/2)
/// k3 = h * f(t_n + h/2, y_n + k2/2)
/// k4 = h * f(t_n + h, y_n + k3)
/// y_{n+1} = y_n + (k1 + 2*k2 + 2*k3 + k4) / 6
///
/// # Arguments
/// * `f` - The derivative function f(t, y) -> dy/dt
/// * `t0` - Initial time
/// * `y0` - Initial value
/// * `t_end` - Final time
/// * `steps` - Number of integration steps
///
/// # Returns
/// Vector of (t, y) points representing the solution
pub fn runge_kutta_4<F>(f: F, t0: f64, y0: f64, t_end: f64, steps: usize) -> Vec<(f64, f64)>
where
    F: Fn(f64, f64) -> f64,
{
    if steps == 0 {
        return vec![(t0, y0)];
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    for _ in 0..steps {
        let k1 = h * f(t, y);
        let k2 = h * f(t + h / 2.0, y + k1 / 2.0);
        let k3 = h * f(t + h / 2.0, y + k2 / 2.0);
        let k4 = h * f(t + h, y + k3);

        y += (k1 + 2.0 * k2 + 2.0 * k3 + k4) / 6.0;
        t += h;
        points.push((t, y));
    }

    points
}

/// Solve a system of ODEs using Euler's method.
///
/// For systems of the form:
/// dy1/dt = f1(t, y1, y2, ...)
/// dy2/dt = f2(t, y1, y2, ...)
/// ...
///
/// # Arguments
/// * `f` - Vector of derivative functions
/// * `t0` - Initial time
/// * `y0` - Initial values vector
/// * `t_end` - Final time
/// * `steps` - Number of integration steps
///
/// # Returns
/// Vector of (t, [y1, y2, ...]) points representing the solution
pub fn euler_system<F>(
    f: F,
    t0: f64,
    y0: Vec<f64>,
    t_end: f64,
    steps: usize,
) -> Vec<(f64, Vec<f64>)>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    if steps == 0 {
        return vec![(t0, y0.clone())];
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    let mut t = t0;
    let mut y = y0.clone();

    points.push((t, y.clone()));

    for _ in 0..steps {
        let dydt = f(t, &y);
        let n = y.len().min(dydt.len());
        for i in 0..n {
            y[i] += h * dydt[i];
        }
        t += h;
        points.push((t, y.clone()));
    }

    points
}

/// Solve a system of ODEs using the 4th-order Runge-Kutta method.
///
/// # Arguments
/// * `f` - Vector of derivative functions
/// * `t0` - Initial time
/// * `y0` - Initial values vector
/// * `t_end` - Final time
/// * `steps` - Number of integration steps
///
/// # Returns
/// Vector of (t, [y1, y2, ...]) points representing the solution
pub fn runge_kutta_4_system<F>(
    f: F,
    t0: f64,
    y0: Vec<f64>,
    t_end: f64,
    steps: usize,
) -> Vec<(f64, Vec<f64>)>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    if steps == 0 {
        return vec![(t0, y0.clone())];
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    let mut t = t0;
    let mut y = y0.clone();
    let n = y.len();

    points.push((t, y.clone()));

    let mut y_temp = vec![0.0; n];
    for _ in 0..steps {
        let k1 = f(t, &y);
        let k1 = if k1.len() == n { k1 } else { vec![0.0; n] };

        for i in 0..n {
            y_temp[i] = y[i] + h / 2.0 * k1[i];
        }
        let k2 = f(t + h / 2.0, &y_temp);
        let k2 = if k2.len() == n { k2 } else { vec![0.0; n] };

        for i in 0..n {
            y_temp[i] = y[i] + h / 2.0 * k2[i];
        }
        let k3 = f(t + h / 2.0, &y_temp);
        let k3 = if k3.len() == n { k3 } else { vec![0.0; n] };

        for i in 0..n {
            y_temp[i] = y[i] + h * k3[i];
        }
        let k4 = f(t + h, &y_temp);
        let k4 = if k4.len() == n { k4 } else { vec![0.0; n] };

        for i in 0..n {
            y[i] += h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
        }
        t += h;
        points.push((t, y.clone()));
    }

    points
}

/// Solve an ODE using the adaptive Runge-Kutta-Fehlberg (RKF45) method.
///
/// RKF45 uses embedded 4th and 5th order Runge-Kutta formulas to estimate
/// the local truncation error and adjust the step size dynamically.
///
/// # Arguments
/// * `f` - The derivative function f(t, y) -> dy/dt
/// * `t0` - Initial time
/// * `y0` - Initial value
/// * `t_end` - Final time
/// * `tol` - Desired tolerance for error control
///
/// # Returns
/// Vector of (t, y) points representing the solution
pub fn runge_kutta_45<F>(f: F, t0: f64, y0: f64, t_end: f64, tol: f64) -> Vec<(f64, f64)>
where
    F: Fn(f64, f64) -> f64,
{
    let mut points = Vec::new();
    let mut t = t0;
    let mut y = y0;
    let span = t_end - t0;

    if span.abs() < 1e-15 {
        points.push((t0, y0));
        return points;
    }

    let direction = span.signum();
    let mut h = span.abs() / 10.0 * direction;
    let h_min = span.abs() * 1e-10;
    let h_max = span.abs();
    let safety = 0.9;

    points.push((t, y));

    let max_steps = 100_000;
    let mut step_count = 0;

    while (t_end - t).abs() > tol && step_count < max_steps {
        step_count += 1;

        let remaining = t_end - t;
        if remaining.abs() < h.abs() {
            h = remaining;
        }

        let h_abs = h.abs();

        let k1 = h * f(t, y);
        let k2 = h * f(t + h / 4.0, y + k1 / 4.0);
        let k3 = h * f(t + 3.0 * h / 8.0, y + 3.0 * k1 / 32.0 + 9.0 * k2 / 32.0);
        let k4 = h * f(
            t + 12.0 * h / 13.0,
            y + 1932.0 * k1 / 2197.0 - 7200.0 * k2 / 2197.0 + 7296.0 * k3 / 2197.0,
        );
        let k5 = h * f(
            t + h,
            y + 439.0 * k1 / 216.0 - 8.0 * k2 + 3680.0 * k3 / 513.0 - 845.0 * k4 / 4104.0,
        );
        let k6 = h * f(
            t + h / 2.0,
            y - 8.0 * k1 / 27.0 + 2.0 * k2 - 3544.0 * k3 / 2565.0 + 1859.0 * k4 / 4104.0
                - 11.0 * k5 / 40.0,
        );

        let y4 = y + 25.0 * k1 / 216.0 + 1408.0 * k3 / 2565.0 + 2197.0 * k4 / 4104.0 - k5 / 5.0;
        let y5 = y + 16.0 * k1 / 135.0 + 6656.0 * k3 / 12825.0 + 28561.0 * k4 / 56430.0
            - 9.0 * k5 / 50.0
            + 2.0 * k6 / 55.0;

        let err = (y5 - y4).abs();

        if err <= tol || h_abs <= h_min {
            t += h;
            y = y5;
            points.push((t, y));
        }

        let new_h_abs = if err < 1e-15 {
            (h_abs * 4.0).min(h_max)
        } else {
            let factor = safety * (tol / err).powf(0.2);
            (h_abs * factor.clamp(0.1, 4.0)).min(h_max).max(h_min)
        };
        h = new_h_abs * direction;
    }

    points
}

/// Solve a system of ODEs using the adaptive Runge-Kutta-Fehlberg (RKF45) method.
///
/// # Arguments
/// * `f` - Derivative function f(t, &[y]) -> Vec<dy/dt>
/// * `t0` - Initial time
/// * `y0` - Initial values slice
/// * `t_end` - Final time
/// * `tol` - Desired tolerance for error control
///
/// # Returns
/// Vector of (t, Vec<y>) points representing the solution
pub fn runge_kutta_45_system<F>(
    f: F,
    t0: f64,
    y0: &[f64],
    t_end: f64,
    tol: f64,
) -> Vec<(f64, Vec<f64>)>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    let n = y0.len();
    let mut points = Vec::new();
    let mut t = t0;
    let mut y = y0.to_vec();
    let span = t_end - t0;

    if span.abs() < 1e-15 || n == 0 {
        points.push((t0, y0.to_vec()));
        return points;
    }

    let direction = span.signum();
    let mut h = span.abs() / 10.0 * direction;
    let h_min = span.abs() * 1e-10;
    let h_max = span.abs();
    let safety = 0.9;

    points.push((t, y.clone()));

    let max_steps = 100_000;
    let mut step_count = 0;

    while (t_end - t).abs() > tol && step_count < max_steps {
        step_count += 1;

        let remaining = t_end - t;
        if remaining.abs() < h.abs() {
            h = remaining;
        }

        let h_abs = h.abs();

        let pad = |v: Vec<f64>| -> Vec<f64> {
            if v.len() == n {
                v
            } else {
                vec![0.0; n]
            }
        };

        let k1 = pad(f(t, &y));
        let y2: Vec<f64> = (0..n).map(|i| y[i] + h * k1[i] / 4.0).collect();
        let k2 = pad(f(t + h / 4.0, &y2));
        let y3: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (3.0 * k1[i] / 32.0 + 9.0 * k2[i] / 32.0))
            .collect();
        let k3 = pad(f(t + 3.0 * h / 8.0, &y3));
        let y4: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (1932.0 * k1[i] / 2197.0 - 7200.0 * k2[i] / 2197.0 + 7296.0 * k3[i] / 2197.0)
            })
            .collect();
        let k4 = pad(f(t + 12.0 * h / 13.0, &y4));
        let y5: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (439.0 * k1[i] / 216.0 - 8.0 * k2[i] + 3680.0 * k3[i] / 513.0
                        - 845.0 * k4[i] / 4104.0)
            })
            .collect();
        let k5 = pad(f(t + h, &y5));
        let y6: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (-8.0 * k1[i] / 27.0 + 2.0 * k2[i] - 3544.0 * k3[i] / 2565.0
                        + 1859.0 * k4[i] / 4104.0
                        - 11.0 * k5[i] / 40.0)
            })
            .collect();
        let k6 = pad(f(t + h / 2.0, &y6));

        let mut y4_sol = vec![0.0; n];
        let mut y5_sol = vec![0.0; n];
        for i in 0..n {
            y4_sol[i] = y[i]
                + h * (25.0 * k1[i] / 216.0 + 1408.0 * k3[i] / 2565.0 + 2197.0 * k4[i] / 4104.0
                    - k5[i] / 5.0);
            y5_sol[i] = y[i]
                + h * (16.0 * k1[i] / 135.0 + 6656.0 * k3[i] / 12825.0 + 28561.0 * k4[i] / 56430.0
                    - 9.0 * k5[i] / 50.0
                    + 2.0 * k6[i] / 55.0);
        }

        let err = (0..n)
            .map(|i| (y5_sol[i] - y4_sol[i]).abs())
            .fold(0.0f64, f64::max);

        if err <= tol || h_abs <= h_min {
            t += h;
            y = y5_sol;
            points.push((t, y.clone()));
        }

        let new_h_abs = if err < 1e-15 {
            (h_abs * 4.0).min(h_max)
        } else {
            let factor = safety * (tol / err).powf(0.2);
            (h_abs * factor.clamp(0.1, 4.0)).min(h_max).max(h_min)
        };
        h = new_h_abs * direction;
    }

    points
}

/// Solve an ODE using the implicit Backward Euler method for stiff problems.
///
/// Backward Euler: y_{n+1} = y_n + h * f(t_{n+1}, y_{n+1})
/// The implicit equation is solved at each step via Newton iteration.
///
/// # Arguments
/// * `f` - The derivative function f(t, y) -> dy/dt
/// * `jac` - The Jacobian df/dy at (t, y)
/// * `t0` - Initial time
/// * `y0` - Initial value
/// * `t_end` - Final time
/// * `steps` - Number of integration steps
///
/// # Returns
/// Vector of (t, y) points representing the solution
pub fn backward_euler<F, G>(
    f: F,
    jac: G,
    t0: f64,
    y0: f64,
    t_end: f64,
    steps: usize,
) -> Vec<(f64, f64)>
where
    F: Fn(f64, f64) -> f64,
    G: Fn(f64, f64) -> f64,
{
    if steps == 0 {
        return vec![(t0, y0)];
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    let max_newton = 50;
    let newton_tol = 1e-12;

    for _ in 0..steps {
        let t_new = t + h;
        let mut y_new = y;
        for _ in 0..max_newton {
            let g = y_new - y - h * f(t_new, y_new);
            let dg = 1.0 - h * jac(t_new, y_new);
            if dg.abs() < 1e-15 {
                break;
            }
            let delta = g / dg;
            y_new -= delta;
            if delta.abs() < newton_tol {
                break;
            }
        }
        y = y_new;
        t = t_new;
        points.push((t, y));
    }

    points
}

/// Convert ODE solution to Point2 vector for plotting.
///
/// # Arguments
/// * `solution` - Vector of (t, y) points from ODE solver
///
/// # Returns
/// Vector of Point2 where x=t and y=solution
pub fn solution_to_points(solution: &[(f64, f64)]) -> Vec<Point2> {
    solution.iter().map(|(t, y)| Point2::new(*t, *y)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euler_exponential() {
        // dy/dt = y, y(0) = 1 => y(t) = e^t
        let f = |_t: f64, y: f64| y;
        let solution = euler(f, 0.0, 1.0, 1.0, 100);

        // Check final value is close to e^1 ≈ 2.718
        let (_, y_final) = solution.last().unwrap();
        assert!((y_final - std::f64::consts::E).abs() < 0.02);
    }

    #[test]
    fn test_rk4_exponential() {
        // dy/dt = y, y(0) = 1 => y(t) = e^t
        let f = |_t: f64, y: f64| y;
        let solution = runge_kutta_4(f, 0.0, 1.0, 1.0, 100);

        // RK4 should be more accurate than Euler
        let (_, y_final) = solution.last().unwrap();
        assert!((y_final - std::f64::consts::E).abs() < 0.0001);
    }

    #[test]
    fn test_euler_linear() {
        // dy/dt = 2, y(0) = 0 => y(t) = 2t
        let f = |_t: f64, _y: f64| 2.0;
        let solution = euler(f, 0.0, 0.0, 5.0, 50);

        let (t_final, y_final) = solution.last().unwrap();
        assert!((y_final - 2.0 * t_final).abs() < 0.01);
    }

    #[test]
    fn test_rk4_linear() {
        // dy/dt = 2, y(0) = 0 => y(t) = 2t
        let f = |_t: f64, _y: f64| 2.0;
        let solution = runge_kutta_4(f, 0.0, 0.0, 5.0, 50);

        let (t_final, y_final) = solution.last().unwrap();
        assert!((y_final - 2.0 * t_final).abs() < 0.0001);
    }

    #[test]
    fn test_euler_system() {
        // System: dx/dt = y, dy/dt = -x (simple harmonic oscillator)
        // x(0) = 1, y(0) = 0 => x(t) = cos(t), y(t) = -sin(t)
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];

        let solution = euler_system(f, 0.0, vec![1.0, 0.0], std::f64::consts::PI, 100);
        let (_, final_state) = solution.last().unwrap();

        // At t=π, x should be close to cos(π) = -1
        assert!((final_state[0] - (-1.0)).abs() < 0.1);
    }

    #[test]
    fn test_rk4_system() {
        // System: dx/dt = y, dy/dt = -x (simple harmonic oscillator)
        // x(0) = 1, y(0) = 0 => x(t) = cos(t), y(t) = -sin(t)
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];

        let solution = runge_kutta_4_system(f, 0.0, vec![1.0, 0.0], std::f64::consts::PI, 100);
        let (_, final_state) = solution.last().unwrap();

        // RK4 should be more accurate
        assert!((final_state[0] - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_solution_to_points() {
        let solution = vec![(0.0, 1.0), (1.0, 2.0), (2.0, 3.0)];
        let points = solution_to_points(&solution);

        assert_eq!(points.len(), 3);
        assert!((points[0].x - 0.0).abs() < 0.001);
        assert!((points[0].y - 1.0).abs() < 0.001);
        assert!((points[2].x - 2.0).abs() < 0.001);
        assert!((points[2].y - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_euler_zero_steps() {
        let f = |_t: f64, y: f64| y;
        let solution = euler(f, 0.0, 1.0, 1.0, 0);
        assert_eq!(solution.len(), 1);
        assert_eq!(solution[0], (0.0, 1.0));
    }

    #[test]
    fn test_rk4_zero_steps() {
        let f = |_t: f64, y: f64| y;
        let solution = runge_kutta_4(f, 0.0, 1.0, 1.0, 0);
        assert_eq!(solution.len(), 1);
        assert_eq!(solution[0], (0.0, 1.0));
    }

    #[test]
    fn test_euler_system_zero_steps() {
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];
        let solution = euler_system(f, 0.0, vec![1.0, 0.0], 1.0, 0);
        assert_eq!(solution.len(), 1);
        assert_eq!(solution[0].1, vec![1.0, 0.0]);
    }

    #[test]
    fn test_rk4_system_zero_steps() {
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];
        let solution = runge_kutta_4_system(f, 0.0, vec![1.0, 0.0], 1.0, 0);
        assert_eq!(solution.len(), 1);
        assert_eq!(solution[0].1, vec![1.0, 0.0]);
    }

    #[test]
    fn test_euler_system_wrong_length() {
        // f returns a vector of different length — should not panic
        let f = |_t: f64, _state: &[f64]| vec![1.0]; // returns 1, state has 2
        let solution = euler_system(f, 0.0, vec![1.0, 0.0], 1.0, 10);
        assert_eq!(solution.len(), 11);
        // Only first component should be updated
        assert!(solution[10].1[0].is_finite());
    }

    #[test]
    fn test_rk4_system_wrong_length() {
        // f returns a vector of different length — should not panic
        let f = |_t: f64, _state: &[f64]| vec![1.0]; // returns 1, state has 2
        let solution = runge_kutta_4_system(f, 0.0, vec![1.0, 0.0], 1.0, 10);
        assert_eq!(solution.len(), 11);
    }

    #[test]
    fn test_euler_negative_direction() {
        // t_end < t0 → h is negative, should still work
        let f = |_t: f64, y: f64| y;
        let solution = euler(f, 1.0, 1.0, 0.0, 10);
        assert_eq!(solution.len(), 11);
        assert!((solution[0].0 - 1.0).abs() < 1e-10);
        assert!((solution[10].0 - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_rk45_exponential() {
        // dy/dt = y, y(0) = 1 => y(t) = e^t
        let f = |_t: f64, y: f64| y;
        let solution = runge_kutta_45(f, 0.0, 1.0, 1.0, 1e-6);

        let (_, y_final) = solution.last().unwrap();
        assert!((y_final - std::f64::consts::E).abs() < 1e-4);
    }

    #[test]
    fn test_rk45_harmonic_oscillator() {
        // System: dx/dt = y, dy/dt = -x => x(t) = cos(t), y(t) = -sin(t)
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];
        let solution = runge_kutta_45_system(f, 0.0, &[1.0, 0.0], std::f64::consts::PI, 1e-6);

        let (_, final_state) = solution.last().unwrap();
        // At t=π, x ≈ cos(π) = -1, y ≈ -sin(π) = 0
        assert!((final_state[0] - (-1.0)).abs() < 1e-3);
        assert!(final_state[1].abs() < 1e-3);
    }

    #[test]
    fn test_backward_euler_stiff() {
        // dy/dt = -1000*y, y(0) = 1 => y(t) = exp(-1000*t)
        // Stiff problem: explicit Euler requires h < 0.002 for stability
        let f = |_t: f64, y: f64| -1000.0 * y;
        let jac = |_t: f64, _y: f64| -1000.0;
        let solution = backward_euler(f, jac, 0.0, 1.0, 0.01, 100);

        let (_, y_final) = solution.last().unwrap();
        // Backward Euler is stable even with large steps
        // Exact: exp(-10) ≈ 4.5e-5; BE ≈ (10/11)^100 ≈ 7.3e-5
        assert!(*y_final > 0.0);
        assert!(*y_final < 1e-3, "should be small, got {y_final}");
    }

    #[test]
    fn test_backward_euler_nonstiff() {
        // dy/dt = y, y(0) = 1 => y(t) = e^t
        let f = |_t: f64, y: f64| y;
        let jac = |_t: f64, _y: f64| 1.0;
        let solution = backward_euler(f, jac, 0.0, 1.0, 1.0, 1000);

        let (_, y_final) = solution.last().unwrap();
        // BE: (1/(1-h))^n with h=0.001, n=1000 ≈ e^1
        assert!((y_final - std::f64::consts::E).abs() < 0.01);
    }

    #[test]
    fn test_rk45_zero_span() {
        let f = |_t: f64, y: f64| y;
        let solution = runge_kutta_45(f, 1.0, 1.0, 1.0, 1e-6);
        assert_eq!(solution.len(), 1);
    }

    #[test]
    fn test_rk45_system_zero_span() {
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];
        let solution = runge_kutta_45_system(f, 1.0, &[1.0, 0.0], 1.0, 1e-6);
        assert_eq!(solution.len(), 1);
    }

    #[test]
    fn test_backward_euler_zero_steps() {
        let f = |_t: f64, y: f64| y;
        let jac = |_t: f64, _y: f64| 1.0;
        let solution = backward_euler(f, jac, 0.0, 1.0, 1.0, 0);
        assert_eq!(solution.len(), 1);
        assert_eq!(solution[0], (0.0, 1.0));
    }
}
