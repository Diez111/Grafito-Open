//! Integrales impropias: límites infinitos y singularidades en los extremos.
//!
//! Resuelve por la definición: se calcula la antiderivada simbólica `F =∫ f`
//! con `symbolic::integrate_typed` y el valor es `lim_{x→hi⁻} F(x) − lim_{x→lo⁺}
//! F(x)`. Los extremos finitos se evalúan con **límite lateral desde el interior**
//! del intervalo, que es lo correcto para una integral impropia por singularidad
//! en el borde, y los extremos infinitos con `limit_{pos,neg}_infinity_typed`.
//!
//! Principios del crate: cerebro puro (sin I/O), `MathResult<T>` como tipo de
//! resultado del dominio, y **error honesto sobre número inventado** — una
//! integral que diverge devuelve `DivergentIntegral`, no un valor grande.
//!
//! No hay iteración propia ni cuadratura: todo lo numérico ya vive en
//! `symbolic`/`cas`, así que este módulo solo compone límites.

use crate::symbolic;
use crate::{MathError, MathResult};

/// Resultado de evaluar un extremo del intervalo, conservando la certeza.
struct Extremo {
    valor: f64,
    /// `true` si el límite salió exacto; `false` si vino aproximado.
    exacto: bool,
    error_estimate: f64,
}

/// Evalúa la antiderivada `F` en un extremo del intervalo de integración.
///
/// Extremo infinito → límite en ±∞. Extremo finito → límite **lateral** desde
/// el interior del intervalo (`hi` se aborda por la izquierda, `lo` por la
/// derecha), que tolera singularidades removibles y detecta las divergentes.
fn evalua_extremo(antiderivada: &str, var: &str, x: f64, es_superior: bool) -> MathResult<Extremo> {
    let limite = if x == f64::INFINITY {
        symbolic::limit_pos_infinity_typed(antiderivada, var)
    } else if x == f64::NEG_INFINITY {
        symbolic::limit_neg_infinity_typed(antiderivada, var)
    } else if es_superior {
        symbolic::limit_below_typed(antiderivada, var, x)
    } else {
        symbolic::limit_above_typed(antiderivada, var, x)
    };

    match limite {
        MathResult::Exact(valor) => MathResult::Exact(Extremo {
            valor,
            exacto: true,
            error_estimate: 0.0,
        }),
        MathResult::Approximate {
            value,
            error_estimate,
        } => MathResult::Exact(Extremo {
            valor: value,
            exacto: false,
            error_estimate,
        }),
        MathResult::DomainError(e) => MathResult::DomainError(e),
        MathResult::NotConverged(e) => MathResult::NotConverged(e),
        MathResult::Unsupported(e) => MathResult::Unsupported(e),
        MathResult::ResourceLimit(e) => MathResult::ResourceLimit(e),
    }
}

