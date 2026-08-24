#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_core::{function_sampling, FunctionObj};
use std::collections::HashMap;

fn samples(expr: &str) -> Vec<(f64, Option<f64>)> {
    samples_with_grid(expr, 100)
}

fn samples_with_grid(expr: &str, grid_size: usize) -> Vec<(f64, Option<f64>)> {
    samples_in_domain(expr, (-1.0, 1.0), grid_size, &HashMap::new())
}

fn samples_in_domain(
    expr: &str,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> Vec<(f64, Option<f64>)> {
    let function = FunctionObj::new(expr);
    let samples =
        function_sampling::samples_or_compute(&function, domain, grid_size, variables).clone();
    samples
}

fn has_finite_segment_across(samples: &[(f64, Option<f64>)], x: f64) -> bool {
    samples.windows(2).any(|pair| {
        let ((left_x, left_y), (right_x, right_y)) = (pair[0], pair[1]);
        left_x < x && x < right_x && left_y.is_some() && right_y.is_some()
    })
}

#[test]
fn cpu_sampling_retains_finite_large_positive_and_negative_constants() {
    for (expr, expected) in [("1000000", 1_000_000.0), ("-1000000", -1_000_000.0)] {
        let values = samples(expr);

        assert!(!values.is_empty());
        assert!(values.iter().all(|(_, value)| {
            value.is_some_and(|value| value.is_finite() && (value - expected).abs() < f64::EPSILON)
        }));
    }
}

#[test]
fn cpu_sampling_retains_finite_values_beyond_f32_range() {
    let values = samples("10^100");

    assert!(values.iter().all(|(_, value)| {
        value.is_some_and(|value| value.is_finite() && value.abs() > f32::MAX as f64)
    }));
}

#[test]
fn cpu_sampling_marks_non_finite_results_as_gaps() {
    let values = samples("1/0");

    assert!(values.iter().all(|(_, value)| value.is_none()));
}

#[test]
fn cpu_sampling_marks_invalid_bessel_orders_as_gaps_without_order_zero_points() {
    for expr in [
        "besselj(0/0, x)",
        "bessely(1.5, x)",
        "besseli(1/0, x)",
        "besselj(1001, x)",
        "bessely(-2147483648, x)",
    ] {
        let values = samples_in_domain(expr, (1.0, 2.0), 32, &HashMap::new());
        assert!(
            values.iter().all(|(_, value)| value.is_none()),
            "{expr} produced a finite plot point: {values:?}"
        );
    }

    let dynamic_invalid = HashMap::from([("n".to_string(), 1.5)]);
    let values = samples_in_domain("besselj(n, x)", (1.0, 2.0), 32, &dynamic_invalid);
    assert!(values.iter().all(|(_, value)| value.is_none()));

    let dynamic_valid = HashMap::from([("n".to_string(), 2.0)]);
    let values = samples_in_domain("besselj(n, x)", (1.0, 2.0), 32, &dynamic_valid);
    assert!(values
        .iter()
        .all(|(_, value)| value.is_some_and(f64::is_finite)));
}

#[test]
fn cpu_sampling_preserves_valid_bessel_curves_in_high_precision_mode() {
    use grafito_geometry::precision::{is_high_precision_mode, set_high_precision_mode};

    let previous_mode = is_high_precision_mode();
    set_high_precision_mode(true);
    let valid_samples = [
        "besselj(2, x)",
        "bessely(2, x)",
        "besseli(2, x)",
        "besselj(2, tan(x))",
        "piecewise(x < 0, besselj(x + 1e-20, x), besselj(2, tan(x)))",
        "piecewise(gamma(x) < 0, besselj(x + 1e-20, x), besselj(2, tan(x)))",
    ]
    .map(|expr| samples_in_domain(expr, (1.0, 2.0), 32, &HashMap::new()));
    // This differs from an integer only in DD precision. A f64 fallback would
    // incorrectly round it to the current x value and produce finite samples.
    let invalid_samples = [
        "besselj(x + 1e-20, x)",
        "bessely(x + 1e-20, x)",
        "besseli(x + 1e-20, x)",
    ]
    .map(|expr| samples_in_domain(expr, (1.0, 2.0), 32, &HashMap::new()));
    set_high_precision_mode(previous_mode);

    assert!(
        valid_samples.iter().all(|values| values
            .iter()
            .all(|(_, value)| value.is_some_and(f64::is_finite))),
        "valid Bessel samples became gaps in high precision mode: {valid_samples:?}"
    );
    assert!(invalid_samples
        .iter()
        .all(|values| values.iter().all(|(_, value)| value.is_none())));
}

#[test]
fn cpu_sampling_supports_mathematica_style_function_call_brackets() {
    let values = samples("Sin[x] / (x^2 + 1)");

    assert!(!values.is_empty());
    assert!(values.iter().all(|(_, value)| {
        value.is_some_and(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
    }));
}

#[test]
fn cpu_sampling_does_not_refine_an_expression_that_cannot_compile() {
    let values = samples_with_grid("Sin[x", 10);

    assert_eq!(values, vec![(-2.0, None), (2.0, None)]);
}

#[test]
fn cpu_sampling_does_not_bridge_a_reciprocal_pole() {
    let values = samples("1/x");

    assert!(values
        .iter()
        .any(|(x, value)| x.abs() < f64::EPSILON && value.is_none()));
    assert!(!has_finite_segment_across(&values, 0.0));
}

#[test]
fn cpu_sampling_refines_finite_high_frequency_functions() {
    let linear = samples_with_grid("x", 32);
    let high_frequency = samples_with_grid("sin(100*x)", 32);

    assert!(
        high_frequency.len() > linear.len() * 4,
        "expected dense adaptive samples, got {} high-frequency versus {} linear",
        high_frequency.len(),
        linear.len()
    );
    assert!(high_frequency
        .iter()
        .all(|(x, y)| x.is_finite() && y.is_some_and(f64::is_finite)));

    let visible_samples: Vec<_> = high_frequency
        .iter()
        .copied()
        .filter(|(x, _)| (-1.0..=1.0).contains(x))
        .collect();
    let largest_gap = visible_samples
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].0)
        .fold(0.0_f64, f64::max);
    assert!(
        largest_gap <= 0.02,
        "sin(100*x) retained a visibly long segment of width {largest_gap}"
    );
}

#[test]
fn cpu_sampling_does_not_bridge_tangent_poles_without_exact_samples() {
    let values = samples_with_grid("tan(x)", 64);

    assert!(!has_finite_segment_across(
        &values,
        -std::f64::consts::FRAC_PI_2
    ));
    assert!(!has_finite_segment_across(
        &values,
        std::f64::consts::FRAC_PI_2
    ));
}

#[test]
fn cpu_sampling_does_not_oversample_a_linear_function() {
    let values = samples_with_grid("x", 32);

    assert!(values.len() <= 33);
}

#[test]
fn cpu_sampling_caps_adaptive_work_for_unresolvable_frequencies() {
    let values = samples_with_grid("sin(10000*x)", 200);

    assert!(values.len() <= function_sampling::MAX_SAMPLES_PER_FUNCTION);
}
