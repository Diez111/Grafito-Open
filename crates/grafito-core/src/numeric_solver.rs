//! Small Levenberg-Marquardt solver for systems of non-linear equations.
//!
//! The solver is intentionally minimal: it targets the small, dense systems that
//! arise from 2D geometric constraints (a handful of point coordinates and radii).
//! The Jacobian is approximated with finite differences and the LM normal
//! equations are solved with a simple dense Gaussian elimination.

/// Index of a scalar variable in the solver vector.
pub type VarIndex = usize;

/// A single equation (or coupled set of equations) that can be evaluated for a
/// given variable vector.
pub trait ConstraintEquation: Send + Sync {
    /// Number of scalar equations produced by this constraint.
    fn dimension(&self) -> usize;

    /// Evaluate residuals given the current variable vector.
    fn residual(&self, vars: &[f64]) -> Vec<f64>;
}

/// Statistics returned by a successful solve.
#[derive(Debug, Clone)]
pub struct SolveStats {
    pub iterations: usize,
    pub final_residual: f64,
}

/// Errors that can occur while solving.
#[derive(Debug, Clone)]
pub enum SolveError {
    /// The maximum number of iterations was reached.
    MaxIterations { final_residual: f64 },
    /// The augmented normal matrix was singular.
    SingularJacobian,
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
}

impl Default for NumericSolver {
    fn default() -> Self {
        Self {
            max_iter: 100,
            lambda: 1e-3,
            lambda_scale: 10.0,
            tol: 1e-9,
        }
    }
}

impl NumericSolver {
    /// Solve `equations(vars) ≈ 0` using the Levenberg-Marquardt algorithm.
    ///
    /// On success, `vars` is updated to the final values and statistics are
    /// returned. On failure, `vars` is left in the best state found so far.
    #[allow(clippy::needless_range_loop)]
    pub fn solve(
        &self,
        vars: &mut [f64],
        equations: &[Box<dyn ConstraintEquation>],
    ) -> Result<SolveStats, SolveError> {
        let m: usize = equations.iter().map(|eq| eq.dimension()).sum();
        if m == 0 || vars.is_empty() {
            return Err(SolveError::NoEquations);
        }

        let n = vars.len();
        let mut lambda = self.lambda;
        let mut r = compute_residual(vars, equations, m);
        let mut residual_norm = norm(&r);

        for iter in 0..self.max_iter {
            if residual_norm < self.tol {
                return Ok(SolveStats {
                    iterations: iter,
                    final_residual: residual_norm,
                });
            }

            let j = finite_difference_jacobian(vars, equations, &r, m, n);

            // Build the normal equations: (J^T J + lambda*I) delta = -J^T r
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

            let delta = match solve_linear_system(&jtj, &rhs) {
                Some(d) => d,
                None => return Err(SolveError::SingularJacobian),
            };

            if norm(&delta) < self.tol {
                return Ok(SolveStats {
                    iterations: iter,
                    final_residual: residual_norm,
                });
            }

            let mut trial = vars.to_vec();
            for i in 0..n {
                trial[i] += delta[i];
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
}

fn compute_residual(vars: &[f64], equations: &[Box<dyn ConstraintEquation>], m: usize) -> Vec<f64> {
    let mut r = Vec::with_capacity(m);
    for eq in equations {
        r.extend(eq.residual(vars));
    }
    r
}

fn finite_difference_jacobian(
    vars: &[f64],
    equations: &[Box<dyn ConstraintEquation>],
    r0: &[f64],
    m: usize,
    n: usize,
) -> Vec<Vec<f64>> {
    let mut j = vec![vec![0.0; n]; m];

    for i in 0..n {
        let h = 1e-8 * vars[i].abs().max(1.0);
        let mut vars_plus = vars.to_vec();
        vars_plus[i] += h;
        let r_plus = compute_residual(&vars_plus, equations, m);
        for k in 0..m {
            j[k][i] = (r_plus[k] - r0[k]) / h;
        }
    }

    j
}

#[allow(clippy::needless_range_loop)]
fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
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

    for col in 0..n {
        let mut max_row = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-12 {
            return None;
        }
        aug.swap(col, max_row);

        for row in (col + 1)..n {
            let factor = aug[row][col] / aug[col][col];
            for c in col..=n {
                aug[row][c] -= factor * aug[col][c];
            }
        }
    }

    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }
    Some(x)
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
}
