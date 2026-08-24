#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_geometry::matrices::{
    condition_number, norm_frobenius, null_space, rank, solve_linear_system, Matrix,
};

fn relative_error(actual: f64, expected: f64) -> f64 {
    (actual - expected).abs() / expected.abs().max(f64::MIN_POSITIVE)
}

fn backward_residual(a: &Matrix, x: &Matrix, b: &Matrix) -> f64 {
    let residual = a.mul(x).unwrap().sub(b).unwrap();
    norm_frobenius(&residual) / (norm_frobenius(a) * norm_frobenius(x) + norm_frobenius(b))
}

fn null_vector_residual(a: &Matrix, vector: &[f64]) -> f64 {
    let x = Matrix::new(vector.len(), 1, vector.to_vec()).unwrap();
    let zero = Matrix::zeros(a.rows, 1);
    backward_residual(a, &x, &zero)
}

#[test]
fn rank_and_null_space_are_invariant_under_uniform_scaling() {
    let full_rank = Matrix::from_rows(vec![vec![4.0, 1.0], vec![2.0, 3.0]]).unwrap();
    let singular = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();

    for scale in [1e-20, 1e20] {
        let scaled_full_rank = full_rank.scale(scale);
        assert_eq!(rank(&scaled_full_rank), Some(2), "scale={scale:e}");
        assert!(
            null_space(&scaled_full_rank).unwrap().is_empty(),
            "full-rank matrix acquired a null space at scale={scale:e}"
        );

        let scaled_singular = singular.scale(scale);
        assert_eq!(rank(&scaled_singular), Some(1), "scale={scale:e}");
        let basis = null_space(&scaled_singular).unwrap();
        assert_eq!(basis.len(), 1, "scale={scale:e}, basis={basis:?}");
        assert!(
            null_vector_residual(&scaled_singular, &basis[0]) < 1e-14,
            "null-space residual too large at scale={scale:e}"
        );
    }
}

#[test]
fn condition_number_is_invariant_under_uniform_scaling() {
    let full_rank = Matrix::from_rows(vec![vec![4.0, 1.0], vec![2.0, 3.0]]).unwrap();
    let expected = condition_number(&full_rank).unwrap();
    let singular = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();

    for scale in [1e-20, 1e20] {
        let actual = condition_number(&full_rank.scale(scale)).unwrap();
        assert!(
            relative_error(actual, expected) < 1e-12,
            "condition changed from {expected:e} to {actual:e} at scale={scale:e}"
        );
        assert!(
            condition_number(&singular.scale(scale))
                .unwrap()
                .is_infinite(),
            "singular matrix reported a finite condition at scale={scale:e}"
        );
    }
}

#[test]
fn inverse_is_invariant_under_uniform_scaling_and_has_a_small_residual() {
    let matrix = Matrix::from_rows(vec![vec![4.0, 1.0], vec![2.0, 3.0]]).unwrap();
    let expected = Matrix::from_rows(vec![vec![0.3, -0.1], vec![-0.2, 0.4]]).unwrap();
    let identity = Matrix::identity(2);
    let singular = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();

    for scale in [1e-20, 1e20] {
        let scaled = matrix.scale(scale);
        let inverse = scaled
            .inverse()
            .unwrap_or_else(|| panic!("invertible matrix rejected at scale={scale:e}"));
        for row in 0..2 {
            for col in 0..2 {
                assert!(
                    relative_error(inverse.get(row, col), expected.get(row, col) / scale) < 1e-12,
                    "wrong inverse entry ({row},{col}) at scale={scale:e}"
                );
            }
        }
        assert!(
            backward_residual(&scaled, &inverse, &identity) < 1e-14,
            "inverse residual too large at scale={scale:e}"
        );
        assert!(
            singular.scale(scale).inverse().is_none(),
            "singular matrix became invertible at scale={scale:e}"
        );
    }
}

#[test]
fn determinant_preserves_uniform_scale_and_exact_singularity() {
    let identity = Matrix::identity(4);
    let singular = Matrix::from_rows(vec![
        vec![1.0, 0.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0],
        vec![0.0, 0.0, 0.0, 0.0],
    ])
    .unwrap();

    for scale in [1e-20_f64, 1e20_f64] {
        let expected = scale.powi(4);
        let actual = identity.scale(scale).determinant().unwrap();
        assert!(
            relative_error(actual, expected) < 1e-12,
            "determinant was {actual:e}, expected {expected:e} at scale={scale:e}"
        );
        assert_eq!(
            singular.scale(scale).determinant(),
            Some(0.0),
            "singular determinant changed at scale={scale:e}"
        );
    }
}

#[test]
fn linear_solve_is_invariant_under_uniform_scaling_and_has_a_small_residual() {
    let matrix = Matrix::from_rows(vec![vec![4.0, 1.0], vec![2.0, 3.0]]).unwrap();
    let expected = Matrix::from_rows(vec![vec![1.0], vec![-2.0]]).unwrap();
    let rhs = Matrix::from_rows(vec![vec![2.0], vec![-4.0]]).unwrap();
    let singular = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();
    let singular_rhs = Matrix::from_rows(vec![vec![3.0], vec![6.0]]).unwrap();

    for scale in [1e-20, 1e20] {
        let scaled = matrix.scale(scale);
        let scaled_rhs = rhs.scale(scale);
        let solution = solve_linear_system(&scaled, &scaled_rhs)
            .unwrap_or_else(|| panic!("solvable system rejected at scale={scale:e}"));
        for row in 0..2 {
            assert!(
                relative_error(solution.get(row, 0), expected.get(row, 0)) < 1e-12,
                "wrong solution row {row} at scale={scale:e}"
            );
        }
        assert!(
            backward_residual(&scaled, &solution, &scaled_rhs) < 1e-14,
            "solve residual too large at scale={scale:e}"
        );
        assert!(
            solve_linear_system(&singular.scale(scale), &singular_rhs.scale(scale)).is_none(),
            "singular system produced a solution at scale={scale:e}"
        );
    }
}
