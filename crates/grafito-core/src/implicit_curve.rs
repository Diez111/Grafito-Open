//! Shared implicit-curve evaluation and caching support.
//!
//! The heavy grid evaluation is performed once per view/expression change and
//! the resulting line segments are cached inside [`ImplicitCurveObj`]. Both the
//! CPU painter path (`grafito-app`) and the GPU geometry builder path
//! (`grafito-render`) consume the cached world-space segments.

use crate::object::{ImplicitCurveObj, ImplicitCurveSegments, RelationOperator};
use grafito_geometry::{expr, Point2};
use rayon::prelude::*;
use std::collections::HashMap;

/// Choose a grid resolution that keeps cells close to screen pixels while
/// avoiding excessive work on high-DPI or huge canvases.
///
/// Target ~2 pixels per cell, clamped between 40 and 256 samples per axis.
pub fn recommended_grid_size(canvas_width: f32, canvas_height: f32) -> usize {
    let cells_x = (canvas_width as f64).clamp(128.0, 1024.0);
    let cells_y = (canvas_height as f64).clamp(128.0, 1024.0);
    cells_x.max(cells_y) as usize
}

/// Compute or retrieve cached world-space line segments for an implicit curve.
///
/// The cache key covers the expression, operator, contour configuration, view
/// bounds, grid resolution and document variables. When any of these change the
/// grid is re-evaluated; otherwise the previous segments are returned.
pub fn segments_or_compute<'a>(
    ic: &'a ImplicitCurveObj,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> std::sync::RwLockReadGuard<'a, ImplicitCurveSegments> {
    let key = ic.cache_key(view_bounds, grid_size, variables);
    {
        let cached_key = ic.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return ic.cached_segments.read().unwrap_or_else(|p| p.into_inner());
        }
    }

    let segments = evaluate_implicit_curve(ic, view_bounds, grid_size, variables);
    *ic.cached_segments
        .write()
        .unwrap_or_else(|p| p.into_inner()) = segments;
    *ic.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    ic.cached_segments.read().unwrap_or_else(|p| p.into_inner())
}

/// Evaluate an implicit curve over a rectangular world-space domain.
///
/// Returns one list of world-space line segments per contour level, obtained
/// with marching squares. The scalar field evaluation is parallelised over
/// grid rows via Rayon.
pub fn evaluate_implicit_curve(
    ic: &ImplicitCurveObj,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> ImplicitCurveSegments {
    let (x_min, x_max, y_min, y_max) = view_bounds;
    if grid_size == 0
        || !x_min.is_finite()
        || !x_max.is_finite()
        || !y_min.is_finite()
        || !y_max.is_finite()
    {
        return Vec::new();
    }

    let levels: Vec<f64> = ic
        .contour_levels
        .as_ref()
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![0.0]);

    // Pre-parse both sides once. If parsing fails we fall back to per-cell
    // evaluation so that an error on one side does not silently drop the curve.
    let parsed_lhs = expr::prepare_function_ast(&ic.expr_lhs, variables, &["x", "y"]).ok();
    let parsed_rhs = expr::prepare_function_ast(&ic.expr_rhs, variables, &["x", "y"]).ok();

    let dx = (x_max - x_min) / grid_size as f64;
    let dy = (y_max - y_min) / grid_size as f64;
    if dx == 0.0 || dy == 0.0 {
        return Vec::new();
    }

    // For relations other than equality we compute `lhs - rhs` and treat the
    // filled side as the positive one. Less/LessEq => lhs - rhs <= 0, so the
    // boundary is still at zero and we render the zero contour as usual.
    let eval_cell = |x: f64, y: f64| -> f64 {
        let lhs = if let Some(ast) = &parsed_lhs {
            ast.eval_2d("x", x, "y", y)
        } else {
            expr::evaluate(&ic.expr_lhs, &[("x".to_string(), x), ("y".to_string(), y)])
                .unwrap_or(f64::NAN)
        };
        let rhs = if let Some(ast) = &parsed_rhs {
            ast.eval_2d("x", x, "y", y)
        } else {
            expr::evaluate(&ic.expr_rhs, &[("x".to_string(), x), ("y".to_string(), y)])
                .unwrap_or(f64::NAN)
        };
        if !lhs.is_finite() || !rhs.is_finite() {
            return f64::NAN;
        }
        match ic.operator {
            RelationOperator::Eq => lhs - rhs,
            RelationOperator::Less => lhs - rhs,
            RelationOperator::Greater => rhs - lhs,
            RelationOperator::LessEq => lhs - rhs,
            RelationOperator::GreaterEq => rhs - lhs,
        }
    };

    // Evaluate the scalar field in parallel (one row per thread). Each row is
    // a Vec of (grid_size + 1) sample values.
    let rows: Vec<Vec<f64>> = (0..=grid_size)
        .into_par_iter()
        .map(|j| {
            let y = y_min + j as f64 * dy;
            (0..=grid_size)
                .map(|i| {
                    let x = x_min + i as f64 * dx;
                    let v = eval_cell(x, y);
                    if v.is_finite() {
                        v
                    } else {
                        f64::NAN
                    }
                })
                .collect()
        })
        .collect();

    // Build per-level segments. This is serial but cheap compared to evaluation.
    let mut per_level: ImplicitCurveSegments = ImplicitCurveSegments::new();
    let mut total = 0usize;
    const MAX_SEGMENTS: usize = 200_000;
    for level in &levels {
        let segs = marching_squares_level(&rows, *level, x_min, y_min, dx, dy);
        total += segs.len();
        per_level.push((*level, segs));
        if total > MAX_SEGMENTS {
            break;
        }
    }

    // Safety cap: avoid pathological curves exploding the vertex buffer.
    if total > MAX_SEGMENTS {
        let mut kept = 0usize;
        per_level.retain_mut(|(_, segs)| {
            if kept >= MAX_SEGMENTS {
                segs.clear();
                false
            } else if kept + segs.len() > MAX_SEGMENTS {
                let take = MAX_SEGMENTS - kept;
                segs.truncate(take);
                kept = MAX_SEGMENTS;
                true
            } else {
                kept += segs.len();
                true
            }
        });
    }

    per_level
}

