//! Ordinary Differential Equation (ODE) solvers.
//!
//! This module provides numerical methods for solving initial value problems
//! of the form dy/dt = f(t, y) with y(t0) = y0.

use crate::Point2;
use std::fmt;

/// Máximo de pasos aceptados por los solvers públicos de paso fijo.
pub const MAX_ODE_STEPS: usize = 100_000;
/// Máxima cantidad de componentes de estado de un sistema ODE.
pub const MAX_ODE_SYSTEM_DIMENSION: usize = 4_096;
/// Máxima cantidad de escalares retenidos por una trayectoria de sistema, incluido el tiempo.
pub const MAX_ODE_TRAJECTORY_SCALARS: usize = 1_048_576;

/// Error de los solvers de sistemas ODE de paso fijo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OdeSystemError {
    /// El tiempo, el estado inicial o el paso no es finito o representable.
    InvalidInput,
    /// El estado inicial supera la dimensión permitida.
    StateDimensionLimit { max_dimension: usize },
    /// La trayectoria completa superaría el límite de escalares retenidos.
    TrajectoryScalarLimit { max_scalars: usize },
    /// El sistema no pudo reservar memoria dentro de un presupuesto ya validado.
    AllocationFailed,
    /// Una derivada no tiene la misma dimensión que el estado.
    StageDimensionMismatch { expected: usize, actual: usize },
    /// Una derivada o un estado intermedio contiene un valor no finito.
    NonFiniteStage,
}

impl fmt::Display for OdeSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "el tiempo o el estado inicial no es finito"),
            Self::StateDimensionLimit { max_dimension } => {
                write!(
                    f,
                    "el sistema supera la dimensión máxima de {max_dimension}"
                )
            }
            Self::TrajectoryScalarLimit { max_scalars } => {
                write!(
                    f,
                    "la trayectoria supera el límite de {max_scalars} escalares"
                )
            }
            Self::AllocationFailed => write!(f, "no se pudo reservar memoria para la trayectoria"),
            Self::StageDimensionMismatch { expected, actual } => {
                write!(
                    f,
                    "una etapa devolvió dimensión {actual}; se esperaba {expected}"
                )
            }
            Self::NonFiniteStage => write!(f, "una etapa produjo un valor no finito"),
        }
    }
}

/// Failure that prevents an adaptive RKF45 solve from reaching its endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rkf45Error {
    /// The initial state, endpoint, or integration span is not finite.
    InvalidInput,
    /// A scalar RKF45 stage produced a non-finite value.
    NonFiniteStage,
    /// A system derivative stage had the wrong shape or a non-finite value.
    InvalidSystemStage,
    /// The step size became too small to advance the floating-point time value.
    StepSizeUnderflow,
    /// The solver reached its minimum representable step while its error still exceeded tolerance.
    ToleranceUnmet,
    /// The bounded solver budget was exhausted before reaching `t_end`.
    ResourceLimit { max_steps: usize },
    /// A system request exceeded its state, trajectory, or allocation budget.
    SystemResource(OdeSystemError),
}

impl fmt::Display for Rkf45Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "entrada no finita o intervalo no representable"),
            Self::NonFiniteStage => write!(f, "una etapa produjo un valor no finito"),
            Self::InvalidSystemStage => {
                write!(
                    f,
                    "una etapa del sistema tuvo dimensión o valores inválidos"
                )
            }
            Self::StepSizeUnderflow => write!(f, "el paso no puede avanzar el tiempo"),
            Self::ToleranceUnmet => write!(f, "no se pudo satisfacer la tolerancia solicitada"),
            Self::ResourceLimit { max_steps } => {
                write!(f, "se agotó el límite de {max_steps} pasos")
            }
            Self::SystemResource(error) => error.fmt(f),
        }
    }
}

/// Error de una integración implícita con Euler hacia atrás.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackwardEulerError {
    /// El tiempo, estado inicial o intervalo no es finito o representable.
    InvalidInput,
    /// La derivada, el Jacobiano o un iterado de Newton no es finito.
    NonFiniteStage { step: usize },
    /// El Jacobiano de la ecuación implícita es singular.
    SingularJacobian { step: usize },
    /// Newton agotó su presupuesto sin satisfacer la tolerancia.
    NotConverged { step: usize, max_iterations: usize },
}

impl fmt::Display for BackwardEulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "entrada no finita o intervalo no representable"),
            Self::NonFiniteStage { step } => {
                write!(f, "el paso {step} produjo un valor no finito")
            }
            Self::SingularJacobian { step } => {
                write!(f, "el Jacobiano del paso {step} es singular")
            }
            Self::NotConverged {
                step,
                max_iterations,
            } => write!(
                f,
                "Newton no convergió en el paso {step} tras {max_iterations} iteraciones"
            ),
        }
    }
}

fn bounded_steps(steps: usize) -> usize {
    steps.min(MAX_ODE_STEPS)
}

fn validate_system_dimension(dimension: usize) -> Result<(), OdeSystemError> {
    if dimension > MAX_ODE_SYSTEM_DIMENSION {
        return Err(OdeSystemError::StateDimensionLimit {
            max_dimension: MAX_ODE_SYSTEM_DIMENSION,
        });
    }
    Ok(())
}

fn validate_system_trajectory(dimension: usize, steps: usize) -> Result<(), OdeSystemError> {
    let retained_points = steps
        .checked_add(1)
        .ok_or(OdeSystemError::TrajectoryScalarLimit {
            max_scalars: MAX_ODE_TRAJECTORY_SCALARS,
        })?;
    let scalars_per_point =
        dimension
            .checked_add(1)
            .ok_or(OdeSystemError::TrajectoryScalarLimit {
                max_scalars: MAX_ODE_TRAJECTORY_SCALARS,
            })?;
    let retained_scalars = retained_points.checked_mul(scalars_per_point).ok_or(
        OdeSystemError::TrajectoryScalarLimit {
            max_scalars: MAX_ODE_TRAJECTORY_SCALARS,
        },
    )?;
    if retained_scalars > MAX_ODE_TRAJECTORY_SCALARS {
        return Err(OdeSystemError::TrajectoryScalarLimit {
            max_scalars: MAX_ODE_TRAJECTORY_SCALARS,
        });
    }
    Ok(())
}

fn validate_system_request(dimension: usize, steps: usize) -> Result<(), OdeSystemError> {
    validate_system_dimension(dimension)?;
    validate_system_trajectory(dimension, steps)
}

fn validate_system_stage(stage: Vec<f64>, expected: usize) -> Result<Vec<f64>, OdeSystemError> {
    if stage.len() != expected {
        return Err(OdeSystemError::StageDimensionMismatch {
            expected,
            actual: stage.len(),
        });
    }
    validate_finite_system_stage(&stage)?;
    Ok(stage)
}

fn validate_finite_system_stage(stage: &[f64]) -> Result<(), OdeSystemError> {
    if stage.iter().any(|value| !value.is_finite()) {
        return Err(OdeSystemError::NonFiniteStage);
    }
    Ok(())
}

fn reserve_system_trajectory(points: usize) -> Result<Vec<(f64, Vec<f64>)>, OdeSystemError> {
    let mut trajectory = Vec::new();
    trajectory
        .try_reserve_exact(points)
        .map_err(|_| OdeSystemError::AllocationFailed)?;
    Ok(trajectory)
}

fn copy_system_state(state: &[f64]) -> Result<Vec<f64>, OdeSystemError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(state.len())
        .map_err(|_| OdeSystemError::AllocationFailed)?;
    copy.extend_from_slice(state);
    Ok(copy)
}

fn zero_system_state(dimension: usize) -> Result<Vec<f64>, OdeSystemError> {
    let mut state = Vec::new();
    state
        .try_reserve_exact(dimension)
        .map_err(|_| OdeSystemError::AllocationFailed)?;
    state.resize(dimension, 0.0);
    Ok(state)
}

