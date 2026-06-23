use evalexpr::*;
fn main() {
    let mut ctx = HashMapContext::new();
    for x in 0..=5 {
        ctx.set_value("x".into(), Value::Float(x as f64)).unwrap();
        println!("x={} -> {:?}", x, eval_with_context("x^2", &ctx));
    }
}
