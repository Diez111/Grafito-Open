//! Restricciones declarativas para simplificación simbólica con dominio explícito.

use std::collections::BTreeSet;

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
}

/// Colección de hipótesis disponibles para una operación simbólica.
///
/// Los hechos se conservan por separado para no inventar restricciones. Las
/// consultas sí aplican las implicaciones seguras: `Positive` implica `NonZero`
/// y `Real`; `Integer` implica `Real`; y todo valor real conocido es complejo.
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

    fn insert(&mut self, fact: Assumption) -> &mut Self {
        self.facts.insert(fact);
        self
    }

    fn contains(&self, variable: &str, make_fact: impl FnOnce(String) -> Assumption) -> bool {
        self.facts.contains(&make_fact(variable.to_owned()))
    }
}
