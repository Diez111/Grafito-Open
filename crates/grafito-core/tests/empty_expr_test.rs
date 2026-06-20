//! Verifica que el cache maneja expresiones vacías o malformed.

use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_empty_lhs() {
    // lhs vacío. ¿Qué pasa con el cache?
    let ic = ImplicitCurveObj::new("", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let result = ic.get_cached_asts(&vars, &["x", "y"]);
    // No debe panicar. Puede ser None o Some, pero no panic.
    match result {
        Some((_, _)) => println!("empty lhs: parsed OK (unexpected)"),
        None => println!("empty lhs: None (expected)"),
    }
}

#[test]
fn test_empty_rhs() {
    let ic = ImplicitCurveObj::new("x^2", "", RelationOperator::Eq);
    let vars = HashMap::new();
    let result = ic.get_cached_asts(&vars, &["x", "y"]);
    match result {
        Some((_, _)) => println!("empty rhs: parsed OK (unexpected)"),
        None => println!("empty rhs: None (expected)"),
    }
}

#[test]
fn test_invalid_expr() {
    let ic = ImplicitCurveObj::new("x ++ y", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let result = ic.get_cached_asts(&vars, &["x", "y"]);
    match result {
        Some((_, _)) => println!("invalid: parsed OK (unexpected)"),
        None => println!("invalid: None (expected)"),
    }
}
