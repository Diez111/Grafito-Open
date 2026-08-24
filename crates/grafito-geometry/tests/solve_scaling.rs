#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_geometry::{ast::parse_ast, symbolic};

#[test]
fn polynomial_solve_is_invariant_under_a_nonzero_global_scale() {
    let ordinary = symbolic::solve("x - 1", "x").expect("ordinary linear solve");
    let tiny = symbolic::solve("1e-15*x - 1e-15", "x").expect("scaled linear solve");

    assert_eq!(ordinary, "x = 1.00000000");
    assert_eq!(tiny, ordinary);
}

#[test]
fn complex_polynomial_solver_keeps_tiny_nonzero_coefficients() {
    let ast = parse_ast("1e-15*x - 1e-15").expect("valid polynomial");
    let roots = symbolic::solve_polynomial_complex(&ast, "x").expect("polynomial detected");

    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!((roots[0].0 - 1.0).abs() < 1e-10, "{roots:?}");
    assert!(roots[0].1.abs() < 1e-10, "{roots:?}");
}

#[test]
fn numeric_solver_does_not_turn_small_positive_minima_into_roots() {
    for expression in ["x^2 + 1e-12", "1e-15*(sin(x)^2 + 1e-12)", "exp(x)"] {
        let ast = parse_ast(expression).expect("valid expression");
        let roots = symbolic::find_real_roots_numeric(&ast, "x", -20.0, 20.0);

        assert!(roots.is_empty(), "{expression}: {roots:?}");
    }
}

#[test]
fn polynomial_solve_handles_negative_scales_and_disparate_coefficients() {
    assert_eq!(
        symbolic::solve("-1e-15*(x - 1)", "x").unwrap(),
        "x = 1.00000000"
    );
    assert_eq!(
        symbolic::solve("1e-16*x + 1", "x").unwrap(),
        "x = -10000000000000000.00000000"
    );

    let ast = parse_ast("1e-28*x + 1e-15").unwrap();
    let roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!((roots[0].0 + 1e13).abs() <= 1e-2, "{roots:?}");
    assert_eq!(roots[0].1, 0.0, "{roots:?}");
}

#[test]
fn complex_solver_preserves_small_genuine_imaginary_roots() {
    let ast = parse_ast("x^2 + 1e-20").unwrap();
    let mut roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
    roots.sort_by(|left, right| left.1.total_cmp(&right.1));

    assert_eq!(roots.len(), 2, "{roots:?}");
    assert!(roots[0].0.abs() < 1e-15, "{roots:?}");
    assert!((roots[0].1 + 1e-10).abs() < 1e-15, "{roots:?}");
    assert!(roots[1].0.abs() < 1e-15, "{roots:?}");
    assert!((roots[1].1 - 1e-10).abs() < 1e-15, "{roots:?}");
}

#[test]
fn direct_subnormal_scale_does_not_reduce_root_accuracy() {
    let ordinary = parse_ast("sin(x)").unwrap();
    let scaled = parse_ast("1e-320*sin(x)").unwrap();

    let ordinary_roots = symbolic::find_real_roots_numeric(&ordinary, "x", -4.0, 4.0);
    let scaled_roots = symbolic::find_real_roots_numeric(&scaled, "x", -4.0, 4.0);

    assert_eq!(scaled_roots.len(), ordinary_roots.len(), "{scaled_roots:?}");
    for (actual, expected) in scaled_roots.iter().zip(ordinary_roots) {
        assert!((actual - expected).abs() < 1e-10, "{scaled_roots:?}");
    }
}

#[test]
fn nested_finite_scale_is_removed_before_it_can_underflow_coefficients() {
    let roots = symbolic::solve("1e-15*(1e-310*(x-1))", "x").unwrap();
    assert_eq!(roots, "x = 1.00000000");
}

