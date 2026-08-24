#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_complex::parse_complex;
use num_complex::Complex64;
use std::collections::HashMap;

fn eval(input: &str) -> Complex64 {
    parse_complex(input)
        .expect("zeta expression should parse")
        .eval(&HashMap::new())
        .expect("zeta expression should evaluate")
}

fn assert_complex_close(actual: Complex64, expected: Complex64, tolerance: f64) {
    let error = (actual - expected).norm();
    assert!(
        error <= tolerance,
        "expected {expected:?}, got {actual:?} (error {error:e}, tolerance {tolerance:e})"
    );
}

#[test]
fn zeta_two_matches_basel_sum() {
    assert_complex_close(
        eval("zeta(2)"),
        Complex64::new(std::f64::consts::PI.powi(2) / 6.0, 0.0),
        2e-12,
    );
}

#[test]
fn negative_even_integer_is_exact_trivial_zero() {
    assert_eq!(eval("zeta(-2)"), Complex64::new(0.0, 0.0));
}

#[test]
fn negative_odd_integer_uses_analytic_continuation() {
    assert_complex_close(eval("zeta(-1)"), Complex64::new(-1.0 / 12.0, 0.0), 2e-13);
}

#[test]
fn critical_strip_value_matches_high_precision_oracle() {
    // mpmath, 80 decimal digits: zeta(0.5 + 14 i).
    let expected = Complex64::new(0.022_241_142_609_993_59, -0.103_258_123_266_450_06);
    assert_complex_close(eval("zeta(0.5 + 14i)"), expected, 2e-10);
}

#[test]
fn zeta_commutes_with_complex_conjugation() {
    let expr = parse_complex("zeta(z)").expect("zeta expression should parse");
    let z = Complex64::new(-0.75, 7.5);
    let value = expr
        .eval(&HashMap::from([("z".to_string(), z)]))
        .expect("zeta(z) should evaluate");
    let conjugate_value = expr
        .eval(&HashMap::from([("z".to_string(), z.conj())]))
        .expect("zeta(conj(z)) should evaluate");

    assert_complex_close(conjugate_value, value.conj(), 1e-12);
}

#[test]
fn zeta_pole_uses_nonfinite_special_function_convention() {
    let value = eval("zeta(1)");
    assert!(value.re.is_nan() && value.im.is_nan(), "got {value:?}");
}

#[test]
fn zeta_rejects_nonfinite_arguments_with_nan_pair() {
    let expr = parse_complex("zeta(z)").expect("zeta expression should parse");
    for z in [
        Complex64::new(f64::NAN, 0.0),
        Complex64::new(f64::INFINITY, 0.0),
        Complex64::new(0.0, f64::NEG_INFINITY),
    ] {
        let value = expr
            .eval(&HashMap::from([("z".to_string(), z)]))
            .expect("nonfinite special-function input should evaluate to NaN");
        assert!(
            value.re.is_nan() && value.im.is_nan(),
            "zeta({z:?}) returned {value:?}"
        );
    }
}