fn marching_squares_level(
    rows: &[Vec<f64>],
    level: f64,
    x_min: f64,
    y_min: f64,
    dx: f64,
    dy: f64,
) -> Vec<(Point2, Point2)> {
    let grid_size = rows.len().saturating_sub(1);
    if grid_size == 0 {
        return Vec::new();
    }

    let mut segments = Vec::new();
    for i in 0..grid_size {
        let x0 = x_min + i as f64 * dx;
        let x1 = x0 + dx;
        for j in 0..grid_size {
            let y0 = y_min + j as f64 * dy;
            let y1 = y0 + dy;

            let v00 = rows[j][i];
            let v10 = rows[j][i + 1];
            let v01 = rows[j + 1][i];
            let v11 = rows[j + 1][i + 1];

            if v00.is_nan() || v10.is_nan() || v01.is_nan() || v11.is_nan() {
                continue;
            }

            let s00 = (v00 - level) >= 0.0;
            let s10 = (v10 - level) >= 0.0;
            let s01 = (v01 - level) >= 0.0;
            let s11 = (v11 - level) >= 0.0;

            let case = (s00 as u8) | ((s10 as u8) << 1) | ((s11 as u8) << 2) | ((s01 as u8) << 3);

            if case == 0 || case == 15 {
                continue;
            }

            let interp = |va: f64, vb: f64, pa: f64, pb: f64| -> f64 {
                let denom = (va - level) - (vb - level);
                if denom.abs() < 1e-15 {
                    (pa + pb) * 0.5
                } else {
                    let t = (va - level) / denom;
                    pa + t * (pb - pa)
                }
            };

            let mut push = |a: Point2, b: Point2| segments.push((a, b));

            let bottom = |t: f64| Point2::new(x0 + t * (x1 - x0), y0);
            let top = |t: f64| Point2::new(x0 + t * (x1 - x0), y1);
            let left = |t: f64| Point2::new(x0, y0 + t * (y1 - y0));
            let right = |t: f64| Point2::new(x1, y0 + t * (y1 - y0));

            let ib = interp(v00, v10, 0.0, 1.0);
            let ir = interp(v10, v11, 0.0, 1.0);
            let it = interp(v01, v11, 0.0, 1.0);
            let il = interp(v00, v01, 0.0, 1.0);

            match case {
                1 | 14 => push(bottom(ib), left(il)),
                2 | 13 => push(right(ir), bottom(ib)),
                3 | 12 => push(right(ir), left(il)),
                4 | 11 => push(top(it), right(ir)),
                5 => {
                    push(bottom(ib), left(il));
                    push(top(it), right(ir));
                }
                6 | 9 => push(top(it), bottom(ib)),
                7 | 8 => push(top(it), left(il)),
                10 => {
                    push(right(ir), bottom(ib));
                    push(left(il), top(it));
                }
                _ => {}
            }
        }
    }

    segments
}
