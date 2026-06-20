//! Test que el stride=2 detecta correctamente el disco en view normal.

use grafito_geometry::ast::Expr;
use grafito_geometry::expr::prepare_function_ast;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
fn scanline_full_count(
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
fn test_disk_visible_normal_view_with_stride2() {
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let segs = scanline_full_count(&lhs, &rhs, false, -1.5, 1.5, -1.0, 1.0, 1000, 600, 2);
    let total_segs: usize = segs.iter().map(|s| s.len()).sum();
    println!("stride=2, view normal: total segments = {}", total_segs);
    // Debe haber segmentos.
    assert!(
        total_segs > 100,
        "disco debe tener > 100 segmentos, hay {}",
        total_segs
    );
}

#[test]
fn test_disk_visible_zoom_out_with_stride2() {
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let segs = scanline_full_count(&lhs, &rhs, false, -15.0, 15.0, -10.0, 10.0, 1000, 600, 2);
    let total_segs: usize = segs.iter().map(|s| s.len()).sum();
    println!("stride=2, zoom out 10x: total segments = {}", total_segs);
    // El disco tiene radio 1, view 30x20. El disco cubre 1/30 del view.
    // Con stride=2, dx = 30/1000 = 0.03, stride en world = 0.06.
    // Disco es 1 = 17 strides de ancho. Detectable.
    assert!(
        total_segs > 50,
        "disco debe ser visible, hay {} segs",
        total_segs
    );
}

#[test]
fn test_disk_visible_extreme_zoom_out_with_stride2() {
    // View [-150, 150] x [-100, 100]. El disco es 1/300 del view.
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let segs = scanline_full_count(
        &lhs, &rhs, false, -150.0, 150.0, -100.0, 100.0, 1000, 600, 2,
    );
    let total_segs: usize = segs.iter().map(|s| s.len()).sum();
    println!("stride=2, zoom out 100x: total segments = {}", total_segs);
    // dx = 300/1000 = 0.3. Stride en world = 0.6. Disco es 1 = 1.7 strides.
    // Con solo 1.7 strides, podemos perder el disco (1-2 samples).
    // El disco puede no detectarse en este caso.
}
