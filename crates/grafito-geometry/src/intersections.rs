//! Geometric intersection algorithms.
//!
//! Computes intersection points between pairs of geometric primitives:
//! - Line-Line (solves 2x2 linear system)
//! - Line-Circle (quadratic discriminant)
//! - Circle-Circle (radical axis method)
//! - Segment-Segment (orientation tests)
//! - Function-Line (Newton root finding)
//! - Function-Function (Newton root finding)
//! - Line-Conic (exact quadratic substitution, [`line_conic`])
//! - Conic-Conic (circle pairs delegated; general case is an honest
//!   [`ConicConicOutcome::Unsupported`] stub, see [`conic_conic`])
//!
//! Tolerances in the conic routines derive from [`crate::lines::geom_eps`]
//! evaluated at the problem scale instead of fixed magic constants.

use crate::lines::geom_eps;
use crate::Point2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntersectionResult {
    None,
    One(Point2),
    Two(Point2, Point2),
    Infinite,
}

/// Intersection of two infinite lines defined by (p1, p2) and (q1, q2).
pub fn line_line(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> IntersectionResult {
    let dx1 = a2.x - a1.x;
    let dy1 = a2.y - a1.y;
    let dx2 = b2.x - b1.x;
    let dy2 = b2.y - b1.y;

    if dx1.hypot(dy1) < 1e-12 || dx2.hypot(dy2) < 1e-12 {
        return IntersectionResult::None;
    }

    let det = dx1 * dy2 - dy1 * dx2;

    if det.abs() < 1e-12 {
        let d = (b1.x - a1.x) * dy1 - (b1.y - a1.y) * dx1;
        if d.abs() < 1e-12 {
            IntersectionResult::Infinite
        } else {
            IntersectionResult::None
        }
    } else {
        let t = ((b1.x - a1.x) * dy2 - (b1.y - a1.y) * dx2) / det;
        let x = a1.x + t * dx1;
        let y = a1.y + t * dy1;
        IntersectionResult::One(Point2::new(x, y))
    }
}

/// Intersection of a line (p1, p2) with a circle (center, radius).
pub fn line_circle(p1: Point2, p2: Point2, center: Point2, radius: f64) -> IntersectionResult {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let fx = p1.x - center.x;
    let fy = p1.y - center.y;

    let a = dx * dx + dy * dy;
    let b = 2.0 * (fx * dx + fy * dy);
    let c = fx * fx + fy * fy - radius * radius;

    if !radius.is_finite() || radius <= 0.0 || a < 1e-24 {
        return IntersectionResult::None;
    }

    let discriminant = b * b - 4.0 * a * c;

    if discriminant < -1e-12 {
        IntersectionResult::None
    } else if discriminant.abs() < 1e-12 {
        let t = -b / (2.0 * a);
        let x = p1.x + t * dx;
        let y = p1.y + t * dy;
        IntersectionResult::One(Point2::new(x, y))
    } else {
        let sqrt_d = discriminant.sqrt();
        let t1 = (-b - sqrt_d) / (2.0 * a);
        let t2 = (-b + sqrt_d) / (2.0 * a);
        let x1 = p1.x + t1 * dx;
        let y1 = p1.y + t1 * dy;
        let x2 = p1.x + t2 * dx;
        let y2 = p1.y + t2 * dy;
        IntersectionResult::Two(Point2::new(x1, y1), Point2::new(x2, y2))
    }
}

/// Intersection of two circles (c1, r1) and (c2, r2).
pub fn circle_circle(c1: Point2, r1: f64, c2: Point2, r2: f64) -> IntersectionResult {
    if !r1.is_finite() || !r2.is_finite() || r1 <= 0.0 || r2 <= 0.0 {
        return IntersectionResult::None;
    }

    let dx = c2.x - c1.x;
    let dy = c2.y - c1.y;
    let d = (dx * dx + dy * dy).sqrt();

    if d < 1e-12 && (r1 - r2).abs() < 1e-12 {
        return IntersectionResult::Infinite;
    }

    if d > r1 + r2 + 1e-12 || d < (r1 - r2).abs() - 1e-12 {
        return IntersectionResult::None;
    }

    if (d - (r1 + r2)).abs() < 1e-12 || (d - (r1 - r2).abs()).abs() < 1e-12 {
        let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
        let x = c1.x + a * dx / d;
        let y = c1.y + a * dy / d;
        return IntersectionResult::One(Point2::new(x, y));
    }

    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).sqrt();
    let px = c1.x + a * dx / d;
    let py = c1.y + a * dy / d;

    let rx = -dy * (h / d);
    let ry = dx * (h / d);

    IntersectionResult::Two(Point2::new(px + rx, py + ry), Point2::new(px - rx, py - ry))
}

