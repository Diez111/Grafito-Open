//! Verifica que el scanline con stride=2 produce segments visibles
//! para `x^2 + y^2 < 1` con el view default de la app.

use grafito_geometry::ast::Expr;
use grafito_geometry::expr::prepare_function_ast;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
fn scanline_full(
    lhs: &Expr,
    rhs: &Expr,
    swap: bool,
    view_xmin: f64,
    view_xmax: f64,
    view_ymin: f64,
    view_ymax: f64,
    cols: usize,
    rows: usize,
    stride: i32,
) -> Vec<Vec<(f64, f64)>> {
    let dx = (view_xmax - view_xmin) / cols as f64;
    let dy = (view_ymax - view_ymin) / rows as f64;
    let mut out = Vec::with_capacity(rows);
    for y_pixel in 0..rows {
        let wy = view_ymin + (y_pixel as f64 + 0.5) * dy;
        let n = (cols as i32 / stride) + 1;
        let mut world_xs: Vec<f64> = vec![0.0; n as usize];
        let mut fs: Vec<f64> = vec![0.0; n as usize];
        let mut insides: Vec<bool> = vec![false; n as usize];
        for k in 0..n {
            let x_pixel = (k * stride) as usize;
            if x_pixel >= cols {
                break;
            }
            let wx = view_xmin + (x_pixel as f64 + 0.5) * dx;
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
fn test_lt_fill_view_normal() {
    // View default de la app: scale=50, screen=800x600.
    // Para un canvas de 1000x600, view = [-10, 10] x [-6, 6].
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let segs = scanline_full(&lhs, &rhs, false, -10.0, 10.0, -6.0, 6.0, 1000, 600, 2);
    let total_segs: usize = segs.iter().map(|s| s.len()).sum();
    let rows_with_fill: usize = segs.iter().filter(|s| !s.is_empty()).count();
    println!("total segments: {}", total_segs);
    println!("rows with fill: {}", rows_with_fill);
    // El disco tiene diametro 2 en view [-10, 10] (20 unidades).
    // Ratio = 2/12 (alto) = 16.7% de las filas.
    // Esperamos ~100 filas con fill.
    assert!(
        rows_with_fill > 50,
        "debe haber > 50 filas con fill, hay {}",
        rows_with_fill
    );
}

#[test]
fn test_gt_fill_view_normal() {
    // x^2 + y^2 > 1: el EXTERIOR del disco.
    // En view [-10, 10], la mayoría está afuera.
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let segs = scanline_full(&lhs, &rhs, true, -10.0, 10.0, -6.0, 6.0, 1000, 600, 2);
    let total_segs: usize = segs.iter().map(|s| s.len()).sum();
    let rows_with_fill: usize = segs.iter().filter(|s| !s.is_empty()).count();
    println!("GT total segments: {}", total_segs);
    println!("GT rows with fill: {}", rows_with_fill);
    assert!(
        rows_with_fill > 500,
        "debe haber > 500 filas con fill, hay {}",
        rows_with_fill
    );
}
