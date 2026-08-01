//! Shared implicit-curve evaluation and caching support.
//!
//! The heavy grid evaluation is performed once per view/expression change and
//! the resulting line segments are cached inside [`ImplicitCurveObj`]. Both the
//! CPU painter path (`grafito-app`) and the GPU geometry builder path
//! (`grafito-render`) consume the cached world-space segments.

use crate::object::{ImplicitCurveObj, ImplicitCurveSegments, RelationOperator};
use crate::RenderQuality;
use grafito_geometry::{expr, Point2};
use rayon::prelude::*;
use std::collections::HashMap;

/// Maximum resolution supported by the implicit-curve cache and render paths.
pub const MAX_IMPLICIT_GRID_SIZE: usize = 1024;
/// Maximum number of marching-squares segments retained for one curve.
pub const MAX_MARCHING_SQUARES_SEGMENTS: usize = 50_000;
/// Maximum cells visited while extracting all contour levels for one curve.
pub const MAX_MARCHING_SQUARES_WORK_UNITS: usize = crate::validation::MAX_CONTOUR_WORK_UNITS;

const MAX_IMPLICIT_GRID_CELLS: usize = MAX_IMPLICIT_GRID_SIZE * MAX_IMPLICIT_GRID_SIZE;

/// Returns the number of scalar samples for a square grid when its dimensions
/// are representable and within the public per-object limit.
fn implicit_grid_sample_count(grid_size: usize) -> Option<usize> {
    if grid_size > MAX_IMPLICIT_GRID_SIZE {
        return None;
    }
    let samples_per_axis = grid_size.checked_add(1)?;
    samples_per_axis.checked_mul(samples_per_axis)
}

/// Choose a grid resolution that keeps cells close to screen pixels while
/// avoiding excessive work on high-DPI or huge canvases.
///
/// Clamped between 128 and 1024 samples per axis.
pub fn recommended_grid_size(canvas_width: f32, canvas_height: f32) -> usize {
    ((canvas_width.max(canvas_height)) as f64).clamp(128.0, MAX_IMPLICIT_GRID_SIZE as f64) as usize
}

/// Choose a grid resolution capped by the current render quality.
pub fn recommended_grid_size_for_quality(
    canvas_width: f32,
    canvas_height: f32,
    quality: RenderQuality,
) -> usize {
    let base = recommended_grid_size(canvas_width, canvas_height);
    match quality {
        RenderQuality::Preview => base.min(128),
        RenderQuality::Normal => base.min(512),
        RenderQuality::High => base.min(1024),
    }
}

/// Expand the visible view bounds by `pad_factor` and snap the result to a
/// coarse grid so that small pans do not invalidate the cache.
pub fn padded_snapped_bounds(
    view_bounds: (f64, f64, f64, f64),
    pad_factor: f64,
    snap_cells: usize,
) -> (f64, f64, f64, f64) {
    let (vx_min, vx_max, vy_min, vy_max) = view_bounds;
    let cx = (vx_min + vx_max) * 0.5;
    let cy = (vy_min + vy_max) * 0.5;
    let half_w = (vx_max - vx_min) * 0.5 * pad_factor;
    let half_h = (vy_max - vy_min) * 0.5 * pad_factor;

    let cells = snap_cells.max(1) as f64;
    let cell_x = (vx_max - vx_min) / cells;
    let cell_y = (vy_max - vy_min) / cells;

    let (x_min, mut x_max) = if cell_x > 0.0 {
        (
            ((cx - half_w) / cell_x).floor() * cell_x,
            ((cx + half_w) / cell_x).ceil() * cell_x,
        )
    } else {
        (cx - half_w, cx + half_w)
    };

    let (y_min, mut y_max) = if cell_y > 0.0 {
        (
            ((cy - half_h) / cell_y).floor() * cell_y,
            ((cy + half_h) / cell_y).ceil() * cell_y,
        )
    } else {
        (cy - half_h, cy + half_h)
    };

    // Defensive: ensure a non-degenerate domain.
    if x_min >= x_max {
        x_max = x_min + f64::EPSILON;
    }
    if y_min >= y_max {
        y_max = y_min + f64::EPSILON;
    }

    (x_min, x_max, y_min, y_max)
}

