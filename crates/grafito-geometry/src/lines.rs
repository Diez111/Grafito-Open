//! Line, ray and segment utilities.

use crate::{Point2, AABB};
use serde::{Deserialize, Serialize};

/// Extensión del objeto definido por dos puntos parametrizado como
/// `start + t * (end - start)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    /// Segmento finito entre ambos extremos.
    #[default]
    Segment,
    /// Recta infinita que pasa por ambos puntos.
    Line,
    /// Semirrecta que comienza en `start` y pasa por `end`.
    Ray,
}

impl LineKind {
    /// Indica si el parámetro pertenece a la extensión geométrica.
    pub fn contains_t(self, t: f64) -> bool {
        match self {
            Self::Segment => (0.0..=1.0).contains(&t),
            Self::Ray => t >= 0.0,
            Self::Line => true,
        }
    }

    /// Restringe un parámetro al punto más cercano de la extensión.
    pub fn clamp_t(self, t: f64) -> f64 {
        match self {
            Self::Segment => t.clamp(0.0, 1.0),
            Self::Ray => t.max(0.0),
            Self::Line => t,
        }
    }
}

/// Scale-aware geometric epsilon.
///
/// Returns a tolerance proportional to `scale` (a characteristic length of
/// the problem: coordinate magnitude, segment length, viewport extent).
/// Non-finite or non-positive scales fall back to `1e-12`. The result is
/// clamped to `[1e-15, 1e-6]` so it never vanishes into rounding noise nor
/// grows into sloppiness.
pub fn geom_eps(scale: f64) -> f64 {
    if !scale.is_finite() || scale <= 0.0 {
        return 1e-12;
    }
    let eps = scale.max(1.0) * 64.0 * f64::EPSILON;
    eps.clamp(1e-15, 1e-6)
}

/// Perpendicular distance from `p` to the infinite line through `a` and `b`.
pub fn distance_point_to_line(p: Point2, a: Point2, b: Point2) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
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
    if len_sq < 1e-12 {
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
    if ab2 < 1e-12 {
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
    if len_sq < 1e-12 {
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
    if !a.x.is_finite()
        || !a.y.is_finite()
        || !b.x.is_finite()
        || !b.y.is_finite()
        || !rect.min.x.is_finite()
        || !rect.min.y.is_finite()
        || !rect.max.x.is_finite()
        || !rect.max.y.is_finite()
        || rect.min.x > rect.max.x
        || rect.min.y > rect.max.y
    {
        return None;
    }

    let dx = b.x - a.x;
    let dy = b.y - a.y;

    // Finite endpoints can still overflow while calculating their direction.
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }

    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        return (a.x >= rect.min.x && a.x <= rect.max.x && a.y >= rect.min.y && a.y <= rect.max.y)
            .then_some((a, a));
    }

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
        if p[i].abs() < 1e-12 {
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

    let start = Point2::new(a.x + t0 * dx, a.y + t0 * dy);
    let end = Point2::new(a.x + t1 * dx, a.y + t1 * dy);
    (start.x.is_finite() && start.y.is_finite() && end.x.is_finite() && end.y.is_finite())
        .then_some((start, end))
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
    fn degenerate_infinite_lines_clip_to_their_finite_point() {
        let rect = AABB::new(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0));
        let point = Point2::new(1.0, 1.0);

        for clipped in [
            clip_line_to_rect(point, point, rect),
            clip_ray_to_rect(point, point, rect),
        ] {
            let (start, end) = clipped.expect("a point inside the rectangle should remain visible");
            assert!(start.x.is_finite() && start.y.is_finite());
            assert!(end.x.is_finite() && end.y.is_finite());
            assert_eq!(start, point);
            assert_eq!(end, point);
        }
    }

    #[test]
    fn clipping_extreme_finite_endpoints_omits_overflowing_directions() {
        let rect = AABB::new(Point2::new(-1.0, -1.0), Point2::new(1.0, 1.0));
        let a = Point2::new(-f64::MAX, 0.0);
        let b = Point2::new(f64::MAX, 0.0);

        for clipped in [
            clip_line_to_rect(a, b, rect),
            clip_ray_to_rect(a, b, rect),
            clip_segment_to_rect(a, b, rect),
        ] {
            assert!(clipped.is_none());
        }
    }

    #[test]
    fn test_line_param_at_point() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(2.0, 0.0);
        assert!((line_param_at_point(Point2::new(3.0, 0.0), a, b) - 1.5).abs() < 1e-10);
        assert!((line_param_at_point(Point2::new(1.0, 2.0), a, b) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn geom_eps_scales_with_magnitude() {
        let base = geom_eps(1.0);
        assert!((1e-15..=1e-6).contains(&base), "clamped, got {base}");
        assert!(geom_eps(1000.0) > base, "must grow with scale");
        assert!(geom_eps(0.0).is_finite());
        assert!(geom_eps(f64::NAN).is_finite());
        assert!(geom_eps(f64::INFINITY).is_finite());
        assert!(geom_eps(-5.0).is_finite());
    }
}
