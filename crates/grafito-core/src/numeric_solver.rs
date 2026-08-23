//! Small Levenberg-Marquardt solver for systems of non-linear equations.
//!
//! The solver is intentionally minimal: it targets the small, dense systems that
//! arise from 2D geometric constraints (a handful of point coordinates and radii).
//! The Jacobian can be supplied analytically by constraint implementations or
//! approximated with finite differences. The LM normal equations are solved with
//! a simple dense Gaussian elimination with Tikhonov regularization for rank-
//! deficient Jacobians.

use rayon::prelude::*;
use std::fmt;

/// Index of a scalar variable in the solver vector.
pub type VarIndex = usize;

/// Largest aggregate residual dimension accepted by the dense solver.
pub const MAX_CONSTRAINT_EQUATIONS: usize = 10_000;
/// Largest aggregate dense matrix element count accepted by the solver.
pub const MAX_SOLVER_MATRIX_ELEMENTS: usize = 8_000_000;
/// Largest variable count that keeps dense normal-equation elimination bounded.
pub const MAX_SOLVER_VARIABLES: usize = 192;
/// Largest elimination operation count allowed per iteration and per configured solve.
pub const MAX_SOLVER_ELIMINATION_OPERATIONS: usize = 8_000_000;
/// Largest `J^T J` operation count allowed per iteration and per configured solve.
pub const MAX_NORMAL_EQUATION_OPERATIONS: usize = 8_000_000;
/// Largest configured number of LM iterations accepted by the public solver API.
pub const MAX_SOLVER_ITERATIONS: usize = 1_000;

/// A single equation (or coupled set of equations) that can be evaluated for a
/// given variable vector.
pub trait ConstraintEquation: Send + Sync {
    /// Number of scalar equations produced by this constraint.
    fn dimension(&self) -> usize;

    /// Evaluate residuals given the current variable vector.
    fn residual(&self, vars: &[f64]) -> Vec<f64>;

    /// Optional analytic Jacobian.
    ///
    /// Returns a list of `(row, col, value)` triples where `row` is the local
    /// row index inside this equation's residual block (`0..dimension()`),
    /// `col` is the global variable index, and `value` is the partial
    /// derivative `dr[row] / dvars[col]`.
    ///
    /// An empty return value means the solver will fall back to finite
    /// differences for this equation.
    fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
        Vec::new()
    }

    /// Validates any variable indices held by this equation before evaluation.
    ///
    /// The default supports equations that only use the supplied slice through
    /// safe access. Equations that store variable indices override it so the
    /// solver can return a typed error rather than allowing an invalid index to
    /// reach their residual or Jacobian implementation.
    fn validate_variables(&self, _vars: &[f64]) -> Result<(), VarIndex> {
        Ok(())
    }
}

/// Per-variable bounds used during solving.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bounds {
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

impl Bounds {
    pub fn new(lower: Option<f64>, upper: Option<f64>) -> Self {
        Self { lower, upper }
    }
}

/// Statistics returned by a successful solve.
#[derive(Debug, Clone)]
pub struct SolveStats {
    pub iterations: usize,
    pub final_residual: f64,
    /// Rough estimate of the condition number of the last augmented normal
    /// matrix (`J^T J + lambda I`). A value of `1.0` means the matrix was well
    /// conditioned or regularization kept it invertible.
    pub condition_number_estimate: f64,
}

