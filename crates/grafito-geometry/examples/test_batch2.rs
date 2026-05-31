use std::collections::HashMap;
use grafito_geometry::expr::eval_batch_2d;

fn main() {
    let vars = HashMap::new();
    let points = vec![(0.0, 0.0), (3.0, 0.0)];
    let res = eval_batch_2d("9", "x", "y", points.clone().into_iter(), &vars);
    println!("9: {:?}", res);
    
    let res2 = eval_batch_2d("x^2+y^2", "x", "y", points.clone().into_iter(), &vars);
    println!("x^2+y^2: {:?}", res2);
    
    let res3 = eval_batch_2d("y", "x", "y", points.clone().into_iter(), &vars);
    println!("y: {:?}", res3);
    
    let res4 = eval_batch_2d("abs(x)", "x", "y", points.clone().into_iter(), &vars);
    println!("abs(x): {:?}", res4);
}
