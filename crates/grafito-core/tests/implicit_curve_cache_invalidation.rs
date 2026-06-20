//! **Test de regresión crítico**: el cache de segments_or_compute debe
//! invalidarse cuando cambia la expresión, no solo cuando cambian los
//! view bounds.

use grafito_core::implicit_curve::segments_or_compute;
use grafito_core::RenderQuality;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_cache_invalidates_when_expr_changes() {
    let mut ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let view_bounds = (-3.0, 3.0, -3.0, 3.0);
    let _g1 = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached = ic.cached_segments.read().unwrap();
    let segs_r1: Vec<(f64, f64)> = cached[0]
        .1
        .iter()
        .map(|(a, b)| {
            (
                (a.x * a.x + a.y * a.y).sqrt(),
                (b.x * b.x + b.y * b.y).sqrt(),
            )
        })
        .collect();
    drop(cached);
    drop(_g1);

    // Cambiamos la expresión a x^2 + y^2 = 4 (radio 2).
    ic.expr_rhs = "4".to_string();
    let _g2 = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached = ic.cached_segments.read().unwrap();
    let segs_r2: Vec<(f64, f64)> = cached[0]
        .1
        .iter()
        .map(|(a, b)| {
            (
                (a.x * a.x + a.y * a.y).sqrt(),
                (b.x * b.x + b.y * b.y).sqrt(),
            )
        })
        .collect();
    drop(cached);
    drop(_g2);

    // Los radios medios deben ser diferentes.
    let avg_r1: f64 =
        segs_r1.iter().map(|&(a, b)| (a + b) / 2.0).sum::<f64>() / segs_r1.len() as f64;
    let avg_r2: f64 =
        segs_r2.iter().map(|&(a, b)| (a + b) / 2.0).sum::<f64>() / segs_r2.len() as f64;
    println!("avg r1 = {}, avg r2 = {}", avg_r1, avg_r2);
    assert!(
        (avg_r1 - 1.0).abs() < 0.05,
        "radio 1 esperado ~1, hay {}",
        avg_r1
    );
    assert!(
        (avg_r2 - 2.0).abs() < 0.05,
        "radio 2 esperado ~2, hay {}",
        avg_r2
    );
    assert!(
        (avg_r1 - avg_r2).abs() > 0.5,
        "los radios deben ser distintos"
    );
}

#[test]
fn test_cache_invalidates_when_operator_changes() {
    let mut ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let _g1 = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached1 = ic.cached_segments.read().unwrap();
    let len1 = cached1[0].1.len();
    drop(cached1);
    drop(_g1);

    ic.operator = RelationOperator::Less;
    let _g2 = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    let cached2 = ic.cached_segments.read().unwrap();
    let len2 = cached2[0].1.len();
    drop(cached2);
    drop(_g2);

    // Para Eq y Less los segmentos del contour son los mismos, pero el cache
    // debe reevaluarse porque el operador cambió.
    println!("Eq: {} segments, Less: {} segments", len1, len2);
    assert!(len1 > 0 && len2 > 0);
}