fn push_system_point(
    trajectory: &mut Vec<(f64, Vec<f64>)>,
    time: f64,
    state: &[f64],
) -> Result<(), OdeSystemError> {
    if trajectory.len() == trajectory.capacity() {
        trajectory
            .try_reserve(1)
            .map_err(|_| OdeSystemError::AllocationFailed)?;
    }
    trajectory.push((time, copy_system_state(state)?));
    Ok(())
}

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
    let steps = bounded_steps(steps);
    if steps == 0 || t0 == t_end {
        return vec![(t0, y0)];
    }
    if !t0.is_finite() || !y0.is_finite() || !t_end.is_finite() {
        return Vec::new();
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    if !h.is_finite() || h == 0.0 || t0 + h == t0 {
        return Vec::new();
    }
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    for step in 0..steps {
        let dydt = f(t, y);
        if !dydt.is_finite() {
            return Vec::new();
        }
        let next_y = y + h * dydt;
        let next_t = if step + 1 == steps { t_end } else { t + h };
        if !next_y.is_finite() || !next_t.is_finite() || next_t == t {
            return Vec::new();
        }
        y = next_y;
        t = next_t;
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
    let steps = bounded_steps(steps);
    if steps == 0 || t0 == t_end {
        return vec![(t0, y0)];
    }
    if !t0.is_finite() || !y0.is_finite() || !t_end.is_finite() {
        return Vec::new();
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    if !h.is_finite() || h == 0.0 || t0 + h == t0 {
        return Vec::new();
    }
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    for step in 0..steps {
        let k1 = h * f(t, y);
        let k2 = h * f(t + h / 2.0, y + k1 / 2.0);
        let k3 = h * f(t + h / 2.0, y + k2 / 2.0);
        let k4 = h * f(t + h, y + k3);

        let next_y = y + (k1 + 2.0 * k2 + 2.0 * k3 + k4) / 6.0;
        let next_t = if step + 1 == steps { t_end } else { t + h };
        if ![k1, k2, k3, k4, next_y, next_t]
            .iter()
            .all(|value| value.is_finite())
            || next_t == t
        {
            return Vec::new();
        }
        y = next_y;
        t = next_t;
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
/// Compatibility wrapper for [`try_euler_system`].
///
/// Returns an empty vector if the checked solve fails. New callers should use
/// [`try_euler_system`] to receive the structured error.
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
    try_euler_system(f, t0, y0, t_end, steps).unwrap_or_default()
}

/// Solve a system of ODEs using Euler's method with structured errors.
pub fn try_euler_system<F>(
    f: F,
    t0: f64,
    y0: Vec<f64>,
    t_end: f64,
    steps: usize,
) -> Result<Vec<(f64, Vec<f64>)>, OdeSystemError>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    let steps = bounded_steps(steps);
    let steps = if t0 == t_end { 0 } else { steps };
    validate_system_request(y0.len(), steps)?;
    if !t0.is_finite() || !t_end.is_finite() || y0.iter().any(|value| !value.is_finite()) {
        return Err(OdeSystemError::InvalidInput);
    }
    let mut points = reserve_system_trajectory(steps + 1)?;
    if steps == 0 {
        points.push((t0, y0));
        return Ok(points);
    }
    let h = (t_end - t0) / steps as f64;
    if !h.is_finite() || h == 0.0 {
        return Err(OdeSystemError::InvalidInput);
    }
    let mut t = t0;
    let mut y = y0;

    push_system_point(&mut points, t, &y)?;

    for _ in 0..steps {
        let dydt = validate_system_stage(f(t, &y), y.len())?;
        for i in 0..y.len() {
            y[i] += h * dydt[i];
        }
        validate_finite_system_stage(&y)?;
        let next_t = t + h;
        if !next_t.is_finite() || next_t == t {
            return Err(OdeSystemError::InvalidInput);
        }
        t = next_t;
        push_system_point(&mut points, t, &y)?;
    }

    Ok(points)
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
/// Compatibility wrapper for [`try_runge_kutta_4_system`].
///
/// Returns an empty vector if the checked solve fails. New callers should use
/// [`try_runge_kutta_4_system`] to receive the structured error.
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
    try_runge_kutta_4_system(f, t0, y0, t_end, steps).unwrap_or_default()
}

/// Solve a system of ODEs using RK4 with structured errors.
pub fn try_runge_kutta_4_system<F>(
    f: F,
    t0: f64,
    y0: Vec<f64>,
    t_end: f64,
    steps: usize,
) -> Result<Vec<(f64, Vec<f64>)>, OdeSystemError>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    let steps = bounded_steps(steps);
    let steps = if t0 == t_end { 0 } else { steps };
    validate_system_request(y0.len(), steps)?;
    if !t0.is_finite() || !t_end.is_finite() || y0.iter().any(|value| !value.is_finite()) {
        return Err(OdeSystemError::InvalidInput);
    }
    let mut points = reserve_system_trajectory(steps + 1)?;
    if steps == 0 {
        points.push((t0, y0));
        return Ok(points);
    }
    let h = (t_end - t0) / steps as f64;
    if !h.is_finite() || h == 0.0 {
        return Err(OdeSystemError::InvalidInput);
    }
    let mut t = t0;
    let mut y = y0;
    let n = y.len();

    push_system_point(&mut points, t, &y)?;

    let mut y_temp = zero_system_state(n)?;
    for _ in 0..steps {
        let k1 = validate_system_stage(f(t, &y), n)?;

        for i in 0..n {
            y_temp[i] = y[i] + h / 2.0 * k1[i];
        }
        validate_finite_system_stage(&y_temp)?;
        let k2 = validate_system_stage(f(t + h / 2.0, &y_temp), n)?;

        for i in 0..n {
            y_temp[i] = y[i] + h / 2.0 * k2[i];
        }
        validate_finite_system_stage(&y_temp)?;
        let k3 = validate_system_stage(f(t + h / 2.0, &y_temp), n)?;

        for i in 0..n {
            y_temp[i] = y[i] + h * k3[i];
        }
        validate_finite_system_stage(&y_temp)?;
        let k4 = validate_system_stage(f(t + h, &y_temp), n)?;

        for i in 0..n {
            y_temp[i] = y[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
        }
        validate_finite_system_stage(&y_temp)?;
        y.copy_from_slice(&y_temp);
        let next_t = t + h;
        if !next_t.is_finite() || next_t == t {
            return Err(OdeSystemError::InvalidInput);
        }
        t = next_t;
        push_system_point(&mut points, t, &y)?;
    }

    Ok(points)
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
/// Compatibility wrapper for [`try_runge_kutta_45`].
///
/// Returns an empty vector if the checked solve fails. New callers that need
/// the failure reason should use [`try_runge_kutta_45`].
pub fn runge_kutta_45<F>(f: F, t0: f64, y0: f64, t_end: f64, tol: f64) -> Vec<(f64, f64)>
where
    F: Fn(f64, f64) -> f64,
{
    try_runge_kutta_45(f, t0, y0, t_end, tol).unwrap_or_default()
}

/// Resuelve una ODE escalar con RKF45 y expone los fallos de integración.
pub fn try_runge_kutta_45<F>(
    f: F,
    t0: f64,
    y0: f64,
    t_end: f64,
    tol: f64,
) -> Result<Vec<(f64, f64)>, Rkf45Error>
where
    F: Fn(f64, f64) -> f64,
{
    if !t0.is_finite() || !y0.is_finite() || !t_end.is_finite() || !tol.is_finite() || tol <= 0.0 {
        return Err(Rkf45Error::InvalidInput);
    }
    if t0 == t_end {
        return Ok(vec![(t0, y0)]);
    }

    let span = t_end - t0;
    if !span.is_finite() {
        return Err(Rkf45Error::InvalidInput);
    }
    let mut points = Vec::new();
    let mut t = t0;
    let mut y = y0;

    let direction = span.signum();
    let mut h = span.abs() / 10.0 * direction;
    let h_min = span.abs() * 1e-10;
    let h_max = span.abs();
    let safety = 0.9;

    if h == 0.0 || !h.is_finite() {
        return Err(Rkf45Error::StepSizeUnderflow);
    }

    points.push((t, y));

    let max_steps = MAX_ODE_STEPS;
    let mut step_count = 0;

    while (t_end - t) * direction > 0.0 && step_count < max_steps {
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

        if [k1, k2, k3, k4, k5, k6, y4, y5, err]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(Rkf45Error::NonFiniteStage);
        }

        if err <= tol {
            let next_t = if remaining.abs() <= h_abs {
                t_end
            } else {
                t + h
            };
            if next_t == t {
                return Err(Rkf45Error::StepSizeUnderflow);
            }
            t = next_t;
            y = y5;
            points.push((t, y));
        } else if h_abs <= h_min {
            return Err(Rkf45Error::ToleranceUnmet);
        }

        let new_h_abs = if err < 1e-15 {
            (h_abs * 4.0).min(h_max)
        } else {
            let factor = safety * (tol / err).powf(0.2);
            (h_abs * factor.clamp(0.1, 4.0)).min(h_max).max(h_min)
        };
        if new_h_abs == 0.0 || !new_h_abs.is_finite() {
            return Err(Rkf45Error::StepSizeUnderflow);
        }
        h = new_h_abs * direction;
    }

    if t == t_end {
        Ok(points)
    } else {
        Err(Rkf45Error::ResourceLimit {
            max_steps: MAX_ODE_STEPS,
        })
    }
}

/// Solve a system of ODEs using the adaptive Runge-Kutta-Fehlberg (RKF45) method.
///
/// # Arguments
/// * `f` - Derivative function `f(t, &[y]) -> Vec<dy/dt>`
/// * `t0` - Initial time
/// * `y0` - Initial values slice
/// * `t_end` - Final time
/// * `tol` - Desired tolerance for error control
///
/// Compatibility wrapper for [`try_runge_kutta_45_system`].
///
/// Returns an empty vector if the checked solve fails. New callers that need
/// the failure reason should use [`try_runge_kutta_45_system`].
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
    try_runge_kutta_45_system(f, t0, y0, t_end, tol).unwrap_or_default()
}

/// Resuelve un sistema ODE con RKF45 y expone los fallos de integración.
pub fn try_runge_kutta_45_system<F>(
    f: F,
    t0: f64,
    y0: &[f64],
    t_end: f64,
    tol: f64,
) -> Result<Vec<(f64, Vec<f64>)>, Rkf45Error>
where
    F: Fn(f64, &[f64]) -> Vec<f64>,
{
    let n = y0.len();
    validate_system_dimension(n).map_err(Rkf45Error::SystemResource)?;
    if !t0.is_finite()
        || !t_end.is_finite()
        || y0.iter().any(|value| !value.is_finite())
        || !tol.is_finite()
        || tol <= 0.0
    {
        return Err(Rkf45Error::InvalidInput);
    }
    if t0 == t_end {
        validate_system_trajectory(n, 0).map_err(Rkf45Error::SystemResource)?;
        let mut points = reserve_system_trajectory(1).map_err(Rkf45Error::SystemResource)?;
        points.push((
            t0,
            copy_system_state(y0).map_err(Rkf45Error::SystemResource)?,
        ));
        return Ok(points);
    }
    if n == 0 {
        return Err(Rkf45Error::InvalidInput);
    }
    validate_system_trajectory(n, MAX_ODE_STEPS).map_err(Rkf45Error::SystemResource)?;

    let span = t_end - t0;
    if !span.is_finite() {
        return Err(Rkf45Error::InvalidInput);
    }
    let mut points = Vec::new();
    let mut t = t0;
    let mut y = copy_system_state(y0).map_err(Rkf45Error::SystemResource)?;

    let direction = span.signum();
    let mut h = span.abs() / 10.0 * direction;
    let h_min = span.abs() * 1e-10;
    let h_max = span.abs();
    let safety = 0.9;

    if h == 0.0 || !h.is_finite() {
        return Err(Rkf45Error::StepSizeUnderflow);
    }

    push_system_point(&mut points, t, &y).map_err(Rkf45Error::SystemResource)?;

    let max_steps = MAX_ODE_STEPS;
    let mut step_count = 0;

    while (t_end - t) * direction > 0.0 && step_count < max_steps {
        step_count += 1;

        let remaining = t_end - t;
        if remaining.abs() < h.abs() {
            h = remaining;
        }

        let h_abs = h.abs();

        let stages = |v: Vec<f64>| -> Option<Vec<f64>> {
            if v.len() == n && v.iter().all(|value| value.is_finite()) {
                Some(v)
            } else {
                None
            }
        };

        let Some(k1) = stages(f(t, &y)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };
        let y2: Vec<f64> = (0..n).map(|i| y[i] + h * k1[i] / 4.0).collect();
        if y2.iter().any(|value| !value.is_finite()) {
            return Err(Rkf45Error::NonFiniteStage);
        }
        let Some(k2) = stages(f(t + h / 4.0, &y2)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };
        let y3: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (3.0 * k1[i] / 32.0 + 9.0 * k2[i] / 32.0))
            .collect();
        if y3.iter().any(|value| !value.is_finite()) {
            return Err(Rkf45Error::NonFiniteStage);
        }
        let Some(k3) = stages(f(t + 3.0 * h / 8.0, &y3)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };
        let y4: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (1932.0 * k1[i] / 2197.0 - 7200.0 * k2[i] / 2197.0 + 7296.0 * k3[i] / 2197.0)
            })
            .collect();
        if y4.iter().any(|value| !value.is_finite()) {
            return Err(Rkf45Error::NonFiniteStage);
        }
        let Some(k4) = stages(f(t + 12.0 * h / 13.0, &y4)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };
        let y5: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (439.0 * k1[i] / 216.0 - 8.0 * k2[i] + 3680.0 * k3[i] / 513.0
                        - 845.0 * k4[i] / 4104.0)
            })
            .collect();
        if y5.iter().any(|value| !value.is_finite()) {
            return Err(Rkf45Error::NonFiniteStage);
        }
        let Some(k5) = stages(f(t + h, &y5)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };
        let y6: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h
                    * (-8.0 * k1[i] / 27.0 + 2.0 * k2[i] - 3544.0 * k3[i] / 2565.0
                        + 1859.0 * k4[i] / 4104.0
                        - 11.0 * k5[i] / 40.0)
            })
            .collect();
        if y6.iter().any(|value| !value.is_finite()) {
            return Err(Rkf45Error::NonFiniteStage);
        }
        let Some(k6) = stages(f(t + h / 2.0, &y6)) else {
            return Err(Rkf45Error::InvalidSystemStage);
        };

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

        if y4_sol.iter().any(|value| !value.is_finite())
            || y5_sol.iter().any(|value| !value.is_finite())
            || !err.is_finite()
        {
            return Err(Rkf45Error::NonFiniteStage);
        }

        if err <= tol {
            let next_t = if remaining.abs() <= h_abs {
                t_end
            } else {
                t + h
            };
            if next_t == t {
                return Err(Rkf45Error::StepSizeUnderflow);
            }
            t = next_t;
            y = y5_sol;
            push_system_point(&mut points, t, &y).map_err(Rkf45Error::SystemResource)?;
        } else if h_abs <= h_min {
            return Err(Rkf45Error::ToleranceUnmet);
        }

        let new_h_abs = if err < 1e-15 {
            (h_abs * 4.0).min(h_max)
        } else {
            let factor = safety * (tol / err).powf(0.2);
            (h_abs * factor.clamp(0.1, 4.0)).min(h_max).max(h_min)
        };
        if new_h_abs == 0.0 || !new_h_abs.is_finite() {
            return Err(Rkf45Error::StepSizeUnderflow);
        }
        h = new_h_abs * direction;
    }

    if t == t_end {
        Ok(points)
    } else {
        Err(Rkf45Error::ResourceLimit {
            max_steps: MAX_ODE_STEPS,
        })
    }
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
/// Compatibility wrapper for [`try_backward_euler`].
///
/// Returns an empty vector if Newton fails or a non-finite value is produced.
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
    try_backward_euler(f, jac, t0, y0, t_end, steps).unwrap_or_default()
}

