//! Test que reproduce el bug crítico del stride: cuando el usuario
//! hace zoom in a un disco, el stride=8 salta el disco entero.

use grafito_geometry::ast::Expr;
use grafito_geometry::expr::prepare_function_ast;
use std::collections::HashMap;

fn scanline_with_stride(
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
) -> (usize, usize) {
    let dx = (view_xmax - view_xmin) / cols as f64;
    let dy = (view_ymax - view_ymin) / rows as f64;
    let mut rows_with_fill = 0;
    let mut total_inside = 0;
    for y_pixel in 0..rows {
        let wy = view_ymin + (y_pixel as f64 + 0.5) * dy;
        let n = (cols as i32 / stride) + 1;
        let mut insides: Vec<bool> = vec![false; n as usize];
        for k in 0..n {
            let x_pixel = (k * stride) as usize;
            if x_pixel >= cols {
                break;
            }
            let wx = view_xmin + (x_pixel as f64 + 0.5) * dx;
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
        let mut filling = false;
        let mut inside_in_row = false;
        for k in 0..n {
            if !filling && insides[k as usize] {
                filling = true;
                inside_in_row = true;
            } else if filling && !insides[k as usize] {
                filling = false;
            }
        }
        // Si la fila está "dentro" en el primer o último sample, también
        // cuenta como fila con fill.
        if !inside_in_row && (n > 0 && (insides[0] || insides[(n - 1) as usize])) {
            inside_in_row = true;
        }
        if inside_in_row {
            rows_with_fill += 1;
        }
        // También cuento cuántos samples están dentro.
        for k in 0..n {
            if insides[k as usize] {
                total_inside += 1;
            }
        }
    }
    (rows_with_fill, total_inside)
}

#[test]
fn test_zoom_in_disk_with_stride8() {
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    let cols = 1000;
    let rows = 600;

    // View normal
    let (rows_normal, inside_normal) =
        scanline_with_stride(&lhs, &rhs, false, -1.5, 1.5, -1.0, 1.0, cols, rows, 8);
    println!(
        "view [-1.5, 1.5]: rows={}, inside={}",
        rows_normal, inside_normal
    );

    // View zoom in 100x
    let (rows_zoomed, inside_zoomed) =
        scanline_with_stride(&lhs, &rhs, false, -0.015, 0.015, -0.01, 0.01, cols, rows, 8);
    println!(
        "view [-0.015, 0.015]: rows={}, inside={}",
        rows_zoomed, inside_zoomed
    );

    // View zoom out 10x (el disco es 1/30 del view)
    let (rows_zoomed_out, inside_zoomed_out) =
        scanline_with_stride(&lhs, &rhs, false, -15.0, 15.0, -10.0, 10.0, cols, rows, 8);
    println!(
        "view [-15, 15]: rows={}, inside={}",
        rows_zoomed_out, inside_zoomed_out
    );

    // El disco debe ser visible en TODOS los views.
    assert!(rows_normal > 0, "view normal: disco invisible");
    assert!(
        rows_zoomed > 0,
        "view zoom in: disco invisible (count={})",
        rows_zoomed
    );
    assert!(rows_zoomed_out > 0, "view zoom out: disco invisible");
}
