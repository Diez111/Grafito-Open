//! Verifica si evalexpr maneja y².

#[test]
fn test_eval_y_squared() {
    use std::collections::HashMap;
    let mut vars = HashMap::new();
    vars.insert("y".to_string(), 3.0);
    let result = grafito_geometry::expr::evaluate("y²", &[("y".to_string(), 3.0)]);
    println!("y² at 3.0: {:?}", result);
    // No debe panicar. Puede ser Ok o Err.
}