/// Errors that can occur while solving.
#[derive(Debug, Clone)]
pub enum SolveError {
    /// The maximum number of iterations was reached.
    MaxIterations { final_residual: f64 },
    /// No equations were supplied or no variables were present.
    NoEquations,
    /// An equation returned a residual vector with a different size than its declaration.
    MalformedResidualDimension {
        equation: usize,
        expected: usize,
        actual: usize,
    },
    /// An analytic Jacobian entry addresses a row or column outside the system.
    MalformedJacobianEntry {
        equation: usize,
        row: usize,
        column: usize,
    },
    /// An equation produced a non-finite residual value.
    NonFiniteResidual { equation: usize, row: usize },
    /// An equation produced a non-finite analytic Jacobian value.
    NonFiniteJacobian {
        equation: usize,
        row: usize,
        column: usize,
    },
    /// An input variable or warm-start value is non-finite.
    NonFiniteVariable { index: usize },
    /// An equation refers to a variable outside the supplied vector.
    VariableIndexOutOfBounds {
        equation: usize,
        index: usize,
        variables: usize,
    },
    /// Solver configuration is not finite or is outside its valid range.
    InvalidConfiguration { field: &'static str },
    /// A variable bound is invalid or non-finite.
    InvalidBounds { index: usize },
    /// An internal numeric operation overflowed or became non-finite.
    NonFiniteComputation { stage: &'static str },
    /// An equation declared a residual block too large for the dense solver.
    EquationDimensionLimitExceeded {
        equation: usize,
        dimension: usize,
        maximum: usize,
    },
    /// The aggregate dense matrices would exceed the solver's memory budget.
    SystemTooLarge {
        equations: usize,
        variables: usize,
        maximum_elements: usize,
    },
    /// A dense solver resource budget would be exceeded before allocation.
    ResourceLimitExceeded {
        resource: &'static str,
        requested: usize,
        maximum: usize,
    },
    /// A fixed system has no free variables and does not satisfy its constraints.
    Unsatisfied { final_residual: f64 },
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaxIterations { final_residual } => {
                write!(f, "solver did not converge (residual {final_residual})")
            }
            Self::NoEquations => write!(f, "solver requires at least one equation and variable"),
            Self::MalformedResidualDimension {
                equation,
                expected,
                actual,
            } => write!(
                f,
                "equation {equation} returned {actual} residuals, expected {expected}"
            ),
            Self::MalformedJacobianEntry {
                equation,
                row,
                column,
            } => write!(
                f,
                "equation {equation} returned invalid Jacobian entry ({row}, {column})"
            ),
            Self::NonFiniteResidual { equation, row } => {
                write!(
                    f,
                    "equation {equation} produced non-finite residual row {row}"
                )
            }
            Self::NonFiniteJacobian {
                equation,
                row,
                column,
            } => write!(
                f,
                "equation {equation} produced non-finite Jacobian entry ({row}, {column})"
            ),
            Self::NonFiniteVariable { index } => {
                write!(f, "solver variable {index} is non-finite")
            }
            Self::VariableIndexOutOfBounds {
                equation,
                index,
                variables,
            } => write!(
                f,
                "equation {equation} references variable {index}, but only {variables} variables were supplied"
            ),
            Self::InvalidConfiguration { field } => {
                write!(f, "solver configuration {field} is invalid")
            }
            Self::InvalidBounds { index } => {
                write!(f, "solver bounds for variable {index} are invalid")
            }
            Self::NonFiniteComputation { stage } => {
                write!(f, "solver computation became non-finite during {stage}")
            }
            Self::EquationDimensionLimitExceeded {
                equation,
                dimension,
                maximum,
            } => write!(
                f,
                "equation {equation} declares dimension {dimension}, maximum is {maximum}"
            ),
            Self::SystemTooLarge {
                equations,
                variables,
                maximum_elements,
            } => write!(
                f,
                "solver system {equations}x{variables} exceeds {maximum_elements} matrix elements"
            ),
            Self::ResourceLimitExceeded {
                resource,
                requested,
                maximum,
            } => write!(
                f,
                "solver {resource} resource request {requested} exceeds maximum {maximum}"
            ),
            Self::Unsatisfied { final_residual } => write!(
                f,
                "fixed constraints are unsatisfied (residual {final_residual})"
            ),
        }
    }
}

impl std::error::Error for SolveError {}

/// Levenberg-Marquardt solver configuration.
#[derive(Debug, Clone)]
pub struct NumericSolver {
    pub max_iter: usize,
    pub lambda: f64,
    pub lambda_scale: f64,
    pub tol: f64,
    /// Tikhonov regularization added to near-zero diagonal entries of the
    /// augmented normal matrix when the Jacobian is rank deficient.
    pub regularization: f64,
}

impl Default for NumericSolver {
    fn default() -> Self {
        Self {
            max_iter: 100,
            lambda: 1e-3,
            lambda_scale: 10.0,
            tol: 1e-9,
            regularization: 1e-12,
        }
    }
}

impl NumericSolver {
    /// Solve `equations(vars) ≈ 0` using the Levenberg-Marquardt algorithm.
    ///
    /// On success, `vars` is updated to the final values and statistics are
    /// returned. On failure, `vars` is left in the best state found so far.
    pub fn solve(
        &self,
        vars: &mut [f64],
        equations: &[Box<dyn ConstraintEquation>],
    ) -> Result<SolveStats, SolveError> {
        self.solve_with_warm_start(vars, equations, None)
    }

    /// Solve with an optional warm-start vector.
    ///
    /// If `warm_start` is provided and has the same length as `vars`, its
    /// values are copied into `vars` before the first iteration.
    pub fn solve_with_warm_start(
        &self,
        vars: &mut [f64],
        equations: &[Box<dyn ConstraintEquation>],
        warm_start: Option<&[f64]>,
    ) -> Result<SolveStats, SolveError> {
        self.solve_with_warm_start_and_bounds(vars, equations, warm_start, &[])
    }

    /// Solve with optional per-variable bounds.
    ///
    /// After each Levenberg-Marquardt step the trial variables are clamped to
    /// their bounds. Missing bounds or a shorter `bounds` slice are treated as
    /// unbounded.
    pub fn solve_with_bounds(
        &self,
        vars: &mut [f64],
        equations: &[Box<dyn ConstraintEquation>],
        bounds: &[Bounds],
    ) -> Result<SolveStats, SolveError> {
        self.solve_with_warm_start_and_bounds(vars, equations, None, bounds)
    }