/// Compute or retrieve cached world-space line segments for an implicit curve.
///
/// The cache key covers the expression, operator, contour configuration,
/// padded/snapped view bounds, grid resolution and document variables. When
/// any of these change the grid is re-evaluated; otherwise the previous
/// segments are returned.
pub fn segments_or_compute<'a>(
    ic: &'a ImplicitCurveObj,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
    quality: RenderQuality,
) -> std::sync::RwLockReadGuard<'a, ImplicitCurveSegments> {
    let padded_bounds = padded_snapped_bounds(view_bounds, 2.0, 64);
    let grid_size = match quality {
        RenderQuality::Preview => grid_size.min(128),
        RenderQuality::Normal => grid_size.min(512),
        RenderQuality::High => grid_size.min(MAX_IMPLICIT_GRID_SIZE),
    };
    let key = ic.cache_key(padded_bounds, grid_size, variables);

    {
        let cached_key = ic.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if let Some(cached) = cached_key.as_ref() {
            if cached == &key {
                let cached_region = ic.cached_region.read().unwrap_or_else(|p| {
                    log::warn!("cache lock envenenado; recuperando estado parcial");
                    p.into_inner()
                });
                if let Some((rx_min, rx_max, ry_min, ry_max)) = *cached_region {
                    let (vx_min, vx_max, vy_min, vy_max) = view_bounds;
                    if vx_min >= rx_min && vx_max <= rx_max && vy_min >= ry_min && vy_max <= ry_max
                    {
                        return ic.cached_segments.read().unwrap_or_else(|p| {
                            log::warn!("cache lock envenenado; recuperando estado parcial");
                            p.into_inner()
                        });
                    }
                }
            }
        }
    }

    let segments = evaluate_implicit_curve(ic, padded_bounds, grid_size, variables);
    *ic.cached_segments.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = segments;
    *ic.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    *ic.cached_region.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(padded_bounds);
    ic.cached_segments.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
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
        || implicit_grid_sample_count(grid_size).is_none()
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
            let mut row = Vec::with_capacity(grid_size + 1);
            for i in 0..=grid_size {
                let x = x_min + i as f64 * dx;
                let v = eval_cell(x, y);
                row.push(if v.is_finite() { v } else { f64::NAN });
            }
            row
        })
        .collect();

    marching_squares_from_grid(&rows, &levels, x_min, y_min, x_max, y_max)
}

/// Validate the persisted contour limits before a runtime command creates a
/// curve. Extraction also defends these limits, but rejecting early preserves
/// command atomicity and matches document persistence rules.
pub fn validate_contour_levels(levels: &[f64]) -> Result<(), String> {
    if levels.len() > crate::validation::MAX_CONTOUR_LEVELS {
        return Err(format!(
            "contour level count {} exceeds maximum {}",
            levels.len(),
            crate::validation::MAX_CONTOUR_LEVELS
        ));
    }

    let work = levels
        .len()
        .checked_mul(MAX_IMPLICIT_GRID_CELLS)
        .ok_or_else(|| "contour work budget overflowed".to_string())?;
    if work > MAX_MARCHING_SQUARES_WORK_UNITS {
        return Err(format!(
            "contour work budget {} exceeds maximum {}",
            work, MAX_MARCHING_SQUARES_WORK_UNITS
        ));
    }

    for (index, level) in levels.iter().copied().enumerate() {
        if !level.is_finite() {
            return Err(format!("contour level {index} must be finite"));
        }
        if levels[..index].contains(&level) {
            return Err("contains duplicate contour levels".to_string());
        }
    }
    Ok(())
}

/// Extract contour segments from an already evaluated scalar grid.
///
/// Work is charged before every cell visit, including cells with no crossing,
/// and segments are retained as they are produced. This keeps both budgets
/// bounded without materializing a dense contour level first.
pub fn marching_squares_from_grid(
    rows: &[Vec<f64>],
    levels: &[f64],
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
) -> ImplicitCurveSegments {
    let grid_size = rows.len().saturating_sub(1);
    if grid_size == 0
        || !x_min.is_finite()
        || !y_min.is_finite()
        || !x_max.is_finite()
        || !y_max.is_finite()
        || rows.iter().any(|row| row.len() != grid_size + 1)
    {
        return Vec::new();
    }

    let dx = (x_max - x_min) / grid_size as f64;
    let dy = (y_max - y_min) / grid_size as f64;
    if dx == 0.0 || dy == 0.0 || !dx.is_finite() || !dy.is_finite() {
        return Vec::new();
    }

    let mut remaining_work = MAX_MARCHING_SQUARES_WORK_UNITS;
    let mut remaining_segments = MAX_MARCHING_SQUARES_SEGMENTS;
    let mut contours = Vec::with_capacity(levels.len().min(crate::validation::MAX_CONTOUR_LEVELS));
    for level in levels
        .iter()
        .copied()
        .take(crate::validation::MAX_CONTOUR_LEVELS)
    {
        if remaining_work == 0 || remaining_segments == 0 {
            break;
        }
        let segments = marching_squares_level(
            rows,
            level,
            (x_min, y_min),
            (dx, dy),
            &mut remaining_work,
            &mut remaining_segments,
        );
        contours.push((level, segments));
    }
    contours
}

