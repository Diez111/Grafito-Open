#[test]
fn test_atan2() {
    use grafito_geometry::expr::preprocess_expr;
    use grafito_geometry::ast::parse_ast;
    let expr = "atan2(1,1)";
    let pp = preprocess_expr(expr);
    eprintln!("preprocessed: {:?}", pp);
    match parse_ast(&pp) {
        Ok(ast) => {
            eprintln!("AST: {:?}", ast);
            let s = ast.simplify();
            eprintln!("simplified: {:?}", s);
            let v = s.eval_at("x", 0.0);
            eprintln!("eval_at: {}", v);
            assert!((v - 0.785398).abs() < 0.01, "got {}", v);
        }
        Err(e) => panic!("parse_ast failed: {}", e),
    }
}
#[test]
fn test_new_funcs() {
    assert!((grafito_geometry::expr::eval_function("floor(3.7)", 0.0).unwrap() - 3.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("cbrt(8)", 0.0).unwrap() - 2.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("erf(0)", 0.0).unwrap()).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("gamma(5)", 0.0).unwrap() - 24.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("besselj(0,0)", 0.0).unwrap() - 1.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("heaviside(x-2)", 3.0).unwrap() - 1.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("min(3,5)", 0.0).unwrap() - 3.0).abs() < 0.01);
    assert!((grafito_geometry::expr::eval_function("clamp(10, 0, 5)", 0.0).unwrap() - 5.0).abs() < 0.01);
}
