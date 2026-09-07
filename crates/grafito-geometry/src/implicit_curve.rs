//! Marching squares for implicit curves `F(x, y) = 0`.
//!
//! Cells where all four corners are finite are contoured with linear edge
//! interpolation. The ambiguous saddle cases (5 and 10) are resolved with the
//! Asymptotic Decider (Nielson–Hamann): the sign of `F` at the cell center
//! decides which pair of edges is connected. Cells with unknown corners or an
//! unknown center emit no segments instead of guessing.
//!
//! Budgets: expressions longer than [`MAX_EXPR_LENGTH`] are rejected, the grid
//! is capped at [`MAX_GRID_CELLS`] cells, and output at [`MAX_IMPLICIT_SEGMENTS`]
//! segments (mirrors the workspace `MAX_OBJECT_COUNT` budget). When the cap is
//! hit the tracer stops and reports [`ImplicitTrace::truncated`] honestly.

use crate::{Point2, AABB};

/// Maximum accepted expression length (mirrors `MAX_EXPR_LENGTH`).
pub const MAX_EXPR_LENGTH: usize = 2000;
/// Maximum grid cells per trace call (256 x 256).
pub const MAX_GRID_CELLS: usize = 65_536;
/// Maximum output segments per trace call (mirrors `MAX_OBJECT_COUNT`).
pub const MAX_IMPLICIT_SEGMENTS: usize = 5000;
/// Maximum grid resolution per axis.
pub const MAX_GRID_AXIS: usize = 256;

/// Resolution of an ambiguous saddle cell (marching squares cases 5 and 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SaddleResolution {
    /// Cell center is inside (`F < 0`): the inside region connects through it.
    ThroughCenter,
    /// Cell center is outside (`F >= 0`): the inside corners stay separate.
    AroundCenter,
    /// Center value unknown (`NaN`): emit nothing rather than guess.
    Gap,
}

/// Output of [`trace_implicit`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImplicitTrace {
    /// Contour segments in world coordinates.
    pub segments: Vec<(Point2, Point2)>,
    /// `true` when the segment cap stopped the trace early.
    pub truncated: bool,
}

/// Errors returned by [`trace_implicit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplicitError {
    /// Empty expression.
    EmptyExpression,
    /// Expression longer than the budget.
    TooLong { len: usize, max: usize },
    /// Degenerate or non-finite bounds.
    BadBounds,
    /// Grid resolution of zero.
    BadResolution,
}

impl std::fmt::Display for ImplicitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyExpression => write!(f, "empty expression"),
            Self::TooLong { len, max } => {
                write!(f, "expression too long: {len} chars (max {max})")
            }
            Self::BadBounds => write!(f, "invalid bounds"),
            Self::BadResolution => write!(f, "grid resolution must be at least 1"),
        }
    }
}

impl std::error::Error for ImplicitError {}

/// Finite-only evaluation helper: `None` for errors, `NaN` and infinities.
pub fn eval_implicit(expr: &str, x: f64, y: f64) -> Option<f64> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    match crate::expr::evaluate(expr, &[("x".to_string(), x), ("y".to_string(), y)]) {
        Ok(v) if v.is_finite() => Some(v),
        _ => None,
    }
}

/// Case index for corners `[bottom_left, bottom_right, top_right, top_left]`.
/// Bit `i` is set when corner `i` is inside (`F < 0`).
pub fn marching_case_index(inside: [bool; 4]) -> u8 {
    let mut case = 0_u8;
    for (i, yes) in inside.iter().enumerate() {
        if *yes {
            case |= 1 << i;
        }
    }
    case
}