    /// Solve with optional warm start and per-variable bounds.
    #[allow(clippy::needless_range_loop)]
    pub fn solve_with_warm_start_and_bounds(
        &self,
        vars: &mut [f64],
        equations: &[Box<dyn ConstraintEquation>],
        warm_start: Option<&[f64]>,
        bounds: &[Bounds],
    ) -> Result<SolveStats, SolveError> {
        self.validate_configuration()?;
        let n = vars.len();
        if n > MAX_SOLVER_VARIABLES {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "variables",
                requested: n,
                maximum: MAX_SOLVER_VARIABLES,
            });
        }

        let mut m = 0usize;
        for (equation, eq) in equations.iter().enumerate() {
            let dimension = eq.dimension();
            if dimension > MAX_CONSTRAINT_EQUATIONS {
                return Err(SolveError::EquationDimensionLimitExceeded {
                    equation,
                    dimension,
                    maximum: MAX_CONSTRAINT_EQUATIONS,
                });
            }
            m = m
                .checked_add(dimension)
                .ok_or(SolveError::EquationDimensionLimitExceeded {
                    equation,
                    dimension,
                    maximum: MAX_CONSTRAINT_EQUATIONS,
                })?;
            if m > MAX_CONSTRAINT_EQUATIONS {
                return Err(SolveError::EquationDimensionLimitExceeded {
                    equation,
                    dimension,
                    maximum: MAX_CONSTRAINT_EQUATIONS,
                });
            }
        }
        if m == 0 {
            return Err(SolveError::NoEquations);
        }

        let jacobian_elements = m.checked_mul(n).ok_or(SolveError::ResourceLimitExceeded {
            resource: "matrix elements",
            requested: usize::MAX,
            maximum: MAX_SOLVER_MATRIX_ELEMENTS,
        })?;
        let normal_elements = n.checked_mul(n).ok_or(SolveError::ResourceLimitExceeded {
            resource: "matrix elements",
            requested: usize::MAX,
            maximum: MAX_SOLVER_MATRIX_ELEMENTS,
        })?;
        let augmented_elements = n
            .checked_add(1)
            .and_then(|width| n.checked_mul(width))
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "matrix elements",
                requested: usize::MAX,
                maximum: MAX_SOLVER_MATRIX_ELEMENTS,
            })?;
        let total_matrix_elements = jacobian_elements
            .checked_add(normal_elements)
            .and_then(|total| total.checked_add(augmented_elements))
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "matrix elements",
                requested: usize::MAX,
                maximum: MAX_SOLVER_MATRIX_ELEMENTS,
            })?;
        if total_matrix_elements > MAX_SOLVER_MATRIX_ELEMENTS {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "matrix elements",
                requested: total_matrix_elements,
                maximum: MAX_SOLVER_MATRIX_ELEMENTS,
            });
        }
        let normal_equation_operations = m
            .checked_mul(n)
            .and_then(|entries| entries.checked_mul(n))
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested: usize::MAX,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            })?;
        if normal_equation_operations > MAX_NORMAL_EQUATION_OPERATIONS {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested: normal_equation_operations,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            });
        }
        let aggregate_normal_equation_operations = normal_equation_operations
            .checked_mul(self.max_iter)
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested: usize::MAX,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            })?;
        if aggregate_normal_equation_operations > MAX_NORMAL_EQUATION_OPERATIONS {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested: aggregate_normal_equation_operations,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            });
        }
        let elimination_operations = n
            .checked_mul(n)
            .and_then(|square| n.checked_add(1).and_then(|width| square.checked_mul(width)))
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "elimination operations",
                requested: usize::MAX,
                maximum: MAX_SOLVER_ELIMINATION_OPERATIONS,
            })?;
        if elimination_operations > MAX_SOLVER_ELIMINATION_OPERATIONS {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "elimination operations",
                requested: elimination_operations,
                maximum: MAX_SOLVER_ELIMINATION_OPERATIONS,
            });
        }
        let aggregate_elimination_operations = elimination_operations
            .checked_mul(self.max_iter)
            .ok_or(SolveError::ResourceLimitExceeded {
                resource: "elimination operations",
                requested: usize::MAX,
                maximum: MAX_SOLVER_ELIMINATION_OPERATIONS,
            })?;
        if aggregate_elimination_operations > MAX_SOLVER_ELIMINATION_OPERATIONS {
            return Err(SolveError::ResourceLimitExceeded {
                resource: "elimination operations",
                requested: aggregate_elimination_operations,
                maximum: MAX_SOLVER_ELIMINATION_OPERATIONS,
            });
        }
        if let Some(ws) = warm_start {
            if ws.len() == n {
                vars.copy_from_slice(ws);
            }
        }

        for (index, value) in vars.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(SolveError::NonFiniteVariable { index });
            }
        }
        for (equation, eq) in equations.iter().enumerate() {
            if let Err(index) = eq.validate_variables(vars) {
                return Err(SolveError::VariableIndexOutOfBounds {
                    equation,
                    index,
                    variables: n,
                });
            }
        }
        for (index, bound) in bounds.iter().take(n).enumerate() {
            let lower_valid = bound.lower.is_none_or(f64::is_finite);
            let upper_valid = bound.upper.is_none_or(f64::is_finite);
            if !lower_valid
                || !upper_valid
                || matches!((bound.lower, bound.upper), (Some(lower), Some(upper)) if lower > upper)
            {
                return Err(SolveError::InvalidBounds { index });
            }
        }

        let mut lambda = self.lambda;
        let mut r = compute_residual(vars, equations, m)?;
        let mut residual_norm = norm(&r).ok_or(SolveError::NonFiniteComputation {
            stage: "residual norm",
        })?;
        let mut condition_estimate = 1.0;

        if n == 0 {
            return if residual_norm < self.tol {
                Ok(SolveStats {
                    iterations: 0,
                    final_residual: residual_norm,
                    condition_number_estimate: condition_estimate,
                })
            } else {
                Err(SolveError::Unsatisfied {
                    final_residual: residual_norm,
                })
            };
        }

        for iter in 0..self.max_iter {
            if residual_norm < self.tol {
                return Ok(SolveStats {
                    iterations: iter,
                    final_residual: residual_norm,
                    condition_number_estimate: condition_estimate,
                });
            }

            let j = Self::compute_jacobian(vars, equations, &r, m, n)?;

            // Build the normal equations: (J^T J + lambda I) delta = -J^T r
            let mut jtj = vec![vec![0.0; n]; n];
            for i in 0..n {
                let mut acc = 0.0;
                for k in 0..m {
                    acc += j[k][i] * j[k][i];
                }
                if !acc.is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "normal matrix diagonal",
                    });
                }
                jtj[i][i] = acc + lambda;
                if !jtj[i][i].is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "damped normal matrix diagonal",
                    });
                }
                for j_ in (i + 1)..n {
                    let mut acc = 0.0;
                    for k in 0..m {
                        acc += j[k][i] * j[k][j_];
                    }
                    if !acc.is_finite() {
                        return Err(SolveError::NonFiniteComputation {
                            stage: "normal matrix off-diagonal",
                        });
                    }
                    jtj[i][j_] = acc;
                    jtj[j_][i] = acc;
                }
            }

            let mut rhs = vec![0.0; n];
            for i in 0..n {
                let mut acc = 0.0;
                for k in 0..m {
                    acc += j[k][i] * r[k];
                }
                if !acc.is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "normal equation right-hand side",
                    });
                }
                rhs[i] = -acc;
            }

            let (delta, cond) =
                solve_linear_system_with_regularization(&jtj, &rhs, lambda, self.regularization)?;
            condition_estimate = cond;

            let delta = match delta {
                Some(d) => d,
                None => {
                    // With Tikhonov regularization this should not happen,
                    // but if it does we report max iterations with the
                    // current residual.
                    return Err(SolveError::MaxIterations {
                        final_residual: residual_norm,
                    });
                }
            };

            if norm(&delta).ok_or(SolveError::NonFiniteComputation { stage: "step norm" })?
                < self.tol
            {
                return Err(SolveError::MaxIterations {
                    final_residual: residual_norm,
                });
            }

            let mut step_accepted = false;
            let mut alpha = 1.0;
            let mut trial = vars.to_vec();
            let mut r_trial = Vec::new();
            let mut residual_trial = f64::INFINITY;

            // Backtracking line search: try alpha = 1.0, 0.5, 0.25, 0.125
            for _ in 0..4 {
                for i in 0..n {
                    trial[i] = vars[i] + alpha * delta[i];
                    if let Some(b) = bounds.get(i) {
                        if let Some(lower) = b.lower {
                            trial[i] = trial[i].max(lower);
                        }
                        if let Some(upper) = b.upper {
                            trial[i] = trial[i].min(upper);
                        }
                    }
                }

                if !trial.iter().all(|&x| x.is_finite()) {
                    alpha *= 0.5;
                    continue;
                }

                let rt = compute_residual(&trial, equations, m)?;
                let rt_norm = norm(&rt).ok_or(SolveError::NonFiniteComputation {
                    stage: "trial residual norm",
                })?;

                if rt_norm.is_finite() && rt_norm < residual_norm {
                    r_trial = rt;
                    residual_trial = rt_norm;
                    step_accepted = true;
                    break;
                }

                alpha *= 0.5;
            }

            if step_accepted {
                vars.copy_from_slice(&trial);
                r = r_trial;
                residual_norm = residual_trial;
                lambda /= self.lambda_scale;
            } else {
                lambda *= self.lambda_scale;
                if !lambda.is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "damping adjustment",
                    });
                }
            }
        }

        if residual_norm < self.tol {
            return Ok(SolveStats {
                iterations: self.max_iter,
                final_residual: residual_norm,
                condition_number_estimate: condition_estimate,
            });
        }

        Err(SolveError::MaxIterations {
            final_residual: residual_norm,
        })
    }

    fn compute_jacobian(
        vars: &[f64],
        equations: &[Box<dyn ConstraintEquation>],
        r0: &[f64],
        m: usize,
        n: usize,
    ) -> Result<Vec<Vec<f64>>, SolveError> {
        let analytic: Vec<Vec<(usize, usize, f64)>> =
            equations.par_iter().map(|eq| eq.jacobian(vars)).collect();

        for (equation, (eq, triples)) in equations.iter().zip(&analytic).enumerate() {
            for &(row, column, value) in triples {
                if row >= eq.dimension() || column >= n {
                    return Err(SolveError::MalformedJacobianEntry {
                        equation,
                        row,
                        column,
                    });
                }
                if !value.is_finite() {
                    return Err(SolveError::NonFiniteJacobian {
                        equation,
                        row,
                        column,
                    });
                }
            }
        }

        if analytic.iter().all(|triples| !triples.is_empty()) {
            Self::analytic_jacobian(equations, &analytic, m, n)
        } else {
            Self::finite_difference_jacobian(vars, equations, r0, m, n)
        }
    }

    fn analytic_jacobian(
        equations: &[Box<dyn ConstraintEquation>],
        analytic: &[Vec<(usize, usize, f64)>],
        m: usize,
        n: usize,
    ) -> Result<Vec<Vec<f64>>, SolveError> {
        let row_offsets: Vec<usize> = equations
            .iter()
            .scan(0usize, |offset, eq| {
                let start = *offset;
                *offset += eq.dimension();
                Some(start)
            })
            .collect();

        let mut j = vec![vec![0.0; n]; m];
        for (offset, triples) in row_offsets.iter().zip(analytic.iter()) {
            for &(local_row, col, value) in triples {
                let entry = &mut j[offset + local_row][col];
                *entry += value;
                if !entry.is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "analytic Jacobian accumulation",
                    });
                }
            }
        }

        Ok(j)
    }

    fn finite_difference_jacobian(
        vars: &[f64],
        equations: &[Box<dyn ConstraintEquation>],
        r0: &[f64],
        m: usize,
        n: usize,
    ) -> Result<Vec<Vec<f64>>, SolveError> {
        let mut j = vec![vec![0.0; n]; m];

        // Parallelise over columns: each perturbed evaluation is independent.
        let cols: Vec<usize> = (0..n).collect();
        let col_results: Vec<Result<(usize, Vec<f64>), SolveError>> = cols
            .par_iter()
            .map(|&i| {
                let h = 1e-8 * vars[i].abs().max(1.0);
                let mut vars_plus = vars.to_vec();
                vars_plus[i] += h;
                let r_plus = compute_residual(&vars_plus, equations, m)?;
                let mut col = Vec::with_capacity(m);
                for k in 0..m {
                    let value = (r_plus[k] - r0[k]) / h;
                    if !value.is_finite() {
                        return Err(SolveError::NonFiniteComputation {
                            stage: "finite-difference Jacobian",
                        });
                    }
                    col.push(value);
                }
                Ok((i, col))
            })
            .collect();

        for result in col_results {
            let (i, col) = result?;
            for k in 0..m {
                j[k][i] = col[k];
            }
        }

        Ok(j)
    }

    fn validate_configuration(&self) -> Result<(), SolveError> {
        if self.max_iter > MAX_SOLVER_ITERATIONS {
            return Err(SolveError::InvalidConfiguration { field: "max_iter" });
        }
        if !self.lambda.is_finite() || self.lambda < 0.0 {
            return Err(SolveError::InvalidConfiguration { field: "lambda" });
        }
        if !self.lambda_scale.is_finite() || self.lambda_scale <= 1.0 {
            return Err(SolveError::InvalidConfiguration {
                field: "lambda_scale",
            });
        }
        if !self.tol.is_finite() || self.tol <= 0.0 {
            return Err(SolveError::InvalidConfiguration { field: "tol" });
        }
        if !self.regularization.is_finite() || self.regularization <= 0.0 {
            return Err(SolveError::InvalidConfiguration {
                field: "regularization",
            });
        }
        Ok(())
    }
}

