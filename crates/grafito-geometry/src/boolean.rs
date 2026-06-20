//! 2D polygon boolean operations via the `geo` crate.

use crate::Point2;
use geo::{LineString, MultiPolygon, Polygon};

/// Convert a Grafito polygon (list of `Point2` vertices) into a `geo::Polygon`.
///
/// The ring is automatically closed if it is not already closed, which is
/// required by many `geo` algorithms.
pub fn polygon_to_geo(vertices: &[Point2]) -> Polygon<f64> {
    let mut coords: Vec<(f64, f64)> = vertices.iter().map(|p| (p.x, p.y)).collect();
    if coords.len() >= 3 && coords.first() != coords.last() {
        coords.push(coords[0]);
    }
    Polygon::new(LineString::from(coords), vec![])
}

/// Convert the exterior ring of a `geo::Polygon` back into Grafito vertices.
pub fn geo_to_polygon(poly: &Polygon<f64>) -> Vec<Point2> {
    poly.exterior()
        .coords()
        .map(|c| Point2::new(c.x, c.y))
        .collect()
}

/// Convert a `geo::MultiPolygon` into a list of Grafito vertex rings.
pub fn multipolygon_to_polygons(mp: &MultiPolygon<f64>) -> Vec<Vec<Point2>> {
    mp.iter().map(geo_to_polygon).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::prelude::*;

    fn circle_polygon(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Point2> {
        (0..n)
            .map(|i| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                Point2::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    #[test]
    fn polygon_geo_roundtrip_preserves_vertices() {
        let verts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(4.0, 3.0),
            Point2::new(0.0, 3.0),
        ];
        let geo = polygon_to_geo(&verts);
        let back = geo_to_polygon(&geo);
        // geo closes the ring, so the returned ring repeats the first point.
        assert!((back[0].x - 0.0).abs() < 1e-12);
        assert!((back[1].x - 4.0).abs() < 1e-12);
        assert!((back[2].x - 4.0).abs() < 1e-12);
        assert!((back[3].x - 0.0).abs() < 1e-12);
        // The closing vertex equals the first.
        assert!((back.first().unwrap().x - back.last().unwrap().x).abs() < 1e-12);
    }

    #[test]
    fn intersection_of_two_circles_as_polygons_has_positive_area() {
        // Two unit circles centered at (0,0) and (0.6,0), approximated as
        // 64-gons so the overlap lens is non-empty.
        let a = circle_polygon(0.0, 0.0, 1.0, 64);
        let b = circle_polygon(0.6, 0.0, 1.0, 64);
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let inter = geo_a.intersection(&geo_b);
        let area = inter.unsigned_area();
        assert!(
            area > 0.0,
            "intersection area should be positive, got {}",
            area
        );
        // Intersection cannot exceed either circle's polygon area (~π).
        assert!(area < std::f64::consts::PI);
    }

    #[test]
    fn union_of_disjoint_squares_has_area_equal_to_sum() {
        let a = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let b = vec![
            Point2::new(10.0, 10.0),
            Point2::new(11.0, 10.0),
            Point2::new(11.0, 11.0),
            Point2::new(10.0, 11.0),
        ];
        let geo_a = polygon_to_geo(&a);
        let geo_b = polygon_to_geo(&b);
        let union = geo_a.union(&geo_b);
        assert!(
            (union.unsigned_area() - 2.0).abs() < 1e-9,
            "disjoint union area should be 2.0, got {}",
            union.unsigned_area()
        );
    }

    #[test]
    fn difference_of_nested_squares_is_the_ring() {
        // Outer 4x4 square minus inner 2x2 square => area 16 - 4 = 12.
        let outer = vec![
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(4.0, 4.0),
            Point2::new(0.0, 4.0),
        ];
        let inner = vec![
            Point2::new(1.0, 1.0),
            Point2::new(3.0, 1.0),
            Point2::new(3.0, 3.0),
            Point2::new(1.0, 3.0),
        ];
        let diff = polygon_to_geo(&outer).difference(&polygon_to_geo(&inner));
        assert!(
            (diff.unsigned_area() - 12.0).abs() < 1e-9,
            "difference area should be 12.0, got {}",
            diff.unsigned_area()
        );
    }

    #[test]
    fn multipolygon_to_polygons_extracts_each_ring() {
        // Two disjoint squares produce a MultiPolygon with two polygons.
        let a = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let b = vec![
            Point2::new(10.0, 10.0),
            Point2::new(11.0, 10.0),
            Point2::new(11.0, 11.0),
            Point2::new(10.0, 11.0),
        ];
        let mp = polygon_to_geo(&a).union(&polygon_to_geo(&b));
        let rings = multipolygon_to_polygons(&mp);
        assert_eq!(
            rings.len(),
            2,
            "expected two disjoint polygons in the union"
        );
    }
}
