#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{insert_implicit_multiplication, process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

#[test]
fn command_parser_rejects_trailing_text_atomically() {
    for command in [
        "Derivative[x^2,x] trailing",
        "l'hopital[sin(x),x,x,0] trailing",
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = (serde_json::to_value(&document).unwrap(), document.version);

        let outcome = run(&mut document, command);

        assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
        assert_eq!(
            (serde_json::to_value(&document).unwrap(), document.version),
            before,
            "{command} must be atomic"
        );
    }
}

#[test]
fn invalid_solve_expressions_are_errors_and_atomic() {
    for command in ["Solve[]", "Solve[unknown_function(x),x]"] {
        let mut document = Document::new();
        let before = (serde_json::to_value(&document).unwrap(), document.version);

        let outcome = run(&mut document, command);

        assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
        assert_eq!(
            (serde_json::to_value(&document).unwrap(), document.version),
            before,
            "{command} must be atomic"
        );
    }
}

#[test]
fn solve_accepts_supported_bracketed_math_calls() {
    let mut document = Document::new();

    let outcome = run(&mut document, "Solve[Sin[x],x,-1,1]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(document.object_count() > 0);
}

fn root_x_positions(document: &Document) -> Vec<f64> {
    let mut roots: Vec<_> = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Point(point) if point.label.starts_with("Raíz") => Some(point.position.x),
            _ => None,
        })
        .collect();
    roots.sort_by(|left, right| left.total_cmp(right));
    roots
}

#[test]
fn solve_is_scale_invariant_and_does_not_emit_false_root_objects() {
    let mut document = Document::new();

    let outcome = run(&mut document, "Solve[1e-15*x-1e-15,x,-2,2]");

    let CommandOutcome::Message(message) = outcome else {
        panic!("unexpected outcome: {outcome:?}");
    };
    assert!(
        message.len() < 512,
        "message grew to {} bytes",
        message.len()
    );
    assert_eq!(root_x_positions(&document), vec![1.0]);
    assert_eq!(document.object_count(), 2, "one graph plus one root");
}

#[test]
fn numeric_solve_uses_relative_residuals_for_scaled_functions() {
    let mut sine = Document::new();
    let outcome = run(&mut sine, "Solve[1e-15*sin(x),x,-4,4]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&sine);
    assert_eq!(roots.len(), 3, "{roots:?}");
    for (actual, expected) in roots
        .iter()
        .zip([-std::f64::consts::PI, 0.0, std::f64::consts::PI])
    {
        assert!((actual - expected).abs() < 1e-8, "{roots:?}");
    }

    let mut constant = Document::new();
    let outcome = run(&mut constant, "Solve[1e-15,x,-2,2]");
    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
        "{outcome:?}"
    );
    assert!(root_x_positions(&constant).is_empty());
    assert_eq!(constant.object_count(), 1, "only the graph is expected");
}

#[test]
fn scientific_notation_survives_implicit_multiplication() {
    assert_eq!(insert_implicit_multiplication("1e-15*x"), "1e-15*x");
    assert_eq!(insert_implicit_multiplication("1E+15x"), "1E+15*x");
    assert_eq!(insert_implicit_multiplication("2exp(x)"), "2*exp(x)");
}

#[test]
fn solve_does_not_create_roots_for_strictly_positive_functions() {
    for command in [
        "Solve[exp(x),x,-20,20]",
        "Solve[1e-15*(sin(x)^2+1e-12),x,-4,4]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
            "{command}: {outcome:?}"
        );
        assert!(root_x_positions(&document).is_empty(), "{command}");
        assert_eq!(document.object_count(), 1, "{command}: only the graph");
    }
}

#[test]
fn solve_rejects_invalid_variables_and_bounds_atomically() {
    for command in [
        "Solve[sin(x),x+1,-2,2]",
        "Solve[sin(x),x,NaN,1]",
        "Solve[sin(x),x,not_a_number,1]",
        "Solve[sin(x),x,2,-2]",
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = (serde_json::to_value(&document).unwrap(), document.version);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            (serde_json::to_value(&document).unwrap(), document.version),
            before,
            "{command} must be atomic"
        );
    }
}

#[test]
fn solve_resolves_document_values_and_enforces_the_requested_interval() {
    let mut bounded = Document::new();
    bounded.set_variable("limit".into(), 1.0);
    let outcome = run(&mut bounded, "Solve[sin(x-2),x,-limit,limit]");
    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
        "{outcome:?}"
    );
    assert!(root_x_positions(&bounded).is_empty());

    let mut polynomial = Document::new();
    let outcome = run(&mut polynomial, "Solve[x-10,x,-1,1]");
    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
        "{outcome:?}"
    );
    assert!(root_x_positions(&polynomial).is_empty());

    let mut parameterized = Document::new();
    parameterized.set_variable("k".into(), 2.5);
    let outcome = run(&mut parameterized, "Solve[k*x-5,x,-3,3]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(root_x_positions(&parameterized), vec![2.0]);
}

