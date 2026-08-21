//! Currículum — mapea niveles a objetivos de aprendizaje (UTN y secundaria).

use crate::level::UTNProgram;
use serde::{Deserialize, Serialize};

/// Objetivo de aprendizaje atómico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningObjective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub program: Option<UTNProgram>,
}

impl LearningObjective {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        program: Option<UTNProgram>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            program,
        }
    }
}

/// Currículum estático — UTN AM1/AM2/Álgebra + Secundaria.
#[derive(Debug, Clone)]
pub struct Curriculum;

impl Curriculum {
    /// Todos los LOs de UTN AM1 (funciones, límites, derivadas, integrales).
    pub fn utn_am1() -> Vec<LearningObjective> {
        vec![
            LearningObjective::new(
                "am1-func",
                "Funciones",
                "Dominio, imagen, composición, inversa",
                Some(UTNProgram::AM1),
            ),
            LearningObjective::new(
                "am1-lim",
                "Límites",
                "Límites laterales, indeterminaciones, asintotas",
                Some(UTNProgram::AM1),
            ),
            LearningObjective::new(
                "am1-der",
                "Derivadas",
                "Definición, reglas, recta tangente, extremos",
                Some(UTNProgram::AM1),
            ),
            LearningObjective::new(
                "am1-int",
                "Integrales",
                "Primitivas, área, Barrow, impropias",
                Some(UTNProgram::AM1),
            ),
        ]
    }
    pub fn utn_am2() -> Vec<LearningObjective> {
        vec![
            LearningObjective::new(
                "am2-edo",
                "EDO",
                "Variables separables, lineales, aplicaciones",
                Some(UTNProgram::AM2),
            ),
            LearningObjective::new(
                "am2-series",
                "Series",
                "Convergencia, Taylor, Fourier",
                Some(UTNProgram::AM2),
            ),
        ]
    }
    pub fn secondary() -> Vec<LearningObjective> {
        vec![
            LearningObjective::new(
                "sec-pend",
                "Pendiente",
                "Pendiente de recta, tangente intuitiva",
                None,
            ),
            LearningObjective::new("sec-area", "Área", "Área bajo curva, aproximación", None),
            LearningObjective::new(
                "sec-trig",
                "Trigonometría",
                "Seno, coseno, círculo unitario",
                None,
            ),
        ]
    }
    /// Busca LOs que contengan el concepto (case-insensitive, substring).
    pub fn find_for_concept(concept: &str) -> Vec<LearningObjective> {
        let q = concept.to_lowercase();
        let mut all = Vec::new();
        all.extend(Self::utn_am1());
        all.extend(Self::utn_am2());
        all.extend(Self::secondary());
        all.into_iter()
            .filter(|lo| {
                lo.title.to_lowercase().contains(&q)
                    || lo.description.to_lowercase().contains(&q)
                    || lo.id.contains(&q)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn find_deriva_returns_am1_der() {
        let los = Curriculum::find_for_concept("deriv");
        assert!(los.iter().any(|lo| lo.id == "am1-der"));
    }
}