#[test]
fn negative_scale_preserves_complex_roots() {
    let positive = parse_ast("1e-15*(x^2+1)").unwrap();
    let negative = parse_ast("-1e-15*(x^2+1)").unwrap();
    let mut positive_roots = symbolic::solve_polynomial_complex(&positive, "x").unwrap();
    let mut negative_roots = symbolic::solve_polynomial_complex(&negative, "x").unwrap();
    positive_roots.sort_by(|left, right| left.1.total_cmp(&right.1));
    negative_roots.sort_by(|left, right| left.1.total_cmp(&right.1));

    assert_eq!(negative_roots, positive_roots);
}

#[test]
fn small_positive_quadratic_constant_is_not_a_real_root() {
    assert_eq!(
        symbolic::solve("x^2+1e-13", "x").unwrap(),
        "No real roots found"
    );
}

#[test]
fn converged_repeated_complex_roots_are_projected_to_the_known_real_root() {
    let ast = parse_ast("x^4-4*x^3+6*x^2-4*x+1").unwrap();
    let roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();

    assert_eq!(roots.len(), 4, "{roots:?}");
    for root in roots {
        assert!((root.0 - 1.0).abs() < 1e-6, "{root:?}");
        assert_eq!(root.1, 0.0, "{root:?}");
    }
}

#[test]
fn scalar_normalization_is_independent_of_association() {
    for expression in ["6*(x^2+1)", "2*3*(x^2+1)", "(6/2)*(x^2+1)"] {
        let ast = parse_ast(expression).unwrap();
        let mut roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
        roots.sort_by(|left, right| left.1.total_cmp(&right.1));
        assert_eq!(roots, vec![(0.0, -1.0), (0.0, 1.0)], "{expression}");
    }

    assert_eq!(
        symbolic::solve("1e-15*1e-310*(x-1)", "x").unwrap(),
        "x = 1.00000000"
    );
}

#[test]
fn numeric_solver_keeps_boundary_and_factored_even_roots() {
    let boundary = parse_ast("sin(x)").unwrap();
    assert_eq!(
        symbolic::find_real_roots_numeric(&boundary, "x", 0.0, 1.0),
        vec![0.0]
    );

    let even = parse_ast("(sin(x)-0.1)^2").unwrap();
    let roots = symbolic::find_real_roots_numeric(&even, "x", -1.0, 1.0);
    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!((roots[0] - 0.1_f64.asin()).abs() < 1e-10, "{roots:?}");
}

#[test]
fn disparate_high_degree_coefficients_never_publish_nan_roots() {
    assert_eq!(
        symbolic::solve("1e-16*x^4-1", "x").unwrap(),
        "x = -10000.00000000, x = 10000.00000000"
    );

    let quadratic = parse_ast("1e-200*x^2+x").unwrap();
    let mut quadratic_roots = symbolic::solve_polynomial_complex(&quadratic, "x").unwrap();
    quadratic_roots.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert_eq!(quadratic_roots, vec![(-1e200, 0.0), (0.0, 0.0)]);

    let cubic = parse_ast("1e-200*x^3+x^2").unwrap();
    let roots = symbolic::solve_polynomial_complex(&cubic, "x").unwrap();
    assert_eq!(roots.len(), 3, "{roots:?}");
    assert!(roots
        .iter()
        .all(|root| root.0.is_finite() && root.1.is_finite()));
    assert_eq!(roots.iter().filter(|root| **root == (0.0, 0.0)).count(), 2);
    assert!(roots.iter().any(|root| *root == (-1e200, 0.0)), "{roots:?}");
}

#[test]
fn nearby_complex_roots_are_not_reclassified_as_repeated_real_roots() {
    let ast = parse_ast("(x-1048576)*((x-1048576)^2+0.125^2)").unwrap();
    let mut roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
    roots.sort_by(|left, right| left.1.total_cmp(&right.1));

    assert_eq!(roots.len(), 3, "{roots:?}");
    assert!((roots[0].0 - 1048576.0).abs() < 1e-6, "{roots:?}");
    assert!((roots[0].1 + 0.125).abs() < 1e-6, "{roots:?}");
    assert_eq!(roots[1], (1048576.0, 0.0), "{roots:?}");
    assert!((roots[2].0 - 1048576.0).abs() < 1e-6, "{roots:?}");
    assert!((roots[2].1 - 0.125).abs() < 1e-6, "{roots:?}");
}