/// Resuelve una ODE con Euler hacia atrás y expone los fallos de Newton.
pub fn try_backward_euler<F, G>(
    f: F,
    jac: G,
    t0: f64,
    y0: f64,
    t_end: f64,
    steps: usize,
) -> Result<Vec<(f64, f64)>, BackwardEulerError>
where
    F: Fn(f64, f64) -> f64,
    G: Fn(f64, f64) -> f64,
{
    let steps = bounded_steps(steps);
    if !t0.is_finite() || !y0.is_finite() || !t_end.is_finite() {
        return Err(BackwardEulerError::InvalidInput);
    }
    if steps == 0 || t0 == t_end {
        return Ok(vec![(t0, y0)]);
    }
    let mut points = Vec::with_capacity(steps + 1);
    let h = (t_end - t0) / steps as f64;
    if !h.is_finite() || h == 0.0 {
        return Err(BackwardEulerError::InvalidInput);
    }
    let mut t = t0;
    let mut y = y0;

    points.push((t, y));

    const MAX_NEWTON: usize = 50;
    let newton_tol = 1e-12;

    for step_index in 0..steps {
        let step = step_index + 1;
        let t_new = t + h;
        if !t_new.is_finite() || t_new == t {
            return Err(BackwardEulerError::InvalidInput);
        }
        let mut y_new = y;
        let mut converged = false;
        for _ in 0..MAX_NEWTON {
            let derivative = f(t_new, y_new);
            let g = y_new - y - h * derivative;
            if !g.is_finite() || !derivative.is_finite() {
                return Err(BackwardEulerError::NonFiniteStage { step });
            }
            if g.abs() < newton_tol {
                converged = true;
                break;
            }
            let jacobian = jac(t_new, y_new);
            let dg = 1.0 - h * jacobian;
            if !jacobian.is_finite() || !dg.is_finite() {
                return Err(BackwardEulerError::NonFiniteStage { step });
            }
            if dg.abs() < 1e-15 {
                return Err(BackwardEulerError::SingularJacobian { step });
            }
            let delta = g / dg;
            if !delta.is_finite() {
                return Err(BackwardEulerError::NonFiniteStage { step });
            }
            y_new -= delta;
            if !y_new.is_finite() {
                return Err(BackwardEulerError::NonFiniteStage { step });
            }
            if delta.abs() < newton_tol {
                let derivative = f(t_new, y_new);
                let residual = y_new - y - h * derivative;
                if !derivative.is_finite() || !residual.is_finite() {
                    return Err(BackwardEulerError::NonFiniteStage { step });
                }
                if residual.abs() < newton_tol {
                    converged = true;
                    break;
                }
            }
        }
        if !converged {
            let derivative = f(t_new, y_new);
            let residual = y_new - y - h * derivative;
            if !derivative.is_finite() || !residual.is_finite() {
                return Err(BackwardEulerError::NonFiniteStage { step });
            }
            if residual.abs() >= newton_tol {
                return Err(BackwardEulerError::NotConverged {
                    step,
                    max_iterations: MAX_NEWTON,
                });
            }
        }
        y = y_new;
        t = t_new;
        points.push((t, y));
    }

    Ok(points)
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
    fn fixed_step_solvers_cap_untrusted_public_step_counts() {
        let solution = euler(|_t, _y| 0.0, 0.0, 1.0, 1.0, usize::MAX);
        assert_eq!(solution.len(), 100_001);
        let (t, y) = solution
            .last()
            .copied()
            .expect("the initial point is retained");
        assert!((t - 1.0).abs() < 1e-11);
        assert_eq!(y, 1.0);
    }

    #[test]
    fn scalar_fixed_step_solvers_reject_nonprogressing_time() {
        for solution in [
            euler(
                |_t, _y| 1.0,
                10_000_000_000_000_000.0,
                0.0,
                10_000_000_000_000_002.0,
                2,
            ),
            runge_kutta_4(
                |_t, _y| 1.0,
                10_000_000_000_000_000.0,
                0.0,
                10_000_000_000_000_002.0,
                2,
            ),
        ] {
            assert!(solution.is_empty());
        }
    }

    #[test]
    fn system_solvers_reject_states_above_the_dimension_limit() {
        let state = vec![0.0; MAX_ODE_SYSTEM_DIMENSION + 1];
        let expected = OdeSystemError::StateDimensionLimit {
            max_dimension: MAX_ODE_SYSTEM_DIMENSION,
        };

        assert_eq!(
            try_euler_system(|_t, values| values.to_vec(), 0.0, state.clone(), 1.0, 0),
            Err(expected)
        );
        assert_eq!(
            try_runge_kutta_4_system(|_t, values| values.to_vec(), 0.0, state.clone(), 1.0, 0),
            Err(expected)
        );
        assert_eq!(
            try_runge_kutta_45_system(|_t, values| values.to_vec(), 0.0, &state, 1.0, 1e-6),
            Err(Rkf45Error::SystemResource(expected))
        );
    }

    #[test]
    fn system_solvers_reject_trajectories_above_the_scalar_limit() {
        let state = vec![0.0; 10];
        let expected = OdeSystemError::TrajectoryScalarLimit {
            max_scalars: MAX_ODE_TRAJECTORY_SCALARS,
        };

        assert_eq!(
            try_euler_system(
                |_t, values| values.to_vec(),
                0.0,
                state.clone(),
                1.0,
                usize::MAX,
            ),
            Err(expected)
        );
        assert_eq!(
            try_runge_kutta_4_system(
                |_t, values| values.to_vec(),
                0.0,
                state.clone(),
                1.0,
                usize::MAX,
            ),
            Err(expected)
        );
        assert_eq!(
            try_runge_kutta_45_system(|_t, values| values.to_vec(), 0.0, &state, 1.0, 1e-6),
            Err(Rkf45Error::SystemResource(expected))
        );
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
    fn legacy_euler_system_rejects_wrong_length_without_a_partial_trajectory() {
        let f = |_t: f64, _state: &[f64]| vec![1.0]; // returns 1, state has 2
        let solution = euler_system(f, 0.0, vec![1.0, 0.0], 1.0, 10);
        assert!(solution.is_empty());
    }

    #[test]
    fn legacy_rk4_system_rejects_wrong_length_without_a_partial_trajectory() {
        let f = |_t: f64, _state: &[f64]| vec![1.0]; // returns 1, state has 2
        let solution = runge_kutta_4_system(f, 0.0, vec![1.0, 0.0], 1.0, 10);
        assert!(solution.is_empty());
    }

    #[test]
    fn checked_euler_system_rejects_invalid_derivative_vectors() {
        for derivative in [vec![1.0], vec![1.0, 2.0, 3.0]] {
            let actual = derivative.len();
            let result =
                try_euler_system(|_t, _state| derivative.clone(), 0.0, vec![1.0, 0.0], 1.0, 1);
            assert_eq!(
                result,
                Err(OdeSystemError::StageDimensionMismatch {
                    expected: 2,
                    actual,
                })
            );
        }

        let result = try_euler_system(
            |_t, _state| vec![0.0, f64::NAN],
            0.0,
            vec![1.0, 0.0],
            1.0,
            1,
        );
        assert_eq!(result, Err(OdeSystemError::NonFiniteStage));
    }

    #[test]
    fn checked_rk4_system_rejects_invalid_vectors_at_every_stage() {
        use std::cell::Cell;

        for invalid_stage in 1..=4 {
            let calls = Cell::new(0);
            let result = try_runge_kutta_4_system(
                |_t, _state| {
                    let stage = calls.get() + 1;
                    calls.set(stage);
                    if stage == invalid_stage {
                        vec![0.0]
                    } else {
                        vec![0.0, 0.0]
                    }
                },
                0.0,
                vec![1.0, 0.0],
                1.0,
                1,
            );
            assert_eq!(
                result,
                Err(OdeSystemError::StageDimensionMismatch {
                    expected: 2,
                    actual: 1,
                }),
                "wrong-length stage {invalid_stage}"
            );
        }

        for invalid_stage in 1..=4 {
            let calls = Cell::new(0);
            let result = try_runge_kutta_4_system(
                |_t, _state| {
                    let stage = calls.get() + 1;
                    calls.set(stage);
                    if stage == invalid_stage {
                        vec![0.0, f64::INFINITY]
                    } else {
                        vec![0.0, 0.0]
                    }
                },
                0.0,
                vec![1.0, 0.0],
                1.0,
                1,
            );
            assert_eq!(
                result,
                Err(OdeSystemError::NonFiniteStage),
                "non-finite stage {invalid_stage}"
            );
        }
    }

    #[test]
    fn checked_fixed_system_zero_span_returns_only_the_initial_state() {
        let initial = vec![1.0, 0.0];
        let euler_solution = try_euler_system(
            |_t, _state| panic!("zero-span Euler must not evaluate the derivative"),
            1.0,
            initial.clone(),
            1.0,
            10,
        )
        .expect("zero-span Euler should be complete");
        let rk4_solution = try_runge_kutta_4_system(
            |_t, _state| panic!("zero-span RK4 must not evaluate the derivative"),
            1.0,
            initial.clone(),
            1.0,
            10,
        )
        .expect("zero-span RK4 should be complete");

        assert_eq!(euler_solution, vec![(1.0, initial.clone())]);
        assert_eq!(rk4_solution, vec![(1.0, initial)]);
    }

    #[test]
    fn checked_fixed_system_accepts_an_empty_state_with_empty_derivatives() {
        let euler_solution = try_euler_system(|_t, _state| Vec::new(), 0.0, Vec::new(), 1.0, 1)
            .expect("the empty Euler system is valid");
        let rk4_solution =
            try_runge_kutta_4_system(|_t, _state| Vec::new(), 0.0, Vec::new(), 1.0, 1)
                .expect("the empty RK4 system is valid");

        assert_eq!(euler_solution, vec![(0.0, vec![]), (1.0, vec![])]);
        assert_eq!(rk4_solution, vec![(0.0, vec![]), (1.0, vec![])]);
    }

    #[test]
    fn checked_fixed_system_rejects_nonfinite_intermediate_states_and_steps() {
        let euler_overflow =
            try_euler_system(|_t, _state| vec![f64::MAX], 0.0, vec![f64::MAX], 1.0, 1);
        assert_eq!(euler_overflow, Err(OdeSystemError::NonFiniteStage));

        let rk4_overflow =
            try_runge_kutta_4_system(|_t, _state| vec![f64::MAX], 0.0, vec![f64::MAX], 1.0, 1);
        assert_eq!(rk4_overflow, Err(OdeSystemError::NonFiniteStage));

        let euler_step =
            try_euler_system(|_t, _state| Vec::new(), -f64::MAX, Vec::new(), f64::MAX, 1);
        assert_eq!(euler_step, Err(OdeSystemError::InvalidInput));

        let rk4_step =
            try_runge_kutta_4_system(|_t, _state| Vec::new(), -f64::MAX, Vec::new(), f64::MAX, 1);
        assert_eq!(rk4_step, Err(OdeSystemError::InvalidInput));
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
    fn rk45_legacy_and_checked_apis_have_additive_types() {
        let legacy_scalar: Vec<(f64, f64)> = runge_kutta_45(|_t, y| y, 0.0, 1.0, 1.0, 1e-6);
        let checked_scalar: Result<Vec<(f64, f64)>, Rkf45Error> =
            try_runge_kutta_45(|_t, y| y, 0.0, 1.0, 1.0, 1e-6);

        let legacy_system: Vec<(f64, Vec<f64>)> = runge_kutta_45_system(
            |_t, state| vec![state[1], -state[0]],
            0.0,
            &[1.0, 0.0],
            1.0,
            1e-6,
        );
        let checked_system: Result<Vec<(f64, Vec<f64>)>, Rkf45Error> = try_runge_kutta_45_system(
            |_t, state| vec![state[1], -state[0]],
            0.0,
            &[1.0, 0.0],
            1.0,
            1e-6,
        );

        assert_eq!(
            legacy_scalar,
            checked_scalar.expect("checked scalar RKF45 should finish")
        );
        assert_eq!(
            legacy_system,
            checked_system.expect("checked system RKF45 should finish")
        );
    }

    #[test]
    fn rk45_rejects_invalid_tolerances_without_substituting_a_default() {
        assert_eq!(
            try_runge_kutta_45(|_t, _y| 1.0, 0.0, 0.0, 1.0, 0.0),
            Err(Rkf45Error::InvalidInput)
        );
        assert_eq!(
            try_runge_kutta_45_system(|_t, _state| vec![1.0], 0.0, &[0.0], 1.0, f64::NAN),
            Err(Rkf45Error::InvalidInput)
        );
    }

    #[test]
    fn rk45_never_accepts_a_step_above_the_requested_tolerance() {
        let sharp_transition = |t: f64| ((t - 0.5) / 1e-12).tanh();
        assert_eq!(
            try_runge_kutta_45(|t, _y| sharp_transition(t), 0.0, 0.0, 1.0, 1e-14),
            Err(Rkf45Error::ToleranceUnmet)
        );
        assert_eq!(
            try_runge_kutta_45_system(
                |t, _state| vec![sharp_transition(t)],
                0.0,
                &[0.0],
                1.0,
                1e-14,
            ),
            Err(Rkf45Error::ToleranceUnmet)
        );
    }

    #[test]
    fn rk45_legacy_wrappers_fail_safely_without_partial_trajectories() {
        let scalar = runge_kutta_45(|_t, _y| f64::NAN, 0.0, 0.0, 1.0, 1e-6);
        assert!(scalar.is_empty());

        let system = runge_kutta_45_system(|_t, _state| vec![0.0], 0.0, &[0.0, 0.0], 1.0, 1e-6);
        assert!(system.is_empty());
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
    fn rk45_system_rejects_invalid_vectors_at_every_stage() {
        use std::cell::Cell;

        for invalid_stage in 1..=6 {
            let calls = Cell::new(0);
            let result = try_runge_kutta_45_system(
                |_t, _state| {
                    let stage = calls.get() + 1;
                    calls.set(stage);
                    if stage == invalid_stage {
                        vec![0.0]
                    } else {
                        vec![0.0, 0.0]
                    }
                },
                0.0,
                &[0.0, 0.0],
                1.0,
                1e-6,
            );
            assert_eq!(result, Err(Rkf45Error::InvalidSystemStage));
        }

        for invalid_stage in 1..=6 {
            let calls = Cell::new(0);
            let result = try_runge_kutta_45_system(
                |_t, _state| {
                    let stage = calls.get() + 1;
                    calls.set(stage);
                    if stage == invalid_stage {
                        vec![0.0, f64::NAN]
                    } else {
                        vec![0.0, 0.0]
                    }
                },
                0.0,
                &[0.0, 0.0],
                1.0,
                1e-6,
            );
            assert_eq!(result, Err(Rkf45Error::InvalidSystemStage));
        }
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
    fn rk45_reaches_the_requested_endpoint_even_with_large_tolerance() {
        let solution = runge_kutta_45(|_t, _y| 1.0, 0.0, 0.0, 1.0, 2.0);
        let (t, y) = solution.last().copied().unwrap();
        assert!((t - 1.0).abs() < 1e-12, "ended at {t}");
        assert!((y - 1.0).abs() < 1e-12, "got {y}");
    }

    #[test]
    fn legacy_backward_euler_rejects_newton_failure_without_a_partial_trajectory() {
        let checked = try_backward_euler(|_t, _y| 1.0, |_t, _y| 10.0, 0.0, 0.0, 0.1, 1);
        assert_eq!(
            checked,
            Err(BackwardEulerError::SingularJacobian { step: 1 })
        );

        let solution = backward_euler(|_t, _y| 1.0, |_t, _y| 10.0, 0.0, 0.0, 0.1, 1);
        assert!(solution.is_empty());
    }

    #[test]
    fn checked_backward_euler_reports_non_convergence() {
        let result = try_backward_euler(|_t, y| 1.0 - y, |_t, _y| 0.0, 0.0, 0.0, 1.0, 1);

        assert_eq!(
            result,
            Err(BackwardEulerError::NotConverged {
                step: 1,
                max_iterations: 50,
            })
        );
    }

    #[test]
    fn checked_backward_euler_reports_nonfinite_derivative_and_jacobian_stages() {
        let derivative = try_backward_euler(|_t, _y| f64::NAN, |_t, _y| 0.0, 0.0, 0.0, 1.0, 1);
        assert_eq!(
            derivative,
            Err(BackwardEulerError::NonFiniteStage { step: 1 })
        );

        let jacobian = try_backward_euler(|_t, _y| 1.0, |_t, _y| f64::INFINITY, 0.0, 0.0, 1.0, 1);
        assert_eq!(
            jacobian,
            Err(BackwardEulerError::NonFiniteStage { step: 1 })
        );
    }

    #[test]
    fn checked_backward_euler_zero_span_does_not_run_newton() {
        let result = try_backward_euler(
            |_t, _y| panic!("zero-span backward Euler must not evaluate the derivative"),
            |_t, _y| panic!("zero-span backward Euler must not evaluate the Jacobian"),
            1.0,
            2.0,
            1.0,
            10,
        );

        assert_eq!(result, Ok(vec![(1.0, 2.0)]));
    }

    #[test]
    fn test_rk45_system_zero_span() {
        let f = |_t: f64, state: &[f64]| vec![state[1], -state[0]];
        let solution = runge_kutta_45_system(f, 1.0, &[1.0, 0.0], 1.0, 1e-6);
        assert_eq!(solution.len(), 1);
    }

    #[test]
    fn rk45_empty_system_is_defined_only_for_a_zero_span() {
        let zero_span = try_runge_kutta_45_system(
            |_t, _state| panic!("zero-span RKF45 must not evaluate the derivative"),
            1.0,
            &[],
            1.0,
            1e-6,
        );
        assert_eq!(zero_span, Ok(vec![(1.0, vec![])]));

        let nonzero_span = try_runge_kutta_45_system(|_t, _state| Vec::new(), 0.0, &[], 1.0, 1e-6);
        assert_eq!(nonzero_span, Err(Rkf45Error::InvalidInput));
    }

    #[test]
    fn rk45_reports_resource_limited_scalar_integrations() {
        let result = try_runge_kutta_45(|t, _y| (100_000.0 * t).sin(), 0.0, 0.0, 1.0, 1e-12);

        assert_eq!(
            result,
            Err(Rkf45Error::ResourceLimit {
                max_steps: MAX_ODE_STEPS
            })
        );
    }

    #[test]
    fn rk45_reports_resource_limited_system_integrations() {
        let result = try_runge_kutta_45_system(
            |t, _state| vec![(100_000.0 * t).sin(), (100_000.0 * t).cos()],
            0.0,
            &[0.0, 0.0],
            1.0,
            1e-12,
        );

        assert_eq!(
            result,
            Err(Rkf45Error::ResourceLimit {
                max_steps: MAX_ODE_STEPS
            })
        );
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

// ---------------------------------------------------------------------------
// Frente G-A: EDOs simbólicas de 1er orden (separables + lineales).
//
// `y' = a(x)·y + b(x)` por factor integrante `μ = exp(∫−a dx)`;
// `y' = g(x)·h(y)` por separación `∫dy/h(y) = ∫g(x)dx`.
// El resto devuelve `Err` honesto. Referencia GeoGebra: `SolveODE`.
// Presupuesto: entradas ≤ 2000 bytes; integración delegada a
// `symbolic::integrate_typed` (Hermite/Rothstein + Risch-Norman).
// ---------------------------------------------------------------------------

/// Máximo de bytes por expresión de una EDO simbólica.
pub const MAX_ODE_SYMBOLIC_BYTES: usize = 2000;

/// Error honesto del solver simbólico de EDOs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OdeSymbolicError {
    /// Entrada vacía o mayor a 2000 bytes.
    InputTooLong { provided: usize, maximum: usize },
    /// Variable no es identificador válido.
    InvalidVariable { variable: String },
    /// No parsea con el AST de Grafito.
    Parse { reason: String },
    /// No es separable ni lineal de 1er orden.
    NotSupported { hint: String },
    /// Una integral del método no tiene primitiva implementada.
    IntegrationFailed { expr: String },
}

impl std::fmt::Display for OdeSymbolicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLong { provided, maximum } => {
                write!(f, "EDO de {provided} bytes excede el máximo {maximum}")
            }
            Self::InvalidVariable { variable } => {
                write!(f, "variable '{variable}' no es un identificador válido")
            }
            Self::Parse { reason } => write!(f, "no se pudo parsear la EDO: {reason}"),
            Self::NotSupported { hint } => write!(f, "EDO no soportada: {hint}"),
            Self::IntegrationFailed { expr } => {
                write!(f, "sin primitiva para '{expr}'; método no aplicable")
            }
        }
    }
}

impl std::error::Error for OdeSymbolicError {}

/// Clase de una EDO de 1er orden `y' = rhs(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstOrderKind {
    /// `rhs = a(x)·y + b(x)`.
    Linear,
    /// `rhs = g(x)·h(y)` (o `g(x)/k(y)`).
    Separable,
    /// Ninguna de las anteriores.
    Unknown,
}

fn check_ode_identifier(name: &str) -> Result<String, OdeSymbolicError> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(OdeSymbolicError::InvalidVariable {
            variable: name.to_string(),
        });
    }
    Ok(name.to_string())
}

