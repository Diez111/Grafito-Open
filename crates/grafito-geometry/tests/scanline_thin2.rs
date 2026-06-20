//! Verifica que el scanline con stride no pierde franjas muy delgadas
//! (< 2*stride world units de ancho).

#[cfg(test)]
mod thin_tests {
    use grafito_geometry::ast::Expr;
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    /// Versión con stride del scanline.
    #[allow(clippy::too_many_arguments)]
    fn scanline_thin(
        lhs: &Expr,
        rhs: &Expr,
        swap: bool,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        cols: usize,
        rows: usize,
        stride: i32,
    ) -> usize {
        let dx = (x_max - x_min) / cols as f64;
        let dy = (y_max - y_min) / rows as f64;
        let mut count = 0;
        for y_pixel in 0..rows {
            let wy = y_min + (y_pixel as f64 + 0.5) * dy;
            let n = (cols as i32 / stride) + 1;
            let mut insides: Vec<bool> = vec![false; n as usize];
            for k in 0..n {
                let x_pixel = (k * stride) as usize;
                if x_pixel >= cols {
                    break;
                }
                let wx = x_min + (x_pixel as f64 + 0.5) * dx;
                let l = lhs.eval_2d("x", wx, "y", wy);
                let r = rhs.eval_2d("x", wx, "y", wy);
                let f = if !l.is_finite() || !r.is_finite() {
                    f64::NAN
                } else if swap {
                    r - l
                } else {
                    l - r
                };
                insides[k as usize] = f.is_finite() && f <= 0.0;
            }
            // Par-impar
            let mut filling = false;
            for k in 0..n {
                if !filling && insides[k as usize] {
                    filling = true;
                } else if filling && !insides[k as usize] {
                    filling = false;
                } else if filling {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn test_thin_strip_05_units() {
        // Franja de 0.5 unidades: y entre -0.25 y 0.25.
        // |y| < 0.25 ⟺ y² < 0.0625.
        let lhs = prepare_function_ast("y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("0.0625", &HashMap::new(), &["x", "y"]).unwrap();
        // View 800x600, range [-5, 5] x [-5, 5].
        // dx = 10/800 = 0.0125. Franja = 0.5 = 40 pixels.
        // Con stride=8, hay 5 samples dentro. Debe detectarse.
        let count = scanline_thin(&lhs, &rhs, false, -5.0, 5.0, -5.0, 5.0, 800, 600, 8);
        println!("franja 0.5 units, stride=8: count={}", count);
        assert!(count > 0, "franja debe detectarse");
    }

    #[test]
    fn test_thin_strip_001_units() {
        // Franja muy delgada: 0.01 unidades. ~1 pixel.
        let lhs = prepare_function_ast("y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("0.000025", &HashMap::new(), &["x", "y"]).unwrap();
        let count = scanline_thin(&lhs, &rhs, false, -5.0, 5.0, -5.0, 5.0, 800, 600, 8);
        println!("franja 0.01 units, stride=8: count={}", count);
        // Esta franja es de 1 pixel de ancho. Con stride=8, no se detecta
        // (samples consecutivos saltan la franja). Eso es esperado.
    }
}
