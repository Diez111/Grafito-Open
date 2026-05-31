fn deriv(sigma: f64, rho: f64, beta: f64, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    (sigma * (y - x), x * (rho - z) - y, x * y - beta * z)
}

fn main() {
    let mut x = 0.1;
    let mut y = 0.0;
    let mut z = 0.0;
    let dt = 0.005;
    let steps = 20000;
    let skip = 100;
    let mut pts = 0;
    for i in 0..steps {
        let (k1x, k1y, k1z) = deriv(10.0, 28.0, 2.66, x, y, z);
        let (k2x, k2y, k2z) = deriv(10.0, 28.0, 2.66, x + 0.5 * dt * k1x, y + 0.5 * dt * k1y, z + 0.5 * dt * k1z);
        let (k3x, k3y, k3z) = deriv(10.0, 28.0, 2.66, x + 0.5 * dt * k2x, y + 0.5 * dt * k2y, z + 0.5 * dt * k2z);
        let (k4x, k4y, k4z) = deriv(10.0, 28.0, 2.66, x + dt * k3x, y + dt * k3y, z + dt * k3z);
        x += dt / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
        y += dt / 6.0 * (k1y + 2.0 * k2y + 2.0 * k3y + k4y);
        z += dt / 6.0 * (k1z + 2.0 * k2z + 2.0 * k3z + k4z);
        if x.is_nan() || y.is_nan() || z.is_nan() { break; }
        if i >= skip {
            pts += 1;
        }
    }
    println!("Generated {} points", pts);
    println!("Last point: {}, {}, {}", x, y, z);
}
