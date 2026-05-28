#[test]
fn test_implicit_mul() {
    println!("2x: {:?}", crate::expr::eval_function("2x", 1.0));
    println!("2*x: {:?}", crate::expr::eval_function("2*x", 1.0));
    println!("2x^2: {:?}", crate::expr::eval_function("2x^2", 2.0));
}
