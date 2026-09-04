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

/// IDs de los 43 LOs del currículum cubiertos por [`bkt_params_for_lo_opt`].
///
/// Espejo manual de `Curriculum::all()` en
/// `grafito-pedagogy/src/curriculum.rs`: 5 primaria, 11 secundaria, 8 AM1,
/// 7 AM2, 6 Álgebra y 6 Probabilidad = 43 en total (ver test `all_counts`
/// allí).
/// `grafito-profile` no depende de `grafito-pedagogy` (hoja sin ciclos), así
/// que la cobertura se verifica contra esta lista: si el currículum añade,
/// renombra o quita un LO hay que actualizar esta constante Y el `match` de
/// [`bkt_params_for_lo_opt`]; el test `all_los_covered_against_mirror` falla
/// en caso contrario (longitud + `is_some` por ID).
pub const ALL_LO_IDS: [&str; 43] = [
    "pri-conteo",
    "pri-fracc-vis",
    "pri-perim-area",
    "pri-proporciones",
    "pri-datos",
    "sec-fracc",
    "sec-prop",
    "sec-ec",
    "sec-lineal",
    "sec-cuad",
    "sec-pend",
    "sec-area",
    "sec-trig",
    "sec-vect",
    "sec-prob",
    "sec-pitagoras",
    "am1-func",
    "am1-lim",
    "am1-cont",
    "am1-der",
    "am1-der-aplic",
    "am1-int",
    "am1-int-aplic",
    "am1-sucesiones",
    "am2-edo",
    "am2-series",
    "am2-taylor",
    "am2-multivariable",
    "am2-int-multi",
    "am2-campos",
    "am2-teoremas",
    "alg-vectores",
    "alg-rectas-planos",
    "alg-matrices",
    "alg-determinantes",
    "alg-conicas",
    "alg-transformaciones",
    "prob-basica",
    "prob-var",
    "prob-distribuciones",
    "prob-inferencia",
    "prob-regresion",
    "prob-muestreo",
];

