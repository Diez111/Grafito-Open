#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::str::FromStr;

use grafito_geometry::{
    assumptions::{Assumption, Assumptions},
    ast::{parse_ast, Expr},
    exact::{ExactRational, ExactRationalError},
    symbolic::{evaluate_exact_rational, simplify_with_assumptions, SimplificationOutcome},
};

#[test]
fn exact_rationals_normalize_parse_display_and_compare() {
    let normalized = ExactRational::new(-6, -8).unwrap();
    assert_eq!(normalized.numerator(), 3);
    assert_eq!(normalized.denominator(), 4);
    assert_eq!(normalized.to_string(), "3/4");
    assert_eq!(ExactRational::new(0, -9).unwrap().to_string(), "0");
    assert_eq!(
        ExactRational::from_str(" -12 / 18 ").unwrap().to_string(),
        "-2/3"
    );
    assert!(ExactRational::new(-1, 2).unwrap() < ExactRational::new(0, 1).unwrap());
    assert!(ExactRational::new(i128::MAX, i128::MAX).unwrap() > ExactRational::zero());
}

#[test]
fn exact_rationals_perform_checked_arithmetic() {
    let one_sixth = ExactRational::from_str("1/6").unwrap();
    let one_fourth = ExactRational::from_str("1/4").unwrap();

    assert_eq!(
        one_sixth.checked_add(one_fourth).unwrap().to_string(),
        "5/12"
    );
    assert_eq!(
        one_sixth.checked_sub(one_fourth).unwrap().to_string(),
        "-1/12"
    );
    assert_eq!(
        one_sixth
            .checked_mul(ExactRational::new(9, 5).unwrap())
            .unwrap()
            .to_string(),
        "3/10"
    );
    assert_eq!(
        one_sixth
            .checked_div(ExactRational::new(2, 3).unwrap())
            .unwrap()
            .to_string(),
        "1/4"
    );
}

#[test]
fn exact_rationals_report_domain_parse_and_overflow_errors() {
    assert_eq!(
        ExactRational::new(1, 0),
        Err(ExactRationalError::ZeroDenominator)
    );
    assert_eq!(
        ExactRational::from_str("1/2/3"),
        Err(ExactRationalError::InvalidFormat)
    );
    assert_eq!(
        ExactRational::one().checked_div(ExactRational::zero()),
        Err(ExactRationalError::DivisionByZero)
    );
    assert_eq!(
        ExactRational::new(i128::MAX, 1)
            .unwrap()
            .checked_add(ExactRational::one()),
        Err(ExactRationalError::Overflow)
    );
    assert_eq!(
        ExactRational::new(1, i128::MIN),
        Err(ExactRationalError::Overflow)
    );
}

#[test]
fn assumptions_propagate_safe_numeric_facts() {
    let mut assumptions = Assumptions::new();
    assumptions.assume_integer("n");
    assumptions.assume_positive("p");
    assumptions.assume_complex("z");

    assert!(assumptions.is_integer("n"));
    assert!(assumptions.is_real("n"));
    assert!(assumptions.is_complex("n"));
    assert!(assumptions.is_positive("p"));
    assert!(assumptions.is_nonzero("p"));
    assert!(assumptions.is_real("p"));
    assert!(assumptions.is_complex("z"));
    assert!(!assumptions.is_real("z"));
}

#[test]
fn exact_symbolic_evaluation_uses_rationals_without_f64_rounding() {
    assert_eq!(
        evaluate_exact_rational("7/6 + 5/6").unwrap(),
        Some(ExactRational::new(2, 1).unwrap())
    );
    assert_eq!(
        evaluate_exact_rational("1/3 + 1/6").unwrap(),
        Some(ExactRational::new(1, 2).unwrap())
    );
    assert_eq!(evaluate_exact_rational("x + 1"), Ok(None));
}

#[test]
fn conditional_simplification_preserves_x_over_x_domain() {
    let expression = parse_ast("x/x").unwrap();
    let assumptions = Assumptions::new();

    match simplify_with_assumptions(&expression, &assumptions) {
        SimplificationOutcome::Conditional(result) => {
            assert_eq!(result.expression, Expr::Const(1.0));
            assert!(result.conditions.contains(&Assumption::NonZero("x".into())));
        }
        other => panic!("expected a conditional result, got {other:?}"),
    }

    let mut nonzero_x = Assumptions::new();
    nonzero_x.assume_nonzero("x");
    assert_eq!(
        simplify_with_assumptions(&expression, &nonzero_x),
        SimplificationOutcome::Unconditional(Expr::Const(1.0))
    );
}

#[test]
fn conditional_simplification_preserves_zero_over_x_and_zero_exponent_domains() {
    let assumptions = Assumptions::new();

    for source in ["0/x", "x^0"] {
        match simplify_with_assumptions(&parse_ast(source).unwrap(), &assumptions) {
            SimplificationOutcome::Conditional(result) => {
                assert_eq!(
                    result.expression,
                    Expr::Const(if source == "0/x" { 0.0 } else { 1.0 })
                );
                assert!(result.conditions.contains(&Assumption::NonZero("x".into())));
            }
            other => panic!("expected a conditional result for {source}, got {other:?}"),
        }
    }

    assert_eq!(
        simplify_with_assumptions(&parse_ast("0^0").unwrap(), &assumptions),
        SimplificationOutcome::Unconditional(parse_ast("0^0").unwrap())
    );
}
