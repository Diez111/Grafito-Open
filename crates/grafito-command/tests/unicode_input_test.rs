//! Verifica que process_input maneja correctamente la entrada Unicode
//! que el usuario podría escribir (x², π, etc.).

use grafito_command::commands::process_input;
use grafito_core::{Document, GeoObject, ImplicitCurveObj, RelationOperator};

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
