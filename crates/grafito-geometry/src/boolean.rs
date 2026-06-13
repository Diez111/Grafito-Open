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
