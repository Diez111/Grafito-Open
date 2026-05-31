use std::collections::HashMap;
use grafito_geometry::expr::eval_batch_2d;

fn main() {
    let vars = HashMap::new();
    
    // Simulating what render_2d.rs does for x^2+y^2=9
    // LHS = "x^2+y^2", RHS = "9"
    let lhs = "x^2+y^2";
    let rhs = "9";
    
    let points: Vec<(f64, f64)> = vec![
        (0.0, 0.0),   // 0+0=0, f=0-9=-9
        (3.0, 0.0),   // 9+0=9, f=9-9=0 (ON circle)
        (0.0, 3.0),   // 0+9=9, f=9-9=0 (ON circle)
        (2.0, 2.0),   // 4+4=8, f=8-9=-1
        (3.0, 3.0),   // 9+9=18, f=18-9=9
    ];
    
    let lhs_vals = eval_batch_2d(lhs, "x", "y", points.clone().into_iter(), &vars);
    println!("LHS vals for '{lhs}': {:?}", lhs_vals);
    
    let rhs_vals = eval_batch_2d(rhs, "x", "y", points.clone().into_iter(), &vars);
    println!("RHS vals for '{rhs}': {:?}", rhs_vals);
    
    // Compute f = lhs - rhs
    if let (Ok(l), Ok(r)) = (lhs_vals, rhs_vals) {
        for (i, (lv, rv)) in l.iter().zip(r.iter()).enumerate() {
            let f = match (lv, rv) {
                (Some(a), Some(b)) => Some(a - b),
                _ => None,
            };
            println!("Point {:?}: f = {:?}", points[i], f);
        }
    }
}