/// Asymptotic Decider for saddle cases 5 (`0b0101`) and 10 (`0b1010`).
///
/// `center` is `F` at the cell center (`None` when unknown). A negative center
/// means the inside region connects through it ([`SaddleResolution::ThroughCenter`]);
/// otherwise the inside corners stay separate.
pub fn resolve_saddle(case: u8, center: Option<f64>) -> SaddleResolution {
    debug_assert!(
        case == 5 || case == 10,
        "resolve_saddle only handles cases 5 and 10"
    );
    match center {
        None => SaddleResolution::Gap,
        Some(c) if !c.is_finite() => SaddleResolution::Gap,
        Some(c) if c < 0.0 => SaddleResolution::ThroughCenter,
        _ => SaddleResolution::AroundCenter,
    }
}

/// Edge numbering for the case tables: 0 = bottom (bl→br),
/// 1 = right (br→tr), 2 = top (tr→tl), 3 = left (tl→bl).
/// Interpolate the zero crossing on the edge between two corners.
/// Returns `None` when either value is unknown or both share a sign.
fn crossing(a: Point2, fa: f64, b: Point2, fb: f64) -> Option<Point2> {
    if !fa.is_finite() || !fb.is_finite() {
        return None;
    }
    if (fa < 0.0) == (fb < 0.0) {
        return None;
    }
    let denom = (fa - fb).abs();
    if denom <= f64::MIN_POSITIVE {
        return None;
    }
    let t = fa.abs() / denom;
    Some(Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)))
}

/// Edge pairs for a non-saddle case. Returns up to two segments.
fn case_edges(case: u8) -> Option<[(u8, u8); 2]> {
    let pair = match case {
        1 | 14 => [(3, 0), (u8::MAX, u8::MAX)],
        2 | 13 => [(0, 1), (u8::MAX, u8::MAX)],
        3 | 12 => [(3, 1), (u8::MAX, u8::MAX)],
        4 | 11 => [(1, 2), (u8::MAX, u8::MAX)],
        6 | 9 => [(0, 2), (u8::MAX, u8::MAX)],
        7 | 8 => [(3, 2), (u8::MAX, u8::MAX)],
        _ => return None,
    };
    Some(pair)
}

/// Edge pairs for a saddle case given the decider resolution.
///
/// Case 5 has `bl` and `tr` inside; case 10 has `br` and `tl` inside.
/// `ThroughCenter` connects the inside region through the middle, so the
/// contour hugs the outside corners; `AroundCenter` does the opposite.
fn saddle_edges(case: u8, resolution: SaddleResolution) -> [(u8, u8); 2] {
    match (case, resolution) {
        (5, SaddleResolution::ThroughCenter) => [(0, 1), (2, 3)],
        (5, _) => [(3, 0), (1, 2)],
        (_, SaddleResolution::ThroughCenter) => [(3, 0), (1, 2)],
        _ => [(0, 1), (2, 3)],
    }
}

