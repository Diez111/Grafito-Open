//! Restricciones declarativas para simplificación simbólica con dominio explícito.

use std::collections::{BTreeMap, BTreeSet};

/// Hecho matemático conocido sobre una variable simbólica.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Assumption {
    /// La variable pertenece a los reales.
    Real(String),
    /// La variable pertenece a los complejos.
    Complex(String),
    /// La variable pertenece a los enteros.
    Integer(String),
    /// La variable es distinta de cero.
    NonZero(String),
    /// La variable es estrictamente positiva.
    Positive(String),
    /// La variable es mayor o igual a cero.
    NonNegative(String),
    /// La variable es menor o igual a cero.
    NonPositive(String),
}

/// Colección de hipótesis disponibles para una operación simbólica.
///
/// Los hechos se conservan por separado para no inventar restricciones. Las
/// consultas sí aplican las implicaciones seguras: `Positive` implica
/// `NonZero`, `NonNegative` y `Real`; `NonNegative`/`NonPositive` implican
/// `Real`; `Integer` implica `Real`; y todo valor real conocido es complejo.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Assumptions {
    facts: BTreeSet<Assumption>,
}

impl Assumptions {
    /// Crea un conjunto de hipótesis vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Añade que `variable` es real.
    pub fn assume_real(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::Real(variable.into()))
    }

    /// Añade que `variable` es compleja.
    pub fn assume_complex(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::Complex(variable.into()))
    }

    /// Añade que `variable` es entera.
    pub fn assume_integer(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::Integer(variable.into()))
    }

    /// Añade que `variable` no es cero.
    pub fn assume_nonzero(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::NonZero(variable.into()))
    }

    /// Añade que `variable` es estrictamente positiva.
    pub fn assume_positive(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::Positive(variable.into()))
    }

    /// Añade que `variable` es mayor o igual a cero.
    pub fn assume_nonnegative(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::NonNegative(variable.into()))
    }

    /// Añade que `variable` es menor o igual a cero.
    pub fn assume_nonpositive(&mut self, variable: impl Into<String>) -> &mut Self {
        self.insert(Assumption::NonPositive(variable.into()))
    }

    /// Construye hipótesis desde el mapa `variable → kind` del documento.
    ///
    /// Solo reconoce kinds canónicos (`positive`, `nonzero`, `real`,
    /// `integer`, `complex`, `nonnegative`, `nonpositive`); `negative`
    /// implica `Real` + `NonZero` y `zero` implica `Real`. El resto
    /// (texto libre de `Assume`) se ignora: jamás se inventa una
    /// restricción a partir de texto no interpretado.
    #[must_use]
    pub fn from_map(map: &BTreeMap<String, String>) -> Self {
        let mut out = Self::new();
        for (variable, kind) in map {
            match kind.as_str() {
                "positive" => {
                    out.assume_positive(variable);
                }
                "nonzero" => {
                    out.assume_nonzero(variable);
                }
                "real" => {
                    out.assume_real(variable);
                }
                "integer" => {
                    out.assume_integer(variable);
                }
                "complex" => {
                    out.assume_complex(variable);
                }
                "nonnegative" => {
                    out.assume_nonnegative(variable);
                }
                "nonpositive" => {
                    out.assume_nonpositive(variable);
                }
                "negative" => {
                    out.assume_real(variable);
                    out.assume_nonzero(variable);
                }
                "zero" => {
                    out.assume_real(variable);
                }
                _ => {}
            }
        }
        out
    }

    /// Devuelve los hechos declarados, en orden determinista.
    #[must_use]
    pub fn facts(&self) -> &BTreeSet<Assumption> {
        &self.facts
    }

    /// Indica si `variable` se conoce como real.
    #[must_use]
    pub fn is_real(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::Real)
            || self.is_integer(variable)
            || self.is_positive(variable)
            || self.is_nonnegative(variable)
            || self.is_nonpositive(variable)
    }

    /// Indica si `variable` se conoce como compleja.
    #[must_use]
    pub fn is_complex(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::Complex) || self.is_real(variable)
    }

    /// Indica si `variable` se conoce como entera.
    #[must_use]
    pub fn is_integer(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::Integer)
    }

    /// Indica si `variable` se conoce como distinta de cero.
    #[must_use]
    pub fn is_nonzero(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::NonZero) || self.is_positive(variable)
    }

    /// Indica si `variable` se conoce como estrictamente positiva.
    #[must_use]
    pub fn is_positive(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::Positive)
    }

    /// Indica si `variable` se conoce como mayor o igual a cero.
    #[must_use]
    pub fn is_nonnegative(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::NonNegative) || self.is_positive(variable)
    }

    /// Indica si `variable` se conoce como menor o igual a cero.
    #[must_use]
    pub fn is_nonpositive(&self, variable: &str) -> bool {
        self.contains(variable, Assumption::NonPositive)
    }

    fn insert(&mut self, fact: Assumption) -> &mut Self {
        self.facts.insert(fact);
        self
    }

    fn contains(&self, variable: &str, make_fact: impl FnOnce(String) -> Assumption) -> bool {
        self.facts.contains(&make_fact(variable.to_owned()))
    }
}

impl Assumption {
    /// Describe un hecho en microcopy español para mostrar condiciones.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Assumption::Real(v) => format!("{v} ∈ ℝ"),
            Assumption::Complex(v) => format!("{v} ∈ ℂ"),
            Assumption::Integer(v) => format!("{v} ∈ ℤ"),
            Assumption::NonZero(v) => format!("{v} ≠ 0"),
            Assumption::Positive(v) => format!("{v} > 0"),
            Assumption::NonNegative(v) => format!("{v} ≥ 0"),
            Assumption::NonPositive(v) => format!("{v} ≤ 0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_map_recognizes_canonical_kinds_only() {
        let map: BTreeMap<String, String> = [
            ("x".to_string(), "positive".to_string()),
            ("y".to_string(), "nonnegative".to_string()),
            ("z".to_string(), "nonpositive".to_string()),
            ("w".to_string(), "texto libre ignorado".to_string()),
        ]
        .into_iter()
        .collect();
        let a = Assumptions::from_map(&map);
        assert!(a.is_positive("x"));
        assert!(a.is_nonnegative("y"));
        assert!(!a.is_nonzero("y"), "≥0 incluye el cero");
        assert!(a.is_nonpositive("z"));
        assert!(a.is_real("y") && a.is_real("z"));
        assert!(!a.is_real("w"), "texto libre no inventa hechos");
    }

    #[test]
    fn negative_implies_real_and_nonzero() {
        let map: BTreeMap<String, String> = [("x".to_string(), "negative".to_string())]
            .into_iter()
            .collect();
        let a = Assumptions::from_map(&map);
        assert!(a.is_real("x") && a.is_nonzero("x"));
        assert!(!a.is_positive("x"));
    }

    #[test]
    fn describe_renders_spanish_conditions() {
        assert_eq!(Assumption::NonZero("x".into()).describe(), "x ≠ 0");
        assert_eq!(Assumption::Positive("x".into()).describe(), "x > 0");
        assert_eq!(Assumption::NonNegative("x".into()).describe(), "x ≥ 0");
        assert_eq!(Assumption::NonPositive("x".into()).describe(), "x ≤ 0");
    }
}
