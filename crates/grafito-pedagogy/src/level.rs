//! Niveles pedagógicos — de primaria a UTN.

use serde::{Deserialize, Serialize};

/// Programa UTN asociado a un nivel universitario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UTNProgram {
    /// Análisis Matemático I (funciones, límites, derivadas, integrales)
    AM1,
    /// Análisis Matemático II (EDO, series, multivariable)
    AM2,
    /// Álgebra y Geometría Analítica (vectores, matrices, cónicas)
    Algebra,
    /// Probabilidad y Estadística
    Probabilidad,
}

impl UTNProgram {
    pub const fn label(self) -> &'static str {
        match self {
            Self::AM1 => "UTN AM1",
            Self::AM2 => "UTN AM2",
            Self::Algebra => "UTN Álgebra",
            Self::Probabilidad => "UTN Probabilidad",
        }
    }
}

/// Nivel pedagógico que controla el andamiaje y la formalidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PedagogicalLevel {
    /// Primaria (intuitivo, ejemplos concretos, sin formalismo)
    Primary,
    /// Secundaria (gráficos, pendiente, área, trigonometría)
    #[default]
    Secondary,
    /// Universidad genérica (definiciones formales, demostraciones)
    University,
    /// UTN con programa específico (AM1/AM2/Álgebra/Probabilidad)
    UTN(UTNProgram),
}

impl PedagogicalLevel {
    /// Idioma del andamiaje: primario/secundaria usa metáforas, uni usa formal.
    pub const fn is_formal(self) -> bool {
        matches!(self, Self::University | Self::UTN(_))
    }

    /// ¿Debe usar preguntas socráticas antes de dar la respuesta?
    pub const fn uses_socratic(self) -> bool {
        !matches!(self, Self::Primary)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primaria",
            Self::Secondary => "Secundaria",
            Self::University => "Universidad",
            Self::UTN(p) => p.label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level_labels_are_not_empty() {
        for lvl in [
            PedagogicalLevel::Primary,
            PedagogicalLevel::Secondary,
            PedagogicalLevel::University,
            PedagogicalLevel::UTN(UTNProgram::AM1),
        ] {
            assert!(!lvl.label().is_empty());
        }
    }
    #[test]
    fn primary_is_not_formal() {
        assert!(!PedagogicalLevel::Primary.is_formal());
        assert!(PedagogicalLevel::UTN(UTNProgram::AM1).is_formal());
    }
}
