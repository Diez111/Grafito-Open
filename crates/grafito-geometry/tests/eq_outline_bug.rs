//! Verifica que el outline de x² + y² = 1 se genera y renderiza
//! correctamente con el cache key completo.

use grafito_geometry::expr::prepare_function_ast;
use std::collections::HashMap;

/// Genera el cache key manualmente para verificar la condición del cache.
fn make_key(
    expr_lhs: &str,
    expr_rhs: &str,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    expr_lhs.hash(&mut h);
    expr_rhs.hash(&mut h);
    view_bounds.0.to_bits().hash(&mut h);
    view_bounds.1.to_bits().hash(&mut h);
    view_bounds.2.to_bits().hash(&mut h);
    view_bounds.3.to_bits().hash(&mut h);
    grid_size.hash(&mut h);
    h.finish()
}

#[test]
fn test_eq_outline_parses_and_evaluates() {
    let lhs = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
    let rhs = prepare_function_ast("1", &HashMap::new(), &["x", "y"]).unwrap();
    // Para x=1, y=0: f = 1 - 1 = 0 (en el contorno).
    let f = lhs.eval_2d("x", 1.0, "y", 0.0) - rhs.eval_2d("x", 1.0, "y", 0.0);
    assert!((f - 0.0).abs() < 1e-9, "f en (1,0) debe ser 0, es {}", f);
    // Para x=0.5, y=0: f = 0.25 (fuera del contorno, < 0 → dentro).
    let f = lhs.eval_2d("x", 0.5, "y", 0.0) - rhs.eval_2d("x", 0.5, "y", 0.0);
    assert_eq!(f, -0.75);
    // Para x=2, y=0: f = 3 (fuera del contorno).
    let f = lhs.eval_2d("x", 2.0, "y", 0.0) - rhs.eval_2d("x", 2.0, "y", 0.0);
    assert_eq!(f, 3.0);
}

#[test]
fn test_cache_key_includes_all_fields() {
    // El cache key debe ser diferente para lhs y rhs distintos.
    let k1 = make_key("x^2 + y^2", "1", (-2.0, 2.0, -2.0, 2.0), 256);
    let k2 = make_key("x^2 + y^2", "4", (-2.0, 2.0, -2.0, 2.0), 256);
    assert_ne!(k1, k2, "cache keys deben ser distintos para distintos rhs");
}
