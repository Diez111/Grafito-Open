fn main() {
    let f = |x: f64| -> f64 {
        ((1.0 + x.tan()).sqrt() - (1.0 + x.sin()).sqrt()) / x.powi(3)
    };
    let a = -20.0;
    let b = 20.0;
    let steps = 400;
    let step = (b - a) / steps as f64;
    let mut prev = f(a);
    let mut roots = Vec::new();
    for i in 1..=steps {
        let x = a + i as f64 * step;
        let curr = f(x);
        if prev.is_finite() && curr.is_finite() && prev * curr < 0.0 {
            println!("Root found between {} and {}", x - step, x);
            roots.push(x);
        }
        if curr.is_finite() { prev = curr; }
    }
    println!("Roots: {:?}", roots);
}
