//! Reproduce EXACTAMENTE el algoritmo del scanline fill del render
//! (incluyendo linear refinement) y verifica que produce los segmentos
//! correctos.

#[cfg(test)]
mod scanline_full_tests {
    use grafito_geometry::ast::Expr;
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;

    /// Replica exacta del algoritmo de scanline fill con stride.
    /// Devuelve los segmentos `(x_left_world, x_right_world)` por fila.
    fn scanline_full(
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
    ) -> Vec<Vec<(f64, f64)>> {
        let dx = (x_max - x_min) / cols as f64;
        let dy = (y_max - y_min) / rows as f64;
        let mut out = Vec::with_capacity(rows);
        for y_pixel in 0..rows {
            let wy = y_min + (y_pixel as f64 + 0.5) * dy;
            let n = (cols as i32 / stride) + 1;
            let mut world_xs: Vec<f64> = vec![0.0; n as usize];
            let mut fs: Vec<f64> = vec![0.0; n as usize];
            let mut insides: Vec<bool> = vec![false; n as usize];
            for k in 0..n {
                let x_pixel = (k * stride) as usize;
                if x_pixel >= cols {
                    break;
                }
                let wx = x_min + (x_pixel as f64 + 0.5) * dx;
                world_xs[k as usize] = wx;
                let l = lhs.eval_2d("x", wx, "y", wy);
                let r = rhs.eval_2d("x", wx, "y", wy);
                let f = if !l.is_finite() || !r.is_finite() {
                    f64::NAN
                } else if swap {
                    r - l
                } else {
                    l - r
                };
                fs[k as usize] = f;
                insides[k as usize] = f.is_finite() && f <= 0.0;
            }
            let mut segments = Vec::new();
            let mut filling = false;
            let mut seg_start: Option<f64> = None;
            for k in 0..n {
                let i = k as usize;
                let inside = insides[i];
                if filling && !inside {
                    if let Some(start_world) = seg_start.take() {
                        let f_prev = fs[i - 1];
                        let f_curr = fs[i];
                        let x_world_curr = world_xs[i];
                        let x_world_prev = if i > 0 { world_xs[i - 1] } else { x_world_curr };
                        let end_x_world = if f_prev.is_finite() && f_curr.is_finite() && i > 0 {
                            let dx_world = x_world_curr - x_world_prev;
                            x_world_prev + dx_world * f_prev / (f_prev - f_curr)
                        } else {
                            x_world_curr
                        };
                        segments.push((start_world, end_x_world));
                    }
                    filling = false;
                } else if !filling && inside {
                    let f_prev = if i > 0 { fs[i - 1] } else { f64::NAN };
                    let f_curr = fs[i];
                    let x_world_curr = world_xs[i];
                    let x_world_prev = if i > 0 { world_xs[i - 1] } else { x_world_curr };
                    let start_x_world = if f_prev.is_finite() && f_curr.is_finite() && i > 0 {
                        let dx_world = x_world_curr - x_world_prev;
                        x_world_prev + dx_world * f_prev / (f_prev - f_curr)
                    } else {
                        x_world_curr
                    };
                    seg_start = Some(start_x_world);
                    filling = true;
                }
            }
            if filling {
                if let Some(start_world) = seg_start.take() {
                    let end_world = world_xs[(n - 1) as usize];
                    segments.push((start_world, end_world));
                }
            }
            out.push(segments);
        }
        out
    }

    #[test]
    fn test_disk_scanline_produces_correct_segments() {
        // x^2 + y^2 <= 1: disco.
        // En y=0, esperamos segmentos (-1, 1).
        // En y=0.5, esperamos segmentos (-0.866, 0.866).
        // En y=0.9, esperamos segmentos (-0.436, 0.436).
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let cols = 200;
        let rows = 200;
        let segments = scanline_full(&lhs, &rhs, false, -2.0, 2.0, -2.0, 2.0, cols, rows, 4);
        assert!(!segments.is_empty());
        let mut found_y0 = false;
        let mut found_y05 = false;
        let mut found_y09 = false;
        for (y_idx, segs) in segments.iter().enumerate() {
            let wy = -2.0 + (y_idx as f64 + 0.5) * (4.0 / rows as f64);
            if (wy - 0.0).abs() < 0.05 && !segs.is_empty() {
                found_y0 = true;
                let (x1, x2) = segs[0];
                println!("y≈0: segmento ({}, {})", x1, x2);
                assert!((x1 + 1.0).abs() < 0.1, "x1 debe estar cerca de -1");
                assert!((x2 - 1.0).abs() < 0.1, "x2 debe estar cerca de 1");
            }
            if (wy - 0.5).abs() < 0.05 && !segs.is_empty() {
                found_y05 = true;
                let (x1, x2) = segs[0];
                let expected = (1.0 - 0.25_f64).sqrt();
                println!(
                    "y≈0.5: segmento ({}, {}), esperado ~({}, {})",
                    x1, x2, -expected, expected
                );
                assert!(
                    (x1 - (-expected)).abs() < 0.1,
                    "x1 debe estar cerca de -0.866"
                );
                assert!((x2 - expected).abs() < 0.1, "x2 debe estar cerca de 0.866");
            }
            if (wy - 0.9).abs() < 0.05 && !segs.is_empty() {
                found_y09 = true;
                let (x1, x2) = segs[0];
                let expected = (1.0 - 0.81_f64).sqrt();
                println!(
                    "y≈0.9: segmento ({}, {}), esperado ~({}, {})",
                    x1, x2, -expected, expected
                );
                assert!(
                    (x1 - (-expected)).abs() < 0.15,
                    "x1 debe estar cerca de -0.436"
                );
                assert!((x2 - expected).abs() < 0.15, "x2 debe estar cerca de 0.436");
            }
        }
        assert!(found_y0, "no se encontró fila y≈0");
        assert!(found_y05, "no se encontró fila y≈0.5");
        assert!(found_y09, "no se encontró fila y≈0.9");
    }

    #[test]
    fn test_no_fill_outside_disk() {
        // Filas con |y| > 1 no deben tener fill.
        let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
        let segments = scanline_full(&lhs, &rhs, false, -3.0, 3.0, -3.0, 3.0, 600, 600, 4);
        // Filas con y=2.5 (índice cercano a (2.5+3)/6 * 600 = 550).
        // y en world = -3 + (550 + 0.5) * 6/600 = -3 + 5.505 = 2.505.
        // f = 2.505² + x² - 1. Para cualquier x en [-3, 3], f > 0.
        // No debe haber segmentos.
        let segs_at_25 = &segments[550];
        println!("y=2.5: {} segmentos", segs_at_25.len());
        assert_eq!(segs_at_25.len(), 0, "fila y=2.5 debe estar vacía");
    }
}