fn compute_residual(
    vars: &[f64],
    equations: &[Box<dyn ConstraintEquation>],
    m: usize,
) -> Result<Vec<f64>, SolveError> {
    let per_eq: Vec<Vec<f64>> = equations.par_iter().map(|eq| eq.residual(vars)).collect();
    let mut r = Vec::with_capacity(m);
    for (equation, (eq, eq_r)) in equations.iter().zip(per_eq).enumerate() {
        let expected = eq.dimension();
        let actual = eq_r.len();
        if actual != expected {
            return Err(SolveError::MalformedResidualDimension {
                equation,
                expected,
                actual,
            });
        }
        for (row, value) in eq_r.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(SolveError::NonFiniteResidual { equation, row });
            }
        }
        r.extend(eq_r);
    }
    debug_assert_eq!(r.len(), m);
    Ok(r)
}

/// Solve `a x = b` by Gaussian elimination with partial pivoting and
/// Tikhonov regularization.
///
/// If a pivot diagonal is smaller than `reg`, it is replaced with
/// `lambda + reg`. This keeps the normal equations invertible even when the
/// Jacobian is rank deficient.
///
/// Returns the solution (if any) and a rough condition-number estimate based
/// on the ratio of largest to smallest diagonal magnitude encountered.
#[allow(clippy::needless_range_loop)]
fn solve_linear_system_with_regularization(
    a: &[Vec<f64>],
    b: &[f64],
    lambda: f64,
    reg: f64,
) -> Result<(Option<Vec<f64>>, f64), SolveError> {
    let n = b.len();
    if a.len() != n
        || a.iter().any(|row| row.len() != n)
        || a.iter().flatten().any(|value| !value.is_finite())
        || b.iter().any(|value| !value.is_finite())
    {
        return Err(SolveError::NonFiniteComputation {
            stage: "linear system input",
        });
    }
    let mut aug: Vec<Vec<f64>> = a
        .iter()
        .zip(b.iter())
        .map(|(row, bi)| {
            let mut r = row.clone();
            r.push(*bi);
            r
        })
        .collect();

    let mut max_diag = 0.0_f64;
    let mut min_diag = f64::INFINITY;

    for col in 0..n {
        let mut max_row = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        aug.swap(col, max_row);

        if aug[col][col].abs() < reg {
            // Tikhonov regularization: push the pivot away from zero.
            aug[col][col] = lambda + reg;
        }

        let diag_abs = aug[col][col].abs();
        if !diag_abs.is_finite() || diag_abs == 0.0 {
            return Err(SolveError::NonFiniteComputation {
                stage: "linear-system pivot",
            });
        }
        if diag_abs > max_diag {
            max_diag = diag_abs;
        }
        if diag_abs < min_diag {
            min_diag = diag_abs;
        }

        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            if !factor.is_finite() {
                return Err(SolveError::NonFiniteComputation {
                    stage: "linear-system elimination factor",
                });
            }
            for c in col..=n {
                aug[row][c] -= factor * aug[col][c];
                if !aug[row][c].is_finite() {
                    return Err(SolveError::NonFiniteComputation {
                        stage: "linear-system elimination",
                    });
                }
            }
        }
    }

    let condition_estimate = if min_diag.is_finite() && min_diag > 0.0 {
        max_diag / min_diag
    } else {
        1.0
    };
    if !condition_estimate.is_finite() {
        return Err(SolveError::NonFiniteComputation {
            stage: "condition estimate",
        });
    }

    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        if !sum.is_finite() {
            return Err(SolveError::NonFiniteComputation {
                stage: "linear-system back substitution",
            });
        }
        if aug[i][i].abs() < reg {
            return Ok((None, condition_estimate));
        }
        x[i] = sum / aug[i][i];
        if !x[i].is_finite() {
            return Err(SolveError::NonFiniteComputation {
                stage: "linear-system solution",
            });
        }
    }
    Ok((Some(x), condition_estimate))
}

