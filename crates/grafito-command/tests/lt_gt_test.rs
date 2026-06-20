//! Verifica que el command processor maneja correctamente los operadores
//! <, >, <=, >= para ImplicitCurve con Unicode.

use grafito_command::commands::process_input;
use grafito_core::{Document, GeoObject, RelationOperator};

fn check(text: &str, expected_op: RelationOperator) {
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut text.to_string());
    println!("input: {:?} -> outcome: {:?}", text, outcome);
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    let ic = ic.unwrap_or_else(|| panic!("no se creó ImplicitCurve para {:?}", text));
    assert_eq!(ic.expr_lhs, "x^2 + y^2", "lhs para {:?}", text);
    assert_eq!(ic.expr_rhs, "1", "rhs para {:?}", text);
    assert_eq!(ic.operator, expected_op, "operator para {:?}", text);
}

#[test]
fn test_x2_y2_lt_1() {
    check("x² + y² < 1", RelationOperator::Less);
}

#[test]
fn test_x2_y2_gt_1() {
    check("x² + y² > 1", RelationOperator::Greater);
}

#[test]
fn test_x2_y2_le_1() {
    check("x² + y² <= 1", RelationOperator::LessEq);
}

#[test]
fn test_x2_y2_ge_1() {
    check("x² + y² >= 1", RelationOperator::GreaterEq);
}

#[test]
fn test_x2_y2_eq_1() {
    check("x² + y² = 1", RelationOperator::Eq);
}
