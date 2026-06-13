//! Geometric intersection algorithms.
//!
//! Computes intersection points between pairs of geometric primitives:
//! - Line-Line (solves 2x2 linear system)
//! - Line-Circle (quadratic discriminant)
//! - Circle-Circle (radical axis method)
//! - Segment-Segment (orientation tests)
//! - Function-Line (Newton root finding)
//! - Function-Function (Newton root finding)

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
            let t_start = project_point_on_line(b1, a1, a2).max(0.0);
            let t_end = project_point_on_line(b2, a1, a2).min(1.0);
            if t_start < t_end {
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
    find_roots(
        &|x| {
            let fy = crate::expr::evaluate(expr, &[("x".to_string(), x)]).unwrap_or(f64::NAN);
            if fy.is_nan() {
                return f64::NAN;
            }
            fy - (slope * x + intercept)
        },
        x_min,
        x_max,
    )
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
                let root_y =
                    crate::expr::evaluate("x", &[("x".to_string(), root_x)]).unwrap_or(0.0);
                let fy_at_root = f(root_x);
                if fy_at_root.abs() < 1e-6 {
                    let is_duplicate = roots.iter().any(|r: &Point2| (r.x - root_x).abs() < 1e-6);
                    if !is_duplicate {
                        roots.push(Point2::new(root_x, root_y));
                        let _ = root_y;
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
}