fn norm(v: &[f64]) -> Option<f64> {
    let norm = v.iter().copied().fold(0.0_f64, f64::hypot);
    norm.is_finite().then_some(norm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::numeric_constraints::{CoincidentEq, VarOrConst};

    struct CircleIntersection {
        offset: f64,
    }

    impl ConstraintEquation for CircleIntersection {
        fn dimension(&self) -> usize {
            1
        }
        fn residual(&self, vars: &[f64]) -> Vec<f64> {
            let x = vars[0];
            let y = vars[1];
            let d = ((x - self.offset).powi(2) + y.powi(2)).sqrt();
            vec![d - 1.0]
        }
    }

    struct LinearEq {
        a: [f64; 2],
        b: f64,
    }

    impl ConstraintEquation for LinearEq {
        fn dimension(&self) -> usize {
            1
        }
        fn residual(&self, vars: &[f64]) -> Vec<f64> {
            vec![self.a[0] * vars[0] + self.a[1] * vars[1] - self.b]
        }
    }

    struct StationaryNonzeroResidual;

    impl ConstraintEquation for StationaryNonzeroResidual {
        fn dimension(&self) -> usize {
            1
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            vec![1.0]
        }

        fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
            vec![(0, 0, 0.0)]
        }
    }

    struct WrongResidualDimension;

    impl ConstraintEquation for WrongResidualDimension {
        fn dimension(&self) -> usize {
            2
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            vec![0.0]
        }
    }

    struct WrongJacobianDimension;

    impl ConstraintEquation for WrongJacobianDimension {
        fn dimension(&self) -> usize {
            1
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            vec![1.0]
        }

        fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
            vec![(1, 0, 1.0)]
        }
    }

    struct NonFiniteJacobian;

    impl ConstraintEquation for NonFiniteJacobian {
        fn dimension(&self) -> usize {
            1
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            vec![1.0]
        }

        fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
            vec![(0, 0, f64::NAN)]
        }
    }

    struct ExcessiveDimension;

    impl ConstraintEquation for ExcessiveDimension {
        fn dimension(&self) -> usize {
            usize::MAX
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            unreachable!("a rejected dimension must not be evaluated")
        }
    }

    struct ExcessiveNormalEquationWork;

    impl ConstraintEquation for ExcessiveNormalEquationWork {
        fn dimension(&self) -> usize {
            MAX_CONSTRAINT_EQUATIONS
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            unreachable!("a rejected normal-equation workload must not be evaluated")
        }
    }

    struct AnalyticScalarEq {
        index: usize,
        target: f64,
    }

    impl ConstraintEquation for AnalyticScalarEq {
        fn dimension(&self) -> usize {
            1
        }

        fn validate_variables(&self, vars: &[f64]) -> Result<(), VarIndex> {
            if self.index < vars.len() {
                Ok(())
            } else {
                Err(self.index)
            }
        }

        fn residual(&self, vars: &[f64]) -> Vec<f64> {
            vec![vars[self.index] - self.target]
        }

        fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
            vec![(0, self.index, 1.0)]
        }
    }

    struct DuplicateJacobianEntries {
        entries: Vec<(usize, usize, f64)>,
    }

    impl ConstraintEquation for DuplicateJacobianEntries {
        fn dimension(&self) -> usize {
            1
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            vec![0.0]
        }

        fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
            self.entries.clone()
        }
    }

    struct NeverEvaluatedEquation {
        dimension: usize,
    }

    impl ConstraintEquation for NeverEvaluatedEquation {
        fn dimension(&self) -> usize {
            self.dimension
        }

        fn residual(&self, _vars: &[f64]) -> Vec<f64> {
            unreachable!("aggregate work must be rejected before residual evaluation")
        }
    }

    #[test]
    fn max_iterations_is_bounded_before_any_equation_evaluation() {
        for max_iter in [MAX_SOLVER_ITERATIONS - 1, MAX_SOLVER_ITERATIONS] {
            let solver = NumericSolver {
                max_iter,
                ..NumericSolver::default()
            };
            let mut vars = [0.0];
            let error = solver
                .solve(&mut vars, &[])
                .expect_err("a valid configuration reaches the empty-system validation");
            assert!(matches!(error, SolveError::NoEquations));
        }

        let solver = NumericSolver {
            max_iter: MAX_SOLVER_ITERATIONS + 1,
            ..NumericSolver::default()
        };
        let mut vars = [0.0];
        let error = solver
            .solve(&mut vars, &[])
            .expect_err("an excessive max_iter must be rejected first");
        assert!(matches!(
            error,
            SolveError::InvalidConfiguration { field: "max_iter" }
        ));
    }

    #[test]
    fn duplicate_sparse_jacobian_entries_are_accumulated_deterministically() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![
            Box::new(DuplicateJacobianEntries {
                entries: vec![(0, 0, 3.0), (0, 0, -1.0), (0, 0, 2.0)],
            }),
            Box::new(DuplicateJacobianEntries {
                entries: vec![(0, 0, 2.0), (0, 0, -1.0), (0, 0, 3.0)],
            }),
        ];

        let jacobian = NumericSolver::compute_jacobian(&[0.0], &equations, &[0.0, 0.0], 2, 1)
            .expect("duplicate entries are valid sparse Jacobian contributions");

        assert_eq!(jacobian, vec![vec![4.0], vec![4.0]]);
    }

    #[test]
    fn same_object_coincident_assembles_to_zero_jacobian() {
        let x = VarOrConst::new(Some(0), 0.0);
        let y = VarOrConst::new(Some(1), 0.0);
        let equations: Vec<Box<dyn ConstraintEquation>> =
            vec![Box::new(CoincidentEq::new(x, y, x, y))];

        let jacobian = NumericSolver::compute_jacobian(&[3.0, -2.0], &equations, &[0.0, 0.0], 2, 2)
            .expect("same-object aliases are valid analytic contributions");

        assert_eq!(jacobian, vec![vec![0.0, 0.0], vec![0.0, 0.0]]);
    }

    #[test]
    fn same_object_tautology_cannot_change_a_combined_solve() {
        let solver = NumericSolver {
            max_iter: 1,
            tol: 1e-2,
            ..NumericSolver::default()
        };
        let target_equations = || -> Vec<Box<dyn ConstraintEquation>> {
            vec![
                Box::new(AnalyticScalarEq {
                    index: 0,
                    target: 1.0,
                }),
                Box::new(AnalyticScalarEq {
                    index: 1,
                    target: -2.0,
                }),
            ]
        };

        let mut baseline = [0.0, 0.0];
        let baseline_stats = solver
            .solve(&mut baseline, &target_equations())
            .expect("the independent target equations converge in one iteration");

        let x = VarOrConst::new(Some(0), 0.0);
        let y = VarOrConst::new(Some(1), 0.0);
        let mut combined_equations: Vec<Box<dyn ConstraintEquation>> =
            vec![Box::new(CoincidentEq::new(x, y, x, y))];
        combined_equations.extend(target_equations());
        let mut combined = [0.0, 0.0];
        let combined_stats = solver
            .solve(&mut combined, &combined_equations)
            .expect("a zero tautology cannot consume or bias the solve");

        assert_eq!(combined, baseline);
        assert_eq!(combined_stats.final_residual, baseline_stats.final_residual);
    }

    #[test]
    fn aggregate_normal_equation_work_is_rejected_before_evaluation() {
        let equations: Vec<Box<dyn ConstraintEquation>> =
            vec![Box::new(NeverEvaluatedEquation { dimension: 3_000 })];
        let solver = NumericSolver {
            max_iter: MAX_SOLVER_ITERATIONS,
            ..NumericSolver::default()
        };
        let mut vars = [0.0; 2];

        let error = solver
            .solve(&mut vars, &equations)
            .expect_err("aggregate normal-equation work must be bounded");

        assert!(matches!(
            error,
            SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested: 12_000_000,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            }
        ));
    }

    #[test]
    fn aggregate_elimination_work_is_rejected_before_evaluation() {
        let equations: Vec<Box<dyn ConstraintEquation>> =
            vec![Box::new(NeverEvaluatedEquation { dimension: 1 })];
        let solver = NumericSolver {
            max_iter: MAX_SOLVER_ITERATIONS,
            ..NumericSolver::default()
        };
        let mut vars = [0.0; 20];

        let error = solver
            .solve(&mut vars, &equations)
            .expect_err("aggregate elimination work must be bounded");

        assert!(matches!(
            error,
            SolveError::ResourceLimitExceeded {
                resource: "elimination operations",
                requested: 8_400_000,
                maximum: MAX_SOLVER_ELIMINATION_OPERATIONS,
            }
        ));
    }

    #[test]
    fn test_solve_circle_intersection() {
        // x^2 + y^2 = 1  and  (x-1)^2 + y^2 = 1
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![
            Box::new(CircleIntersection { offset: 0.0 }),
            Box::new(CircleIntersection { offset: 1.0 }),
        ];
        let solver = NumericSolver::default();
        let mut vars = [0.5, 0.5];
        let stats = solver
            .solve(&mut vars, &equations)
            .expect("should converge");
        assert!(stats.final_residual < 1e-6);
        assert!((vars[0] - 0.5).abs() < 1e-6);
        assert!((vars[1] - 0.866_025_4).abs() < 1e-4 || (vars[1] + 0.866_025_4).abs() < 1e-4);
    }

    #[test]
    fn test_solve_linear_system_one_iteration() {
        // 2x + y = 5
        // x - y = 1
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![
            Box::new(LinearEq {
                a: [2.0, 1.0],
                b: 5.0,
            }),
            Box::new(LinearEq {
                a: [1.0, -1.0],
                b: 1.0,
            }),
        ];
        let solver = NumericSolver::default();
        let mut vars = [0.0, 0.0];
        let stats = solver
            .solve(&mut vars, &equations)
            .expect("should converge");
        assert!(stats.final_residual < 1e-9);
        assert!((vars[0] - 2.0).abs() < 1e-9);
        assert!((vars[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_solve_with_warm_start() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![
            Box::new(CircleIntersection { offset: 0.0 }),
            Box::new(CircleIntersection { offset: 1.0 }),
        ];
        let solver = NumericSolver::default();
        let mut vars = [0.0, 0.0];
        solver.solve(&mut vars, &equations).unwrap();

        let mut vars2 = [0.0, 0.0];
        let stats2 = solver
            .solve_with_warm_start(&mut vars2, &equations, Some(&vars))
            .unwrap();
        assert!(stats2.final_residual < 1e-6);
        assert!((vars2[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_solve_with_bounds() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![
            Box::new(CircleIntersection { offset: 0.0 }),
            Box::new(CircleIntersection { offset: 1.0 }),
        ];
        let solver = NumericSolver::default();
        // Force the positive intersection by bounding y >= 0.
        let mut vars = [0.0, -10.0];
        let bounds = [Bounds::default(), Bounds::new(Some(0.0), None)];
        let stats = solver
            .solve_with_bounds(&mut vars, &equations, &bounds)
            .expect("should converge");
        assert!(stats.final_residual < 1e-6);
        assert!(vars[1] >= -1e-9, "y should be clamped to lower bound");
        assert!((vars[1] - 0.866_025_4).abs() < 1e-4);
    }

    #[test]
    fn test_underdetermined_system_regularizes() {
        // One equation, two unknowns: 2x + y = 5. The Jacobian is rank 1.
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(LinearEq {
            a: [2.0, 1.0],
            b: 5.0,
        })];
        let solver = NumericSolver::default();
        let mut vars = [0.0, 0.0];
        let stats = solver
            .solve(&mut vars, &equations)
            .expect("should converge");
        assert!(stats.final_residual < 1e-6);
        assert!((2.0 * vars[0] + vars[1] - 5.0).abs() < 1e-6);
        assert!(
            stats.condition_number_estimate.is_finite(),
            "condition estimate should be finite"
        );
    }

    #[test]
    fn stationary_nonzero_residual_is_not_reported_as_converged() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(StationaryNonzeroResidual)];
        let mut vars = [0.0];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("a zero step cannot converge a nonzero residual");

        assert!(matches!(
            error,
            SolveError::MaxIterations { final_residual } if (final_residual - 1.0).abs() < 1e-12
        ));
    }

    #[test]
    fn malformed_residual_dimensions_return_a_typed_error() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(WrongResidualDimension)];
        let mut vars = [0.0];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("malformed residuals must not panic");

        assert!(matches!(
            error,
            SolveError::MalformedResidualDimension {
                equation: 0,
                expected: 2,
                actual: 1,
            }
        ));
    }

    #[test]
    fn malformed_jacobian_dimensions_return_a_typed_error() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(WrongJacobianDimension)];
        let mut vars = [0.0];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("malformed Jacobians must not be ignored");

        assert!(matches!(
            error,
            SolveError::MalformedJacobianEntry {
                equation: 0,
                row: 1,
                column: 0,
            }
        ));
    }

    #[test]
    fn non_finite_jacobian_values_return_a_typed_error() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(NonFiniteJacobian)];
        let mut vars = [0.0];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("non-finite Jacobians must not enter the linear system");

        assert!(matches!(
            error,
            SolveError::NonFiniteJacobian {
                equation: 0,
                row: 0,
                column: 0,
            }
        ));
    }

    #[test]
    fn excessive_equation_dimension_is_rejected_before_allocation() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(ExcessiveDimension)];
        let mut vars = [0.0];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("a hostile dimension must not reach Vec allocation");

        assert!(matches!(
            error,
            SolveError::EquationDimensionLimitExceeded {
                equation: 0,
                dimension: usize::MAX,
                ..
            }
        ));
    }

    #[test]
    fn excessive_variable_count_is_rejected_before_dense_solver_allocation() {
        let equations: Vec<Box<dyn ConstraintEquation>> = vec![Box::new(LinearEq {
            a: [1.0, 0.0],
            b: 0.0,
        })];
        let mut vars = vec![0.0; MAX_SOLVER_VARIABLES + 1];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("a dense normal equation beyond the variable cap must be rejected");

        assert!(matches!(
            error,
            SolveError::ResourceLimitExceeded {
                resource: "variables",
                requested,
                maximum: MAX_SOLVER_VARIABLES,
            } if requested == MAX_SOLVER_VARIABLES + 1
        ));
    }

    #[test]
    fn excessive_normal_equation_work_is_rejected_before_residual_evaluation() {
        let equations: Vec<Box<dyn ConstraintEquation>> =
            vec![Box::new(ExcessiveNormalEquationWork)];
        let mut vars = vec![0.0; MAX_SOLVER_VARIABLES];

        let error = NumericSolver::default()
            .solve(&mut vars, &equations)
            .expect_err("a quadratic normal-equation workload must be rejected");

        assert!(matches!(
            error,
            SolveError::ResourceLimitExceeded {
                resource: "normal equation operations",
                requested,
                maximum: MAX_NORMAL_EQUATION_OPERATIONS,
            } if requested > MAX_NORMAL_EQUATION_OPERATIONS
        ));
    }
}
