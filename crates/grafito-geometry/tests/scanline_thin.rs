//! Verifica que el scanline con stride no pierde regiones delgadas.

#[cfg(test)]
mod scanline_thin_tests {
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    #[test]
    fn test_thin_strip_detected() {
        // Una franja delgada: x entre -0.1 y 0.1.
        let lhs = prepare_function_ast("x^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("0.01", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 300;
        let rows = 100;
        let dx = 3.0 / cols as f64;
        let dy = 3.0 / rows as f64;

        // Sample con stride=8, igual que el render real.
        for y in 0..rows {
            let wy = -1.5 + (y as f64 + 0.5) * dy;
            let n = (cols as i32 / 8) + 1;
            for k in 0..n {
                let x_pixel = (k * 8) as usize;
                if x_pixel >= cols {
                    break;
                }
                let wx = -1.5 + (x_pixel as f64 + 0.5) * dx;
                let l = lhs.eval_2d("x", wx, "y", wy);
                let r = rhs.eval_2d("x", wx, "y", wy);
                let f = l - r;
                if y == 50 && f.abs() < 0.5 {
                    println!("k={} x={:.3} f={:.4} inside={}", k, wx, f, f <= 0.0);
                }
            }
            if y == 50 {
                break;
            }
        }
    }
}
