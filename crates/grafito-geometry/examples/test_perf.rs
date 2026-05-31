use grafito_geometry::expr::eval_batch_2d;
use std::collections::HashMap;
fn main() {
    let mut points = Vec::new();
    for j in 0..251 {
        for i in 0..251 {
            points.push((i as f64, j as f64));
        }
    }
    let start = std::time::Instant::now();
    let res = eval_batch_2d("x^2+y^2", "x", "y", points.into_iter(), &HashMap::new());
    println!("Elapsed: {:?}", start.elapsed());
}
