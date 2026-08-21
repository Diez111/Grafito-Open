//! Newtypes con invariantes para evitar confundir unidades.
//!
//! Siguiendo la guía rust-design: cada valor con invariante (radio >0, ángulo,
//! porcentaje 0..100) se envuelve en un struct que valida en construcción y
//! ofrece conversiones explícitas.

use serde::{Deserialize, Serialize};

/// Radio geométrico: siempre finito y > 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radius(f64);

impl Radius {
    /// Crea un radio validado. Retorna `None` si no es finito o ≤ 0.
    pub fn try_new(value: f64) -> Option<Self> {
        if value.is_finite() && value > 0.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Crea sin validar — solo para deserialización o tests internos.
    pub fn new_unchecked(value: f64) -> Self {
        Self(value)
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl From<Radius> for f64 {
    fn from(r: Radius) -> Self {
        r.0
    }
}

impl TryFrom<f64> for Radius {
    type Error = String;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::try_new(value).ok_or_else(|| format!("radio inválido: {value} debe ser finito y > 0"))
    }
}

/// Ángulo en radianes, siempre finito.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Angle(f64);

impl Angle {
    pub fn from_radians(rad: f64) -> Option<Self> {
        if rad.is_finite() {
            Some(Self(rad))
        } else {
            None
        }
    }

    pub fn from_degrees(deg: f64) -> Option<Self> {
        Self::from_radians(deg.to_radians())
    }

    pub fn radians(self) -> f64 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        self.0.to_degrees()
    }
}

impl From<Angle> for f64 {
    fn from(a: Angle) -> Self {
        a.0
    }
}

/// Porcentaje 0..=100, finito.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Percentage(f64);

impl Percentage {
    pub fn try_new(value: f64) -> Option<Self> {
        if value.is_finite() && (0.0..=100.0).contains(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Percentage {
    type Error = String;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::try_new(value)
            .ok_or_else(|| format!("porcentaje inválido: {value} debe estar en 0..=100"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_rejects_non_positive() {
        assert!(Radius::try_new(5.0).is_some());
        assert!(Radius::try_new(0.0).is_none());
        assert!(Radius::try_new(-1.0).is_none());
        assert!(Radius::try_new(f64::NAN).is_none());
        assert!(Radius::try_new(f64::INFINITY).is_none());
    }

    #[test]
    fn angle_roundtrip() {
        let a = Angle::from_degrees(90.0).unwrap();
        assert!((a.radians() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((a.degrees() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn percentage_bounds() {
        assert!(Percentage::try_new(50.0).is_some());
        assert!(Percentage::try_new(101.0).is_none());
        assert!(Percentage::try_new(-1.0).is_none());
    }
}
