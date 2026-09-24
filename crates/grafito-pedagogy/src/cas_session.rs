//! Puente CAS→sesión (Tarea 1): 1 `CasStep` = 1 [`TeachingStep`].
//!
//! [`CasStepAdapter`] es el adaptador mínimo entre el stepper real del CAS
//! (`grafito-geometry::cas_steps`: `CasStep { before, after, rule,
//! description }` + `steps_for_op`) y la sesión de enseñanza. Cada `CasStep`
//! baja a un [`TeachingStep`] con:
//!
//! - `id`: `"cas{i}"` (1-based),
//! - `title`: `"Paso {i}: {rule}"` (la regla tiene `Display` estable),
//! - `explanation`: justificación (`CasStep::description`) + resultado
//!   (`CasStep::after`),
//! - `math_expr`: `after` **solo si** pasa el CAS-gate [`verify_math_expr`]
//!   (mismo parser del canvas); si no parsea → `None` honesto, el paso
//!   se conserva igual,
//! - `whiteboard_hint`: la reescritura `before → after` para hidratar la
//!   pizarra,
//! - `cue_ms`: escalonado uniforme ([`CAS_CUE_STEP_MS`] por paso) para que
//!   [`crate::teaching::TeachingSession::revealed_steps`] los muestre de a
//!   uno (el revelado progresivo ya está resuelto en `teaching.rs`).
//!
//! **Cierre del loop** (circuito cerrado con el FSM socrático): el **último**
//! paso lleva el remate `with_final_check(probe, expected)` vía
//! [`CasStepsASesion::a_sesion_con_remate`]; luego
//! [`crate::teaching::TeachingStep::assess_final`] +
//! [`crate::socratic::SocraticFsm::mark_success`] cierran el ciclo (ver test
//! end-to-end en `teaching.rs`).
//!
//! Patrón: **trait local sobre tipo foráneo** (idéntico a
//! [`crate::guion_session::GuionASesion`]): `grafito-geometry` no depende de
//! `grafito-pedagogy`, así que el grafo de dependencias no se invierte.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin pánicos.

use grafito_geometry::cas_steps::CasStep;

use crate::teaching::{verify_math_expr, TeachingSession, TeachingStep, TeachingTopic};

/// Escalón de `cue_ms` entre pasos CAS: 5 s. Uniforme a propósito: sin
/// duración real por paso en la traza del CAS, un escalón fijo es el reparto
/// honesto (el `Playlist::schedule` del guion sí trae duraciones reales).
pub const CAS_CUE_STEP_MS: u64 = 5_000;

/// Adaptador mínimo `CasStep → TeachingStep` (trait local sobre tipo foráneo).
pub trait CasStepAdapter {
    /// Baja este `CasStep` a un [`TeachingStep`] con `id "cas{index + 1}"`.
    fn a_teaching_step(&self, index: usize) -> TeachingStep;
}

impl CasStepAdapter for CasStep {
    fn a_teaching_step(&self, index: usize) -> TeachingStep {
        let k = index.saturating_add(1);
        let description = self.description.trim();
        let after = self.after.trim();
        // explanation = justificación + expr_after (el `after` también vive en
        // `math_expr`, pero el paso debe leerse completo sin la pizarra).
        let explanation = if description.is_empty() {
            after.to_string()
        } else if after.is_empty() {
            description.to_string()
        } else {
            format!("{description}\n{after}")
        };
        let before = self.before.trim();
        let hint = if before.is_empty() {
            after.to_string()
        } else {
            format!("{before} → {after}")
        };
        let mut step = TeachingStep::new(
            format!("cas{k}"),
            format!("Paso {k}: {}", self.rule),
            explanation,
        )
        .with_whiteboard(hint)
        .with_cue((index as u64).saturating_mul(CAS_CUE_STEP_MS));
        // CAS-gate: solo baja como `math_expr` el `after` que el parser del
        // canvas puede evaluar; el resto queda honesto (`None`, `verified=false`).
        if verify_math_expr(after) {
            step = step.with_math(after);
        }
        step
    }
}

/// Adaptador mínimo traza del CAS → [`TeachingSession`] (trait local sobre el
/// slice de tipo foráneo: mismo patrón que [`CasStepAdapter`]).
pub trait CasStepsASesion {
    /// Baja la traza completa a una [`TeachingSession`] sin remate (pasos
    /// expositivos; el integrador suma el remate después).
    fn a_sesion(&self, concepto: &str) -> TeachingSession;
    /// Igual que [`Self::a_sesion`] pero el **último** paso cierra el loop con
    /// `with_final_check(probe, expected)`. Traza vacía → sesión vacía sin
    /// remate (no se inventa el remate de una traza que no existe).
    fn a_sesion_con_remate(&self, concepto: &str, probe: &str, expected: &str) -> TeachingSession;
}

/// Construcción común: `remate = Some((probe, expected))` solo sobre el último paso.
fn sesion_desde_pasos(
    concepto: &str,
    pasos: &[CasStep],
    remate: Option<(&str, &str)>,
) -> TeachingSession {
    let n = pasos.len();
    let mut steps = Vec::with_capacity(n);
    for (i, paso) in pasos.iter().enumerate() {
        let mut step =
            paso.a_teaching_step(i)
                .with_manim(format!("cas:{}/{}", i.saturating_add(1), n));
        if n > 0 && i == n.saturating_sub(1) {
            if let Some((probe, expected)) = remate {
                step = step.with_final_check(probe, expected);
            }
        }
        steps.push(step);
    }
    // `TeachingSession::new` re-aplica el CAS-gate y marca `verified`.
    TeachingSession::new(TeachingTopic::from_text(concepto), steps)
}

