#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Test que el parser maneja y² y z².

use grafito_geometry::expr::prepare_function_ast;
use std::collections::BTreeMap;

#[test]
fn test_y2_with_explicit_substitution() {
    // Después de replace, el texto no debería tener y².
    let result = prepare_function_ast("y^2", &BTreeMap::new(), &["y"]);
    let v = result.unwrap().eval_at("y", 3.0);
    assert_eq!(v, 9.0);
}

#[test]
fn test_unicode_squared_in_user_input() {
    // Después del fix, y² se reemplaza por y^2 antes de parsear.
    let result = prepare_function_ast("y²", &BTreeMap::new(), &["y"]);
    assert!(result.is_ok(), "y² debería parsearse como y^2");
    let v = result.unwrap().eval_at("y", 3.0);
    assert_eq!(v, 9.0);
}

#[test]
fn test_x2_works() {
    // x² se reemplaza por x^2 antes de parsear.
    let result = prepare_function_ast("x²", &BTreeMap::new(), &["x"]);
    assert!(result.is_ok());
    let v = result.unwrap().eval_at("x", 3.0);
    assert_eq!(v, 9.0);
}

#[test]
fn test_x_squared_plus_y_squared_works() {
    // x² + y² = 9 debe parsear correctamente.
    let result = prepare_function_ast("x² + y²", &BTreeMap::new(), &["x", "y"]);
    assert!(result.is_ok());
    let v = result.unwrap().eval_2d("x", 0.0, "y", 3.0);
    assert_eq!(v, 9.0);
}