/// Intersection of two segments.
pub fn segment_segment(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> IntersectionResult {
    let result = line_line(a1, a2, b1, b2);
    match result {
        IntersectionResult::One(p) => {
            if point_on_segment(p, a1, a2) && point_on_segment(p, b1, b2) {
                IntersectionResult::One(p)
            } else {
                IntersectionResult::None
            }
        }
        IntersectionResult::Infinite => {
            let tb1 = project_point_on_line(b1, a1, a2);
            let tb2 = project_point_on_line(b2, a1, a2);
            let t_start = tb1.min(tb2).max(0.0);
            let t_end = tb1.max(tb2).min(1.0);
            if (t_start - t_end).abs() < 1e-12 {
                IntersectionResult::One(Point2::new(
                    a1.x + t_start * (a2.x - a1.x),
                    a1.y + t_start * (a2.y - a1.y),
                ))
            } else if t_start < t_end {
                let p1 = Point2::new(
                    a1.x + t_start * (a2.x - a1.x),
                    a1.y + t_start * (a2.y - a1.y),
                );
                let p2 = Point2::new(a1.x + t_end * (a2.x - a1.x), a1.y + t_end * (a2.y - a1.y));
                IntersectionResult::Two(p1, p2)
            } else {
                IntersectionResult::None
            }
        }
        _ => result,
    }
}

fn point_on_segment(p: Point2, a: Point2, b: Point2) -> bool {
    let d = a.distance(&b);
    if d < 1e-12 {
        return p.distance(&a) < 1e-9;
    }
    let t = ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / (d * d);
    if !(-1e-12..=1.0 + 1e-12).contains(&t) {
        return false;
    }
    let proj = Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
    p.distance(&proj) < 1e-9
}

fn project_point_on_line(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d2 = dx * dx + dy * dy;
    if d2 < 1e-12 {
        return 0.0;
    }
    ((p.x - a.x) * dx + (p.y - a.y) * dy) / d2
}

/// Intersection of a function f(x) with a line y = mx + b.
/// Uses Newton's method from multiple starting points.
pub fn function_line(
    expr: &str,
    slope: f64,
    intercept: f64,
    x_min: f64,
    x_max: f64,
) -> Vec<Point2> {
    let mut roots = find_roots(
        &|x| {
            let fy = crate::expr::evaluate(expr, &[("x".to_string(), x)]).unwrap_or(f64::NAN);
            if fy.is_nan() {
                return f64::NAN;
            }
            fy - (slope * x + intercept)
        },
        x_min,
        x_max,
    );
    for point in &mut roots {
        point.y = slope * point.x + intercept;
    }
    roots
}

/// Intersection of two functions f(x) and g(x).
pub fn function_function(expr_f: &str, expr_g: &str, x_min: f64, x_max: f64) -> Vec<Point2> {
    find_roots(
        &|x| {
            let fy = crate::expr::evaluate(expr_f, &[("x".to_string(), x)]).unwrap_or(f64::NAN);
            let gy = crate::expr::evaluate(expr_g, &[("x".to_string(), x)]).unwrap_or(f64::NAN);
            if fy.is_nan() || gy.is_nan() {
                return f64::NAN;
            }
            fy - gy
        },
        x_min,
        x_max,
    )
}

fn find_roots(f: &dyn Fn(f64) -> f64, x_min: f64, x_max: f64) -> Vec<Point2> {
    let mut roots = Vec::new();
    let steps = 100;
    let dx = (x_max - x_min) / steps as f64;
    let mut prev_y = f(x_min);

    for i in 1..=steps {
        let x = x_min + i as f64 * dx;
        let y = f(x);
        if y.is_nan() || prev_y.is_nan() {
            prev_y = y;
            continue;
        }
        if prev_y * y <= 0.0 {
            let root_x = newton(f, x - dx * 0.5, 30);
            if root_x.is_finite() && root_x >= x_min && root_x <= x_max {
                let fy_at_root = f(root_x);
                if fy_at_root.abs() < 1e-6 {
                    let is_duplicate = roots.iter().any(|r: &Point2| (r.x - root_x).abs() < 1e-6);
                    if !is_duplicate {
                        roots.push(Point2::new(root_x, fy_at_root));
                    }
                }
            }
        }
        prev_y = y;
    }
    roots
}

fn newton(f: &dyn Fn(f64) -> f64, initial: f64, max_iter: usize) -> f64 {
    let h = 1e-6;
    let mut x = initial;
    for _ in 0..max_iter {
        let fx = f(x);
        if fx.abs() < 1e-12 {
            return x;
        }
        let df = (f(x + h) - f(x - h)) / (2.0 * h);
        if df.abs() < 1e-15 {
            return f64::NAN;
        }
        let new_x = x - fx / df;
        if (new_x - x).abs() < 1e-12 {
            return new_x;
        }
        x = new_x;
    }
    if f(x).abs() < 1e-6 {
        x
    } else {
        f64::NAN
    }
}

/// General conic: `a*x^2 + b*x*y + c*y^2 + d*x + e*y + f = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conic {
    /// Coefficient of `x^2`.
    pub a: f64,
    /// Coefficient of `x*y`.
    pub b: f64,
    /// Coefficient of `y^2`.
    pub c: f64,
    /// Coefficient of `x`.
    pub d: f64,
    /// Coefficient of `y`.
    pub e: f64,
    /// Constant term.
    pub f: f64,
}

