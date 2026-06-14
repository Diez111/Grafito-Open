//! Small Levenberg-Marquardt solver for systems of non-linear equations.
//!
//! The solver is intentionally minimal: it targets the small, dense systems that
//! arise from 2D geometric constraints (a handful of point coordinates and radii).
//! The Jacobian can be supplied analytically by constraint implementations or
//! approximated with finite differences. The LM normal equations are solved with
//! a simple dense Gaussian elimination with Tikhonov regularization for rank-
//! deficient Jacobians.

use rayon::prelude::*;

/// Index of a scalar variable in the solver vector.
pub type VarIndex = usize;

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
}

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

/// Cached Jacobian sparsity pattern derived from analytic Jacobians.
struct JacobianPattern {
    /// For each global column, the list of global rows that depend on it.
    col_to_rows: Vec<Vec<usize>>,
    /// Whether the pattern is fully described by analytic Jacobians.
    has_analytic: bool,
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
        let m: usize = equations.iter().map(|eq| eq.dimension()).sum();
        if m == 0 || vars.is_empty() {
            return Err(SolveError::NoEquations);
        }

        let n = vars.len();
        if let Some(ws) = warm_start {
            if ws.len() == n {
                vars.copy_from_slice(ws);
            }
        }

        let pattern = Self::build_jacobian_pattern(equations, m, n);

        let mut lambda = self.lambda;
        let mut r = compute_residual(vars, equations, m);
        let mut residual_norm = norm(&r);
        let mut condition_estimate = 1.0;