/// Integral impropia `∫_lo^hi f(x) dx`.
///
/// Admite `lo = f64::NEG_INFINITY`, `hi = f64::INFINITY` y singularidades en
/// cualquiera de los dos extremos. Devuelve:
///
/// - `Exact` si ambos límites son exactos,
/// - `Approximate` si alguno vino numérico (con la estimación propagada),
/// - `DomainError(DivergentIntegral)` si la integral diverge,
/// - `Unsupported(AntiderivativeUnavailable)` si no hay antiderivada simbólica.
///
/// Cotas: la antiderivada hereda `MAX_EXPR_LENGTH` de la validación y los
/// límites reutilizan el presupuesto de `cas::gruntz_limit`.
pub fn improper_integral(expr: &str, var: &str, lo: f64, hi: f64) -> MathResult<f64> {
    if lo.is_nan() || hi.is_nan() {
        return MathResult::DomainError(MathError::NonFiniteLimitPoint {
            expression: expr.to_string(),
            variable: var.to_string(),
            at: if lo.is_nan() { lo } else { hi },
        });
    }

    // `hi` debe ir después de `lo` en la recta extendida.
    if hi < lo {
        return MathResult::DomainError(MathError::IntervalDomainViolation {
            expression: expr.to_string(),
            variable: var.to_string(),
            lower: lo,
            upper: hi,
        });
    }

    // Intervalo degenerado: ∫_a^a = 0. En el infinito no está definido.
    if hi == lo {
        return if lo.is_infinite() {
            MathResult::DomainError(MathError::NonFiniteLimitPoint {
                expression: expr.to_string(),
                variable: var.to_string(),
                at: lo,
            })
        } else {
            MathResult::Exact(0.0)
        };
    }

    // Antiderivada simbólica: `Unsupported` si el motor no llega.
    let antiderivada = match symbolic::integrate_typed(expr, var) {
        MathResult::Exact(f) => f,
        MathResult::Approximate { value, .. } => value,
        MathResult::DomainError(e) => return MathResult::DomainError(e),
        MathResult::NotConverged(e) => return MathResult::NotConverged(e),
        MathResult::Unsupported(_) => {
            return MathResult::Unsupported(MathError::AntiderivativeUnavailable {
                expression: expr.to_string(),
                variable: var.to_string(),
            })
        }
        MathResult::ResourceLimit(e) => return MathResult::ResourceLimit(e),
    };

    let extremo_finito = |x: f64, es_superior: bool, en: f64| -> MathResult<Extremo> {
        match evalua_extremo(&antiderivada, var, x, es_superior) {
            ok @ MathResult::Exact(_) | ok @ MathResult::Approximate { .. } => ok,
            // Un límite lateral que no existe o no converge significa que la
            // integral impropia diverge en ese extremo.
            MathResult::DomainError(_) | MathResult::NotConverged(_) => {
                MathResult::DomainError(MathError::DivergentIntegral {
                    expression: expr.to_string(),
                    variable: var.to_string(),
                    at: en,
                })
            }
            other => other,
        }
    };

    let fin = match extremo_finito(hi, true, hi) {
        MathResult::Exact(e) => e,
        MathResult::Approximate { value: e, .. } => e,
        MathResult::DomainError(e) => return MathResult::DomainError(e),
        MathResult::NotConverged(e) => return MathResult::NotConverged(e),
        MathResult::Unsupported(e) => return MathResult::Unsupported(e),
        MathResult::ResourceLimit(e) => return MathResult::ResourceLimit(e),
    };
    let ini = match extremo_finito(lo, false, lo) {
        MathResult::Exact(e) => e,
        MathResult::Approximate { value: e, .. } => e,
        MathResult::DomainError(e) => return MathResult::DomainError(e),
        MathResult::NotConverged(e) => return MathResult::NotConverged(e),
        MathResult::Unsupported(e) => return MathResult::Unsupported(e),
        MathResult::ResourceLimit(e) => return MathResult::ResourceLimit(e),
    };

    let valor = fin.valor - ini.valor;
    if !valor.is_finite() {
        return MathResult::DomainError(MathError::DivergentIntegral {
            expression: expr.to_string(),
            variable: var.to_string(),
            at: if hi.is_infinite() { hi } else { lo },
        });
    }

    if fin.exacto && ini.exacto {
        MathResult::Exact(valor)
    } else {
        MathResult::Approximate {
            value: valor,
            error_estimate: fin.error_estimate + ini.error_estimate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrae el valor de un `MathResult` de éxito, sea `Exact` o `Approximate`.
    ///
    /// El motor de límites puede caer al repliegue numérico para extremos
    /// finitos; en ese caso el resultado llega como `Approximate` con su
    /// estimación de error, que también validamos.
    fn valor(r: MathResult<f64>) -> f64 {
        match r {
            MathResult::Exact(v) => v,
            MathResult::Approximate {
                value,
                error_estimate,
            } => {
                assert!(
                    error_estimate.is_finite() && error_estimate >= 0.0,
                    "la estimación de error debe ser finita y no negativa"
                );
                value
            }
            otro => panic!("se esperaba un valor, llegó {otro:?}"),
        }
    }

    #[test]
    fn exponencial_en_cero_a_infinito_es_uno() {
        // ∫_0^∞ e^{−x} dx = 1
        let v = valor(improper_integral("exp(-x)", "x", 0.0, f64::INFINITY));
        assert!((v - 1.0).abs() < 1e-4, "vino {v}");
    }

    #[test]
    fn potencia_inversa_en_uno_a_infinito_es_uno() {
        // ∫_1^∞ x^{−2} dx = 1
        let v = valor(improper_integral("1/x^2", "x", 1.0, f64::INFINITY));
        assert!((v - 1.0).abs() < 1e-4, "vino {v}");
    }

    #[test]
    fn divergente_logaritmica_en_cero() {
        // ∫_0^1 1/x dx = ln(x)|_0^1 → diverge
        match improper_integral("1/x", "x", 0.0, 1.0) {
            MathResult::DomainError(MathError::DivergentIntegral { at, .. }) => {
                assert_eq!(at, 0.0, "la divergencia está en el borde inferior")
            }
            otro => panic!("se esperaba DivergentIntegral, llegó {otro:?}"),
        }
    }

    #[test]
    fn divergente_en_ambos_extremos_infinitos() {
        // ∫_{−∞}^{∞} x dx diverge (no hay límite finito en +∞).
        match improper_integral("x", "x", f64::NEG_INFINITY, f64::INFINITY) {
            MathResult::DomainError(MathError::DivergentIntegral { .. }) => {}
            otro => panic!("se esperaba DivergentIntegral, llegó {otro:?}"),
        }
    }

    #[test]
    fn impropia_propia_sin_singularidad_da_valor() {
        // ∫_0^1 x² dx = 1/3 (caso regular, sin impropiedad real).
        let v = valor(improper_integral("x^2", "x", 0.0, 1.0));
        assert!((v - 1.0 / 3.0).abs() < 1e-4, "vino {v}");
    }

    #[test]
    fn intervalo_degenerado_es_cero() {
        assert!(matches!(
            improper_integral("x^2", "x", 2.0, 2.0),
            MathResult::Exact(v) if v == 0.0
        ));
    }

    #[test]
    fn extremos_invertidos_rechazados() {
        assert!(matches!(
            improper_integral("x^2", "x", 1.0, 0.0),
            MathResult::DomainError(MathError::IntervalDomainViolation { .. })
        ));
    }

    #[test]
    fn nan_en_extremos_rechazado() {
        assert!(matches!(
            improper_integral("x^2", "x", f64::NAN, 1.0),
            MathResult::DomainError(MathError::NonFiniteLimitPoint { .. })
        ));
    }
}
