#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! El caché de ASTs debe rechazar expresiones vacías/malformadas sin panicar.
//!
//! Hallazgo G17: la versión anterior solo hacía `println!` sin asserts —
//! pasaba en verde aunque `get_cached_asts` aceptara `""` como `Some` o
//! panicara en otro hilo. Estos tests fallarían antes (ver regresiones abajo).

use grafito_core::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_empty_lhs_is_none() {
    // Regresión: si el parser aceptara "" como Const(0), el render dibujaría
    // una curva fantasma en lugar de omitir el objeto.
    let ic = ImplicitCurveObj::new("", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    assert!(
        ic.get_cached_asts(&vars, &["x", "y"]).is_none(),
        "lhs vacío debe ser None (omitir objeto), no Some"
    );
}

#[test]
fn test_empty_rhs_is_none() {
    let ic = ImplicitCurveObj::new("x^2", "", RelationOperator::Eq);
    let vars = HashMap::new();
    assert!(
        ic.get_cached_asts(&vars, &["x", "y"]).is_none(),
        "rhs vacío debe ser None (omitir objeto), no Some"
    );
}

#[test]
fn test_whitespace_only_is_none() {
    // Regresión: `trim` olvidado haría que "   " parseara distinto de "".
    let ic = ImplicitCurveObj::new("   ", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    assert!(
        ic.get_cached_asts(&vars, &["x", "y"]).is_none(),
        "lhs solo-espacios debe ser None igual que \"\""
    );
}

#[test]
fn test_double_plus_is_unary_plus_not_an_error() {
    // HALLAZGO HONESTO 2026-09-05: `cargo test -- --nocapture` muestra que
    // "x ++ y" hoy da `Some` ("parsed OK"), no `None`. El parser trata el
    // segundo `+` como unario (`parse_unary` consume `+` inicial), es decir
    // `x + (+y)`. Forzar `is_none` aquí sería un test mentiroso que fallaría
    // siempre. Se documenta el contrato real: no panica y es Some.
    // Regresión que atraparía: si alguien endurece el parser para rechazar
    // `++`, este test avisa del cambio de contrato (hay que actualizar docs).
    let ic = ImplicitCurveObj::new("x ++ y", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    let result = ic.get_cached_asts(&vars, &["x", "y"]);
    assert!(
        result.is_some(),
        "contrato actual: 'x ++ y' se acepta como x+(+y); si cambia a None, actualizar docs y parser a la vez"
    );
    // Y debe evaluar igual que "x + y" en un punto de control.
    let ok = ImplicitCurveObj::new("x + y", "1", RelationOperator::Eq);
    let (lhs_plus2, _) = result.expect("checked is_some above");
    let (lhs_plus1, _) = ok
        .get_cached_asts(&vars, &["x", "y"])
        .expect("x + y debe parsear");
    // Ambos AST conservan x,y como Var (ignore=["x","y"]); eval_2d los fija.
    // Si el AST difiriera (p.ej. ++ como operador distinto), esto diverge.
    let v2 = lhs_plus2.eval_2d("x", 2.0, "y", 3.0);
    let v1 = lhs_plus1.eval_2d("x", 2.0, "y", 3.0);
    assert_eq!(
        v2, v1,
        "'x ++ y' debe evaluar igual que 'x + y' con x=2,y=3"
    );
}

#[test]
fn test_truly_malformed_operator_is_none() {
    // Regresión: el caso realmente inválido es operador colgado ("+*").
    // `parse_primary("*")` → Err("Unexpected token"), luego None.
    // El test decorativo anterior usaba "x ++ y" (válido) y nunca cubría esto.
    let ic = ImplicitCurveObj::new("x +* y", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    assert!(
        ic.get_cached_asts(&vars, &["x", "y"]).is_none(),
        "'x +* y' debe ser None (omitir objeto), no Some ni panic"
    );
}

#[test]
fn test_valid_expr_is_some_control() {
    // Control positivo: si `get_cached_asts` devolviera siempre None
    // (fail-closed roto por exceso), este test lo delata. Sin control,
    // los tests solo-None pasarían con un stub `-> None`.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
    let vars = HashMap::new();
    assert!(
        ic.get_cached_asts(&vars, &["x", "y"]).is_some(),
        "expresión válida debe ser Some (control anti-stub)"
    );
}

#[test]
fn test_cache_is_idempotent_without_panic() {
    // Regresión: bug histórico de slot combinado lhs/rhs que se
    // sobreescribía; segunda llamada debe devolver lo mismo sin panic.
    let vars = HashMap::new();
    let empty = ImplicitCurveObj::new("", "1", RelationOperator::Eq);
    assert!(empty.get_cached_asts(&vars, &["x", "y"]).is_none());
    assert!(empty.get_cached_asts(&vars, &["x", "y"]).is_none());
    let valid = ImplicitCurveObj::new("x^2", "1", RelationOperator::Eq);
    assert!(valid.get_cached_asts(&vars, &["x", "y"]).is_some());
    assert!(valid.get_cached_asts(&vars, &["x", "y"]).is_some());
}