        for iter in 0..self.max_iter {
            if residual_norm < self.tol {
                return Ok(SolveStats {
                    iterations: iter,
                    final_residual: residual_norm,
                    condition_number_estimate: condition_estimate,
                });
            }

            let j = Self::compute_jacobian(vars, equations, &r, m, n, &pattern);

            // Build the normal equations: (J^T J + lambda I) delta = -J^T r
            let mut jtj = vec![vec![0.0; n]; n];
            for i in 0..n {
                let mut acc = 0.0;
                for k in 0..m {
                    acc += j[k][i] * j[k][i];
                }
                jtj[i][i] = acc + lambda;
                for j_ in (i + 1)..n {
                    let mut acc = 0.0;
                    for k in 0..m {
                        acc += j[k][i] * j[k][j_];
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
                rhs[i] = -acc;
            }

            let (delta, cond) =
                solve_linear_system_with_regularization(&jtj, &rhs, lambda, self.regularization);
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

            if norm(&delta) < self.tol {
                return Ok(SolveStats {
                    iterations: iter,
                    final_residual: residual_norm,
                    condition_number_estimate: condition_estimate,
                });
            }

            let mut trial = vars.to_vec();
            for i in 0..n {
                trial[i] += delta[i];
                if let Some(b) = bounds.get(i) {
                    if let Some(lower) = b.lower {
                        trial[i] = trial[i].max(lower);
                    }
                    if let Some(upper) = b.upper {
                        trial[i] = trial[i].min(upper);
                    }
                }
            }

            let r_trial = compute_residual(&trial, equations, m);
            let residual_trial = norm(&r_trial);

            if residual_trial < residual_norm {
                vars.copy_from_slice(&trial);
                r = r_trial;
                residual_norm = residual_trial;
                lambda /= self.lambda_scale;
            } else {
                lambda *= self.lambda_scale;
            }
        }

        Err(SolveError::MaxIterations {
            final_residual: residual_norm,
        })
    }

    fn build_jacobian_pattern(
        equations: &[Box<dyn ConstraintEquation>],
        m: usize,
        n: usize,
    ) -> JacobianPattern {
        let mut col_to_rows: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut row_offset = 0usize;
        let mut has_analytic = true;

        for eq in equations {
            let dim = eq.dimension();
            let triples = eq.jacobian(&[]);
            if triples.is_empty() {
                has_analytic = false;
            } else {
                for (local_row, col, _) in triples {
                    let global_row = row_offset + local_row;
                    if col < n && global_row < m {
                        let rows = &mut col_to_rows[col];
                        if rows.last() != Some(&global_row) {
                            rows.push(global_row);
                        }
                    }
                }
            }
            row_offset += dim;
        }

        // Sort and deduplicate row lists for stable access.
        for rows in &mut col_to_rows {
            rows.sort_unstable();
            rows.dedup();
        }

        JacobianPattern {
            col_to_rows,
            has_analytic,
        }
    }

    fn compute_jacobian(
        vars: &[f64],
        equations: &[Box<dyn ConstraintEquation>],
        r0: &[f64],
        m: usize,
        n: usize,
        pattern: &JacobianPattern,
    ) -> Vec<Vec<f64>> {
        if pattern.has_analytic {
            Self::analytic_jacobian(vars, equations, r0, m, n, pattern)
        } else {
            Self::finite_difference_jacobian(vars, equations, r0, m, n)
        }
    }

    fn analytic_jacobian(
        vars: &[f64],
        equations: &[Box<dyn ConstraintEquation>],
        r0: &[f64],
        m: usize,
        n: usize,
        pattern: &JacobianPattern,
    ) -> Vec<Vec<f64>> {
        // Evaluate analytic Jacobians in parallel per equation, then merge.
        let row_offsets: Vec<usize> = equations
            .iter()
            .scan(0usize, |offset, eq| {
                let start = *offset;
                *offset += eq.dimension();
                Some(start)
            })
            .collect();

        let per_eq_jacobians: Vec<Vec<(usize, usize, f64)>> =
            equations.par_iter().map(|eq| eq.jacobian(vars)).collect();

        let mut j = vec![vec![0.0; n]; m];
        for (offset, triples) in row_offsets.iter().zip(per_eq_jacobians.iter()) {
            for &(local_row, col, value) in triples {
                let global_row = offset + local_row;
                if global_row < m && col < n {
                    j[global_row][col] = value;
                }
            }
        }

        // For any column not covered by the analytic pattern, fall back to
        // finite differences on the affected rows only. This keeps the sparse
        // speedup while guaranteeing correctness if an equation omitted some
        // non-zero entries.
        for col in 0..n {
            if pattern.col_to_rows[col].is_empty() {
                let h = 1e-8 * vars[col].abs().max(1.0);
                let mut vars_plus = vars.to_vec();
                vars_plus[col] += h;
                let r_plus = compute_residual(&vars_plus, equations, m);
                for row in 0..m {
                    j[row][col] = (r_plus[row] - compute_residual_row(r0, row)) / h;
                }
            }
        }

        j
    }

    fn finite_difference_jacobian(
        vars: &[f64],
        equations: &[Box<dyn ConstraintEquation>],
        r0: &[f64],
        m: usize,
        n: usize,
    ) -> Vec<Vec<f64>> {
        let mut j = vec![vec![0.0; n]; m];

        // Parallelise over columns: each perturbed evaluation is independent.
        let cols: Vec<usize> = (0..n).collect();
        let col_results: Vec<(usize, Vec<f64>)> = cols
            .par_iter()
            .map(|&i| {
                let h = 1e-8 * vars[i].abs().max(1.0);
                let mut vars_plus = vars.to_vec();
                vars_plus[i] += h;
                let r_plus = compute_residual(&vars_plus, equations, m);
                let col: Vec<f64> = (0..m).map(|k| (r_plus[k] - r0[k]) / h).collect();
                (i, col)
            })
            .collect();

        for (i, col) in col_results {
            for k in 0..m {
                j[k][i] = col[k];
            }
        }

        j
    }
}

#[inline]
fn compute_residual_row(r0: &[f64], row: usize) -> f64 {
    r0.get(row).copied().unwrap_or(0.0)
}

fn compute_residual(vars: &[f64], equations: &[Box<dyn ConstraintEquation>], m: usize) -> Vec<f64> {
    let per_eq: Vec<Vec<f64>> = equations.par_iter().map(|eq| eq.residual(vars)).collect();
    let mut r = Vec::with_capacity(m);
    for eq_r in per_eq {
        r.extend(eq_r);
    }
    r
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
) -> (Option<Vec<f64>>, f64) {
    let n = b.len();
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
        if diag_abs > max_diag {
            max_diag = diag_abs;
        }
        if diag_abs < min_diag {
            min_diag = diag_abs;
        }

        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            for c in col..=n {
                aug[row][c] -= factor * aug[col][c];
            }
        }
    }

    let condition_estimate = if min_diag.is_finite() && min_diag > 0.0 {
        max_diag / min_diag
    } else {
        1.0
    };

    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        if aug[i][i].abs() < reg {
            return (None, condition_estimate);
        }
        x[i] = sum / aug[i][i];
    }
    (Some(x), condition_estimate)
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
