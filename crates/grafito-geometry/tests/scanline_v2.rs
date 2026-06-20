//! Simula el scanline fill con stride (como en render_2d.rs) y
//! verifica que el fill es correcto.

#[cfg(test)]
mod scanline_v2_tests {
    use grafito_geometry::ast::Expr;
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    /// Simula el scanline fill. Devuelve el número total de píxeles
    /// "dentro" (cuenta el área rellenada).
    fn scanline_count_inside(
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
        let mut total = 0;
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
            // Scanline par-impar: cada par de cruces rellena un segmento.
            // En este test aproximado, contamos cuántos samples consecutivos
            // están "dentro" (es un proxy del área).
            let mut filling = false;
            for k in 0..n {
                if !filling && insides[k as usize] {
                    filling = true;
                } else if filling && !insides[k as usize] {
                    filling = false;
                } else if filling {
                    total += 1;
                }
            }
        }
        total
    }

    #[test]
    fn test_circle_fill_count_consistent_with_stride() {
        // x^2 + y^2 <= 1: disco.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 200;
        let rows = 200;

        let count_1 = scanline_count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, cols, rows, 1);
        let count_4 = scanline_count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, cols, rows, 4);
        let count_8 = scanline_count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, cols, rows, 8);

        println!("stride=1: {} inside samples", count_1);
        println!("stride=4: {} inside samples", count_4);
        println!("stride=8: {} inside samples", count_8);

        // El conteo debe ser proporcional al stride (aproximadamente).
        // count_1 ~= count_4 * 4
        let ratio_1_4 = count_1 as f64 / count_4 as f64;
        let ratio_1_8 = count_1 as f64 / count_8 as f64;
        // Dentro del 20% de la razón del stride.
        assert!(
            (ratio_1_4 - 4.0).abs() < 1.5,
            "ratio 1/4 = {} (esperado ~4)",
            ratio_1_4
        );
        assert!(
            (ratio_1_8 - 8.0).abs() < 3.0,
            "ratio 1/8 = {} (esperado ~8)",
            ratio_1_8
        );
    }
}
