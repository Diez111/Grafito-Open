//! Bayesian Knowledge Tracing (BKT) para el perfil pedagógico.
//! Modelo bayesiano que estima P(sabe) por rama y se actualiza con cada
//! respuesta. Sin dependencias externas, determinista y testeable.

use serde::{Deserialize, Serialize};

/// Parámetros BKT por habilidad/rama.
///
/// - `p_init`: probabilidad inicial de saber la habilidad.
/// - `p_learn`: probabilidad de aprender tras un intento.
/// - `p_guess`: probabilidad de acertar sin saber.
/// - `p_slip`: probabilidad de fallar aun sabiendo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BktParams {
    pub p_init: f64,
    pub p_learn: f64,
    pub p_guess: f64,
    pub p_slip: f64,
}

impl Default for BktParams {
    fn default() -> Self {
        Self {
            p_init: 0.3,
            p_learn: 0.3,
            p_guess: 0.2,
            p_slip: 0.1,
        }
    }
}

impl BktParams {
    /// Crea parámetros validando rangos (0,1) para cada probabilidad.
    pub fn new(p_init: f64, p_learn: f64, p_guess: f64, p_slip: f64) -> Result<Self, String> {
        let candidate = Self {
            p_init,
            p_learn,
            p_guess,
            p_slip,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    /// Valida que cada parámetro esté en \[0,1\] y que guess/slip no sean degenerados.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("p_init", self.p_init),
            ("p_learn", self.p_learn),
            ("p_guess", self.p_guess),
            ("p_slip", self.p_slip),
        ] {
            if !value.is_finite() {
                return Err(format!("{name} no es finito"));
            }
            if !(0.0..=1.0).contains(&value) {
                return Err(format!("{name}={value} fuera de rango 0..=1"));
            }
        }
        if self.p_guess >= 1.0 || self.p_slip >= 1.0 {
            return Err("p_guess/p_slip deben ser < 1.0".to_string());
        }
        Ok(())
    }
}

/// Estado BKT por rama: probabilidad actual de dominio latente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BktState {
    /// P(sabe) actual.
    pub p_known: f64,
}

impl Default for BktState {
    fn default() -> Self {
        Self { p_known: 0.3 }
    }
}

impl BktState {
    /// Crea estado con probabilidad inicial dada (clamp 0..=1).
    pub fn new(p_known: f64) -> Self {
        Self {
            p_known: p_known.clamp(0.0, 1.0),
        }
    }

    /// Crea desde parámetros (usa p_init).
    pub fn from_params(params: &BktParams) -> Self {
        Self {
            p_known: params.p_init.clamp(0.0, 1.0),
        }
    }

    /// Actualiza bayesianamente con la evidencia `correct`.
    ///
    /// Fórmula estándar BKT:
    /// - si acierta: posterior = p*(1-slip) / (p*(1-slip)+(1-p)*guess)
    /// - si falla: posterior = p*slip / (p*slip+(1-p)*(1-guess))
    ///
    /// Luego: p' = posterior + (1-posterior)*p_learn. Retorna el nuevo
    /// `p_known` y muta el estado.
    pub fn update(&mut self, correct: bool, params: &BktParams) -> f64 {
        let next = bkt_update(self.p_known, correct, params);
        self.p_known = next;
        next
    }

    /// Indica si el dominio latente supera el umbral (default 0.8).
    pub fn is_mastered(&self, threshold: f64) -> bool {
        self.p_known >= threshold.clamp(0.0, 1.0)
    }
}

/// Función pura: calcula el siguiente `p_known` sin mutar estado externo.
/// Útil para `BranchState` que guarda solo `f64`.
pub fn bkt_update(p_known: f64, correct: bool, params: &BktParams) -> f64 {
    let p = p_known.clamp(0.0, 1.0);
    let guess = params.p_guess.clamp(0.0, 1.0);
    let slip = params.p_slip.clamp(0.0, 1.0);
    let learn = params.p_learn.clamp(0.0, 1.0);

    // Evitar división por cero con pequeños épsilon.
    let posterior = if correct {
        let num = p * (1.0 - slip);
        let denom = num + (1.0 - p) * guess;
        if denom <= f64::EPSILON {
            p
        } else {
            num / denom
        }
    } else {
        let num = p * slip;
        let denom = num + (1.0 - p) * (1.0 - guess);
        if denom <= f64::EPSILON {
            p
        } else {
            num / denom
        }
    };
    let posterior = posterior.clamp(0.0, 1.0);
    let next = posterior + (1.0 - posterior) * learn;
    next.clamp(0.0, 1.0)
}

/// Parámetros por defecto genéricos (usados si no hay mapeo de rama).
pub const BKT_DEFAULT_PARAMS: BktParams = BktParams {
    p_init: 0.3,
    p_learn: 0.3,
    p_guess: 0.2,
    p_slip: 0.1,
};

