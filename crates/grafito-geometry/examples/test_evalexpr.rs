use evalexpr::*;
fn main() {
    let mut ctx = HashMapContext::new();
    ctx.set_value("x".into(), Value::Float(3.0)).unwrap();
    println!("{:?}", eval_with_context("x^2", &ctx));
}
