//! Verifica que el marching squares produce segmentos para casos comunes.

use grafito_core::implicit_curve::{evaluate_implicit_curve, segments_or_compute};
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_marching_squares_circle_eq_produces_segments() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let segments = evaluate_implicit_curve(&ic, view_bounds, 256, &HashMap::new());
    assert!(!segments.is_empty(), "debería haber al menos un nivel");
    let (_level, segs) = &segments[0];
    println!("segments count: {}", segs.len());
    assert!(!segs.is_empty(), "debería haber segmentos para el círculo");
    // Verificar que los segmentos están en el contorno r=1.
    for (a, b) in segs.iter().take(5) {
        let ra = (a.x * a.x + a.y * a.y).sqrt();
        let rb = (b.x * b.x + b.y * b.y).sqrt();
        println!(
            "segment: ({:.3}, {:.3}) -> ({:.3}, {:.3}) r={:.3} r={:.3}",
            a.x, a.y, b.x, b.y, ra, rb
        );
        assert!(
            (ra - 1.0).abs() < 0.05,
            "segmento A no está en r=1: r={}",
            ra
        );
        assert!(
            (rb - 1.0).abs() < 0.05,
            "segmento B no está en r=1: r={}",
            rb
        );
    }
}

#[test]
fn test_marching_squares_circle_less_produces_segments() {
    // Para <, los segmentos también deben estar en el contorno.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let segments = evaluate_implicit_curve(&ic, view_bounds, 256, &HashMap::new());
    assert!(!segments.is_empty(), "debería haber al menos un nivel");
    let (_level, segs) = &segments[0];
    assert!(!segs.is_empty(), "debería haber segmentos para el disco");
    println!("segments count: {}", segs.len());
}