/// Trace `F(x, y) = 0` inside `bounds` on an `nx` by `ny` grid.
///
/// Resolutions are clamped to `1..=MAX_GRID_AXIS`. Segments are capped at
/// [`MAX_IMPLICIT_SEGMENTS`]; when the cap is hit the trace stops early with
/// `truncated = true`.
pub fn trace_implicit(
    expr: &str,
    bounds: AABB,
    nx: usize,
    ny: usize,
) -> Result<ImplicitTrace, ImplicitError> {
    if expr.trim().is_empty() {
        return Err(ImplicitError::EmptyExpression);
    }
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(ImplicitError::TooLong {
            len: expr.len(),
            max: MAX_EXPR_LENGTH,
        });
    }
    if !bounds.min.x.is_finite()
        || !bounds.min.y.is_finite()
        || !bounds.max.x.is_finite()
        || !bounds.max.y.is_finite()
        || bounds.min.x >= bounds.max.x
        || bounds.min.y >= bounds.max.y
    {
        return Err(ImplicitError::BadBounds);
    }
    if nx == 0 || ny == 0 {
        return Err(ImplicitError::BadResolution);
    }
    let nx = nx.min(MAX_GRID_AXIS);
    let ny = ny.min(MAX_GRID_AXIS);
    // Clamped axes imply nx * ny <= 256 * 256 == MAX_GRID_CELLS, so the cell
    // budget always holds here.

    let dx = (bounds.max.x - bounds.min.x) / nx as f64;
    let dy = (bounds.max.y - bounds.min.y) / ny as f64;

    // Corner values, row-major over (ny+1) x (nx+1).
    let mut grid: Vec<Option<f64>> = Vec::with_capacity((nx + 1) * (ny + 1));
    for j in 0..=ny {
        for i in 0..=nx {
            let x = bounds.min.x + i as f64 * dx;
            let y = bounds.min.y + j as f64 * dy;
            grid.push(eval_implicit(expr, x, y));
        }
    }
    let at = |i: usize, j: usize| -> Option<f64> { grid[j * (nx + 1) + i] };

    let mut out = ImplicitTrace {
        segments: Vec::new(),
        truncated: false,
    };

    for j in 0..ny {
        for i in 0..nx {
            let (Some(fbl), Some(fbr), Some(ftr), Some(ftl)) =
                (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1))
            else {
                continue; // Unknown corner: skip the cell, never guess.
            };
            let case = marching_case_index([fbl < 0.0, fbr < 0.0, ftr < 0.0, ftl < 0.0]);
            if case == 0 || case == 15 {
                continue;
            }

            let bl = Point2::new(bounds.min.x + i as f64 * dx, bounds.min.y + j as f64 * dy);
            let br = Point2::new(bl.x + dx, bl.y);
            let tr = Point2::new(bl.x + dx, bl.y + dy);
            let tl = Point2::new(bl.x, bl.y + dy);

            // Crossing point per edge, computed lazily via helper.
            let x_edge = |edge: u8| -> Option<Point2> {
                match edge {
                    0 => crossing(bl, fbl, br, fbr),
                    1 => crossing(br, fbr, tr, ftr),
                    2 => crossing(tr, ftr, tl, ftl),
                    _ => crossing(tl, ftl, bl, fbl),
                }
            };

            let pairs: [(u8, u8); 2] = if case == 5 || case == 10 {
                let cx = bl.x + dx * 0.5;
                let cy = bl.y + dy * 0.5;
                let center = eval_implicit(expr, cx, cy);
                let resolution = resolve_saddle(case, center);
                if resolution == SaddleResolution::Gap {
                    continue;
                }
                saddle_edges(case, resolution)
            } else if let Some(pairs) = case_edges(case) {
                pairs
            } else {
                continue;
            };

            for (e0, e1) in pairs {
                if e0 == u8::MAX {
                    continue;
                }
                let (Some(p0), Some(p1)) = (x_edge(e0), x_edge(e1)) else {
                    continue;
                };
                // Guard against degenerate zero-length segments.
                if (p0.x - p1.x).hypot(p0.y - p1.y) <= f64::MIN_POSITIVE {
                    continue;
                }
                out.segments.push((p0, p1));
                if out.segments.len() >= MAX_IMPLICIT_SEGMENTS {
                    out.truncated = true;
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_traces_a_closed_loop() {
        let bounds = AABB::new(Point2::new(-1.5, -1.5), Point2::new(1.5, 1.5));
        let out = trace_implicit("x*x+y*y-1", bounds, 24, 24).expect("valid trace");
        assert!(!out.segments.is_empty(), "circle must produce segments");
        assert!(!out.truncated);
        for (a, b) in &out.segments {
            for p in [a, b] {
                let f = p.x * p.x + p.y * p.y - 1.0;
                assert!(f.abs() < 0.05, "segment endpoint off curve: {p:?} f={f}");
            }
        }
    }

    #[test]
    fn saddle_case_uses_the_center_sign() {
        // x*y + 0.1 on [-1,1]^2, single cell: corners bl>0, br<0, tr>0, tl<0.
        let bounds = AABB::new(Point2::new(-1.0, -1.0), Point2::new(1.0, 1.0));
        let out = trace_implicit("x*y+0.1", bounds, 1, 1).expect("valid trace");
        assert_eq!(out.segments.len(), 2, "saddle cell emits 2, got {out:?}");
        // Center F(0,0) = 0.1 > 0 -> AroundCenter -> pairs (0,1) + (2,3).
        for (a, b) in &out.segments {
            for p in [a, b] {
                let f = p.x * p.y + 0.1;
                assert!(f.abs() < 1e-9, "endpoint must sit on the curve: {p:?}");
            }
        }
        // Bottom edge (y=-1) crossing at x=0.1 and right edge (x=1) at y=-0.1.
        let mut xs: Vec<f64> = out.segments.iter().flat_map(|(a, b)| [a.x, b.x]).collect();
        xs.sort_by(|a, b| a.total_cmp(b));
        assert!(
            (xs[0] + 1.0).abs() < 1e-9,
            "left-edge crossing x=-1, got {xs:?}"
        );
        assert!(
            (xs[3] - 1.0).abs() < 1e-9,
            "right-edge crossing x=1, got {xs:?}"
        );
    }

    #[test]
    fn through_center_links_the_other_way() {
        // x*y - 0.1 on [-1,1]^2: center F(0,0) = -0.1 < 0 -> ThroughCenter.
        let bounds = AABB::new(Point2::new(-1.0, -1.0), Point2::new(1.0, 1.0));
        let out = trace_implicit("x*y-0.1", bounds, 1, 1).expect("valid trace");
        assert_eq!(out.segments.len(), 2, "saddle cell emits 2, got {out:?}");
        let mut ys: Vec<f64> = out.segments.iter().flat_map(|(a, b)| [a.y, b.y]).collect();
        ys.sort_by(|a, b| a.total_cmp(b));
        assert!((ys[0] + 1.0).abs() < 1e-9, "bottom-edge y=-1, got {ys:?}");
        assert!((ys[3] - 1.0).abs() < 1e-9, "top-edge y=1, got {ys:?}");
    }

    #[test]
    fn unknown_regions_emit_nothing() {
        // sqrt is undefined for x < 0: cells there are skipped honestly.
        let bounds = AABB::new(Point2::new(-2.0, -2.0), Point2::new(2.0, 2.0));
        let out = trace_implicit("sqrt(x)+sqrt(y)-1", bounds, 8, 8).expect("valid trace");
        assert!(!out.truncated);
        for (a, b) in &out.segments {
            assert!(a.x >= -1e-9 && b.x >= -1e-9);
            assert!(a.y >= -1e-9 && b.y >= -1e-9);
        }
    }

    #[test]
    fn resolve_saddle_gap_on_unknown_center() {
        assert_eq!(resolve_saddle(5, None), SaddleResolution::Gap);
        assert_eq!(resolve_saddle(10, Some(f64::NAN)), SaddleResolution::Gap);
        assert_eq!(
            resolve_saddle(5, Some(-0.5)),
            SaddleResolution::ThroughCenter
        );
        assert_eq!(
            resolve_saddle(10, Some(0.0)),
            SaddleResolution::AroundCenter
        );
    }

    #[test]
    fn rejects_bad_inputs() {
        let bounds = AABB::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0));
        assert!(matches!(
            trace_implicit("", bounds, 4, 4),
            Err(ImplicitError::EmptyExpression)
        ));
        let long = "x".repeat(MAX_EXPR_LENGTH + 1);
        assert!(matches!(
            trace_implicit(&long, bounds, 4, 4),
            Err(ImplicitError::TooLong { .. })
        ));
        let bad = AABB::new(Point2::new(1.0, 1.0), Point2::new(0.0, 0.0));
        assert!(matches!(
            trace_implicit("x", bad, 4, 4),
            Err(ImplicitError::BadBounds)
        ));
        assert!(matches!(
            trace_implicit("x", bounds, 0, 4),
            Err(ImplicitError::BadResolution)
        ));
    }
}