impl CasStepsASesion for [CasStep] {
    fn a_sesion(&self, concepto: &str) -> TeachingSession {
        sesion_desde_pasos(concepto, self, None)
    }

    fn a_sesion_con_remate(&self, concepto: &str, probe: &str, expected: &str) -> TeachingSession {
        sesion_desde_pasos(concepto, self, Some((probe, expected)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socratic::{GuardError, SocraticFsm, SocraticState};
    use grafito_geometry::cas_steps::{steps_for_op, CasOp, RewriteRule};

    fn traza_derivada_x2() -> Vec<CasStep> {
        steps_for_op(&CasOp::Derivative {
            expr: "x^2".into(),
            var: "x".into(),
        })
        .expect("steps_for_op derivada x^2")
    }

    #[test]
    fn cada_cas_step_baja_a_teaching_step_con_cues_escalonados() {
        let traza = traza_derivada_x2();
        assert!(!traza.is_empty(), "la derivada de x^2 debe emitir pasos");
        let s = traza.a_sesion("derivada de x^2");
        assert_eq!(s.steps.len(), traza.len());
        assert_eq!(s.topic, TeachingTopic::Derivada);
        for (i, step) in s.steps.iter().enumerate() {
            let k = i.saturating_add(1);
            assert_eq!(step.id, format!("cas{k}"));
            assert_eq!(step.title, format!("Paso {k}: {}", traza[i].rule));
            // cue_ms uniforme: 0, 5000, 10000, …
            assert_eq!(step.cue_ms, (i as u64) * CAS_CUE_STEP_MS);
            assert!(step.manim_template.is_some());
            // explanation = justificación + after.
            assert!(step.explanation.contains(traza[i].after.trim()));
            // pizarra = reescritura before → after.
            assert!(step.whiteboard_hint.contains('→'));
        }
        // Revelado progresivo: de a uno según el escalón.
        assert_eq!(s.revealed_count(0), 1);
        assert_eq!(s.revealed_count(CAS_CUE_STEP_MS - 1), 1);
        assert_eq!(s.revealed_count(CAS_CUE_STEP_MS), 2.min(s.steps.len()));
        assert_eq!(s.revealed_count(u64::MAX), s.steps.len());
    }

    #[test]
    fn math_expr_solo_si_pasa_el_cas_gate() {
        let traza = traza_derivada_x2();
        let s = traza.a_sesion("derivada de x^2");
        // Al menos un paso con matemática verificada (el `after` de una
        // derivada de polinomio parsea).
        assert!(
            s.steps
                .iter()
                .any(|st| st.verified && st.math_expr.is_some()),
            "la traza de x^2 debe traer al menos una math verificada"
        );
        // Ningún paso conserva math sin verificar (gate de `TeachingSession::new`).
        for st in &s.steps {
            assert!(
                st.math_expr.is_none() || st.verified,
                "math sin verificar en {}",
                st.id
            );
        }
        // Paso con `after` que NO parsea → math honesta en None, paso conservado.
        let crudo = CasStep {
            index: 0,
            rule: RewriteRule::Generic,
            before: "f(x)".into(),
            after: "f'(x)=2x".into(),
            description: "derivada literal".into(),
        };
        let s2 = [crudo].a_sesion("derivada");
        assert_eq!(s2.steps.len(), 1);
        assert_eq!(s2.steps[0].math_expr, None);
        assert!(!s2.steps[0].verified);
    }

    #[test]
    fn remate_en_ultimo_paso_cierra_loop_socratico() {
        let traza = traza_derivada_x2();
        let probe = "Si f(x)=x², ¿cuánto vale f'(1)?";
        let expected = "2";
        let s = traza.a_sesion_con_remate("derivada de x^2", probe, expected);
        // Solo el último paso lleva remate.
        assert!(s.steps[..s.steps.len() - 1]
            .iter()
            .all(|st| st.check.is_none()));
        let ultimo = s.steps.last().expect("al menos un paso");
        let check = ultimo.check.as_ref().expect("último con remate");
        assert_eq!(check.probe, probe);
        assert_eq!(check.expected, expected);
        // Circuito cerrado: assess_final + mark_success.
        let bien = ultimo.assess_final("2").expect("assess");
        assert!(bien.correct);
        let mal = ultimo.assess_final("5").expect("assess");
        assert!(!mal.correct);
        let mut fsm = SocraticFsm::new("derivada");
        fsm.record_attempt(None).expect("intento");
        assert_eq!(
            fsm.mark_success(mal.correct).unwrap_err(),
            GuardError::CheckNotCorrect
        );
        assert!(fsm.mark_success(bien.correct).is_ok());
        assert!(matches!(fsm.state, SocraticState::Summarize));
    }

    #[test]
    fn traza_vacia_da_sesion_honesta() {
        let vacia: Vec<CasStep> = Vec::new();
        let s = vacia.a_sesion_con_remate("derivada", "¿?", "0");
        assert!(s.steps.is_empty());
        // Sin pasos no hay remate inventado.
        assert!(s.revealed_count(u64::MAX) == 0);
    }

    #[test]
    fn sesion_es_determinista() {
        let traza = traza_derivada_x2();
        let a = traza.a_sesion_con_remate("derivada", "probe", "2");
        let b = traza.a_sesion_con_remate("derivada", "probe", "2");
        assert_eq!(a.steps.len(), b.steps.len());
        for (x, y) in a.steps.iter().zip(b.steps.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.explanation, y.explanation);
            assert_eq!(x.math_expr, y.math_expr);
            assert_eq!(x.cue_ms, y.cue_ms);
        }
    }
}
