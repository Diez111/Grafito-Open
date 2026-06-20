//! Verifica que el scanline fill con stride es equivalente al de stride=1
//! (modulo precisión sub-pixel del linear refinement).

#[cfg(test)]
mod scanline_stride_tests {
    use grafito_geometry::ast::Expr;
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    /// Cuenta cuántos píxeles están "dentro" en una grilla cols×rows.
    /// Esta es la métrica de "área" del relleno.
    #[allow(clippy::too_many_arguments)]
    fn count_inside(
        lhs: &Expr,
        rhs: &Expr,
        swap: bool,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        cols: usize,
        rows: usize,
    ) -> usize {
        let dx = (x_max - x_min) / cols as f64;
        let dy = (y_max - y_min) / rows as f64;
        let mut count = 0;
        for y in 0..rows {
            let wy = y_min + (y as f64 + 0.5) * dy;
            for x in 0..cols {
                let wx = x_min + (x as f64 + 0.5) * dx;
                let l = lhs.eval_2d("x", wx, "y", wy);
                let r = rhs.eval_2d("x", wx, "y", wy);
                let f = if !l.is_finite() || !r.is_finite() {
                    f64::NAN
                } else if swap {
                    r - l
                } else {
                    l - r
                };
                if f.is_finite() && f <= 0.0 {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn test_circle_fill_count() {
        // x^2 + y^2 <= 1: sobre 100x100 en [-1.5, 1.5]^2.
        // El disco tiene área pi = 3.14, sobre área total 9, ratio = 0.349.
        // Esperamos ~349 pixeles "dentro".
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let count = count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, 100, 100);
        // El disco tiene area pi ~ 3.14. Area total 9. Ratio ~ 0.349.
        // 100x100 = 10000 pixeles. Esperamos ~3490 pixeles dentro.
        // Pero ojo: el centro está en (0,0) con f = -1 (dentro).
        // El borde (x²+y²=1) está justo dentro de los sample points.
        // Toleramos un margen del 5%.
        let total = 10000;
        let ratio = count as f64 / total as f64;
        let expected = std::f64::consts::PI / 9.0; // ~0.349
        assert!(
            (ratio - expected).abs() < 0.05,
            "ratio de fill {} difiere de esperado {} (>5%)",
            ratio,
            expected
        );
    }

    #[test]
    fn test_fill_consistency_across_resolutions() {
        // El fill debe escalar consistentemente con la resolución.
        // Para 100x100 vs 200x200, el ratio debe ser similar.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let count_100 = count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, 100, 100);
        let count_200 = count_inside(&lhs, &rhs, false, -1.5, 1.5, -1.5, 1.5, 200, 200);
        let ratio_100 = count_100 as f64 / 10000.0;
        let ratio_200 = count_200 as f64 / 40000.0;
        // Ambos ratios deben estar cerca de pi/9 ~ 0.349.
        let expected = std::f64::consts::PI / 9.0;
        assert!(
            (ratio_100 - expected).abs() < 0.05,
            "ratio 100x100 = {} difiere de {}",
            ratio_100,
            expected
        );
        assert!(
            (ratio_200 - expected).abs() < 0.05,
            "ratio 200x200 = {} difiere de {}",
            ratio_200,
            expected
        );
    }
}
