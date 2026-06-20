//! Verifica que el cache del AST en ImplicitCurveObj no rompe el render
//! cuando se llama repetidamente (simulando múltiples frames).

use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_cache_returns_correct_lhs_rhs_across_frames() {
    // Simula el ciclo de render: por cada frame, llamar get_cached_asts
    // y usar los ASTs resultantes para evaluar f = lhs - rhs en (0, 0).
    // El resultado debe ser -1 (porque x²+y²-1 = -1 en el origen),
    // no 0 (que sería el bug del cache compartido).
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::new();

    for _ in 0..100 {
        let (lhs, rhs) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        // f(0, 0) = lhs(0,0) - rhs(0,0) = 0 - 1 = -1.
        let l = lhs.eval_2d("x", 0.0, "y", 0.0);
        let r = rhs.eval_2d("x", 0.0, "y", 0.0);
        assert_eq!(l, 0.0, "lhs(0,0) debe ser 0 (x²+y²)");
        assert_eq!(r, 1.0, "rhs(0,0) debe ser 1");
        let f = l - r;
        assert!(f < 0.0, "f(0,0) = {} debe ser < 0 (interior del disco)", f);
    }
}

#[test]
fn test_cache_with_different_expressions() {
    // Varias curvas con expresiones distintas no deben interferir.
    let mut curves = Vec::new();
    for i in 0..5 {
        let expr = format!("x^2 + y^2 - {}", i as f64 * 0.1);
        let ic = ImplicitCurveObj::new(&expr, "0", RelationOperator::Eq);
        curves.push(ic);
    }
    let vars = HashMap::new();
    for (i, ic) in curves.iter().enumerate() {
        let (lhs, rhs) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        let l = lhs.eval_2d("x", 0.0, "y", 0.0);
        let r = rhs.eval_2d("x", 0.0, "y", 0.0);
        // l debe ser -(i * 0.1) (porque en (0,0), x²+y² = 0)
        let expected_l = -(i as f64) * 0.1;
        assert!(
            (l - expected_l).abs() < 1e-9,
            "curve {}: lhs(0,0) = {} (esperado {})",
            i,
            l,
            expected_l
        );
        assert_eq!(r, 0.0, "rhs siempre es 0");
    }
}
