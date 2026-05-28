#[test]
fn test_eval() {
    println!("{:?}", crate::expr::eval_function("sin(1.0)", 1.0));
    println!("{:?}", crate::expr::eval_function("x^2", 2.0));
    println!("{:?}", crate::expr::eval_function("x**2", 2.0));
    println!("{:?}", crate::expr::eval_function("math::sin(1.0)", 1.0));
}
