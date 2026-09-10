//! Puerta experta al CAS nativo (F2: voto usuario alkahest-cas).
//!
//! Feature OPT-IN `cas-nativo`: usa `alkahest-cas 3` (Expr hash-consed vía
//! `ExprPool`, `simplify::engine`, `to_smtlib`; F4/ArbBall/Lean bajo demanda).
//! Verificado 2026-09-10: el build de alkahest exige FLINT del sistema
//! (>= 2.9, hard dep) + C transitiva (`gmp-mpfr-sys`/`rug`); sin
//! `libflint-dev` el feature no compila y el fallback de abajo manda.
//! Build default (sin la feature): fallback 100% Rust
//! (`grafito-geometry::symbolic` + `grafito-core::symbolic::groebner_gate`).
//! Ninguna función hace I/O ni toca `Document`; todas respetan
//! `MAX_EXPR_LENGTH` 2000 + finito + dominio.

/// Tope de bytes por expresión (paridad con `validation::MAX_EXPR_LENGTH`).
pub const MAX_CAS_EXPR_BYTES: usize = 2000;

/// Indica si el backend experto está compilado en este build.
pub const CAS_NATIVO_ENABLED: bool = cfg!(feature = "cas-nativo");

/// Nombre estable del backend activo (para verificación de tools).
#[must_use]
pub const fn backend_name() -> &'static str {
    if CAS_NATIVO_ENABLED {
        "alkahest-cas 3 (hash-consed Expr, simplify; F4/ArbBall bajo demanda)"
    } else {
        "typed nativo (grafito-geometry::symbolic; fallback sin C)"
    }
}

/// Versión del kernel experto, o `None` sin la feature.
#[must_use]
pub fn cas_nativo_version() -> Option<&'static str> {
    #[cfg(feature = "cas-nativo")]
    {
        Some(alkahest_cas::version())
    }
    #[cfg(not(feature = "cas-nativo"))]
    {
        None
    }
}

/// Verifica `a ≡ b` vía `simplify(a-b) == 0`.
///
/// Con `cas-nativo`: parsea `({a})-({b})` en `ExprPool` y simplifica con el
/// motor experto; el display debe ser `0`. Sin la feature (o si el parse
/// experto falla): fallback local `grafito_geometry::symbolic::simplify`.
/// `Err` honesto ante entrada vacía, sobre-presupuesto o no simplificable.
pub fn verify_equivalence(a: &str, b: &str) -> Result<bool, String> {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return Err("verify_step requiere 'a' y 'b' no vacíos".into());
    }
    if a.len() > MAX_CAS_EXPR_BYTES || b.len() > MAX_CAS_EXPR_BYTES {
        return Err(format!(
            "verify_step excede {MAX_CAS_EXPR_BYTES} bytes por lado"
        ));
    }
    #[cfg(feature = "cas-nativo")]
    if let Ok(expert) = expert_is_zero_difference(a, b) {
        return Ok(expert);
    }
    local_is_zero_difference(a, b)
}

/// Segunda opinión experta: `true` si el display simplificado es `0`.
///
/// `Err` si el parse experto no cubre la sintaxis (el llamante cae al
/// fallback local en vez de inventar equivalencia).
#[cfg(feature = "cas-nativo")]
fn expert_is_zero_difference(a: &str, b: &str) -> Result<bool, String> {
    use alkahest_cas::kernel::Domain;
    use std::collections::HashMap;

    let pool = alkahest_cas::ExprPool::new();
    let mut symbols = HashMap::new();
    // Símbolos libres habituales; el parser experto no los crea solo.
    for name in [
        "x", "y", "z", "t", "u", "v", "w", "a", "b", "c", "k", "n", "m", "pi",
    ] {
        let id = pool.symbol(name, Domain::Real);
        symbols.insert(name.to_owned(), id);
    }
    let difference = format!("({a})-({b})");
    let id = alkahest_cas::parse(&difference, &pool, &mut symbols)
        .map_err(|error| format!("parse experto: {error:?}"))?;
    let simplified = alkahest_cas::simplify::engine::simplify(id, &pool);
    Ok(pool.display(simplified.value).to_string().trim() == "0")
}

/// Fallback local puro: `simplify((a)-(b)) == "0"`.
fn local_is_zero_difference(a: &str, b: &str) -> Result<bool, String> {
    let difference = format!("({a})-({b})");
    match grafito_geometry::symbolic::simplify(&difference) {
        Ok(normalized) => Ok(normalized.trim() == "0"),
        Err(error) => Err(format!("simplify local: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_name_matches_feature_flag() {
        assert_eq!(CAS_NATIVO_ENABLED, cfg!(feature = "cas-nativo"));
        assert_eq!(cas_nativo_version().is_some(), CAS_NATIVO_ENABLED);
        assert!(!backend_name().is_empty());
    }

    #[test]
    fn verify_rejects_empty_or_oversized_input() {
        assert!(verify_equivalence("", "x").is_err());
        assert!(verify_equivalence("x", "").is_err());
        let big = "x".repeat(MAX_CAS_EXPR_BYTES + 1);
        assert!(verify_equivalence(&big, "x").is_err());
    }

    #[test]
    fn identical_expressions_are_equivalent() {
        assert_eq!(verify_equivalence("x", "x"), Ok(true));
    }

    #[test]
    fn clearly_different_expressions_are_not_equivalent() {
        assert_eq!(verify_equivalence("x", "x + 1"), Ok(false));
    }
}