fn check_ode_bytes(expr: &str) -> Result<String, OdeSymbolicError> {
    if expr.is_empty() || expr.len() > MAX_ODE_SYMBOLIC_BYTES {
        return Err(OdeSymbolicError::InputTooLong {
            provided: expr.len(),
            maximum: MAX_ODE_SYMBOLIC_BYTES,
        });
    }
    Ok(expr.replace(' ', ""))
}

fn parse_ode(expr: &str) -> Result<crate::ast::Expr, OdeSymbolicError> {
    crate::ast::parse_ast(expr).map_err(|reason| OdeSymbolicError::Parse { reason })
}

/// Pliega `Neg(Const(c))` a `Const(-c)` en todo el AST.
///
/// El parser produce `Neg(Const)` para literales negativos y el integrador
/// `symbolic` solo reconoce linealidad con lado `Const`; sin este
/// normalizado `exp(-2*x)` no integraría. Recursión total sin pánico.
fn fold_neg_const(e: &crate::ast::Expr) -> crate::ast::Expr {
    use crate::ast::Expr;
    match e {
        Expr::Neg(a) => {
            let inner = fold_neg_const(a);
            if let Expr::Const(c) = inner {
                Expr::Const(-c)
            } else {
                Expr::Neg(Box::new(inner))
            }
        }
        Expr::Add(a, b) => Expr::Add(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Mul(a, b) => Expr::Mul(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Div(a, b) => Expr::Div(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Pow(a, b) => Expr::Pow(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Sin(a) => Expr::Sin(Box::new(fold_neg_const(a))),
        Expr::Cos(a) => Expr::Cos(Box::new(fold_neg_const(a))),
        Expr::Tan(a) => Expr::Tan(Box::new(fold_neg_const(a))),
        Expr::Asin(a) => Expr::Asin(Box::new(fold_neg_const(a))),
        Expr::Acos(a) => Expr::Acos(Box::new(fold_neg_const(a))),
        Expr::Atan(a) => Expr::Atan(Box::new(fold_neg_const(a))),
        Expr::Exp(a) => Expr::Exp(Box::new(fold_neg_const(a))),
        Expr::Ln(a) => Expr::Ln(Box::new(fold_neg_const(a))),
        Expr::Log(a) => Expr::Log(Box::new(fold_neg_const(a))),
        Expr::Sqrt(a) => Expr::Sqrt(Box::new(fold_neg_const(a))),
        Expr::Abs(a) => Expr::Abs(Box::new(fold_neg_const(a))),
        Expr::Sinh(a) => Expr::Sinh(Box::new(fold_neg_const(a))),
        Expr::Cosh(a) => Expr::Cosh(Box::new(fold_neg_const(a))),
        Expr::Tanh(a) => Expr::Tanh(Box::new(fold_neg_const(a))),
        Expr::Floor(a) => Expr::Floor(Box::new(fold_neg_const(a))),
        Expr::Ceil(a) => Expr::Ceil(Box::new(fold_neg_const(a))),
        Expr::Round(a) => Expr::Round(Box::new(fold_neg_const(a))),
        Expr::Sec(a) => Expr::Sec(Box::new(fold_neg_const(a))),
        Expr::Csc(a) => Expr::Csc(Box::new(fold_neg_const(a))),
        Expr::Cot(a) => Expr::Cot(Box::new(fold_neg_const(a))),
        Expr::Asinh(a) => Expr::Asinh(Box::new(fold_neg_const(a))),
        Expr::Acosh(a) => Expr::Acosh(Box::new(fold_neg_const(a))),
        Expr::Atanh(a) => Expr::Atanh(Box::new(fold_neg_const(a))),
        Expr::Sign(a) => Expr::Sign(Box::new(fold_neg_const(a))),
        Expr::Heaviside(a) => Expr::Heaviside(Box::new(fold_neg_const(a))),
        Expr::Cbrt(a) => Expr::Cbrt(Box::new(fold_neg_const(a))),
        Expr::Re(a) => Expr::Re(Box::new(fold_neg_const(a))),
        Expr::Im(a) => Expr::Im(Box::new(fold_neg_const(a))),
        Expr::Arg(a) => Expr::Arg(Box::new(fold_neg_const(a))),
        Expr::Conj(a) => Expr::Conj(Box::new(fold_neg_const(a))),
        Expr::Erf(a) => Expr::Erf(Box::new(fold_neg_const(a))),
        Expr::Erfc(a) => Expr::Erfc(Box::new(fold_neg_const(a))),
        Expr::Gamma(a) => Expr::Gamma(Box::new(fold_neg_const(a))),
        Expr::LnGamma(a) => Expr::LnGamma(Box::new(fold_neg_const(a))),
        Expr::Digamma(a) => Expr::Digamma(Box::new(fold_neg_const(a))),
        Expr::Trigamma(a) => Expr::Trigamma(Box::new(fold_neg_const(a))),
        Expr::Atan2(a, b) => Expr::Atan2(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Modulo(a, b) => {
            Expr::Modulo(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b)))
        }
        Expr::Min(a, b) => Expr::Min(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Max(a, b) => Expr::Max(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Clamp(a, b, c) => Expr::Clamp(
            Box::new(fold_neg_const(a)),
            Box::new(fold_neg_const(b)),
            Box::new(fold_neg_const(c)),
        ),
        Expr::Beta(a, b) => Expr::Beta(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::BesselJ(a, b) => {
            Expr::BesselJ(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b)))
        }
        Expr::BesselY(a, b) => {
            Expr::BesselY(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b)))
        }
        Expr::BesselI(a, b) => {
            Expr::BesselI(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b)))
        }
        Expr::Sum(body, v, s, t) => Expr::Sum(
            Box::new(fold_neg_const(body)),
            v.clone(),
            Box::new(fold_neg_const(s)),
            Box::new(fold_neg_const(t)),
        ),
        Expr::Product(body, v, s, t) => Expr::Product(
            Box::new(fold_neg_const(body)),
            v.clone(),
            Box::new(fold_neg_const(s)),
            Box::new(fold_neg_const(t)),
        ),
        Expr::Piecewise(branches, default) => Expr::Piecewise(
            branches
                .iter()
                .map(|(c, v)| (Box::new(fold_neg_const(c)), Box::new(fold_neg_const(v))))
                .collect(),
            Box::new(fold_neg_const(default)),
        ),
        Expr::Lt(a, b) => Expr::Lt(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Gt(a, b) => Expr::Gt(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Le(a, b) => Expr::Le(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Ge(a, b) => Expr::Ge(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Eq(a, b) => Expr::Eq(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Ne(a, b) => Expr::Ne(Box::new(fold_neg_const(a)), Box::new(fold_neg_const(b))),
        Expr::Const(_) | Expr::Var(_) => e.clone(),
    }
}

/// Parsea y normaliza (`fold_neg_const`) para el integrador `symbolic`.
fn parse_normalized(expr: &str) -> Result<crate::ast::Expr, OdeSymbolicError> {
    parse_ode(expr).map(|ast| fold_neg_const(&ast))
}

/// Descompone `e` como `coef(y)·y + resto` con `coef, resto` sin `y`.
///
/// `None` si no es lineal en `y` (p. ej. `y^2`, `sin(y)`).
fn split_linear_in_y(
    e: &crate::ast::Expr,
    y: &str,
) -> Option<(crate::ast::Expr, crate::ast::Expr)> {
    use crate::ast::Expr;
    match e {
        Expr::Var(name) if name == y => Some((Expr::Const(1.0), Expr::Const(0.0))),
        Expr::Const(_) => Some((Expr::Const(0.0), e.clone())),
        Expr::Var(_) => Some((Expr::Const(0.0), e.clone())),
        Expr::Neg(a) => {
            let (c, r) = split_linear_in_y(a, y)?;
            Some((Expr::Neg(Box::new(c)), Expr::Neg(Box::new(r))))
        }
        Expr::Add(a, b) => {
            let (c1, r1) = split_linear_in_y(a, y)?;
            let (c2, r2) = split_linear_in_y(b, y)?;
            Some((
                Expr::Add(Box::new(c1), Box::new(c2)),
                Expr::Add(Box::new(r1), Box::new(r2)),
            ))
        }
        Expr::Sub(a, b) => {
            let (c1, r1) = split_linear_in_y(a, y)?;
            let (c2, r2) = split_linear_in_y(b, y)?;
            Some((
                Expr::Sub(Box::new(c1), Box::new(c2)),
                Expr::Sub(Box::new(r1), Box::new(r2)),
            ))
        }
        Expr::Mul(a, b) => {
            if !crate::cas::cas_contains_var(a, y) {
                let (c, r) = split_linear_in_y(b, y)?;
                return Some((
                    Expr::Mul(a.clone(), Box::new(c)),
                    Expr::Mul(a.clone(), Box::new(r)),
                ));
            }
            if !crate::cas::cas_contains_var(b, y) {
                let (c, r) = split_linear_in_y(a, y)?;
                return Some((
                    Expr::Mul(Box::new(c), b.clone()),
                    Expr::Mul(Box::new(r), b.clone()),
                ));
            }
            None
        }
        Expr::Div(a, b) => {
            if crate::cas::cas_contains_var(b, y) {
                return None;
            }
            let (c, r) = split_linear_in_y(a, y)?;
            Some((
                Expr::Div(Box::new(c), b.clone()),
                Expr::Div(Box::new(r), b.clone()),
            ))
        }
        _ => {
            if crate::cas::cas_contains_var(e, y) {
                None
            } else {
                Some((Expr::Const(0.0), e.clone()))
            }
        }
    }
}

/// Factoriza `rhs` como `(g(x), h(y))` con `rhs = g·h` o `g/h`.
fn factor_separable(
    e: &crate::ast::Expr,
    x: &str,
    y: &str,
) -> Option<(crate::ast::Expr, crate::ast::Expr, bool)> {
    use crate::ast::Expr;
    match e {
        Expr::Mul(a, b) => {
            if !crate::cas::cas_contains_var(a, y) && !crate::cas::cas_contains_var(b, x) {
                return Some((a.as_ref().clone(), b.as_ref().clone(), true));
            }
            if !crate::cas::cas_contains_var(b, y) && !crate::cas::cas_contains_var(a, x) {
                return Some((b.as_ref().clone(), a.as_ref().clone(), true));
            }
            None
        }
        Expr::Div(a, b) => {
            if !crate::cas::cas_contains_var(a, y) && !crate::cas::cas_contains_var(b, x) {
                return Some((a.as_ref().clone(), b.as_ref().clone(), false));
            }
            None
        }
        _ => None,
    }
}

fn integrate_or_fail(expr: &str, var: &str) -> Result<String, OdeSymbolicError> {
    match crate::symbolic::integrate_typed(expr, var) {
        crate::outcome::MathResult::Exact(prim) => Ok(prim),
        _ => Err(OdeSymbolicError::IntegrationFailed {
            expr: expr.to_string(),
        }),
    }
}

/// Primitiva sobre AST: primero Risch-Norman propio (tolera `Neg(Const)`
/// de literales negativos), luego `symbolic` como respaldo.
fn ode_prim(e: &crate::ast::Expr, var: &str) -> Result<String, OdeSymbolicError> {
    if let Ok(prim) = crate::integral::risch_ast(e, var) {
        return Ok(prim.to_expr_string());
    }
    integrate_or_fail(&e.to_expr_string(), var)
}

/// Clasifica `y' = rhs(x, y)` como lineal, separable o desconocida.
pub fn classify_first_order(
    rhs: &str,
    x: &str,
    y: &str,
) -> Result<FirstOrderKind, OdeSymbolicError> {
    let x = check_ode_identifier(x)?;
    let y = check_ode_identifier(y)?;
    let clean = check_ode_bytes(rhs)?;
    let ast = parse_ode(&clean)?;
    if split_linear_in_y(&ast, &y).is_some() {
        return Ok(FirstOrderKind::Linear);
    }
    if factor_separable(&ast, &x, &y).is_some() {
        return Ok(FirstOrderKind::Separable);
    }
    Ok(FirstOrderKind::Unknown)
}

/// Resuelve `y' + p(x)·y = q(x)` por factor integrante.
///
/// Devuelve `y = (H + C)/μ` con `μ = exp(∫p dx)` explícitos.
pub fn solve_linear_first_order(
    p_expr: &str,
    q_expr: &str,
    x: &str,
) -> Result<String, OdeSymbolicError> {
    let x = check_ode_identifier(x)?;
    let p_ast = parse_normalized(&check_ode_bytes(p_expr)?)?;
    let q_ast = parse_normalized(&check_ode_bytes(q_expr)?)?;

    // μ = exp(∫p dx); H = ∫μ·q por Risch-AST (tolera Neg(Const)) con
    // respaldo en `symbolic`. Si q ≡ 0, H = 0 sin integrar ruido.
    let p_str = p_ast.to_expr_string();
    let p_int = integrate_or_fail(&p_str, &x)?;
    let p_int_ast = parse_normalized(&p_int)?;
    let mu_ast = crate::ast::Expr::Exp(Box::new(p_int_ast));
    let mu = mu_ast.to_expr_string();
    let h = match crate::cas::cas_const_value(&q_ast) {
        Some(0.0) => "0".to_string(),
        _ => {
            let mu_q_ast = crate::ast::Expr::Mul(Box::new(mu_ast), Box::new(q_ast));
            ode_prim(&fold_neg_const(&mu_q_ast), &x).map_err(|_| {
                OdeSymbolicError::IntegrationFailed {
                    expr: mu_q_ast.to_expr_string(),
                }
            })?
        }
    };
    let out = format!("y = ({h} + C)/({mu})");
    if out.len() > MAX_ODE_SYMBOLIC_BYTES * 4 {
        return Err(OdeSymbolicError::IntegrationFailed {
            expr: "solución excede el presupuesto".to_string(),
        });
    }
    Ok(out)
}

/// Resuelve `y' = g(x)·h(y)` por separación `∫dy/h(y) = ∫g(x)dx`.
pub fn solve_separable(g_x: &str, h_y: &str, x: &str, y: &str) -> Result<String, OdeSymbolicError> {
    let x = check_ode_identifier(x)?;
    let y = check_ode_identifier(y)?;
    let g_ast = parse_normalized(&check_ode_bytes(g_x)?)?;
    let h_ast = parse_normalized(&check_ode_bytes(h_y)?)?;

    if !crate::cas::cas_contains_var(&h_ast, &y) {
        let prod_ast = crate::ast::Expr::Mul(Box::new(h_ast), Box::new(g_ast));
        let f = ode_prim(&fold_neg_const(&prod_ast), &x)?;
        return Ok(format!("{y} = {f} + C"));
    }
    let inv_h_ast = crate::ast::Expr::Div(Box::new(crate::ast::Expr::Const(1.0)), Box::new(h_ast));
    let left = ode_prim(&fold_neg_const(&inv_h_ast), &y)?;
    let right = ode_prim(&fold_neg_const(&g_ast), &x)?;
    Ok(format!("{left} = {right} + C"))
}

/// Resuelve `y' = rhs(x, y)` si es lineal o separable.
///
/// Lineal `a(x)·y + b(x)` → forma estándar con `p = −a`, `q = b`;
/// separable `g(x)·h(y)` → separación. Resto: `Err` honesto.
/// Referencia GeoGebra: `SolveODE`.
pub fn solve_ode_first_order(rhs: &str, x: &str, y: &str) -> Result<String, OdeSymbolicError> {
    let x = check_ode_identifier(x)?;
    let y = check_ode_identifier(y)?;
    let clean = check_ode_bytes(rhs)?;
    let ast = parse_ode(&clean)?;

    if let Some((coef, rest)) = split_linear_in_y(&ast, &y) {
        // y' = a·y + b → forma estándar y' + p·y = q con p = −a, q = b.
        let neg_a = crate::ast::Expr::Neg(Box::new(coef.simplify()));
        let p = fold_neg_const(&neg_a.simplify()).to_expr_string();
        let rest_s = fold_neg_const(&rest.simplify()).to_expr_string();
        return solve_linear_first_order(&p, &rest_s, &x);
    }
    if let Some((g, h, is_mul)) = factor_separable(&ast, &x, &y) {
        let g_s = g.simplify().to_expr_string();
        let h_s = h.simplify().to_expr_string();
        if is_mul {
            return solve_separable(&g_s, &h_s, &x, &y);
        }
        let inv_k_ast = crate::ast::Expr::Div(
            Box::new(crate::ast::Expr::Const(1.0)),
            Box::new(parse_normalized(&h_s)?),
        );
        let left = ode_prim(&fold_neg_const(&inv_k_ast), &y)?;
        let right = ode_prim(&fold_neg_const(&parse_normalized(&g_s)?), &x)?;
        return Ok(format!("{left} = {right} + C"));
    }
    Err(OdeSymbolicError::NotSupported {
        hint: "SolveODE soporta EDOs lineales y' = a(x)*y + b(x) y separables y' = g(x)*h(y); el resto (Riccati, Bernoulli general, 2do orden) es L en Tasks.md F10.W5".to_string(),
    })
}

#[cfg(test)]
mod ode_symbolic_tests {
    use super::*;

    #[test]
    fn linear_homogeneous_growth() {
        // y' = 2*y → p = -2, q = 0
        let sol = solve_ode_first_order("2*y", "x", "y").expect("lineal");
        assert!(sol.starts_with("y = "), "got {sol}");
        assert!(sol.contains("exp"), "got {sol}");
        assert!(sol.contains('C'), "got {sol}");
    }

    #[test]
    fn linear_nonhomogeneous_factor_integrante() {
        // y' = -2*y + 3 → p = 2, q = 3 → μ = exp(2*x)
        let sol = solve_linear_first_order("2", "3", "x").expect("lineal p=2 q=3");
        let flat = sol.replace(' ', "");
        assert!(flat.contains("exp(2*x)"), "got {sol}");
        assert!(sol.contains('C'), "got {sol}");
    }

    #[test]
    fn separable_quotient() {
        // y' = x/y → ln|y| = x^2/2 + C
        let sol = solve_ode_first_order("x/y", "x", "y").expect("separable");
        assert!(sol.contains('C'), "got {sol}");
        assert!(sol.contains("ln") || sol.contains("log"), "got {sol}");
    }

    #[test]
    fn separable_direct_api() {
        let sol = solve_separable("x", "y", "x", "y").expect("separable directo");
        assert!(sol.contains('C'), "got {sol}");
        assert!(sol.contains("ln") || sol.contains("log"), "got {sol}");
    }

    #[test]
    fn classify_distinguishes_families() {
        assert_eq!(
            classify_first_order("2*y + x", "x", "y").expect("clasifica"),
            FirstOrderKind::Linear
        );
        assert_eq!(
            classify_first_order("x*y", "x", "y").expect("clasifica"),
            FirstOrderKind::Linear
        );
        assert_eq!(
            classify_first_order("sin(x*y)", "x", "y").expect("clasifica"),
            FirstOrderKind::Unknown
        );
    }

    #[test]
    fn unsupported_ode_is_honest_err() {
        let err =
            solve_ode_first_order("y^2 + sin(x*y)", "x", "y").expect_err("no lineal/separable");
        assert!(
            matches!(err, OdeSymbolicError::NotSupported { .. }),
            "got {err}"
        );
        assert!(format!("{err}").contains("Tasks.md"), "got {err}");
    }

    #[test]
    fn ode_rejects_bad_input() {
        assert!(matches!(
            solve_ode_first_order(&"y".repeat(2001), "x", "y"),
            Err(OdeSymbolicError::InputTooLong { .. })
        ));
        assert!(matches!(
            solve_ode_first_order("y", "x!", "y"),
            Err(OdeSymbolicError::InvalidVariable { .. })
        ));
        assert!(matches!(
            solve_ode_first_order("y +", "x", "y"),
            Err(OdeSymbolicError::Parse { .. })
        ));
    }
}