impl Conic {
    /// Build a conic from general coefficients. Returns `None` for
    /// non-finite coefficients or the all-zero (whole-plane) form.
    pub fn from_general(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Option<Self> {
        let coeffs = [a, b, c, d, e, f];
        if coeffs.iter().any(|v| !v.is_finite()) {
            return None;
        }
        if coeffs.iter().all(|v| *v == 0.0) {
            return None;
        }
        Some(Self { a, b, c, d, e, f })
    }

    /// Circle `(x - cx)^2 + (y - cy)^2 = r^2` as a general conic.
    pub fn circle(center: Point2, radius: f64) -> Option<Self> {
        if !center.x.is_finite() || !center.y.is_finite() {
            return None;
        }
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        Some(Self {
            a: 1.0,
            b: 0.0,
            c: 1.0,
            d: -2.0 * center.x,
            e: -2.0 * center.y,
            f: center.x * center.x + center.y * center.y - radius * radius,
        })
    }

    /// Evaluate the implicit form at `p`.
    pub fn eval(&self, p: Point2) -> f64 {
        self.a * p.x * p.x
            + self.b * p.x * p.y
            + self.c * p.y * p.y
            + self.d * p.x
            + self.e * p.y
            + self.f
    }

    /// Largest coefficient magnitude; used as the tolerance scale.
    fn magnitude(&self) -> f64 {
        self.a
            .abs()
            .max(self.b.abs())
            .max(self.c.abs())
            .max(self.d.abs())
            .max(self.e.abs())
            .max(self.f.abs())
    }

    /// Recover `(center, radius)` when this conic is (numerically) a circle:
    /// `b ~= 0` and `a ~= c != 0`. Returns `None` otherwise.
    pub fn as_circle(&self, eps: f64) -> Option<(Point2, f64)> {
        let tol = if eps.is_finite() && eps > 0.0 {
            eps
        } else {
            1e-12
        };
        let scale = self.magnitude().max(1.0);
        if self.a.abs() <= tol * scale || (self.a - self.c).abs() > tol * scale {
            return None;
        }
        if self.b.abs() > tol * scale {
            return None;
        }
        let cx = -self.d / (2.0 * self.a);
        let cy = -self.e / (2.0 * self.a);
        let r2 = (self.d * self.d + self.e * self.e) / (4.0 * self.a * self.a) - self.f / self.a;
        if !cx.is_finite() || !cy.is_finite() || !r2.is_finite() || r2 <= 0.0 {
            return None;
        }
        Some((Point2::new(cx, cy), r2.sqrt()))
    }
}

/// Intersection of the infinite line through `p1`–`p2` with a conic.
///
/// Exact substitution `p(t) = p1 + t*(p2-p1)` into the conic yields a
/// quadratic `A*t^2 + B*t + C = 0` solved in closed form. `scale` is a
/// characteristic length of the problem (see [`crate::lines::geom_eps`]).
pub fn line_conic(p1: Point2, p2: Point2, conic: &Conic, scale: f64) -> IntersectionResult {
    if !p1.x.is_finite() || !p1.y.is_finite() || !p2.x.is_finite() || !p2.y.is_finite() {
        return IntersectionResult::None;
    }
    let coeffs = [conic.a, conic.b, conic.c, conic.d, conic.e, conic.f];
    if coeffs.iter().any(|v| !v.is_finite()) {
        return IntersectionResult::None;
    }

    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    if !dx.is_finite() || !dy.is_finite() {
        return IntersectionResult::None;
    }
    let coord_scale = p1.x.abs().max(p1.y.abs()).max(dx.abs()).max(dy.abs());
    let eps = geom_eps(scale.max(coord_scale).max(conic.magnitude()));

    let dir2 = dx * dx + dy * dy;
    if dir2 <= eps * eps {
        return IntersectionResult::None;
    }

    let a2 = conic.a * dx * dx + conic.b * dx * dy + conic.c * dy * dy;
    let b2 = 2.0 * conic.a * p1.x * dx
        + conic.b * (p1.x * dy + p1.y * dx)
        + 2.0 * conic.c * p1.y * dy
        + conic.d * dx
        + conic.e * dy;
    let c2 = conic.eval(p1);

    // Degenerate whole-plane conic contains the line.
    if a2.abs() <= eps && b2.abs() <= eps {
        if c2.abs() <= eps * coord_scale.max(1.0).max(conic.magnitude()) {
            return IntersectionResult::Infinite;
        }
        return IntersectionResult::None;
    }

    // Linear (line parallel to a parabola axis, single crossing).
    if a2.abs() <= eps * (b2.abs() + 1.0) && b2.abs() > 0.0 {
        let t = -c2 / b2;
        if !t.is_finite() {
            return IntersectionResult::None;
        }
        return IntersectionResult::One(Point2::new(p1.x + t * dx, p1.y + t * dy));
    }

    let disc = b2 * b2 - 4.0 * a2 * c2;
    let disc_tol = eps * (b2 * b2 + (4.0 * a2 * c2).abs()).max(1.0);
    if disc < -disc_tol {
        IntersectionResult::None
    } else if disc.abs() <= disc_tol {
        let t = -b2 / (2.0 * a2);
        if !t.is_finite() {
            return IntersectionResult::None;
        }
        IntersectionResult::One(Point2::new(p1.x + t * dx, p1.y + t * dy))
    } else {
        if disc < 0.0 {
            return IntersectionResult::None;
        }
        let sqrt_d = disc.sqrt();
        let t1 = (-b2 - sqrt_d) / (2.0 * a2);
        let t2 = (-b2 + sqrt_d) / (2.0 * a2);
        if !t1.is_finite() || !t2.is_finite() {
            return IntersectionResult::None;
        }
        IntersectionResult::Two(
            Point2::new(p1.x + t1 * dx, p1.y + t1 * dy),
            Point2::new(p1.x + t2 * dx, p1.y + t2 * dy),
        )
    }
}

/// Honest outcome of a conic-conic intersection query.
#[derive(Debug, Clone, PartialEq)]
pub enum ConicConicOutcome {
    /// Intersection points (empty when disjoint, up to 4 in general).
    Points(Vec<Point2>),
    /// Not implemented: carries the reason so callers never get a silent lie.
    Unsupported(&'static str),
}

/// Intersection of two conics.
///
/// Circle–circle pairs delegate to [`circle_circle`]. Every other pair
/// requires a general quartic solver, which is out of scope: those return
/// [`ConicConicOutcome::Unsupported`] with the reason attached.
pub fn conic_conic(k1: &Conic, k2: &Conic, scale: f64) -> ConicConicOutcome {
    let eps = geom_eps(scale.max(k1.magnitude()).max(k2.magnitude()));
    match (k1.as_circle(eps), k2.as_circle(eps)) {
        (Some((c1, r1)), Some((c2, r2))) => match circle_circle(c1, r1, c2, r2) {
            IntersectionResult::None => ConicConicOutcome::Points(Vec::new()),
            IntersectionResult::One(p) => ConicConicOutcome::Points(vec![p]),
            IntersectionResult::Two(p1, p2) => ConicConicOutcome::Points(vec![p1, p2]),
            IntersectionResult::Infinite => {
                ConicConicOutcome::Unsupported("coincident circles: infinite intersections")
            }
        },
        _ => ConicConicOutcome::Unsupported(
            "general conic-conic intersection needs a quartic solver: not implemented",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_line_intersecting() {
        let result = line_line(
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(2.0, 0.0),
        );
        match result {
            IntersectionResult::One(p) => {
                assert!((p.x - 1.0).abs() < 1e-9);
                assert!((p.y - 1.0).abs() < 1e-9);
            }
            _ => panic!("Expected one intersection"),
        }
    }

    #[test]
    fn test_line_line_parallel() {
        let result = line_line(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        );
        assert!(matches!(result, IntersectionResult::None));
    }

    #[test]
    fn test_line_circle_secant() {
        let result = line_circle(
            Point2::new(-2.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(0.0, 0.0),
            1.0,
        );
        match result {
            IntersectionResult::Two(p1, p2) => {
                assert!((p1.x + 1.0).abs() < 1e-9 || (p2.x + 1.0).abs() < 1e-9);
                assert!((p1.x - 1.0).abs() < 1e-9 || (p2.x - 1.0).abs() < 1e-9);
            }
            _ => panic!("Expected two intersections"),
        }
    }

    #[test]
    fn test_line_circle_tangent() {
        let result = line_circle(
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 2.0),
            Point2::new(0.0, 0.0),
            1.0,
        );
        match result {
            IntersectionResult::One(p) => {
                assert!((p.x - 1.0).abs() < 1e-9);
                assert!(p.y.abs() < 1e-9);
            }
            _ => panic!("Expected one intersection (tangent)"),
        }
    }

    #[test]
    fn test_circle_circle_two_points() {
        let result = circle_circle(Point2::new(0.0, 0.0), 2.0, Point2::new(2.0, 0.0), 2.0);
        match result {
            IntersectionResult::Two(p1, p2) => {
                assert!((p1.x - 1.0).abs() < 1e-9);
                assert!((p2.x - 1.0).abs() < 1e-9);
            }
            _ => panic!("Expected two intersections"),
        }
    }

    #[test]
    fn test_circle_circle_no_intersection() {
        let result = circle_circle(Point2::new(0.0, 0.0), 1.0, Point2::new(10.0, 0.0), 1.0);
        assert!(matches!(result, IntersectionResult::None));
    }

    #[test]
    fn test_segment_segment_intersecting() {
        let result = segment_segment(
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(2.0, 0.0),
        );
        match result {
            IntersectionResult::One(p) => {
                assert!((p.x - 1.0).abs() < 1e-9);
                assert!((p.y - 1.0).abs() < 1e-9);
            }
            _ => panic!("Expected one intersection"),
        }
    }

    #[test]
    fn test_segment_segment_non_intersecting() {
        let result = segment_segment(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        );
        assert!(matches!(result, IntersectionResult::None));
    }

    #[test]
    fn function_line_returns_the_geometric_y_coordinate() {
        let intersections = function_line("x", 2.0, 1.0, -2.0, 1.0);
        assert_eq!(intersections.len(), 1);
        assert!((intersections[0].x + 1.0).abs() < 1e-6);
        assert!((intersections[0].y + 1.0).abs() < 1e-6);
    }

    #[test]
    fn collinear_segments_handle_reversed_overlap_and_endpoint_touch() {
        let overlap = segment_segment(
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(3.0, 0.0),
            Point2::new(1.0, 0.0),
        );
        assert!(matches!(overlap, IntersectionResult::Two(_, _)));

        let touch = segment_segment(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
        );
        assert!(matches!(touch, IntersectionResult::One(p) if (p.x - 1.0).abs() < 1e-12));
    }

    #[test]
    fn degenerate_line_circle_input_returns_no_intersection() {
        assert!(matches!(
            line_circle(
                Point2::new(0.0, 0.0),
                Point2::new(0.0, 0.0),
                Point2::new(0.0, 0.0),
                1.0,
            ),
            IntersectionResult::None
        ));
    }

    #[test]
    fn degenerate_circle_input_returns_no_intersection() {
        assert!(matches!(
            circle_circle(Point2::new(0.0, 0.0), 0.0, Point2::new(1.0, 0.0), 1.0),
            IntersectionResult::None
        ));
    }

    #[test]
    fn line_conic_matches_line_circle_on_secant() {
        let conic = Conic::circle(Point2::new(0.0, 0.0), 1.0).expect("valid circle");
        let result = line_conic(Point2::new(-2.0, 0.0), Point2::new(2.0, 0.0), &conic, 2.0);
        match result {
            IntersectionResult::Two(p1, p2) => {
                assert!((p1.x.abs() - 1.0).abs() < 1e-9, "got {p1:?}");
                assert!((p2.x.abs() - 1.0).abs() < 1e-9, "got {p2:?}");
                assert!(p1.y.abs() < 1e-9 && p2.y.abs() < 1e-9);
            }
            other => panic!("expected secant Two, got {other:?}"),
        }
    }

    #[test]
    fn line_conic_tangent_and_miss() {
        let conic = Conic::circle(Point2::new(0.0, 0.0), 1.0).expect("valid circle");
        let tangent = line_conic(Point2::new(1.0, -2.0), Point2::new(1.0, 2.0), &conic, 2.0);
        assert!(
            matches!(tangent, IntersectionResult::One(p) if (p.x - 1.0).abs() < 1e-9 && p.y.abs() < 1e-9),
            "got {tangent:?}"
        );
        let miss = line_conic(Point2::new(2.0, -2.0), Point2::new(2.0, 2.0), &conic, 2.0);
        assert!(matches!(miss, IntersectionResult::None), "got {miss:?}");
    }

    #[test]
    fn line_conic_hits_parabola_twice() {
        // y = x^2  <=>  -x^2 + y = 0.
        let parabola = Conic::from_general(-1.0, 0.0, 0.0, 0.0, 1.0, 0.0).expect("valid");
        let result = line_conic(
            Point2::new(-2.0, 0.0),
            Point2::new(2.0, 4.0),
            &parabola,
            4.0,
        );
        match result {
            IntersectionResult::Two(p1, p2) => {
                for p in [p1, p2] {
                    assert!((p.y - p.x * p.x).abs() < 1e-9, "off parabola: {p:?}");
                }
            }
            other => panic!("expected Two, got {other:?}"),
        }
    }

    #[test]
    fn line_conic_rejects_degenerate_inputs() {
        let conic = Conic::circle(Point2::new(0.0, 0.0), 1.0).expect("valid circle");
        assert!(matches!(
            line_conic(Point2::new(1.0, 1.0), Point2::new(1.0, 1.0), &conic, 1.0),
            IntersectionResult::None
        ));
        assert!(Conic::from_general(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_none());
        assert!(Conic::from_general(f64::NAN, 0.0, 0.0, 0.0, 0.0, 1.0).is_none());
    }

    #[test]
    fn conic_conic_delegates_circle_pairs() {
        let k1 = Conic::circle(Point2::new(0.0, 0.0), 2.0).expect("valid");
        let k2 = Conic::circle(Point2::new(2.0, 0.0), 2.0).expect("valid");
        match conic_conic(&k1, &k2, 4.0) {
            ConicConicOutcome::Points(pts) => {
                assert_eq!(pts.len(), 2);
                for p in pts {
                    assert!((p.x - 1.0).abs() < 1e-9, "got {p:?}");
                }
            }
            other => panic!("expected Points, got {other:?}"),
        }
    }

    #[test]
    fn conic_conic_stub_is_honest_for_general_pairs() {
        let parabola = Conic::from_general(-1.0, 0.0, 0.0, 0.0, 1.0, 0.0).expect("valid");
        let circle = Conic::circle(Point2::new(0.0, 0.0), 1.0).expect("valid");
        assert!(matches!(
            conic_conic(&parabola, &circle, 2.0),
            ConicConicOutcome::Unsupported(_)
        ));
        // Coincident circles must not fake points.
        assert!(matches!(
            conic_conic(&circle, &circle, 2.0),
            ConicConicOutcome::Unsupported(_)
        ));
    }
}