fn marching_squares_level(
    rows: &[Vec<f64>],
    level: f64,
    (x_min, y_min): (f64, f64),
    (dx, dy): (f64, f64),
    remaining_work: &mut usize,
    remaining_segments: &mut usize,
) -> Vec<(Point2, Point2)> {
    let grid_size = rows.len().saturating_sub(1);
    if grid_size == 0 {
        return Vec::new();
    }

    let mut segments =
        Vec::with_capacity(grid_size.saturating_mul(2).max(64).min(*remaining_segments));
    'cells: for i in 0..grid_size {
        let x0 = x_min + i as f64 * dx;
        let x1 = x0 + dx;
        for j in 0..grid_size {
            if *remaining_work == 0 || *remaining_segments == 0 {
                break 'cells;
            }
            *remaining_work -= 1;
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
                if denom.abs() < f64::EPSILON * (va.abs() + vb.abs()).max(1.0) {
                    (pa + pb) * 0.5
                } else {
                    let t = (va - level) / denom;
                    pa + t * (pb - pa)
                }
            };

            let mut push = |a: Point2, b: Point2| {
                if *remaining_segments > 0 {
                    segments.push((a, b));
                    *remaining_segments -= 1;
                }
            };

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
            if *remaining_segments == 0 {
                break 'cells;
            }
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImplicitCurveObj, RelationOperator};
    use std::collections::HashMap;

    fn make_circle() -> ImplicitCurveObj {
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq)
    }

    #[test]
    fn test_circle_segments_centroid_is_origin() {
        // Diagnóstico: el render del ImplicitCurve en producción mostraba el
        // círculo desplazado. Este test verifica que el centroide de los
        // segmentos está cerca de (0, 0), NO cerca de otro punto.
        let ic = make_circle();
        let view_bounds = (-2.0, 2.0, -2.0, 2.0);
        let segments = evaluate_implicit_curve(&ic, view_bounds, 256, &HashMap::new());
        assert!(!segments.is_empty(), "should produce at least one level");
        let (_, segs) = &segments[0];
        assert!(!segs.is_empty(), "should produce at least one segment");
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut n = 0;
        for (a, b) in segs {
            cx += a.x + b.x;
            cy += a.y + b.y;
            n += 2;
        }
        cx /= n as f64;
        cy /= n as f64;
        assert!(
            cx.abs() < 0.05,
            "centroide x = {} (expected ~0). Si falla, el render está dibujando \
             los segmentos en una posición desplazada. El bug típico es que \
             `world_to_screen` recibe un `screen_size` que no coincide con el \
             canvas, o que `padded_snapped_bounds` está mal calibrado.",
            cx
        );
        assert!(cy.abs() < 0.05, "centroide y = {} (expected ~0)", cy);
    }

    #[test]
    fn test_circle_radius_is_one() {
        // Verifica que los segmentos están a distancia 1 del origen
        // (con tolerancia por la discretización del marching squares).
        let ic = make_circle();
        let view_bounds = (-2.0, 2.0, -2.0, 2.0);
        let segments = evaluate_implicit_curve(&ic, view_bounds, 256, &HashMap::new());
        let (_, segs) = &segments[0];
        for (a, b) in segs {
            let da = (a.x * a.x + a.y * a.y).sqrt();
            let db = (b.x * b.x + b.y * b.y).sqrt();
            assert!((da - 1.0).abs() < 0.05, "punto a en círculo: r = {}", da);
            assert!((db - 1.0).abs() < 0.05, "punto b en círculo: r = {}", db);
        }
    }

    #[test]
    fn test_disk_region_segments_fill_inside() {
        // Para `x^2 + y^2 <= 1` (región), los segmentos del marching squares
        // forman el **contorno** de la región, no el interior. El relleno
        // del interior se hace por separado en el render (scanline fill).
        // Este test verifica que los segmentos están en el contorno.
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::LessEq);
        let view_bounds = (-2.0, 2.0, -2.0, 2.0);
        let segments = evaluate_implicit_curve(&ic, view_bounds, 256, &HashMap::new());
        let (_, segs) = &segments[0];
        assert!(!segs.is_empty(), "should produce segments");
        for (a, b) in segs {
            let da = (a.x * a.x + a.y * a.y).sqrt();
            let db = (b.x * b.x + b.y * b.y).sqrt();
            assert!(
                (da - 1.0).abs() < 0.05,
                "el contorno del disco debe estar a r=1, pero a está en r={}",
                da
            );
            assert!(
                (db - 1.0).abs() < 0.05,
                "el contorno del disco debe estar a r=1, pero b está en r={}",
                db
            );
        }
    }

    #[test]
    fn implicit_grid_sample_count_rejects_oversized_and_overflowing_dimensions() {
        assert_eq!(
            implicit_grid_sample_count(MAX_IMPLICIT_GRID_SIZE),
            Some((MAX_IMPLICIT_GRID_SIZE + 1) * (MAX_IMPLICIT_GRID_SIZE + 1))
        );
        assert_eq!(implicit_grid_sample_count(MAX_IMPLICIT_GRID_SIZE + 1), None);
        assert_eq!(implicit_grid_sample_count(usize::MAX), None);
    }
}
