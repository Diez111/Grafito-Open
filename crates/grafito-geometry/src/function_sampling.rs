//! Honest sampling of explicit functions `y = f(x)`.
//!
//! A sampler that "never lies" must not draw vertical-looking segments across
//! asymptotes or jumps. [`sample_function`] evaluates the expression on a
//! uniform grid, splits the polyline wherever a discontinuity is detected, and
//! classifies each split with [`BreakKind`].
//!
//! Budgets: expressions longer than [`MAX_EXPR_LENGTH`] are rejected, and the
//! number of sample points is capped at [`MAX_SAMPLE_POINTS`] (mirrors the
//! workspace `MAX_OBJECT_COUNT` budget).

use crate::Point2;

/// Maximum accepted expression length (mirrors `MAX_EXPR_LENGTH`).
pub const MAX_EXPR_LENGTH: usize = 2000;
/// Maximum number of sample points per call (mirrors `MAX_OBJECT_COUNT`).
pub const MAX_SAMPLE_POINTS: usize = 5000;

/// Smallest probe radius used when classifying a break.
const MIN_PROBE: f64 = 1e-12;
/// Inner magnitude must exceed the outer magnitude by this factor to call it a pole.
const POLE_GROWTH: f64 = 4.0;
/// Relative agreement of both sides required for a removable hole.
const HOLE_REL_TOL: f64 = 1e-6;
/// Steepness factor above which a finite segment becomes suspicious.
const STEEP_FACTOR: f64 = 8.0;

/// Honest classification of a discontinuity in `y = f(x)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakKind {
    /// Both sides blow up (e.g. `1/x` at 0, `tan` at pi/2).
    Pole,
    /// Both sides stay finite but disagree (e.g. `abs(x)/x` at 0).
    Jump,
    /// Single undefined point with agreeing limits (e.g. `(x*x-1)/(x-1)` at 1).
    RemovableHole,
    /// Function defined on one side only (e.g. `sqrt(x)` at 0).
    DomainEdge,
}

/// A detected discontinuity at `x` with its classification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakInfo {
    /// Abscissa of the break.
    pub x: f64,
    /// Classification of the break.
    pub kind: BreakKind,
}

/// Result of [`sample_function`]: drawable polylines plus detected breaks.
#[derive(Debug, Clone, PartialEq)]
pub struct SampledFunction {
    /// Drawable pieces; never connected across a break or a gap.
    pub polylines: Vec<Vec<Point2>>,
    /// Detected discontinuities, sorted by `x`.
    pub breaks: Vec<BreakInfo>,
}

/// Errors returned by [`sample_function`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleError {
    /// Empty expression.
    EmptyExpression,
    /// Expression longer than the budget.
    TooLong { len: usize, max: usize },
    /// Invalid range (`x_min >= x_max` or non-finite).
    BadRange,
    /// Fewer than two sample points requested.
    TooFewPoints { got: usize },
}

impl std::fmt::Display for SampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyExpression => write!(f, "empty expression"),
            Self::TooLong { len, max } => {
                write!(f, "expression too long: {len} chars (max {max})")
            }
            Self::BadRange => write!(f, "invalid range"),
            Self::TooFewPoints { got } => write!(f, "need at least 2 points, got {got}"),
        }
    }
}

impl std::error::Error for SampleError {}

/// Finite-only evaluation helper: `None` for errors, `NaN` and infinities.
pub fn eval_sample(expr: &str, x: f64) -> Option<f64> {
    if !x.is_finite() {
        return None;
    }
    match crate::expr::evaluate(expr, &[("x".to_string(), x)]) {
        Ok(y) if y.is_finite() => Some(y),
        _ => None,
    }
}

