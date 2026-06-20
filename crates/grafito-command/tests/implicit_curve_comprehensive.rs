//! Tests de extremo a extremo: crear un ImplicitCurveObj a través
//! del command processor y verificar que el cache se llena.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{GeoObject, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_create_circle_eq_via_command() {
    let mut doc = grafito_core::Document::new();
    let mut input = "x^2 + y^2 = 1".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "process_input falló: {:?}",
        outcome
    );
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some(), "no se creó un ImplicitCurve");
    let ic = ic.unwrap();
    assert_eq!(ic.expr_lhs, "x^2 + y^2");
    assert_eq!(ic.expr_rhs, "1");
    assert_eq!(ic.operator, RelationOperator::Eq);
    // Verificar que el cache se puede llenar.
    let (lhs, rhs) = ic.get_cached_asts(&HashMap::new(), &["x", "y"]).unwrap();
    assert_eq!(lhs.eval_2d("x", 1.0, "y", 0.0), 1.0);
    assert_eq!(rhs.eval_2d("x", 1.0, "y", 0.0), 1.0);
}

#[test]
fn test_create_disk_lt_via_command() {
    let mut doc = grafito_core::Document::new();
    let mut input = "x^2 + y^2 < 1".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "process_input falló: {:?}",
        outcome
    );
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some(), "no se creó un ImplicitCurve");
    let ic = ic.unwrap();
    assert_eq!(ic.expr_lhs, "x^2 + y^2");
    assert_eq!(ic.expr_rhs, "1");
    assert_eq!(ic.operator, RelationOperator::Less);
    let (lhs, rhs) = ic.get_cached_asts(&HashMap::new(), &["x", "y"]).unwrap();
    // f(0,0) = 0 - 1 = -1, dentro.
    let l = lhs.eval_2d("x", 0.0, "y", 0.0);
    let r = rhs.eval_2d("x", 0.0, "y", 0.0);
    assert_eq!(l, 0.0);
    assert_eq!(r, 1.0);
    assert!(l - r < 0.0, "centro debe estar dentro del disco");
}

#[test]
fn test_create_explicit_implicit_curve_via_command() {
    let mut doc = grafito_core::Document::new();
    let mut input = "ImplicitCurve[x^2 + y^2, 1, <]".to_string();
    let outcome = process_input(&mut doc, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "process_input falló: {:?}",
        outcome
    );
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some());
}