/// Mapeo fino por LO individual (43 LOs) con distinción conocido/desconocido.
///
/// Fuente única de verdad para [`bkt_params_for_lo`] e [`is_known_lo`]: los
/// 43 brazos retornan `BktParams` y el `match` completo se envuelve en
/// `Some(...)`; cualquier otro ID usa `return None` temprano. Si se añade un
/// LO en `grafito-pedagogy/src/curriculum.rs`, añadir su brazo aquí Y su ID
/// en [`ALL_LO_IDS`].
///
/// `p_init` escala con `level_min`: primaria (1-2) → 0.40-0.35, secundaria
/// (4-8) → 0.33-0.28, universidad (10-15) → 0.26-0.18. `p_learn`/`p_guess`
/// por bloque temático; `p_slip` crece con dificultad.
pub fn bkt_params_for_lo_opt(lo_id: &str) -> Option<BktParams> {
    Some(match lo_id {
        // Primaria (5) — p_init alto, slip bajo
        "pri-conteo" => BktParams {
            p_init: 0.40,
            p_learn: 0.35,
            p_guess: 0.25,
            p_slip: 0.08,
        },
        "pri-fracc-vis" => BktParams {
            p_init: 0.38,
            p_learn: 0.33,
            p_guess: 0.24,
            p_slip: 0.09,
        },
        "pri-perim-area" => BktParams {
            p_init: 0.36,
            p_learn: 0.32,
            p_guess: 0.22,
            p_slip: 0.09,
        },
        "pri-proporciones" => BktParams {
            p_init: 0.36,
            p_learn: 0.32,
            p_guess: 0.23,
            p_slip: 0.09,
        },
        "pri-datos" => BktParams {
            p_init: 0.38,
            p_learn: 0.33,
            p_guess: 0.24,
            p_slip: 0.08,
        },

        // Secundaria (11) — incluye sec-pitagoras
        "sec-fracc" => BktParams {
            p_init: 0.33,
            p_learn: 0.30,
            p_guess: 0.21,
            p_slip: 0.10,
        },
        "sec-prop" => BktParams {
            p_init: 0.32,
            p_learn: 0.30,
            p_guess: 0.20,
            p_slip: 0.10,
        },
        "sec-ec" => BktParams {
            p_init: 0.30,
            p_learn: 0.29,
            p_guess: 0.20,
            p_slip: 0.11,
        },
        "sec-lineal" => BktParams {
            p_init: 0.30,
            p_learn: 0.29,
            p_guess: 0.19,
            p_slip: 0.11,
        },
        "sec-cuad" => BktParams {
            p_init: 0.28,
            p_learn: 0.28,
            p_guess: 0.19,
            p_slip: 0.12,
        },
        "sec-pend" => BktParams {
            p_init: 0.30,
            p_learn: 0.29,
            p_guess: 0.19,
            p_slip: 0.11,
        },
        "sec-area" => BktParams {
            p_init: 0.30,
            p_learn: 0.29,
            p_guess: 0.20,
            p_slip: 0.11,
        },
        "sec-trig" => BktParams {
            p_init: 0.26,
            p_learn: 0.27,
            p_guess: 0.20,
            p_slip: 0.13,
        },
        "sec-vect" => BktParams {
            p_init: 0.28,
            p_learn: 0.28,
            p_guess: 0.19,
            p_slip: 0.12,
        },
        "sec-prob" => BktParams {
            p_init: 0.31,
            p_learn: 0.30,
            p_guess: 0.23,
            p_slip: 0.10,
        },
        "sec-pitagoras" => BktParams {
            p_init: 0.29,
            p_learn: 0.28,
            p_guess: 0.19,
            p_slip: 0.12,
        },

        // AM1 (8)
        "am1-func" => BktParams {
            p_init: 0.26,
            p_learn: 0.28,
            p_guess: 0.22,
            p_slip: 0.12,
        },
        "am1-lim" => BktParams {
            p_init: 0.24,
            p_learn: 0.27,
            p_guess: 0.21,
            p_slip: 0.13,
        },
        "am1-cont" => BktParams {
            p_init: 0.24,
            p_learn: 0.27,
            p_guess: 0.21,
            p_slip: 0.13,
        },
        "am1-der" => BktParams {
            p_init: 0.22,
            p_learn: 0.26,
            p_guess: 0.22,
            p_slip: 0.14,
        },
        "am1-der-aplic" => BktParams {
            p_init: 0.21,
            p_learn: 0.25,
            p_guess: 0.22,
            p_slip: 0.14,
        },
        "am1-int" => BktParams {
            p_init: 0.22,
            p_learn: 0.26,
            p_guess: 0.22,
            p_slip: 0.14,
        },
        "am1-int-aplic" => BktParams {
            p_init: 0.21,
            p_learn: 0.25,
            p_guess: 0.22,
            p_slip: 0.14,
        },
        "am1-sucesiones" => BktParams {
            p_init: 0.24,
            p_learn: 0.27,
            p_guess: 0.20,
            p_slip: 0.13,
        },

        // AM2 (7)
        "am2-edo" => BktParams {
            p_init: 0.20,
            p_learn: 0.25,
            p_guess: 0.21,
            p_slip: 0.14,
        },
        "am2-series" => BktParams {
            p_init: 0.20,
            p_learn: 0.25,
            p_guess: 0.20,
            p_slip: 0.14,
        },
        "am2-taylor" => BktParams {
            p_init: 0.18,
            p_learn: 0.24,
            p_guess: 0.19,
            p_slip: 0.15,
        },
        "am2-multivariable" => BktParams {
            p_init: 0.19,
            p_learn: 0.25,
            p_guess: 0.20,
            p_slip: 0.14,
        },
        "am2-int-multi" => BktParams {
            p_init: 0.18,
            p_learn: 0.24,
            p_guess: 0.19,
            p_slip: 0.15,
        },
        "am2-campos" => BktParams {
            p_init: 0.18,
            p_learn: 0.24,
            p_guess: 0.19,
            p_slip: 0.15,
        },
        "am2-teoremas" => BktParams {
            p_init: 0.17,
            p_learn: 0.23,
            p_guess: 0.18,
            p_slip: 0.15,
        },

        // Álgebra (6)
        "alg-vectores" => BktParams {
            p_init: 0.25,
            p_learn: 0.28,
            p_guess: 0.19,
            p_slip: 0.12,
        },
        "alg-rectas-planos" => BktParams {
            p_init: 0.23,
            p_learn: 0.27,
            p_guess: 0.19,
            p_slip: 0.13,
        },
        "alg-matrices" => BktParams {
            p_init: 0.25,
            p_learn: 0.28,
            p_guess: 0.20,
            p_slip: 0.12,
        },
        "alg-determinantes" => BktParams {
            p_init: 0.23,
            p_learn: 0.27,
            p_guess: 0.19,
            p_slip: 0.13,
        },
        "alg-conicas" => BktParams {
            p_init: 0.22,
            p_learn: 0.26,
            p_guess: 0.19,
            p_slip: 0.13,
        },
        "alg-transformaciones" => BktParams {
            p_init: 0.20,
            p_learn: 0.25,
            p_guess: 0.18,
            p_slip: 0.14,
        },

        // Probabilidad (6)
        "prob-basica" => BktParams {
            p_init: 0.26,
            p_learn: 0.29,
            p_guess: 0.23,
            p_slip: 0.11,
        },
        "prob-var" => BktParams {
            p_init: 0.24,
            p_learn: 0.28,
            p_guess: 0.23,
            p_slip: 0.12,
        },
        "prob-distribuciones" => BktParams {
            p_init: 0.23,
            p_learn: 0.27,
            p_guess: 0.22,
            p_slip: 0.12,
        },
        "prob-inferencia" => BktParams {
            p_init: 0.21,
            p_learn: 0.26,
            p_guess: 0.22,
            p_slip: 0.13,
        },
        "prob-regresion" => BktParams {
            p_init: 0.22,
            p_learn: 0.26,
            p_guess: 0.22,
            p_slip: 0.13,
        },
        "prob-muestreo" => BktParams {
            p_init: 0.21,
            p_learn: 0.26,
            p_guess: 0.22,
            p_slip: 0.13,
        },

        // Cualquier otro ID es desconocido para el currículum. Incluye las
        // claves legacy de perfil (`functions`/`algebra`/`geometry`/
        // `geometry3d`/`trigonometry`/`calculus`/`stats`/`complex`), que NO
        // son LOs: `bkt_params_for_lo` las resuelve vía `bkt_params_for_branch`
        // para compatibilidad con perfiles antiguos. Se usa `return` temprano
        // para no contaminar el `Some(match ...)` que envuelve los 43 LOs.
        _ => return None,
    })
}

