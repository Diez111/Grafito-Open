use std::collections::HashMap;

use grafito_app::render_2d::{
    complex_mapping_region_contains, complex_mapping_segment_strokes, fill_cache_view_uv,
};
use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use grafito_geometry::Point2;

#[test]
fn inversion_maps_unit_disk_inequality_to_exterior() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::new();
    let map = ConformalMap::Inversion;

    assert_eq!(
        complex_mapping_region_contains(&ic, map, &vars, 2.0, 0.0),
        Some(true),
        "w=(2,0) maps back to z=(0.5,0), inside |z|<1"
    );
    assert_eq!(
        complex_mapping_region_contains(&ic, map, &vars, 0.5, 0.0),
        Some(false),
        "w=(0.5,0) maps back to z=(2,0), outside |z|<1"
    );
}

#[test]
fn inversion_maps_unit_exterior_inequality_to_punctured_disk() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Greater);
    let vars = HashMap::new();
    let map = ConformalMap::Inversion;

    assert_eq!(
        complex_mapping_region_contains(&ic, map, &vars, 0.5, 0.0),
        Some(true),
        "w=(0.5,0) maps back to z=(2,0), inside |z|>1"
    );
    assert_eq!(
        complex_mapping_region_contains(&ic, map, &vars, 2.0, 0.0),
        Some(false),
        "w=(2,0) maps back to z=(0.5,0), outside |z|>1"
    );
}

#[test]
fn fill_cache_view_uv_uses_subrect_inside_cached_region() {
    let cached_region = (-10.0, 10.0, -8.0, 8.0);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);

    let uv = fill_cache_view_uv(cached_region, view_bounds).unwrap();

    assert_eq!(uv, (0.25, 0.75, 0.25, 0.75));
}

#[test]
fn fill_cache_view_uv_rejects_views_outside_cached_region() {
    let cached_region = (-10.0, 10.0, -8.0, 8.0);
    let view_bounds = (-11.0, 5.0, -4.0, 4.0);

    assert!(fill_cache_view_uv(cached_region, view_bounds).is_none());
}

#[test]
fn complex_mapping_segments_do_not_bridge_independent_segments() {
    let segments = vec![
        (Point2::new(1.0, 0.0), Point2::new(2.0, 0.0)),
        (Point2::new(-1.0, 0.0), Point2::new(-2.0, 0.0)),
    ];

    let strokes = complex_mapping_segment_strokes(ConformalMap::Inversion, &segments, 1);

    assert_eq!(strokes.len(), 2);
    assert!(strokes.iter().all(|(a, b)| a.x.signum() == b.x.signum()));
}
