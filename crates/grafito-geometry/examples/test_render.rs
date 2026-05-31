fn main() {
    let expr = "cos^(-1)(1-abs(x))-π";
    let min_x = -10.0;
    let max_x = 10.0;
    let steps = 1000;
    let step = (max_x - min_x) / steps as f64;
    let xs = (0..=steps).map(|i| min_x + i as f64 * step);
    let variables = std::collections::HashMap::new();

    let mut samples: Vec<(f64, Option<f64>)> = Vec::with_capacity(steps + 1);
    match grafito_geometry::expr::eval_function_batch(expr, xs.clone(), &variables) {
        Ok(results) => {
            for (x, y) in xs.zip(results.into_iter()) {
                samples.push((x, y.filter(|v| v.is_finite() && v.abs() < 1e6)));
            }
            let valid_count = samples.iter().filter(|(_, y)| y.is_some()).count();
            println!("Success! Valid points: {}", valid_count);
        }
        Err(e) => {
            println!("Error in eval_function_batch: {}", e);
        }
    }
}
