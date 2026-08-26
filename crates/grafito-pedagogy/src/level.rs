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

/// Representación UDL (Universal Design for Learning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UdlRepresentation {
    /// Concreto: manipulativos, ejemplos físicos.
    Concrete,
    /// Gráfico: diagramas, gráficas, visual.
    Graphic,
    /// Simbólico: notación matemática formal intermedia.
    Symbolic,
    /// Formal: rigor axiomático y demostraciones.
    Formal,
}

/// Perfil UDL asociado a un nivel pedagógico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UdlProfile {
    /// Tipo de representación privilegiada.
    pub representation: UdlRepresentation,
    /// Nivel de lenguaje (1 concreto .. 4 formal).
    pub language_level: u8,
    /// Grupos de herramientas permitidas (ids de plugins / modos).
    pub allowed_tool_groups: Vec<String>,
    /// Descripción humana del perfil.
    pub description: String,
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

    /// Valor numérico del nivel para gamificación/progresión.
    /// Primary 2, Secondary 8, University 15, UTN AM1 12, AM2 14, Álgebra 13, Probabilidad 14.
    pub const fn level_value(self) -> u32 {
        match self {
            Self::Primary => 2,
            Self::Secondary => 8,
            Self::University => 15,
            Self::UTN(UTNProgram::AM1) => 12,
            Self::UTN(UTNProgram::AM2) => 14,
            Self::UTN(UTNProgram::Algebra) => 13,
            Self::UTN(UTNProgram::Probabilidad) => 14,
        }
    }

    /// Perfil UDL asociado al nivel.
    pub fn udl_profile(self) -> UdlProfile {
        match self {
            Self::Primary => UdlProfile {
                representation: UdlRepresentation::Concrete,
                language_level: 1,
                allowed_tool_groups: vec!["manipulativos".into(), "visual".into(), "conteo".into()],
                description: "Concreto: manipulativos y ejemplos visuales".into(),
            },
            Self::Secondary => UdlProfile {
                representation: UdlRepresentation::Graphic,
                language_level: 2,
                allowed_tool_groups: vec![
                    "grafico".into(),
                    "geometrico".into(),
                    "algebra-basica".into(),
                ],
                description: "Gráfico: diagramas, gráficas y geometría".into(),
            },
            Self::University => UdlProfile {
                representation: UdlRepresentation::Symbolic,
                language_level: 3,
                allowed_tool_groups: vec![
                    "simbolico".into(),
                    "demostracion".into(),
                    "calculo".into(),
                ],
                description: "Simbólico: formalismo y demostraciones".into(),
            },
            Self::UTN(_) => UdlProfile {
                representation: UdlRepresentation::Formal,
                language_level: 4,
                allowed_tool_groups: vec![
                    "formal".into(),
                    "riguroso".into(),
                    "calculo-avanzado".into(),
                ],
                description: "Formal: rigor universitario UTN".into(),
            },
        }
    }

    /// Helper gamificación: mapea valor numérico a nivel.
    /// 0..=4 => Primary, 5..=10 => Secondary, 11..=12 => UTN AM1, 13 => Álgebra, 14 => AM2, _ => University.
    /// Nota: Probabilidad comparte 14 con AM2; `from_level_value(14)` retorna AM2.
    pub fn from_level_value(value: u32) -> Self {
        match value {
            0..=4 => Self::Primary,
            5..=10 => Self::Secondary,
            11..=12 => Self::UTN(UTNProgram::AM1),
            13 => Self::UTN(UTNProgram::Algebra),
            14 => Self::UTN(UTNProgram::AM2),
            _ => Self::University,
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
    #[test]
    fn level_value_mapping() {
        assert_eq!(PedagogicalLevel::Primary.level_value(), 2);
        assert_eq!(PedagogicalLevel::Secondary.level_value(), 8);
        assert_eq!(PedagogicalLevel::University.level_value(), 15);
        assert_eq!(PedagogicalLevel::UTN(UTNProgram::AM1).level_value(), 12);
        assert_eq!(PedagogicalLevel::UTN(UTNProgram::AM2).level_value(), 14);
        assert_eq!(PedagogicalLevel::UTN(UTNProgram::Algebra).level_value(), 13);
        assert_eq!(
            PedagogicalLevel::UTN(UTNProgram::Probabilidad).level_value(),
            14
        );
    }
    #[test]
    fn udl_profile_representation() {
        assert_eq!(
            PedagogicalLevel::Primary.udl_profile().representation,
            UdlRepresentation::Concrete
        );
        assert_eq!(
            PedagogicalLevel::Secondary.udl_profile().representation,
            UdlRepresentation::Graphic
        );
        assert_eq!(
            PedagogicalLevel::University.udl_profile().representation,
            UdlRepresentation::Symbolic
        );
        assert_eq!(
            PedagogicalLevel::UTN(UTNProgram::AM1)
                .udl_profile()
                .representation,
            UdlRepresentation::Formal
        );
        assert_eq!(PedagogicalLevel::Primary.udl_profile().language_level, 1);
        assert_eq!(PedagogicalLevel::Secondary.udl_profile().language_level, 2);
    }
    #[test]
    fn from_level_value_roundtrip() {
        assert_eq!(
            PedagogicalLevel::from_level_value(2),
            PedagogicalLevel::Primary
        );
        assert_eq!(
            PedagogicalLevel::from_level_value(8),
            PedagogicalLevel::Secondary
        );
        assert_eq!(
            PedagogicalLevel::from_level_value(12),
            PedagogicalLevel::UTN(UTNProgram::AM1)
        );
        assert_eq!(
            PedagogicalLevel::from_level_value(13),
            PedagogicalLevel::UTN(UTNProgram::Algebra)
        );
        assert_eq!(
            PedagogicalLevel::from_level_value(15),
            PedagogicalLevel::University
        );
    }
}
