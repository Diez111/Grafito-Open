//! Ejercicios — generación y validación, sin red.

use crate::curriculum::LearningObjective;
use crate::level::PedagogicalLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExerciseKind {
    Numeric,
    Symbolic,
    Graphical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExerciseDifficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub prompt: String,
    pub solution: String,
    pub kind: ExerciseKind,
    pub difficulty: ExerciseDifficulty,
    pub lo_id: String,
}

impl Exercise {
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty() || self.solution.trim().is_empty() {
            return Err("ejercicio incompleto".into());
        }
        if self.prompt.len() > 500 || self.solution.len() > 500 {
            return Err("ejercicio demasiado largo".into());
        }
        Ok(())
    }
}

/// Generador determinista — mapea LO + nivel a ejercicio.
#[derive(Debug, Clone, Default)]
pub struct ExerciseGenerator;

impl ExerciseGenerator {
    pub fn generate(&self, lo: &LearningObjective, level: PedagogicalLevel) -> Exercise {
        let (prompt, solution, kind, diff): (String, String, ExerciseKind, ExerciseDifficulty) =
            match lo.id.as_str() {
                "am1-der" => match level {
                    PedagogicalLevel::Primary => (
                        "Si y=x², ¿cuánto vale y cuando x=2?".into(),
                        "4".into(),
                        ExerciseKind::Numeric,
                        ExerciseDifficulty::Easy,
                    ),
                    PedagogicalLevel::Secondary => (
                        "Calcula f'(1) para f(x)=x²".into(),
                        "2".into(),
                        ExerciseKind::Symbolic,
                        ExerciseDifficulty::Medium,
                    ),
                    _ => (
                        "Deriva f(x)=x³·sin(x) y evalúa en x=π/2".into(),
                        "(3x²·sin x + x³·cos x)|_{π/2}".into(),
                        ExerciseKind::Symbolic,
                        ExerciseDifficulty::Hard,
                    ),
                },
                "am1-int" => (
                    "Calcula ∫₀¹ x² dx".into(),
                    "1/3".into(),
                    ExerciseKind::Symbolic,
                    ExerciseDifficulty::Medium,
                ),
                "sec-trig" => (
                    "¿Cuánto vale sin(π/2)?".into(),
                    "1".into(),
                    ExerciseKind::Numeric,
                    ExerciseDifficulty::Easy,
                ),
                _ => (
                    format!("Explica {}", lo.title),
                    lo.description.clone(),
                    ExerciseKind::Graphical,
                    ExerciseDifficulty::Medium,
                ),
            };
        Exercise {
            prompt,
            solution,
            kind,
            difficulty: diff,
            lo_id: lo.id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generate_valid() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let ex = ExerciseGenerator.generate(&lo, PedagogicalLevel::Secondary);
        assert!(ex.validate().is_ok());
    }
}
