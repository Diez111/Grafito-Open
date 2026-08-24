#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_geometry::statistics::{
    chi_squared_cdf, chi_squared_pdf, fit_xy, negative_binomial_cdf, negative_binomial_pmf,
    student_t_cdf, student_t_quantile, FitKind,
};

#[test]
fn student_t_quantile_expands_its_bracket_for_cauchy_tails() {
    let p = 0.999_999;
    let expected = (std::f64::consts::PI * (p - 0.5)).tan();
    let upper = student_t_quantile(p, 1.0);
    let lower = student_t_quantile(1.0 - p, 1.0);

    assert!(upper.is_finite());
    assert!(
        (upper - expected).abs() < 0.01,
        "upper={upper}, expected={expected}"
    );
    assert!(
        (lower + expected).abs() < 0.01,
        "lower={lower}, expected={expected}"
    );
    assert!((student_t_cdf(upper, 1.0) - p).abs() < 1e-12);
    assert!(student_t_quantile(0.5, 1.0).abs() < 1e-12);
    assert!(student_t_quantile(0.0, 1.0).is_nan());
    assert!(student_t_quantile(1.0, 1.0).is_nan());
    assert!(student_t_quantile(f64::NAN, 1.0).is_nan());
    assert!(student_t_quantile(0.5, 0.0).is_nan());
}

#[test]
fn student_t_cauchy_extreme_tail_avoids_square_overflow() {
    let p = 1e-160;
    let expected_magnitude = 3.183_098_861_837_906_7e159;
    let quantile = student_t_quantile(p, 1.0);
    let lower_cdf = student_t_cdf(-expected_magnitude, 1.0);

    assert!(quantile.is_finite());
    assert!(
        ((quantile + expected_magnitude) / expected_magnitude).abs() < 2e-15,
        "quantile={quantile}"
    );
    assert!(((lower_cdf - p) / p).abs() < 2e-15, "cdf={lower_cdf}");
    assert_eq!(student_t_cdf(expected_magnitude, 1.0), 1.0);

    let smallest_finite_tail = student_t_cdf(-f64::MAX, 1.0);
    let expected_smallest_finite_tail = (1.0 / f64::MAX) / std::f64::consts::PI;
    assert!(smallest_finite_tail > 0.0);
    assert!(
        ((smallest_finite_tail - expected_smallest_finite_tail) / expected_smallest_finite_tail)
            .abs()
            < 1e-12,
        "cdf(-f64::MAX)={smallest_finite_tail}"
    );
}

#[test]
fn student_t_cauchy_extreme_quantiles_do_not_clamp_to_f64_max() {
    assert_eq!(student_t_quantile(1e-320, 1.0), f64::NEG_INFINITY);
    let min_positive_expected = -(std::f64::consts::PI * f64::MIN_POSITIVE).recip();
    let min_positive_quantile = student_t_quantile(f64::MIN_POSITIVE, 1.0);
    assert!(min_positive_quantile.is_finite());
    assert!(
        ((min_positive_quantile - min_positive_expected) / min_positive_expected).abs() < 2e-15,
        "quantile={min_positive_quantile}"
    );

    let tail = f64::EPSILON;
    let expected = 1.0 / (std::f64::consts::PI * tail).tan();
    let lower = student_t_quantile(tail, 1.0);
    let upper = student_t_quantile(1.0 - tail, 1.0);
    assert!(
        ((lower + expected) / expected).abs() < 2e-15,
        "lower={lower}"
    );
    assert!(
        ((upper - expected) / expected).abs() < 2e-15,
        "upper={upper}"
    );
}

#[test]
fn chi_squared_pdf_handles_zero_endpoint_by_degrees_of_freedom() {
    assert_eq!(chi_squared_pdf(0.0, 1.0), f64::INFINITY);
    assert_eq!(chi_squared_pdf(0.0, 2.0), 0.5);
    assert_eq!(chi_squared_pdf(0.0, 3.0), 0.0);
    assert_eq!(chi_squared_cdf(0.0, 2.0), 0.0);
}

#[test]
fn chi_squared_rejects_invalid_parameters_and_bounds_finite_results() {
    for k in [f64::NEG_INFINITY, -1.0, 0.0, f64::INFINITY, f64::NAN] {
        assert!(chi_squared_pdf(1.0, k).is_nan());
        assert!(chi_squared_cdf(1.0, k).is_nan());
    }
    assert!(chi_squared_pdf(f64::NAN, 2.0).is_nan());
    assert!(chi_squared_cdf(f64::NAN, 2.0).is_nan());

    for (x, k) in [(0.25, 1.0), (1.0, 2.0), (20.0, 12.0)] {
        let pdf = chi_squared_pdf(x, k);
        let cdf = chi_squared_cdf(x, k);
        assert!(pdf.is_finite() && pdf >= 0.0, "pdf({x}, {k})={pdf}");
        assert!(
            cdf.is_finite() && (0.0..=1.0).contains(&cdf),
            "cdf({x}, {k})={cdf}"
        );
    }
}

#[test]
fn negative_binomial_is_stable_for_large_balanced_counts() {
    let pmf = negative_binomial_pmf(200, 0.5, 200);
    let cdf = negative_binomial_cdf(200, 0.5, 200);

    assert!(pmf.is_finite() && (0.0..=1.0).contains(&pmf));
    assert!(cdf.is_finite() && (0.0..=1.0).contains(&cdf));
    assert!((pmf - 0.019_934_650_981_896).abs() < 1e-12, "pmf={pmf}");
    assert!((cdf - 0.519_934_650_981_896).abs() < 1e-12, "cdf={cdf}");
}