/// Classify the behavior of `expr` around `x0`.
///
/// `step` is the local sampling width; probes are taken at fractions of it.
/// Returns `None` when the function looks continuous there.
pub fn classify_break_at(expr: &str, x0: f64, step: f64) -> Option<BreakKind> {
    if !x0.is_finite() || !step.is_finite() {
        return None;
    }
    let h = step.abs().max(MIN_PROBE);
    let outer_l = eval_sample(expr, x0 - h);
    let inner_l = eval_sample(expr, x0 - h * 0.25);
    let inner_r = eval_sample(expr, x0 + h * 0.25);
    let outer_r = eval_sample(expr, x0 + h);
    let mid = eval_sample(expr, x0);

    let left_finite = outer_l.is_some() || inner_l.is_some();
    let right_finite = inner_r.is_some() || outer_r.is_some();

    if !left_finite && !right_finite {
        // Nothing finite nearby: only claim a domain edge if the center is
        // undefined too; otherwise stay silent (no evidence).
        if mid.is_none() {
            return Some(BreakKind::DomainEdge);
        }
        return None;
    }
    if !left_finite || !right_finite {
        return Some(BreakKind::DomainEdge);
    }

    let y_l = inner_l.or(outer_l);
    let y_r = inner_r.or(outer_r);
    let (Some(y_l), Some(y_r)) = (y_l, y_r) else {
        return Some(BreakKind::DomainEdge);
    };

    // Pole: inner samples tower over the outer ones and the center is
    // undefined (the function blows up between the probes).
    let inner_peak = y_l.abs().max(y_r.abs());
    let outer_peak = outer_l
        .map(f64::abs)
        .into_iter()
        .chain(outer_r.map(f64::abs))
        .fold(0.0_f64, f64::max)
        .max(1.0);
    if mid.is_none() && inner_peak >= POLE_GROWTH * outer_peak {
        return Some(BreakKind::Pole);
    }

    // Removable hole: single undefined point, both sides agree up to the
    // local trend (coarse probes separate the sides by ~slope * dx, so the
    // bare relative check would reject genuine holes).
    let gap = (y_r - y_l).abs();
    let scale = 1.0 + y_l.abs() + y_r.abs();
    let mut hole_tol = HOLE_REL_TOL * scale;
    if let (Some(a), Some(b)) = (outer_l, outer_r) {
        hole_tol = hole_tol.max(0.5 * (b - a).abs() + MIN_PROBE * scale);
    }
    if gap <= hole_tol {
        if mid.is_none() {
            return Some(BreakKind::RemovableHole);
        }
        return None;
    }

    // Midpoint between both sides: steep but continuous, not a break.
    if let Some(y_m) = mid {
        let lo = y_l.min(y_r);
        let hi = y_l.max(y_r);
        let tol = HOLE_REL_TOL * scale + MIN_PROBE;
        if y_m >= lo - tol && y_m <= hi + tol {
            return None;
        }
    }

    Some(BreakKind::Jump)
}

/// Sample `y = f(x)` on `[x_min, x_max]` with `n` uniform points.
///
/// Splits the polyline at every detected break or gap and classifies each
/// split with [`classify_break_at`]. `n` is clamped to `2..=MAX_SAMPLE_POINTS`.
pub fn sample_function(
    expr: &str,
    x_min: f64,
    x_max: f64,
    n: usize,
) -> Result<SampledFunction, SampleError> {
    if expr.trim().is_empty() {
        return Err(SampleError::EmptyExpression);
    }
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(SampleError::TooLong {
            len: expr.len(),
            max: MAX_EXPR_LENGTH,
        });
    }
    if !x_min.is_finite() || !x_max.is_finite() || x_min >= x_max {
        return Err(SampleError::BadRange);
    }
    if n < 2 {
        return Err(SampleError::TooFewPoints { got: n });
    }
    let n = n.min(MAX_SAMPLE_POINTS);

    let dx = (x_max - x_min) / (n as f64 - 1.0);
    let mut ys: Vec<Option<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let x = x_min + i as f64 * dx;
        ys.push(eval_sample(expr, x));
    }

    let mut out = SampledFunction {
        polylines: Vec::new(),
        breaks: Vec::new(),
    };
    let mut current: Vec<Point2> = Vec::new();

    for i in 0..n {
        let x = x_min + i as f64 * dx;
        match ys[i] {
            Some(y) => {
                if let (Some(prev), Some(&Some(prev_y))) = (current.last(), ys[..i].last()) {
                    let _ = prev;
                    let steep = STEEP_FACTOR * dx * (1.0 + prev_y.abs() + y.abs());
                    if (y - prev_y).abs() > steep.max(1.0) {
                        let mid_x = (x - dx + x) * 0.5;
                        match classify_break_at(expr, mid_x, dx) {
                            Some(kind) => {
                                if !current.is_empty() {
                                    out.polylines.push(std::mem::take(&mut current));
                                }
                                push_break(&mut out, mid_x, kind, dx);
                            }
                            None => current.push(Point2::new(x, y)),
                        }
                        continue;
                    }
                }
                current.push(Point2::new(x, y));
            }
            None => {
                // Gap: close the current piece. Only classify at the
                // finite<->gap transition; sustained gaps carry no new
                // information and would spam duplicate breaks.
                if !current.is_empty() {
                    out.polylines.push(std::mem::take(&mut current));
                }
                let neighbor_finite =
                    (i > 0 && ys[i - 1].is_some()) || (i + 1 < n && ys[i + 1].is_some());
                if neighbor_finite {
                    if let Some(kind) = classify_break_at(expr, x, dx) {
                        push_break(&mut out, x, kind, dx);
                    }
                }
            }
        }
    }
    if !current.is_empty() {
        out.polylines.push(current);
    }
    Ok(out)
}

