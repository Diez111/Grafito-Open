use evalexpr::*;
fn main() {
    let mut ctx = HashMapContext::new();
    ctx.set_value("x".into(), Value::Float(-3.0)).unwrap();
    let res = eval_with_context("abs(x)", &ctx);
    println!("abs(x) = {:?}", res);
}
