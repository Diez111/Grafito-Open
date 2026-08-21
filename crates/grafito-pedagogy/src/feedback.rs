//! Feedback formativo — evalúa respuesta y sugiere próximo paso.

use crate::exercise::Exercise;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Misconception {
    Sign,
    Algebra,
    Concept,
    None,
}

#[derive(Debug, Clone)]
pub struct Feedback {
    pub correct: bool,
    pub misconception: Misconception,
    pub message: String,
    pub next_step: String,
}

/// Evaluador determinista — compara normalizando espacios y case.
#[derive(Debug, Clone, Default)]
pub struct FeedbackEngine;

impl FeedbackEngine {
    pub fn assess(&self, exercise: &Exercise, answer: &str) -> Feedback {
        let norm = |s: &str| s.trim().to_lowercase().replace(' ', "");
        let correct = norm(&exercise.solution) == norm(answer);
        if correct {
            Feedback {
                correct: true,
                misconception: Misconception::None,
                message: "¡Correcto! Bien razonado.".into(),
                next_step: "Probá el siguiente nivel o pedí una animación.".into(),
            }
        } else {
            let misconception = if answer.contains('-') && !exercise.solution.contains('-') {
                Misconception::Sign
            } else {
                Misconception::Concept
            };
            Feedback {
                correct: false,
                misconception,
                message: format!(
                    "Casi. Esperaba '{}', revisá el procedimiento.",
                    exercise.solution
                ),
                next_step: "Repasá la pista socrática y pedí la animación de la tangente.".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exercise::{Exercise, ExerciseDifficulty, ExerciseKind};
    #[test]
    fn feedback_correct() {
        let ex = Exercise {
            prompt: "".into(),
            solution: "2".into(),
            kind: ExerciseKind::Numeric,
            difficulty: ExerciseDifficulty::Easy,
            lo_id: "".into(),
        };
        let fb = FeedbackEngine.assess(&ex, "2");
        assert!(fb.correct);
    }
}
