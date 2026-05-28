fn main() {
    let expr = "sin(1.0)";
    println!("{:?}", evalexpr::eval(expr));
    println!("{:?}", evalexpr::eval("x^2"));
}
