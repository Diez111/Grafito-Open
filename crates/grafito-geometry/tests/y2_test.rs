//! Verifica que el parser maneja `y²` (sin x antes).

use grafito_geometry::expr::prepare_function_ast;
use std::collections::HashMap;

#[test]
fn test_y_squared_parses() {
    let result = prepare_function_ast("y²", &HashMap::new(), &["y"]);
    match result {
        Ok(ast) => {
            let v = ast.eval_at("y", 3.0);
            println!("y² at 3.0 = {}", v);
            assert_eq!(v, 9.0);
        }
        Err(e) => panic!("y² should parse, got error: {}", e),
    }
}

#[test]
fn test_x2_plus_y2_parses() {
    let result = prepare_function_ast("x^2 + y²", &HashMap::new(), &["x", "y"]);
    match result {
        Ok(ast) => {
            let v = ast.eval_2d("x", 1.0, "y", 2.0);
            println!("x^2 + y² at (1,2) = {}", v);
            assert_eq!(v, 5.0);
        }
        Err(e) => panic!("x^2 + y² should parse, got error: {}", e),
    }
}