#[test]
fn strictly_positive_quartic_does_not_gain_a_projected_real_root() {
    assert_eq!(
        symbolic::solve("x^4+2e-13*x^2+1e-26", "x").unwrap(),
        "No real roots found"
    );
}

#[test]
fn polynomial_candidates_are_validated_against_the_original_ast() {
    let ast = parse_ast("1e-50*x^3+x^2+1").unwrap();
    let mut roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
    roots.sort_by(|left, right| left.1.total_cmp(&right.1));

    assert_eq!(roots.len(), 3, "{roots:?}");
    assert!(
        roots[0].0.abs() < 1e-8 && (roots[0].1 + 1.0).abs() < 1e-8,
        "{roots:?}"
    );
    assert!(roots[1].0 < -1e49 && roots[1].1 == 0.0, "{roots:?}");
    assert!(
        roots[2].0.abs() < 1e-8 && (roots[2].1 - 1.0).abs() < 1e-8,
        "{roots:?}"
    );
    assert!(roots
        .iter()
        .all(|root| root.0.is_finite() && root.1.is_finite()));
}

#[test]
fn identically_zero_products_do_not_return_arbitrary_factor_roots() {
    for expression in ["0*(x-1)", "(1-1)*(x-2)", "0*(x^2+1)"] {
        let ast = parse_ast(expression).unwrap();
        assert!(symbolic::is_identically_zero(&ast), "{expression}");
        assert!(
            symbolic::solve_polynomial_complex(&ast, "x")
                .unwrap()
                .is_empty(),
            "{expression}"
        );
        assert!(
            symbolic::find_real_roots_numeric(&ast, "x", -3.0, 3.0).is_empty(),
            "{expression}"
        );
    }

    for expression in ["0*(1/x)", "0*sqrt(x)"] {
        let ast = parse_ast(expression).unwrap();
        assert!(!symbolic::is_identically_zero(&ast), "{expression}");
    }
    for expression in ["x+(-x)", "(x+1)-(1+x)"] {
        let ast = parse_ast(expression).unwrap();
        assert!(symbolic::is_identically_zero(&ast), "{expression}");
    }
}

#[test]
fn factored_polynomials_are_solved_without_lossy_expansion() {
    let cases = [
        (
            "(x-100000000)*(x-100000001)*(x-100000002)",
            vec![100000000.0, 100000001.0, 100000002.0],
        ),
        ("(x-0.1)^4", vec![0.1, 0.1, 0.1, 0.1]),
        ("(1e-200*(x-1))*(1e-200*(x-1))", vec![1.0, 1.0]),
    ];

    for (expression, expected) in cases {
        let ast = parse_ast(expression).unwrap();
        let roots = symbolic::solve_polynomial_complex(&ast, "x").unwrap();
        assert_eq!(roots.len(), expected.len(), "{expression}: {roots:?}");
        for (root, expected) in roots.iter().zip(expected) {
            assert_eq!(root.1, 0.0, "{expression}: {roots:?}");
            assert!((root.0 - expected).abs() < 1e-8, "{expression}: {roots:?}");
        }
    }
}

#[test]
fn numeric_solver_decomposes_products_and_accepts_approximate_endpoints() {
    let product = parse_ast("sin(x)^2*(x^2+1)").unwrap();
    let roots = symbolic::find_real_roots_numeric(&product, "x", 3.0, 3.3);
    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!((roots[0] - std::f64::consts::PI).abs() < 1e-10, "{roots:?}");

    let endpoint = parse_ast("sin(x)").unwrap();
    let roots = symbolic::find_real_roots_numeric(&endpoint, "x", 0.0, std::f64::consts::PI);
    assert_eq!(roots.len(), 2, "{roots:?}");
    assert!((roots[0] - 0.0).abs() < 1e-10, "{roots:?}");
    assert!((roots[1] - std::f64::consts::PI).abs() < 1e-10, "{roots:?}");
}
