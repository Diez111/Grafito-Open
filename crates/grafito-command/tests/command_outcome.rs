#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

#[test]
fn test_unknown_command_returns_error() {
    let mut doc = Document::new();
    let mut input = "@@@not_a_command@@@".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "unrecognized command should produce an error, got {:?}",
        outcome
    );
}

#[test]
fn test_distance_with_missing_objects_returns_error() {
    let mut doc = Document::new();
    let mut input = "Distance[A, B]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "Distance with missing objects should error, got {:?}",
        outcome
    );
}

#[test]
fn test_point_command_succeeds() {
    let mut doc = Document::new();
    let mut input = "(1, 2)".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "valid point command should succeed, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 1);
}

#[test]
fn test_cas_derivative_returns_message() {
    let mut doc = Document::new();
    let mut input = "Derivative[x^2, x]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Derivative should return a message, got {:?}",
        outcome
    );
}

#[test]
fn cas_success_text_may_contain_error_identifier() {
    let mut doc = Document::new();
    let mut input = "Derivative[error*x, x]".to_string();

    let outcome = process_input(&mut doc, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("error")),
        "a valid identifier must not determine the outcome type: {outcome:?}"
    );
}

#[test]
fn implicit_curve_folium_of_descartes() {
    let mut doc = Document::new();
    let mut input = "ImplicitCurve[x^3 + y^3 - 3*x*y = 0]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Folium of Descartes should succeed, got {:?}",
        outcome
    );
    let obj = doc
        .objects_iter()
        .find(|(_, obj)| matches!(obj, GeoObject::ImplicitCurve(_)));
    assert!(obj.is_some(), "should have created ImplicitCurve object");
    if let Some((_, GeoObject::ImplicitCurve(ic))) = obj {
        assert_eq!(ic.expr_lhs, "x^3 + y^3 - 3*x*y", "LHS should be correct");
        assert_eq!(ic.expr_rhs, "0", "RHS should be 0");
    }
}

#[test]
fn raw_equation_implicit_curve() {
    let mut doc = Document::new();
    let mut input = "x^3 + y^3 - 3*x*y = 0".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "raw equation should succeed, got {:?}",
        outcome
    );
    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::ImplicitCurve(_))));
}

#[test]
fn implicit_curve_with_leq_operator() {
    let mut doc = Document::new();
    let mut input = "ImplicitCurve[x^2 + y^2 <= 4]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "implicit curve with <= should succeed, got {:?}",
        outcome
    );
}

#[test]
fn standalone_eq_in_tokenizer() {
    // Verify that a standalone = in an expression doesn't cause parse errors
    use grafito_core::Document;
    let mut doc = Document::new();
    let mut input = "x^2 + y^2 = 4".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "equation with = should create implicit curve, got {:?}",
        outcome
    );
}

#[test]
fn test_lorenz_named_params() {
    let mut doc = Document::new();
    let mut input = "Lorenz[sigma=10, rho=28, beta=8/3]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Message(ref m) if m.contains("Lorenz")),
        "Lorenz with named params should succeed, got {:?}",
        outcome
    );
}

#[test]
fn test_lorenz_positional_params() {
    let mut doc = Document::new();
    let mut input = "Lorenz[10, 28, 8/3]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Message(ref m) if m.contains("Lorenz")),
        "Lorenz with positional params should succeed, got {:?}",
        outcome
    );
    let beta = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::Attractor3D(at) => at.params.get(2).copied(),
        _ => None,
    });
    assert_eq!(beta, Some(8.0 / 3.0));
}

#[test]
fn documented_optional_attractor_parameters_reach_the_existing_handlers() {
    for (command, attractor_type, expected_params) in [
        (
            "Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]",
            "aizawa",
            &[0.95, 0.7, 0.6, 3.5, 0.25, 0.1][..],
        ),
        ("Chen[35, 3, 28]", "chen", &[35.0, 3.0, 28.0][..]),
        (
            "Halvorsen[1.4, 0, 0, 0]",
            "halvorsen",
            &[1.4, 0.0, 0.0, 0.0][..],
        ),
        (
            "Dadras[3, 2.7, 1.7, 2, 9]",
            "dadras",
            &[3.0, 2.7, 1.7, 2.0, 9.0][..],
        ),
        (
            "Chua[15.6, 28, -1.143, -0.714]",
            "chua",
            &[15.6, 28.0, -1.143, -0.714][..],
        ),
    ] {
        let mut document = Document::new();
        let mut input = command.to_string();
        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "{command}: {outcome:?}"
        );
        let params = document
            .objects_iter()
            .find_map(|(_, object)| match object {
                GeoObject::Attractor3D(attractor) if attractor.attractor_type == attractor_type => {
                    Some(attractor.params.as_slice())
                }
                _ => None,
            });
        assert_eq!(params, Some(expected_params), "{command}");
    }
}

#[test]
fn curve3d_command_creates_parametric_curve_object() {
    let mut doc = Document::new();
    let mut input = "Curve3D[(cos(t), sin(t), t), t, 0, 2*pi]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "Curve3D should succeed, got {:?}",
        outcome
    );
    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::ParametricCurve3D(_))));
    assert!(!doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Segment3D(_))));
}
