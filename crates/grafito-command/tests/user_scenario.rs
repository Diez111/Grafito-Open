//! Reproduce los escenarios exactos del usuario.

use grafito_command::commands::process_input;
use grafito_core::{Document, GeoObject, ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_user_scenario_eq_circle() {
    // El usuario escribe "x^2 + y^2 = 1" y espera ver el círculo.
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "x^2 + y^2 = 1".to_string());
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
    println!("expr_lhs: {}", ic.expr_lhs);
    println!("expr_rhs: {}", ic.expr_rhs);
    println!("operator: {:?}", ic.operator);
    println!("fill_color: {:?}", ic.fill_color);
    assert_eq!(ic.expr_lhs, "x^2 + y^2");
    assert_eq!(ic.expr_rhs, "1");
    assert_eq!(ic.operator, RelationOperator::Eq);
    // El fill_color debe estar presente (por default) pero el render no
    // lo usará porque es Eq.
    assert!(ic.fill_color.is_some());
}

#[test]
fn test_user_scenario_lt_disk() {
    // El usuario escribe "x^2 + y^2 < 1" y espera ver el disco relleno.
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "x^2 + y^2 < 1".to_string());
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
    assert_eq!(ic.operator, RelationOperator::Less);
    assert!(ic.fill_color.is_some());
}

#[test]
fn test_user_scenario_x_squared_y_squared() {
    // El usuario escribe "x^2 + y^2" sin operador.
    // El command processor debería tratarlo como Eq (default).
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "x^2 + y^2".to_string());
    println!("outcome: {:?}", outcome);
    // El processor crea un Function o un ImplicitCurve.
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    let f = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::Function(f) = o {
            Some(f)
        } else {
            None
        }
    });
    println!("IC: {:?}", ic.is_some());
    println!("Function: {:?}", f.is_some());
}