/// Parámetros BKT diferenciados por rama (heurística pedagógica).
/// Ramas más abstractas tienen guess más alto y learn levemente menor.
pub fn bkt_params_for_branch(branch_id: &str) -> BktParams {
    match branch_id {
        "functions" => BktParams {
            p_init: 0.35,
            p_learn: 0.32,
            p_guess: 0.22,
            p_slip: 0.09,
        },
        "algebra" => BktParams {
            p_init: 0.30,
            p_learn: 0.30,
            p_guess: 0.20,
            p_slip: 0.10,
        },
        "geometry" | "geometry3d" => BktParams {
            p_init: 0.28,
            p_learn: 0.28,
            p_guess: 0.18,
            p_slip: 0.12,
        },
        "trigonometry" => BktParams {
            p_init: 0.25,
            p_learn: 0.27,
            p_guess: 0.20,
            p_slip: 0.13,
        },
        "calculus" => BktParams {
            p_init: 0.20,
            p_learn: 0.25,
            p_guess: 0.22,
            p_slip: 0.14,
        },
        "stats" => BktParams {
            p_init: 0.30,
            p_learn: 0.31,
            p_guess: 0.24,
            p_slip: 0.10,
        },
        "complex" => BktParams {
            p_init: 0.18,
            p_learn: 0.24,
            p_guess: 0.18,
            p_slip: 0.15,
        },
        _ => BktParams::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_validate_rejects_out_of_range() {
        assert!(BktParams::new(0.3, 0.3, 0.2, 0.1).is_ok());
        assert!(BktParams::new(1.5, 0.3, 0.2, 0.1).is_err());
        assert!(BktParams::new(0.3, -0.1, 0.2, 0.1).is_err());
        assert!(BktParams::new(f64::NAN, 0.3, 0.2, 0.1).is_err());
    }

    #[test]
    fn update_correct_increases_and_incorrect_decreases() {
        let params = BktParams::default();
        let mut state = BktState::new(0.3);
        let after_correct = state.update(true, &params);
        assert!(
            after_correct > 0.3,
            "acierto debe subir p_known: {after_correct}"
        );
        let mut state2 = BktState::new(0.5);
        let after_incorrect = state2.update(false, &params);
        assert!(
            after_incorrect < 0.5,
            "fallo debe bajar p_known: {after_incorrect}"
        );
    }

    #[test]
    fn bkt_update_pure_matches_state_update() {
        let params = BktParams::default();
        let p = 0.4;
        let via_fn = bkt_update(p, true, &params);
        let mut s = BktState::new(p);
        let via_state = s.update(true, &params);
        assert!((via_fn - via_state).abs() < 1e-12);
    }

    #[test]
    fn sequence_correct_monotonic_toward_one() {
        let params = BktParams::default();
        let mut s = BktState::new(0.3);
        let mut prev = s.p_known;
        for _ in 0..6 {
            let next = s.update(true, &params);
            assert!(next >= prev, "con aciertos no debe bajar: {prev} -> {next}");
            assert!(next <= 1.0);
            prev = next;
        }
        assert!(prev > 0.85, "tras 6 aciertos debe estar cerca de 1: {prev}");
    }

    #[test]
    fn incorrect_from_high_drops_significantly() {
        let params = BktParams::default();
        let p = bkt_update(0.9, false, &params);
        assert!(p < 0.9);
        // Con slip bajo, un fallo desde 0.9 no colapsa a 0 pero baja notablemente.
        assert!(p < 0.75, "desde 0.9 un fallo debe bajar bajo 0.75: {p}");
    }

    #[test]
    fn handles_edge_probabilities_without_panic() {
        let params = BktParams::default();
        assert!(bkt_update(0.0, true, &params).is_finite());
        assert!(bkt_update(1.0, false, &params).is_finite());
        assert!(bkt_update(0.0, false, &params).is_finite());
        assert!(bkt_update(1.0, true, &params).is_finite());
    }

    #[test]
    fn params_for_branch_returns_sensible_defaults() {
        let a = bkt_params_for_branch("calculus");
        assert!(a.validate().is_ok());
        let b = bkt_params_for_branch("desconocida");
        assert_eq!(b, BktParams::default());
        // Cada rama conocida debe validar
        for id in [
            "functions",
            "algebra",
            "geometry",
            "trigonometry",
            "calculus",
            "stats",
            "complex",
        ] {
            assert!(bkt_params_for_branch(id).validate().is_ok(), "{id}");
        }
    }

    #[test]
    fn is_mastered_threshold() {
        let s = BktState::new(0.85);
        assert!(s.is_mastered(0.8));
        assert!(!s.is_mastered(0.9));
    }
}
