//! Stubs L honestos del perfil (diseño + `Err` + test).
//!
//! Alcance F10: `calibración EM con N≥200` y `Elo on-the-fly` son L — aquí
//! solo vive su diseño y un stub que siempre falla honesto. El frente útil
//! (S/M) es `bkt_posterior` + `FSRS-lite` + `recommend_interleaved`, ya
//! compilados y testeados sin I/O.
//!
//! PII siempre local: ningún stub toca disco ni red.

/// Error honesto de los stubs avanzados (siempre `Err`, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedStubError {
    /// Nombre estable (`EmCalibration`, `Elo`).
    pub feature: &'static str,
    /// Diseño + requisito faltante.
    pub hint: String,
}

impl std::fmt::Display for AdvancedStubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} no implementado: {}", self.feature, self.hint)
    }
}

impl std::error::Error for AdvancedStubError {}

/// Mínimo muestral para calibrar BKT por EM (diseño F10).
pub const EM_MIN_SAMPLES: usize = 200;

/// Diseño calibración EM de parámetros BKT (L).
///
/// Estimaría `p_learn/p_guess/p_slip` por Expectation-Maximization sobre
/// `≥200` intentos por LO (E-step: posteriores por secuencia; M-step:
///
/// medias cerradas), con validación cruzada y fallback a
/// `bkt_params_for_lo` si `samples < 200`. Requiere historial persistido +
/// pipeline batch (fuera del frente). Hoy: stub honesto.
pub fn em_calibration_stub(samples: usize) -> Result<String, AdvancedStubError> {
    let _ = samples;
    Err(AdvancedStubError {
        feature: "EmCalibration",
        hint: format!(
            "diseño F10.W5: EM sobre intentos por LO con N≥{EM_MIN_SAMPLES} + validación cruzada y fallback a bkt_params_for_lo; hoy parámetros fijos por LO"
        ),
    })
}

/// Diseño Elo on-the-fly por ejercicio (L).
///
/// Mantendría un rating por alumno y por ítem (`R_alumno`, `R_item`, `K=32`)
/// actualizado tras cada `assess_answer` (`E = 1/(1+10^((R_item-R_alumno)/400))`),
/// para seleccionar dificultad adaptativa. Requiere banco de ítems calibrado
/// (fuera del frente). Hoy: stub honesto.
pub fn elo_update_stub() -> Result<String, AdvancedStubError> {
    Err(AdvancedStubError {
        feature: "Elo",
        hint: "diseño F10.W5: rating alumno/ítem K=32 con banco calibrado para dificultad adaptativa; hoy dificultad fija por LO"
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_stubs_always_fail_honestly() {
        let em = em_calibration_stub(50).expect_err("L siempre falla");
        assert_eq!(em.feature, "EmCalibration");
        assert!(em.to_string().contains("N≥200"));

        let em_big = em_calibration_stub(500).expect_err("L siempre falla aun con N grande");
        assert_eq!(em_big.feature, "EmCalibration");

        let elo = elo_update_stub().expect_err("L siempre falla");
        assert_eq!(elo.feature, "Elo");
        assert!(elo.to_string().contains("diseño"));
    }
}
