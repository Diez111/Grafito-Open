//! Test del flujo de render de un ImplicitCurve: pre-compute del
//! outline, lectura del cache, y verificación de que los segmentos
//! están en el lugar correcto.

use grafito_core::implicit_curve::segments_or_compute;
use grafito_core::RenderQuality;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_render_flow_x2_y2_eq_1() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let _guard = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    // El cache debe estar lleno.
    let cached = ic.cached_segments.read().unwrap();
    assert!(!cached.is_empty());
    let (_, segs) = &cached[0];
    // Cada segmento debe estar aproximadamente en r=1.
    for (a, b) in segs.iter().take(10) {
        let ra = (a.x * a.x + a.y * a.y).sqrt();
        let rb = (b.x * b.x + b.y * b.y).sqrt();
        assert!((ra - 1.0).abs() < 0.05, "a en r={}", ra);
        assert!((rb - 1.0).abs() < 0.05, "b en r={}", rb);
    }
    drop(cached);
    drop(_guard);
    // Ahora cambiamos el radio: x^2 + y^2 = 4 (radio 2).
    let mut ic2 = ic.clone();
    ic2.expr_rhs = "4".to_string();
    let _g2 = segments_or_compute(&ic2, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached = ic2.cached_segments.read().unwrap();
    let (_, segs) = &cached[0];
    for (a, b) in segs.iter().take(10) {
        let ra = (a.x * a.x + a.y * a.y).sqrt();
        let rb = (b.x * b.x + b.y * b.y).sqrt();
        assert!((ra - 2.0).abs() < 0.1, "a en r={} (esperado ~2)", ra);
        assert!((rb - 2.0).abs() < 0.1, "b en r={} (esperado ~2)", rb);
    }
}

#[test]
fn test_render_flow_x2_y2_lt_1() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::new();
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let _g = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached = ic.cached_segments.read().unwrap();
    let (_, segs) = &cached[0];
    // Para <, los segmentos también están en el contorno r=1.
    for (a, b) in segs.iter().take(10) {
        let ra = (a.x * a.x + a.y * a.y).sqrt();
        let rb = (b.x * b.x + b.y * b.y).sqrt();
        assert!((ra - 1.0).abs() < 0.05, "a en r={}", ra);
        assert!((rb - 1.0).abs() < 0.05, "b en r={}", rb);
    }
}

#[test]
fn test_explicit_implicitcurve_creation() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    assert_eq!(ic.operator, RelationOperator::Eq);
    // El cache se puede llenar.
    let _g = segments_or_compute(
        &ic,
        (-2.0, 2.0, -2.0, 2.0),
        256,
        &HashMap::new(),
        RenderQuality::Normal,
    );
    let cached = ic.cached_segments.read().unwrap();
    assert!(!cached.is_empty());
}