#[test]
fn subnormal_scaled_solve_keeps_root_accuracy() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[1e-320*sin(x),x,-4,4]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&document);
    assert_eq!(roots.len(), 3, "{roots:?}");
    for (actual, expected) in roots
        .iter()
        .zip([-std::f64::consts::PI, 0.0, std::f64::consts::PI])
    {
        assert!((actual - expected).abs() < 1e-8, "{roots:?}");
    }
}

#[test]
fn solve_scaling_survives_equation_and_neutral_wrappers() {
    for command in [
        "Solve[1e-320*sin(x)=0,x,-4,4]",
        "Solve[1e-320*sin(x)+0,x,-4,4]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        let roots = root_x_positions(&document);
        assert_eq!(roots.len(), 3, "{command}: {roots:?}");
        for (actual, expected) in
            roots
                .iter()
                .zip([-std::f64::consts::PI, 0.0, std::f64::consts::PI])
        {
            assert!((actual - expected).abs() < 1e-8, "{command}: {roots:?}");
        }
    }
}

#[test]
fn solve_keeps_endpoint_and_factored_even_roots() {
    let mut endpoint = Document::new();
    let outcome = run(&mut endpoint, "Solve[sin(x),x,0,1]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(root_x_positions(&endpoint), vec![0.0]);

    let mut even = Document::new();
    let outcome = run(&mut even, "Solve[(x-0.12345)^2,x,-1,1]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&even);
    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!(roots.iter().all(|root| (*root - 0.12345).abs() < 1e-10));
}

#[test]
fn solve_does_not_project_positive_quartics_onto_the_real_axis() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[x^4+2e-13*x^2+1e-26,x,-1,1]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let real_roots: Vec<_> = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Point(point)
                if point.label.starts_with("Raíz") && point.position.y == 0.0 =>
            {
                Some(point.position.x)
            }
            _ => None,
        })
        .collect();
    assert!(real_roots.is_empty(), "{outcome:?}");
}

#[test]
fn solve_infers_the_variable_from_the_expression_ast() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[sin(x)]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&document);
    assert!(roots.contains(&0.0), "{roots:?}");
}

#[test]
fn solve_inference_does_not_autodefine_unknowns() {
    let mut inferred = Document::new();
    let outcome = run(&mut inferred, "Solve[u-1]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(root_x_positions(&inferred), vec![1.0]);
    assert!(!inferred.variables.contains_key("u"));

    let mut explicit = Document::new();
    let outcome = run(&mut explicit, "Solve[u^2-1,u,-2,2]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(!explicit.variables.contains_key("u"));

    let mut parameterized = Document::new();
    parameterized.set_variable("k".into(), 2.5);
    let outcome = run(&mut parameterized, "Solve[k*u-5]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(root_x_positions(&parameterized), vec![2.0]);
    assert!(!parameterized.variables.contains_key("u"));
}

#[test]
fn solve_rejects_ambiguous_unknowns_and_undefined_bounds_atomically() {
    for command in [
        "Solve[u+v]",
        "Solve[u+v,u,-2,2]",
        "Solve[sin(x-2),x,-limit,limit]",
    ] {
        let mut document = Document::new();
        let before = (serde_json::to_value(&document).unwrap(), document.version);
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            (serde_json::to_value(&document).unwrap(), document.version),
            before,
            "{command} must be atomic"
        );
    }
}

#[test]
fn solve_finds_approximate_endpoint_and_nested_even_roots() {
    let mut endpoint = Document::new();
    let outcome = run(&mut endpoint, "Solve[sin(x),x,0,pi]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&endpoint);
    assert_eq!(roots.len(), 2, "{roots:?}");
    assert!((roots[1] - std::f64::consts::PI).abs() < 1e-10, "{roots:?}");

    let mut even = Document::new();
    let outcome = run(&mut even, "Solve[(sin(x)-0.1)^2*(x+2),x,-1,1]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&even);
    assert_eq!(roots.len(), 1, "{roots:?}");
    assert!((roots[0] - 0.1_f64.asin()).abs() < 1e-10, "{roots:?}");
}

#[test]
fn solve_does_not_publish_unvalidated_depressed_roots() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[1e-50*x^3+x^2+1,x,-2,2]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&document);
    assert_eq!(roots.len(), 2, "{outcome:?}");
    assert!(roots.iter().all(|root| root.abs() < 1e-8), "{roots:?}");
}

#[test]
fn solve_reports_identically_zero_equations_without_arbitrary_points() {
    for command in ["Solve[0*(x-1),x,-2,2]", "Solve[(1-1)*(x-2),x,-3,3]"] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Message(ref message) if message.contains("todos los valores")),
            "{command}: {outcome:?}"
        );
        assert!(root_x_positions(&document).is_empty(), "{command}");
    }
}

