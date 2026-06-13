//! Line, ray and segment utilities.

use crate::{Point2, AABB};

/// Perpendicular distance from `p` to the infinite line through `a` and `b`.
pub fn distance_point_to_line(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return p.distance(&a);
    }
    let cross = (p.x - a.x) * dy - (p.y - a.y) * dx;
    cross.abs() / len_sq.sqrt()
}

/// Distance from `p` to the ray starting at `a` and passing through `b`.
pub fn distance_point_to_ray(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return p.distance(&a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    if t <= 0.0 {
        p.distance(&a)
    } else {
        distance_point_to_line(p, a, b)
    }
}

/// Distance from `p` to the finite segment `a`–`b`.
pub fn distance_point_to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let ab2 = abx * abx + aby * aby;
    if ab2 == 0.0 {
        return p.distance(&a);
    }
    let t = ((apx * abx + apy * aby) / ab2).clamp(0.0, 1.0);
    let closest = Point2::new(a.x + t * abx, a.y + t * aby);
    p.distance(&closest)
}

/// Parameter `t` such that `a + t*(b-a)` is the closest point to `p` on the
/// infinite line through `a` and `b`.
pub fn line_param_at_point(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return 0.0;
    }
    ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq
}

/// Liang–Barsky clipping of the parameterized line `a + t*(b-a)` to `rect`,
/// with the parameter range restricted to `[t_min, t_max]`.
fn liang_barsky(
    a: Point2,
    b: Point2,
    rect: AABB,
    t_min: f64,
    t_max: f64,
) -> Option<(Point2, Point2)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;

    let p = [-dx, dx, -dy, dy];
    let q = [
        a.x - rect.min.x,
        rect.max.x - a.x,
        a.y - rect.min.y,
        rect.max.y - a.y,
    ];

    let mut t0 = t_min;
    let mut t1 = t_max;

    for i in 0..4 {
        if p[i] == 0.0 {
            if q[i] < 0.0 {
                return None;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }

    if t0 > t1 {
        return None;
    }

    Some((
        Point2::new(a.x + t0 * dx, a.y + t0 * dy),
        Point2::new(a.x + t1 * dx, a.y + t1 * dy),
    ))
}

/// Clip the infinite line through `a` and `b` to `rect`.
pub fn clip_line_to_rect(a: Point2, b: Point2, rect: AABB) -> Option<(Point2, Point2)> {
    liang_barsky(a, b, rect, f64::NEG_INFINITY, f64::INFINITY)
}

/// Clip the ray starting at `a` and passing through `b` to `rect`.
pub fn clip_ray_to_rect(a: Point2, b: Point2, rect: AABB) -> Option<(Point2, Point2)> {
    liang_barsky(a, b, rect, 0.0, f64::INFINITY)
}

/// Clip the finite segment `a`–`b` to `rect`.
pub fn clip_segment_to_rect(a: Point2, b: Point2, rect: AABB) -> Option<(Point2, Point2)> {
    liang_barsky(a, b, rect, 0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_point_to_segment() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(2.0, 0.0);
        assert!((distance_point_to_segment(Point2::new(1.0, 1.0), a, b) - 1.0).abs() < 1e-10);
        assert!((distance_point_to_segment(Point2::new(-1.0, 0.0), a, b) - 1.0).abs() < 1e-10);
        assert!((distance_point_to_segment(Point2::new(3.0, 0.0), a, b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_distance_point_to_line_and_ray() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        assert!((distance_point_to_line(Point2::new(0.5, 2.0), a, b) - 2.0).abs() < 1e-10);
        assert!((distance_point_to_ray(Point2::new(0.5, 2.0), a, b) - 2.0).abs() < 1e-10);
        // Behind the ray origin: distance to origin
        assert!((distance_point_to_ray(Point2::new(-1.0, 0.0), a, b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_clip_segment_to_rect() {
        let rect = AABB::new(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0));
        let clipped =
            clip_segment_to_rect(Point2::new(-1.0, 1.0), Point2::new(3.0, 1.0), rect).unwrap();
        assert!((clipped.0.x - 0.0).abs() < 1e-10);
        assert!((clipped.1.x - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_clip_line_to_rect() {
        let rect = AABB::new(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0));
        let clipped =
            clip_line_to_rect(Point2::new(-1.0, 1.0), Point2::new(3.0, 1.0), rect).unwrap();
        assert!((clipped.0.x - 0.0).abs() < 1e-10);
        assert!((clipped.1.x - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_clip_ray_to_rect() {
        let rect = AABB::new(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0));
        // Ray starting inside, going out
        let clipped = clip_ray_to_rect(Point2::new(1.0, 1.0), Point2::new(3.0, 1.0), rect).unwrap();
        assert!((clipped.0.x - 1.0).abs() < 1e-10);
        assert!((clipped.1.x - 2.0).abs() < 1e-10);

        // Ray starting outside, pointing away: no intersection
        assert!(clip_ray_to_rect(Point2::new(-1.0, 1.0), Point2::new(-2.0, 1.0), rect).is_none());
    }

    #[test]
    fn test_line_param_at_point() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(2.0, 0.0);
        assert!((line_param_at_point(Point2::new(3.0, 0.0), a, b) - 1.5).abs() < 1e-10);
        assert!((line_param_at_point(Point2::new(1.0, 2.0), a, b) - 0.5).abs() < 1e-10);
    }
}
