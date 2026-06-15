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
}
