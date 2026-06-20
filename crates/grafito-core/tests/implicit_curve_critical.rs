//! Tests críticos: reproduce los casos reportados por el usuario.
//! Verifica que el cache del AST, el scanline y el outline
//! funcionan correctamente para `x^2 + y^2 = 1`, `< 1`, `<= 1`.

use grafito_core::implicit_curve::{evaluate_implicit_curve, segments_or_compute};
use grafito_core::RenderQuality;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn critical_test_x2_y2_eq_1_outline() {
    // x^2 + y^2 = 1: outline del círculo.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let segments = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    assert!(!segments.is_empty(), "x^2+y^2=1 debe tener segmentos");
    let (_, segs) = &segments[0];
    // El círculo unitario tiene ~500 segmentos con grid 256.
    assert!(segs.len() > 200, "muy pocos segmentos: {}", segs.len());
}

#[test]
fn critical_test_x2_y2_lt_1_outline() {
    // x^2 + y^2 < 1: el outline también existe (es el contorno del disco).
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let segments = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    let (_, segs) = &segments[0];
    assert!(!segs.is_empty(), "x^2+y^2<1 debe tener segmentos");
    assert!(segs.len() > 200);
}

#[test]
fn critical_test_x2_y2_le_1_outline() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::LessEq);
    let segments = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    let (_, segs) = &segments[0];
    assert!(!segs.is_empty());
    assert!(segs.len() > 200);
}

#[test]
fn critical_test_cache_stability_over_many_frames() {
    // Simula 1000 frames llamando get_cached_asts repetidamente.
    // El cache debe ser estable (mismo AST cada vez).
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::new();
    let (lhs0, rhs0) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
    let expected_l = lhs0.eval_2d("x", 0.5, "y", 0.5);
    let expected_r = rhs0.eval_2d("x", 0.5, "y", 0.5);
    assert_eq!(expected_l, 0.5, "lhs(0.5, 0.5) = 0.5");
    assert_eq!(expected_r, 1.0);

    for frame in 0..1000 {
        let (lhs, rhs) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        let l = lhs.eval_2d("x", 0.5, "y", 0.5);
        let r = rhs.eval_2d("x", 0.5, "y", 0.5);
        assert_eq!(
            l, expected_l,
            "frame {}: lhs(0.5, 0.5) = {} != {}",
            frame, l, expected_l
        );
        assert_eq!(
            r, expected_r,
            "frame {}: rhs(0.5, 0.5) = {} != {}",
            frame, r, expected_r
        );
    }
}

#[test]
fn critical_test_segments_or_compute_fills_cache() {
    // Verifica que segments_or_compute llena el cache correctamente.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::new();
    let view_bounds = (-2.0, 2.0, -2.0, 2.0);
    let _g = segments_or_compute(&ic, view_bounds, 256, &vars, RenderQuality::Normal);
    // El cache debe estar lleno.
    let cached = ic.cached_segments.read().unwrap();
    assert!(!cached.is_empty(), "el cache debe estar lleno");
    let (_, segs) = &cached[0];
    assert!(
        !segs.is_empty(),
        "los segmentos del cache no deben estar vacíos"
    );
    assert!(segs.len() > 200);
}
