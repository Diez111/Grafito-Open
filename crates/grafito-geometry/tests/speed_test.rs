#[cfg(test)]
mod speed_tests {
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;
    use std::time::Instant;

    #[test]
    #[ignore]
    fn bench_eval_2d_simple() {
        let ast = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let n = 2_000_000usize;
        let start = Instant::now();
        let mut sum = 0.0f64;
        for i in 0..n {
            let x = (i as f64 * 0.001).sin();
            let y = (i as f64 * 0.001).cos();
            let v = ast.eval_2d("x", x, "y", y);
            if v.is_finite() {
                sum += v;
            }
        }
        let elapsed = start.elapsed();
        println!("{} evals of x^2+y^2 in {:?}", n, elapsed);
        println!("per eval: {:?}", elapsed / n as u32);
        // Para 2M evals por frame (1 sample/px con 2 sides):
        println!("for 2M evals: {:?}", elapsed);
        println!("sum (sanity): {}", sum);
    }
}
