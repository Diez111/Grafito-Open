use evalexpr::*;
fn main() {
    let mut ctx = HashMapContext::new();
    ctx.set_value("x".into(), Value::Float(-3.0)).unwrap();
    let res = eval_with_context("x^2", &ctx);
    println!("x=-3.0 -> x^2 = {:?}", res);

    // Test the evaluate function with variables
    let vars = vec![("x".to_string(), -3.0)];
    println!(
        "evaluate: {:?}",
        grafito_geometry::expr::evaluate("x^2", &vars)
    );
}
