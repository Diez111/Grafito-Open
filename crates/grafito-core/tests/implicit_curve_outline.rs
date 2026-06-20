//! Verifica que el outline de un ImplicitCurve se genera correctamente.

use grafito_core::implicit_curve::evaluate_implicit_curve;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_outline_for_x2_y2_eq_1() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let segs = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    assert!(!segs.is_empty(), "debe haber al menos un nivel");
    let (_, segments) = &segs[0];
    assert!(!segments.is_empty(), "debe haber segmentos para el círculo");
    // El círculo unitario tiene ~24 segmentos con grid 256.
    println!("outline segments count: {}", segments.len());
}

#[test]
fn test_outline_for_x2_y2_lt_1() {
    // Para <, los segmentos también están en el contorno.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let segs = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    let (_, segments) = &segs[0];
    println!("x²+y²<1 segments: {}", segments.len());
    assert!(!segments.is_empty());
}

#[test]
fn test_outline_for_x2_y2_le_1() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::LessEq);
    let segs = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    let (_, segments) = &segs[0];
    println!("x²+y²≤1 segments: {}", segments.len());
    assert!(!segments.is_empty());
}

#[test]
fn test_outline_for_x_sin_y() {
    // Una curva con seno.
    let ic = ImplicitCurveObj::new("x", "sin(y)", RelationOperator::Eq);
    let segs = evaluate_implicit_curve(&ic, (-5.0, 5.0, -5.0, 5.0), 256, &HashMap::new());
    let (_, segments) = &segs[0];
    println!("x=sin(y) segments: {}", segments.len());
    assert!(!segments.is_empty());
}

#[test]
fn test_outline_for_complex_expr() {
    // Una curva más compleja.
    let ic = ImplicitCurveObj::new("x^2 * y", "1", RelationOperator::Eq);
    let segs = evaluate_implicit_curve(&ic, (-2.0, 2.0, -2.0, 2.0), 256, &HashMap::new());
    let (_, segments) = &segs[0];
    println!("x²y=1 segments: {}", segments.len());
    assert!(!segments.is_empty());
}