fn push_break(out: &mut SampledFunction, x: f64, kind: BreakKind, dx: f64) {
    let sep = dx * 0.75 + MIN_PROBE;
    if out
        .breaks
        .last()
        .is_some_and(|last: &BreakInfo| (last.x - x).abs() < sep)
    {
        return;
    }
    out.breaks.push(BreakInfo { x, kind });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pole_1_over_x_splits_into_two_pieces() {
        let out = sample_function("1/x", -1.0, 1.0, 201).expect("valid range");
        assert_eq!(out.polylines.len(), 2, "pole must split, got {out:?}");
        assert!(
            out.breaks.iter().any(|b| b.kind == BreakKind::Pole),
            "expected a Pole, got {:?}",
            out.breaks
        );
        let bx = out
            .breaks
            .iter()
            .find(|b| b.kind == BreakKind::Pole)
            .map(|b| b.x)
            .unwrap_or(f64::NAN);
        assert!(bx.abs() < 0.05, "pole near x=0, got {bx}");
    }

    #[test]
    fn jump_abs_x_over_x_is_a_jump() {
        let out = sample_function("abs(x)/x", -1.0, 1.0, 201).expect("valid range");
        assert!(
            out.breaks.iter().any(|b| b.kind == BreakKind::Jump),
            "expected a Jump, got {:?}",
            out.breaks
        );
        assert!(out.polylines.len() >= 2);
    }

    #[test]
    fn removable_hole_is_detected() {
        let out = sample_function("(x*x-1)/(x-1)", 0.0, 2.0, 201).expect("valid range");
        assert!(
            out.breaks
                .iter()
                .any(|b| b.kind == BreakKind::RemovableHole && (b.x - 1.0).abs() < 0.05),
            "expected a RemovableHole at x=1, got {:?}",
            out.breaks
        );
    }

    #[test]
    fn sqrt_has_a_domain_edge_at_zero() {
        let out = sample_function("sqrt(x)", -1.0, 1.0, 101).expect("valid range");
        assert!(
            out.breaks.iter().any(|b| b.kind == BreakKind::DomainEdge),
            "expected a DomainEdge, got {:?}",
            out.breaks
        );
        for piece in &out.polylines {
            for p in piece {
                assert!(p.x >= -1e-9, "no points left of the domain: {p:?}");
            }
        }
    }

    #[test]
    fn steep_continuous_function_has_no_breaks() {
        let out = sample_function("x*x", -2.0, 2.0, 101).expect("valid range");
        assert!(out.breaks.is_empty(), "unexpected breaks: {:?}", out.breaks);
        assert_eq!(out.polylines.len(), 1);
    }

    #[test]
    fn rejects_overlong_expression() {
        let expr = "x".repeat(MAX_EXPR_LENGTH + 1);
        assert!(matches!(
            sample_function(&expr, 0.0, 1.0, 10),
            Err(SampleError::TooLong { .. })
        ));
    }

    #[test]
    fn rejects_bad_range_and_too_few_points() {
        assert!(matches!(
            sample_function("x", 1.0, 1.0, 10),
            Err(SampleError::BadRange)
        ));
        assert!(matches!(
            sample_function("x", 0.0, 1.0, 1),
            Err(SampleError::TooFewPoints { .. })
        ));
        assert!(matches!(
            sample_function("  ", 0.0, 1.0, 10),
            Err(SampleError::EmptyExpression)
        ));
    }
}
