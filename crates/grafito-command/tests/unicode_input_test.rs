#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Verifica que process_input maneja correctamente la entrada Unicode
//! que el usuario podría escribir (x², π, etc.).

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject, RelationOperator};

#[test]
fn test_x_squared_y_squared_eq_1() {
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "x² + y² = 1".to_string());
    println!("outcome: {:?}", outcome);
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some());
    let ic = ic.unwrap();
    assert_eq!(ic.expr_lhs, "x^2 + y^2", "x² se debe convertir a x^2");
    assert_eq!(ic.expr_rhs, "1");
    assert_eq!(ic.operator, RelationOperator::Eq);
}

#[test]
fn test_x_squared_y_squared_lt_1() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "x² + y² < 1".to_string());
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some());
    let ic = ic.unwrap();
    assert_eq!(ic.expr_lhs, "x^2 + y^2");
    assert_eq!(ic.expr_rhs, "1");
    assert_eq!(ic.operator, RelationOperator::Less);
}

#[test]
fn test_pi_in_expr() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "x^2 + y^2 = π".to_string());
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some());
    let ic = ic.unwrap();
    assert_eq!(ic.expr_rhs, "pi", "π se debe convertir a pi");
}

#[test]
fn unicode_natural_integral_definition_creates_a_plottable_accumulated_function() {
    let mut document = Document::new();
    let outcome = process_input(&mut document, &mut "f(x): ∫e−x2dx".to_string());

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    let function = document
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::Function(function) if function.label == "f" => Some(function),
            _ => None,
        });
    let function = function.expect("natural integral must create f");
    assert_eq!(function.expr, "exp(-x^2)");
    assert!(function.is_integral);
    assert_eq!(function.integral_var, "x");
    assert_eq!(function.integral_lower, 0.0);
    assert!(
        !document.variables.contains_key("dx"),
        "the differential must not become a document variable"
    );
    let samples = grafito_core::function_sampling::samples_or_compute(
        function,
        (-1.0, 1.0),
        96,
        &document.variables,
    );
    assert!(
        samples
            .iter()
            .any(|(_, value)| value.is_some_and(f64::is_finite)),
        "the accumulated integral must yield finite samples for the renderer"
    );
}

#[test]
fn natural_integral_without_a_differential_fails_atomically() {
    let mut document = Document::new();
    document.set_variable("baseline".into(), 7.0);
    let before = (serde_json::to_value(&document).unwrap(), document.version);
    let outcome = process_input(&mut document, &mut "f(x): ∫e−x2".to_string());

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(
        (serde_json::to_value(&document).unwrap(), document.version),
        before,
        "an incomplete natural integral must not create a malformed function or variables"
    );
}
