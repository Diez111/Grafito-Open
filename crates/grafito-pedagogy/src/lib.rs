#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Grafito Pedagogy — Motor pedagógico puro (sin UI, sin red).
//!
//! Provee niveles, currículum (UTN AM1/AM2/Álgebra, secundaria), scaffold socrático,
//! generación de ejercicios y feedback formativo. Todo testeable sin GPU.
//!
//! # Ejemplo
//! ```
//! use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine};
//! let engine = ScaffoldEngine;
//! let scaffold = engine.scaffold("derivada", PedagogicalLevel::Secondary, &[]);
//! assert!(!scaffold.question.is_empty());
//! ```

pub mod bkt;
pub mod curriculum;
pub mod exam;
pub mod exercise;
pub mod feedback;
pub mod level;
pub mod scaffold;
pub mod socratic;
pub mod teaching;

pub use curriculum::{Curriculum, LearningObjective};
pub use exercise::{Exercise, ExerciseDifficulty, ExerciseGenerator, ExerciseKind, ValidatorKind};
pub use feedback::{Feedback, FeedbackEngine, Misconception, Verdict};
pub use level::PedagogicalLevel;
pub use level::{UTNProgram, UdlProfile, UdlRepresentation};
pub use scaffold::NO_CONCEPT_FALLBACK_QUESTION;
pub use scaffold::{extract_concept, is_exploratory_request, Scaffold, ScaffoldEngine, Turn};
pub use socratic::{GuardError, SocraticFsm, SocraticRepair, SocraticState};
pub use teaching::{TeachingSession, TeachingStep, TeachingTopic};

/// Error tipado del motor pedagógico (siempre en español o código tipado).
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum PedagogyError {
    #[error("concepto vacío")]
    EmptyConcept,
    #[error("nivel inválido: {0}")]
    InvalidLevel(String),
    #[error("currículum no encontrado: {0}")]
    CurriculumNotFound(String),
    #[error("ejercicio inválido: {0}")]
    InvalidExercise(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contiene_ingles_crudo(s: &str) -> bool {
        let low = s.to_lowercase();
        [
            "invalid",
            "not found",
            "failed",
            "too many",
            "too early",
            "already done",
            "empty concept",
            "invalid level",
            "unexpected",
        ]
        .iter()
        .any(|pat| low.contains(pat))
    }

    #[test]
    fn errores_publicos_en_espanol() {
        // PedagogyError: mensajes en español.
        let casos = [
            PedagogyError::EmptyConcept.to_string(),
            PedagogyError::InvalidLevel("99".into()).to_string(),
            PedagogyError::CurriculumNotFound("x".into()).to_string(),
            PedagogyError::InvalidExercise("y".into()).to_string(),
        ];
        for msg in &casos {
            assert!(!msg.trim().is_empty(), "mensaje vacío");
            assert!(
                !contiene_ingles_crudo(msg),
                "inglés crudo en PedagogyError: '{msg}'"
            );
        }
        assert!(casos[0].contains("concepto"));
        assert!(casos[1].contains("nivel"));
        assert!(casos[2].contains("currículum"));
        assert!(casos[3].contains("ejercicio"));

        // GuardError: mensajes en español.
        let guardas = [
            GuardError::TellingTooEarly.to_string(),
            GuardError::TooManyAttempts.to_string(),
            GuardError::InvalidTransition("x".into()).to_string(),
            GuardError::AlreadyDone.to_string(),
            GuardError::NotEnoughAttempts.to_string(),
        ];
        for msg in &guardas {
            assert!(!msg.trim().is_empty());
            assert!(
                !contiene_ingles_crudo(msg),
                "inglés crudo en GuardError: '{msg}'"
            );
        }
        assert!(guardas[0].contains("temprano") || guardas[0].contains("intentos"));
        assert!(guardas[1].contains("intentos"));
        assert!(guardas[2].contains("transición"));
        assert!(guardas[3].contains("finalizado"));
        assert!(guardas[4].contains("intento"));

        // Exercise::validate: errores en español.
        let mut ex = ExerciseGenerator.generate(
            &LearningObjective::new("am1-der", "D", "...", None),
            PedagogicalLevel::Secondary,
        );
        ex.prompt.clear();
        let err = ex.validate().expect_err("debe fallar con prompt vacío");
        assert!(!contiene_ingles_crudo(&err));
        assert!(err.contains("incompleto"));

        ex.prompt = "p".into();
        ex.solution.clear();
        let err2 = ex.validate().expect_err("debe fallar con solución vacía");
        assert!(!contiene_ingles_crudo(&err2));

        // Feedback: mensajes en español.
        let fb = FeedbackEngine.assess(
            &ExerciseGenerator.generate(
                &LearningObjective::new("am1-der", "D", "...", None),
                PedagogicalLevel::Secondary,
            ),
            "respuesta incorrecta de prueba",
        );
        assert!(!contiene_ingles_crudo(&fb.message));
        assert!(!contiene_ingles_crudo(&fb.next_step));
    }
}
