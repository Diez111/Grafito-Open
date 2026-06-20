//! Test end-to-end: el command processor + render del fill.

use grafito_command::commands::process_input;
use grafito_core::{Document, GeoObject, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_lt_disk_end_to_end() {
    // El usuario escribe "x² + y² < 1".
    let mut doc = Document::new();
    process_input(&mut doc, &mut "x² + y² < 1".to_string());

    // El fill del disco debe estar habilitado.
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
    assert!(ic.fill_color.is_some());
    let fill = ic.fill_color.unwrap();
    // El fill debe ser claramente visible (alpha >= 0.3).
    assert!(
        fill.a >= 0.3,
        "fill alpha debe ser >= 0.3 para ser visible, es {}",
        fill.a
    );

    // El AST debe parsearse correctamente.
    let (lhs, rhs) = ic.get_cached_asts(&HashMap::new(), &["x", "y"]).unwrap();
    let l = lhs.eval_2d("x", 0.0, "y", 0.0);
    let r = rhs.eval_2d("x", 0.0, "y", 0.0);
    assert_eq!(l, 0.0);
    assert_eq!(r, 1.0);
    assert!(l - r < 0.0, "centro debe estar dentro");
}

#[test]
fn test_gt_disk_end_to_end() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "x² + y² > 1".to_string());
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(ic.is_some());
    let ic = ic.unwrap();
    assert_eq!(ic.operator, RelationOperator::Greater);
    let (lhs, rhs) = ic.get_cached_asts(&HashMap::new(), &["x", "y"]).unwrap();
    // Para > 1, swap=true. f(0, 0) = rhs - lhs = 1 - 0 = 1 > 0 (fuera).
    // f(2, 0) = 1 - 4 = -3 < 0 (dentro).
    let l_00 = lhs.eval_2d("x", 0.0, "y", 0.0);
    let r_00 = rhs.eval_2d("x", 0.0, "y", 0.0);
    let f_00 = r_00 - l_00; // swap=true
    assert!(f_00 > 0.0, "(0,0) debe estar fuera (>1)");
    let l_20 = lhs.eval_2d("x", 2.0, "y", 0.0);
    let r_20 = rhs.eval_2d("x", 2.0, "y", 0.0);
    let f_20 = r_20 - l_20; // swap=true
    assert!(f_20 < 0.0, "(2,0) debe estar dentro (>1)");
}
