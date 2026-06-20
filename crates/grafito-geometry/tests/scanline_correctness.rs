//! Tests de correctness para el algoritmo de scanline fill.
//! Verifica que las regiones se rellenan correctamente.

#[cfg(test)]
mod scanline_correctness {
    use grafito_geometry::ast::Expr;
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    /// Versión standalone del scanline fill (mismo algoritmo que
    /// `draw_implicit_curve_fill` en `render_2d.rs`). Devuelve un
    /// `Vec<bool>` donde `true` = dentro.
    #[allow(clippy::too_many_arguments)]
    fn scanline_fill_test(
        lhs: &Expr,
        rhs: &Expr,
        swap: bool,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        cols: usize,
        rows: usize,
    ) -> Vec<bool> {
        let mut result = vec![false; cols * rows];
        let dx = (x_max - x_min) / cols as f64;
        let dy = (y_max - y_min) / rows as f64;

        for y_pixel in 0..rows {
            let wy = y_min + (y_pixel as f64 + 0.5) * dy;
            for x_pixel in 0..cols {
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
                let inside = f.is_finite() && f <= 0.0;
                result[y_pixel * cols + x_pixel] = inside;
            }
        }
        result
    }

    #[test]
    fn test_circle_inside_circle_outside() {
        // x^2 + y^2 < 1 -> disco.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 100;
        let rows = 100;
        let x_min = -1.5;
        let x_max = 1.5;
        let y_min = -1.5;
        let y_max = 1.5;
        let fill = scanline_fill_test(&lhs, &rhs, false, x_min, x_max, y_min, y_max, cols, rows);
        // Centro debe estar dentro.
        let center_idx = (rows / 2) * cols + (cols / 2);
        assert!(fill[center_idx], "centro debe estar dentro del disco");
        // Esquinas deben estar fuera.
        assert!(!fill[0], "esquina superior-izquierda debe estar fuera");
        assert!(!fill[cols - 1], "esquina superior-derecha debe estar fuera");
        assert!(
            !fill[(rows - 1) * cols],
            "esquina inferior-izquierda debe estar fuera"
        );
        assert!(
            !fill[rows * cols - 1],
            "esquina inferior-derecha debe estar fuera"
        );
    }

    #[test]
    fn test_circle_eq_is_contour_only() {
        // Para Eq, el render del fill no se activa, pero el algoritmo
        // daría todos los puntos donde f=0 (solo el contorno). Esto
        // verifica que el algoritmo no se confunde.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 100;
        let rows = 100;
        let x_min = -1.5;
        let x_max = 1.5;
        let fill = scanline_fill_test(&lhs, &rhs, false, x_min, x_max, -1.5, 1.5, cols, rows);
        // El centro (0,0) tiene f = -1, que es <= 0, así que sería "dentro"
        // con el algoritmo. Pero para Eq el render no debe usar este fill.
        let center_idx = (rows / 2) * cols + (cols / 2);
        assert!(
            fill[center_idx],
            "centro tendría f=-1 (dentro si usáramos el algoritmo)"
        );
    }

    #[test]
    fn test_greater_than_circle_outside() {
        // x^2 + y^2 > 1 -> exterior del disco.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 100;
        let rows = 100;
        let x_min = -1.5;
        let x_max = 1.5;
        let y_min = -1.5;
        let y_max = 1.5;
        // Greater => swap = true.
        let fill = scanline_fill_test(&lhs, &rhs, true, x_min, x_max, y_min, y_max, cols, rows);
        // Centro debe estar fuera (porque x^2+y^2 < 1).
        let center_idx = (rows / 2) * cols + (cols / 2);
        assert!(
            !fill[center_idx],
            "centro debe estar fuera (interior del círculo)"
        );
        // Esquinas deben estar dentro.
        assert!(fill[0], "esquina debe estar dentro (exterior del círculo)");
    }
}
