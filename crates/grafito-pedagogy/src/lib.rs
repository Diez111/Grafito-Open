#![allow(clippy::unwrap_used, clippy::expect_used)]
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

pub mod curriculum;
pub mod exercise;
pub mod feedback;
pub mod level;
pub mod scaffold;
pub mod teaching;

pub use curriculum::{Curriculum, LearningObjective};
pub use exercise::{Exercise, ExerciseDifficulty, ExerciseGenerator, ExerciseKind};
pub use feedback::{Feedback, FeedbackEngine, Misconception};
pub use level::PedagogicalLevel;
pub use level::UTNProgram;
pub use scaffold::{Scaffold, ScaffoldEngine};
pub use teaching::{TeachingSession, TeachingStep, TeachingTopic};

/// Error tipado del motor pedagógico.
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