#[test]
fn solve_keeps_close_distinct_roots() {
    let mut document = Document::new();
    let outcome = run(
        &mut document,
        "Solve[(x-100000000)*(x-100000000.005),x,99999999,100000001]",
    );

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let roots = root_x_positions(&document);
    assert_eq!(roots.len(), 2, "{roots:?}");
    assert!((roots[0] - 100000000.0).abs() < 1e-8, "{roots:?}");
    assert!((roots[1] - 100000000.005).abs() < 1e-8, "{roots:?}");
}

#[test]
fn approximate_endpoint_root_must_remain_inside_the_interval() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[x+1e-15,x,0,1]");

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
        "{outcome:?}"
    );
    assert!(root_x_positions(&document).is_empty());
}

#[test]
fn analysis_commands_reject_non_finite_results() {
    for command in [
        "ArcLength[1/0,0,1]",
        "VolumeOfRevolution[1/0,0,1]",
        "SurfaceOfRevolution[1/0,0,1]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(document.object_count(), 0, "{command} must not mutate");
    }
}

#[test]
fn conservative_field_checks_require_defined_matching_partials() {
    let mut document = Document::new();

    for command in [
        "IsConservative[[NaN*y, NaN*x], [x, y]]",
        "IsConservative[[NaN, 0], [x, y]]",
        "IsConservative[[NaN, 0, 0], [x, y, z]]",
        "IsConservative[[0, floor(x)], [x, y]]",
    ] {
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Message(ref message) if message.contains("false")),
            "{command}: {outcome:?}"
        );
    }

    for command in [
        "IsConservative[[2*x*y*z, x^2*z, x^2*y], [x, y, z]]",
        "IsConservative[[atan(x+y), atan(x+y)], [x, y]]",
        "IsConservative[[tanh(x+y), tanh(x+y)], [x, y]]",
        "IsConservative[[asinh(x+y), asinh(x+y)], [x, y]]",
    ] {
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Message(ref message) if message.contains("true")),
            "{command}: {outcome:?}"
        );
    }
}

#[test]
fn discrete_distributions_reject_counts_above_the_shared_limit() {
    let commands = [
        "Binomial[10001, 0.5, 0]",
        "Binomial[1, 0.5, 10001]",
        "Poisson[1, 10001]",
        "NegBinomial[10001, 0.5, 0]",
        "NegBinomial[1, 0.5, 10001]",
    ];

    for command in commands {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains("maximum 10000")),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            document.object_count(),
            0,
            "{command} must not create objects"
        );
    }
}

#[test]
fn circle_expr_rejects_non_positive_and_non_finite_radii_before_creation() {
    for command in [
        "CircleExpr[(0, 0), 0]",
        "CircleExpr[(0, 0), -1]",
        "CircleExpr[(0, 0), NaN]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            document.object_count(),
            0,
            "{command} must not create a circle"
        );
    }
}

#[test]
fn circle_expr_preserves_a_valid_expression_bound_radius() {
    let mut document = Document::new();
    document.set_variable("radius".into(), 2.5);

    let outcome = run(&mut document, "CircleExpr[(0, 0), radius]");

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    let circle = document
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::Circle(circle) => Some(circle),
            _ => None,
        })
        .expect("CircleExpr should create a circle");
    assert_eq!(circle.radius_expr.as_deref(), Some("radius"));
    assert!((circle.radius - 2.5).abs() < f64::EPSILON);
}

#[test]
fn triple_integral_rejects_every_malformed_or_non_finite_bound() {
    let commands = [
        "TripleIntegral[1, x, 0/, 1, y, 0, 1, z, 0, 1, 2]",
        "TripleIntegral[1, x, 0, NaN, y, 0, 1, z, 0, 1, 2]",
        "TripleIntegral[1, x, 0, 1, y, 0/, 1, z, 0, 1, 2]",
        "TripleIntegral[1, x, 0, 1, y, 0, NaN, z, 0, 1, 2]",
        "TripleIntegral[1, x, 0, 1, y, 0, 1, z, 0/, 1, 2]",
        "TripleIntegral[1, x, 0, 1, y, 0, 1, z, 0, NaN, 2]",
    ];

    for command in commands {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            document.object_count(),
            0,
            "{command} must not create objects"
        );
    }
}

#[test]
fn rolle_check_is_inconclusive_across_a_discontinuity() {
    let mut document = Document::new();
    let outcome = run(&mut document, "RolleCheck[x^2 + 1/x^2, x, -1, 1]");

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("inconclusive")),
        "got {outcome:?}"
    );
}

#[test]
fn normal_rejects_non_positive_or_non_finite_sigma_without_creation() {
    for command in [
        "Normal[0, 0]",
        "Normal[0, -1]",
        "Normal[0, NaN]",
        "Normal[0, inf]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            document.object_count(),
            0,
            "{command} must not create a function"
        );
    }
}
