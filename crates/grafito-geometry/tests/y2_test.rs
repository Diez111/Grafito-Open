#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Verifica que el parser maneja `y²` (sin x antes).

use grafito_geometry::expr::prepare_function_ast;
use std::collections::BTreeMap;

// NOTE: el caso `y²` → 9.0 se cubre en
// `y_squared_support.rs:16` (`test_unicode_squared_in_user_input`);
// acá solo queda la variante mixta `x^2 + y²`.

#[test]
fn test_x2_plus_y2_parses() {
    let result = prepare_function_ast("x^2 + y²", &BTreeMap::new(), &["x", "y"]);
    match result {
        Ok(ast) => {
            let v = ast.eval_2d("x", 1.0, "y", 2.0);
            println!("x^2 + y² at (1,2) = {}", v);
            assert_eq!(v, 5.0);
        }
        Err(e) => panic!("x^2 + y² should parse, got error: {}", e),
    }
}