/// ¿El ID corresponde a uno de los 43 LOs del currículum?
///
/// Equivale a `bkt_params_for_lo_opt(lo_id).is_some()`. Retorna `false` para
/// claves legacy de rama (`algebra`, `calculus`, …) aunque
/// [`bkt_params_for_lo`] les dé parámetros: esas no son LOs.
pub fn is_known_lo(lo_id: &str) -> bool {
    bkt_params_for_lo_opt(lo_id).is_some()
}

/// Parámetros BKT por LO con fallback compatible.
///
/// - LO conocido (ver [`ALL_LO_IDS`]) → parámetros diferenciados.
/// - Clave legacy de rama (`functions`, `algebra`, `geometry`, `geometry3d`,
///   `trigonometry`, `calculus`, `stats`, `complex`) → delega en
///   [`bkt_params_for_branch`] (perfiles antiguos).
/// - Cualquier otro ID → [`BktParams::default`].
///
/// Sin pánicos: todo `match`, ningún `unwrap`/`expect`.
pub fn bkt_params_for_lo(lo_id: &str) -> BktParams {
    match bkt_params_for_lo_opt(lo_id) {
        Some(params) => params,
        None => match lo_id {
            "functions" | "algebra" | "geometry" | "geometry3d" | "trigonometry" | "calculus"
            | "stats" | "complex" => bkt_params_for_branch(lo_id),
            _ => BktParams::default(),
        },
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

    #[test]
    fn all_lo_ids_mirror_has_43_entries() {
        // Espejo de `Curriculum::all().len()` (ver
        // `grafito-pedagogy/src/curriculum.rs::all_counts`): si el currículum
        // cambia de tamaño, actualizar `ALL_LO_IDS` + `bkt_params_for_lo_opt`.
        assert_eq!(ALL_LO_IDS.len(), 43, "deben ser 43 LOs");
        // Sin duplicados.
        let mut sorted = ALL_LO_IDS;
        sorted.sort_unstable();
        for pair in sorted.windows(2) {
            assert_ne!(pair[0], pair[1], "ID duplicado en ALL_LO_IDS: {}", pair[0]);
        }
    }

    #[test]
    fn all_los_covered_against_mirror() {
        // Verifica por ID que cada LO del espejo tiene parámetros conocidos y
        // válidos. Falla si falta un brazo en `bkt_params_for_lo_opt` (opt
        // retornaría `None`) o si algún parámetro es inválido.
        assert_eq!(ALL_LO_IDS.len(), 43, "deben ser 43 LOs");
        for id in ALL_LO_IDS {
            assert!(
                is_known_lo(id),
                "{id} debe ser un LO conocido (falta brazo en bkt_params_for_lo_opt)"
            );
            let opt = bkt_params_for_lo_opt(id);
            assert!(opt.is_some(), "{id} debe tener parámetros Some");
            let p = bkt_params_for_lo(id);
            assert!(p.validate().is_ok(), "{id} debe validar: {p:?}");
            // p_init decrece con nivel: primaria ~0.36-0.40, secundaria ~0.26-0.33, uni ~0.17-0.26
            assert!(
                (0.15..=0.45).contains(&p.p_init),
                "{id} p_init fuera rango: {}",
                p.p_init
            );
        }
        // Desconocido retorna None + fallback default.
        assert!(!is_known_lo("no-existe"));
        assert!(bkt_params_for_lo_opt("no-existe").is_none());
        assert_eq!(bkt_params_for_lo("no-existe"), BktParams::default());
        // Claves legacy de rama NO son LOs (opt None) pero mantienen delegación.
        for legacy in ["algebra", "calculus", "geometry3d", "stats", "complex"] {
            assert!(!is_known_lo(legacy), "{legacy} no es un LO");
            assert!(bkt_params_for_lo_opt(legacy).is_none());
            assert_eq!(
                bkt_params_for_lo(legacy),
                bkt_params_for_branch(legacy),
                "{legacy} debe delegar en branch"
            );
        }
    }

    #[test]
    fn bkt_params_for_lo_p_init_monotonic_primary_gt_university() {
        let pri = bkt_params_for_lo("pri-conteo").p_init;
        let sec = bkt_params_for_lo("sec-fracc").p_init;
        let uni = bkt_params_for_lo("am2-teoremas").p_init;
        assert!(
            pri > sec,
            "primaria debe tener p_init mayor que secundaria: {pri} vs {sec}"
        );
        assert!(
            sec > uni,
            "secundaria debe tener p_init mayor que uni avanzada: {sec} vs {uni}"
        );
    }
}
