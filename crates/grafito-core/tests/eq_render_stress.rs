//! Test de stress: simular el render de un ImplicitCurve con Eq
//! en un view normal. Detecta crashes.

use grafito_core::implicit_curve::evaluate_implicit_curve;
use grafito_core::implicit_curve::segments_or_compute;
use grafito_core::RenderQuality;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn stress_test_eq_render() {
    // Simula el render de un ImplicitCurve con Eq.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let view_bounds = (-5.0, 5.0, -5.0, 5.0);

    // Pre-compute del cache.
    for _ in 0..10 {
        let _g = segments_or_compute(&ic, view_bounds, 512, &vars, RenderQuality::Normal);
    }

    // Verificar que el cache tiene segmentos.
    let cached = ic.cached_segments.read().unwrap();
    assert!(!cached.is_empty(), "el cache debe estar lleno");
    let (_, segs) = &cached[0];
    println!("segments: {}", segs.len());
    assert!(segs.len() > 100);
}

#[test]
fn stress_test_cached_asts_with_unusual_inputs() {
    // Probar con expresiones que podrían causar problemas.
    let cases = [
        ("x^2 + y^2", "1", RelationOperator::Eq),
        ("x^2 + y^2", "1", RelationOperator::Less),
        ("x^2 + y^2", "1", RelationOperator::LessEq),
        ("x^2 + y^2", "1", RelationOperator::Greater),
        ("x^2 + y^2", "1", RelationOperator::GreaterEq),
        ("sin(x) * cos(y)", "0", RelationOperator::Eq),
        ("x^3 + y^3 - 3*x*y", "0", RelationOperator::Eq),
        ("x^2 - y^2", "1", RelationOperator::Eq),
    ];

    let vars = HashMap::new();
    for (lhs, rhs, op) in &cases {
        let ic = ImplicitCurveObj::new(lhs, rhs, op.clone());
        let (l, r) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        // Solo verificar que eval no panic.
        let _ = l.eval_2d("x", 0.0, "y", 0.0);
        let _ = r.eval_2d("x", 0.0, "y", 0.0);
        println!("ok: {} {} {:?}", lhs, rhs, op);
    }
}

#[test]
fn stress_test_eq_after_clone() {
    // Verificar que el cache se preserva después de clone.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let _g = segments_or_compute(
        &ic,
        (-2.0, 2.0, -2.0, 2.0),
        256,
        &vars,
        RenderQuality::Normal,
    );
    let cached = ic.cached_segments.read().unwrap();
    let len_before = cached[0].1.len();
    drop(cached);
    drop(_g);

    // Clone y verifica que el cache se preserva.
    let ic2 = ic.clone();
    let cached2 = ic2.cached_segments.read().unwrap();
    let len_after = cached2[0].1.len();
    drop(cached2);
    assert_eq!(len_before, len_after);
}
