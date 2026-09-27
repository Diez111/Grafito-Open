#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Verifica si evalexpr maneja y².

#[test]
fn test_eval_y_squared() {
    // Vía `expr::evaluate` (camino distinto a `prepare_function_ast`
    // cubierto en `y_squared_support.rs:16`): y² con y=3 debe dar 9.
    let result = grafito_geometry::expr::evaluate("y²", &[("y".to_string(), 3.0)]);
    assert_eq!(result, Ok(9.0), "evaluate(y², y=3) debe ser 9.0");
}