#[test]
fn negative_binomial_validates_parameters_and_handles_p_one() {
    for (r, p) in [(0, 0.5), (1, 0.0), (1, -0.1), (1, 1.1), (1, f64::NAN)] {
        assert!(negative_binomial_pmf(r, p, 0).is_nan());
        assert!(negative_binomial_cdf(r, p, 0).is_nan());
    }

    assert_eq!(negative_binomial_pmf(3, 1.0, 0), 1.0);
    assert_eq!(negative_binomial_pmf(3, 1.0, 1), 0.0);
    assert_eq!(negative_binomial_cdf(3, 1.0, 10), 1.0);
}

#[test]
fn local_fits_report_residuals_and_rmse_in_original_y_units() {
    let result =
        fit_xy(FitKind::Linear, &[0.0, 1.0, 2.0], &[1.0, 3.0, 5.0]).expect("linear data must fit");

    assert_eq!(result.kind, FitKind::Linear);
    assert_eq!(result.coefficients.len(), 2);
    assert!(result.x_scale > 0.0);
    assert_eq!(result.diagnostics.residuals.len(), 3);
    assert!(result
        .diagnostics
        .residuals
        .iter()
        .all(|residual| residual.abs() < 1e-12));
    assert!(result.diagnostics.rmse < 1e-12);
    assert!((result.diagnostics.r_squared - 1.0).abs() < 1e-12);
    assert!((result.predict(3.0) - 7.0).abs() < 1e-12);
}

#[test]
fn local_fits_normalize_extreme_x_scales_and_emit_round_trippable_expressions() {
    let tiny = fit_xy(FitKind::Linear, &[0.0, 1.0], &[1e-20, 2e-20])
        .expect("tiny finite coefficients must fit");
    let tiny_expression = tiny.expression();
    let evaluated = grafito_geometry::expr::evaluate(&tiny_expression, &[("x".to_string(), 0.25)])
        .expect("generated scientific notation must remain parseable");
    assert!(
        (evaluated - tiny.predict(0.25)).abs() < 1e-30,
        "{tiny_expression}"
    );

    let xs: Vec<f64> = (0..=7).map(|index| 1_000_000.0 + index as f64).collect();
    let ys: Vec<f64> = xs
        .iter()
        .map(|x| {
            let normalized = (x - 1_000_003.5) / 3.5;
            1.0 + 2.0 * normalized + 3.0 * normalized * normalized
        })
        .collect();
    let polynomial = fit_xy(FitKind::Polynomial { degree: 2 }, &xs, &ys)
        .expect("offset polynomial data must fit in normalized coordinates");
    assert!(polynomial.diagnostics.rmse < 1e-11);
    assert!((polynomial.predict(1_000_006.0) - ys[6]).abs() < 1e-10);
}

#[test]
fn local_fit_models_are_deterministic_and_reject_invalid_domains() {
    let polynomial = fit_xy(
        FitKind::Polynomial { degree: 2 },
        &[-1.0, 0.0, 1.0, 2.0],
        &[2.0, 1.0, 2.0, 5.0],
    )
    .expect("quadratic data must fit");
    assert!(polynomial.diagnostics.rmse < 1e-12);
    assert!((polynomial.predict(3.0) - 10.0).abs() < 1e-12);

    let exponential = fit_xy(
        FitKind::Exponential,
        &[0.0, 1.0, 2.0],
        &[
            2.0,
            2.0 * std::f64::consts::E,
            2.0 * std::f64::consts::E.powi(2),
        ],
    )
    .expect("positive exponential data must fit");
    assert!(exponential.diagnostics.rmse < 1e-10);

    let logarithmic = fit_xy(
        FitKind::Logarithmic,
        &[1.0, std::f64::consts::E, std::f64::consts::E.powi(2)],
        &[1.0, 3.0, 5.0],
    )
    .expect("positive logarithmic x values must fit");
    assert!(logarithmic.diagnostics.rmse < 1e-10);

    let power = fit_xy(FitKind::Power, &[1.0, 2.0, 3.0], &[2.0, 8.0, 18.0])
        .expect("positive power data must fit");
    assert!(power.diagnostics.rmse < 1e-10);

    let xs: Vec<f64> = (0..=32)
        .map(|index| std::f64::consts::TAU * index as f64 / 32.0)
        .collect();
    let ys: Vec<f64> = xs.iter().map(|x| 2.0 * x.sin() + 3.0).collect();
    let sinusoidal = fit_xy(FitKind::Sinusoidal, &xs, &ys)
        .expect("one full exact period must fit deterministically");
    assert!(sinusoidal.diagnostics.rmse < 1e-9);

    for (kind, xs, ys) in [
        (FitKind::Linear, vec![1.0, 1.0], vec![1.0, 2.0]),
        (FitKind::Exponential, vec![0.0, 1.0], vec![1.0, 0.0]),
        (FitKind::Logarithmic, vec![0.0, 1.0], vec![1.0, 2.0]),
        (FitKind::Power, vec![1.0, 2.0], vec![1.0, -2.0]),
    ] {
        assert!(
            fit_xy(kind, &xs, &ys).is_err(),
            "{kind:?} must reject its invalid domain"
        );
    }
}
