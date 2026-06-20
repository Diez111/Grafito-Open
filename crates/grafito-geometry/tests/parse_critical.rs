//! Verifica que el parser maneja todas las expresiones que el usuario
//! podría usar con ImplicitCurve.

#[cfg(test)]
mod parse_tests {
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    #[test]
    fn test_parse_x_squared_plus_y_squared() {
        let ast = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let v = ast.eval_2d("x", 0.5, "y", 0.5);
        assert!((v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_just_one() {
        let ast = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let v = ast.eval_2d("x", 100.0, "y", 200.0);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn test_parse_just_zero() {
        let ast = prepare_function_ast("0", &HashMap::new(), &["x", "y"]).unwrap();
        let v = ast.eval_2d("x", 100.0, "y", 200.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_parse_complex_expr() {
        let ast = prepare_function_ast(
            "sin(x) * cos(y) + exp(-x^2 - y^2)",
            &HashMap::new(),
            &["x", "y"],
        )
        .unwrap();
        let v = ast.eval_2d("x", 0.0, "y", 0.0);
        // sin(0)*cos(0) + exp(0) = 0 + 1 = 1
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_sin_y() {
        let ast = prepare_function_ast("sin(y)", &HashMap::new(), &["x", "y"]).unwrap();
        let v = ast.eval_2d("x", 0.0, "y", 0.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_parse_x_squared_times_y() {
        let ast = prepare_function_ast("x^2 * y", &HashMap::new(), &["x", "y"]).unwrap();
        let v = ast.eval_2d("x", 2.0, "y", 3.0);
        assert_eq!(v, 12.0);
    }
}
