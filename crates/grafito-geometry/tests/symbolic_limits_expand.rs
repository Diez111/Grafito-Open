use grafito_geometry::{
    ast::parse_ast,
    symbolic::{expand, limit_typed},
    MathError, MathResult,
};

fn eval(expression: &str, x: f64) -> f64 {
    parse_ast(expression).unwrap().eval_at("x", x)
}

#[test]
fn rejects_an_oscillatory_non_limit() {
    assert!(matches!(
        limit_typed("cos(1/x)", "x", 0.0),
        MathResult::DomainError(MathError::LimitDoesNotExist { .. })
    ));
}

#[test]
fn rejects_an_underflow_masked_oscillatory_non_limit() {
    let expression = "sin(1/x)*exp(-100000000*(abs(x)-0.000001))";
    assert!(matches!(
        limit_typed(expression, "x", 0.0),
        MathResult::DomainError(MathError::LimitDoesNotExist { .. })
    ));
}

#[test]
fn recognizes_squeeze_limits_for_vanishing_factors() {
    for expression in ["x*sin(1/x)", "x^2*cos(1/x)"] {
        match limit_typed(expression, "x", 0.0) {
            MathResult::Approximate {
                value,
                error_estimate,
            } => {
                assert!(value.abs() < 1e-12, "{expression}: {value}");
                assert!(error_estimate.is_finite() && error_estimate >= 0.0);
            }
            result => panic!("expected a squeezed zero limit for {expression}, got {result:?}"),
        }
    }
}

#[test]
fn rejects_squeeze_factors_zero_only_by_rounding_or_underflow() {
    for expression in ["(1e16+1-1e16)*sin(1/x)", "exp(-1000+x)*sin(1/x)"] {
        assert!(matches!(
            limit_typed(expression, "x", 0.0),
            MathResult::DomainError(MathError::LimitDoesNotExist { .. })
        ));
    }
}

#[test]
fn rejects_a_two_sided_positive_pole() {
    for expression in ["1/(x^2)", "1/x"] {
        assert!(matches!(
            limit_typed(expression, "x", 0.0),
            MathResult::DomainError(MathError::LimitDoesNotExist { .. })
        ));
    }
}

#[test]
fn keeps_an_ordinary_finite_limit() {
    for (expression, expected) in [("x^2 + 3", 3.0), ("sin(x)/x", 1.0), ("exp(x)", 1.0)] {
        match limit_typed(expression, "x", 0.0) {
            MathResult::Approximate {
                value,
                error_estimate,
            } => {
                assert!((value - expected).abs() < 1e-8, "{expression}: {value}");
                assert!(error_estimate.is_finite() && error_estimate < 1e-4);
            }
            result => panic!("expected a finite limit for {expression}, got {result:?}"),
        }
    }
}

#[test]
fn expands_a_nonnegative_integer_power() {
    let expanded = expand("(x+1)^2").expect("a small polynomial power must expand");
    assert!(
        !expanded.contains('^'),
        "power was not expanded: {expanded}"
    );
    for x in [-2.0, 0.0, 3.0] {
        assert_eq!(
            eval(&expanded, x),
            eval("(x+1)^2", x),
            "expanded expression changed value at x={x}: {expanded}"
        );
    }

    let moderate = expand("(x+1)^6").expect("ordinary powers must remain within budget");
    assert_eq!(eval(&moderate, 1.0), 64.0);

    assert_eq!(expand("x^0").unwrap(), "x ^ 0");
    assert_eq!(expand("(1/x)^0").unwrap(), "(1 / x) ^ 0");
}

#[test]
fn expansion_preserves_zero_product_domain_holes() {
    for source in ["0*(1/x)", "(1/x)*0"] {
        let expanded = expand(source).expect("the bounded expression must expand");

        assert!(!eval(source, 0.0).is_finite());
        assert!(
            !eval(&expanded, 0.0).is_finite(),
            "expansion erased the x=0 domain hole: {expanded}"
        );
        assert_eq!(eval(&expanded, 2.0), 0.0);
    }

    assert_eq!(expand("0*x").unwrap(), "0");
}

#[test]
fn bounds_factor_and_power_expansion() {
    let factors = (1..=14)
        .map(|constant| format!("(x+{constant})"))
        .collect::<Vec<_>>()
        .join("*");

    for expression in [factors.as_str(), "(x+1)^20"] {
        let error = expand(expression)
            .expect_err("expansion must fail instead of materializing an oversized result");
        assert!(error.contains("budget"), "unexpected error: {error}");
    }
}
