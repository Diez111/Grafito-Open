//! Wrapper polimórfico para evaluación simbólica con promoción Real ↔ Complex.
//!
//! `Value` permite que un mismo `Expr` se evalúe en aritmética real o compleja
//! sin duplicar el AST. Las operaciones aritméticas implementadas en este
//! módulo siguen la convención de "promoción agresiva":
//!
//! | operación | resultado |
//! |-----------|-----------|
//! | Real  + Real   | Real  |
//! | Real  + Complex| Complex (parte imaginaria del Real es 0) |
//! | Complex + Real | Complex |
//! | Complex + Complex| Complex |
//!
//! Esto permite mezclar variables reales (`x`, `a`, `b`) y complejas (`i`)
//! en la misma expresión, lo cual es esencial para mapeos conformes
//! declarados con sintaxis matemática natural.

use num_complex::Complex64;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Valor numérico que puede ser real o complejo con promoción automática.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Número real (f64). Es la representación canónica para `f(x)` y similares.
    Real(f64),
    /// Número complejo. Para datos nuevos, esta es la representación cuando
    /// interviene `i`, `z`, o cualquier valor explícitamente complejo.
    Complex(Complex64),
}

impl Value {
    /// Construye un `Value::Real` si `re.is_finite()`, si no `Complex(NaN + i*NaN)`.
    #[inline]
    pub fn real(re: f64) -> Self {
        Value::Real(re)
    }

    /// Construye un `Value::Complex` con parte imaginaria 0 si `im == 0.0`,
    /// si no `Complex(re, im)`.
    #[inline]
    pub fn complex(re: f64, im: f64) -> Self {
        if im == 0.0 {
            Value::Real(re)
        } else {
            Value::Complex(Complex64::new(re, im))
        }
    }

    /// Devuelve la parte real como `f64` si es Real, o la parte real del
    /// complejo si es Complex. `NaN` si ninguno es finito.
    #[inline]
    pub fn re(self) -> f64 {
        match self {
            Value::Real(r) => r,
            Value::Complex(c) => c.re,
        }
    }

    /// Devuelve la parte imaginaria como `f64` (0.0 si es Real).
    #[inline]
    pub fn im(self) -> f64 {
        match self {
            Value::Real(_) => 0.0,
            Value::Complex(c) => c.im,
        }
    }

    /// Devuelve el valor como `f64` si es Real, o `f64::NAN` si es Complex.
    /// Útil para preservar la API legacy `Expr::eval -> f64`.
    #[inline]
    pub fn as_real(self) -> Option<f64> {
        match self {
            Value::Real(r) => Some(r),
            Value::Complex(c) if c.im == 0.0 => Some(c.re),
            Value::Complex(_) => None,
        }
    }

    /// Devuelve el valor como `Complex64`. Si es Real, lo promueve a
    /// `Complex(re, 0)`.
    #[inline]
    pub fn as_complex(self) -> Complex64 {
        match self {
            Value::Real(r) => Complex64::new(r, 0.0),
            Value::Complex(c) => c,
        }
    }

    /// ¿Es un valor complejo con parte imaginaria distinta de cero?
    #[inline]
    pub fn is_complex(self) -> bool {
        match self {
            Value::Real(_) => false,
            Value::Complex(c) => c.im != 0.0,
        }
    }

    /// ¿Es finito (no NaN ni Inf)?
    #[inline]
    pub fn is_finite(self) -> bool {
        match self {
            Value::Real(r) => r.is_finite(),
            Value::Complex(c) => c.re.is_finite() && c.im.is_finite(),
        }
    }

    /// Convierte un `f64` real en `Value::Real` (preservando el contrato).
    #[inline]
    pub fn from_real(r: f64) -> Self {
        Value::Real(r)
    }

    /// Construye `Value::Complex(re, im)` con la convención de `complex()`.
    #[inline]
    pub fn from_complex(re: f64, im: f64) -> Self {
        Value::complex(re, im)
    }
}

// --- Promociones aritméticas ---
//
// Reglas:
//  - Real op Real        = Real
//  - Real op Complex     = Complex
//  - Complex op Real     = Complex
//  - Complex op Complex  = Complex
//
// "op" se aplica sobre la versión real cuando ambos operandos son reales,
// y sobre la versión compleja en cualquier otro caso.

impl Add for Value {
    type Output = Value;
    fn add(self, rhs: Value) -> Value {
        match (self, rhs) {
            (Value::Real(a), Value::Real(b)) => Value::Real(a + b),
            (a, b) => Value::Complex(a.as_complex() + b.as_complex()),
        }
    }
}

impl Sub for Value {
    type Output = Value;
    fn sub(self, rhs: Value) -> Value {
        match (self, rhs) {
            (Value::Real(a), Value::Real(b)) => Value::Real(a - b),
            (a, b) => Value::Complex(a.as_complex() - b.as_complex()),
        }
    }
}

impl Mul for Value {
    type Output = Value;
    fn mul(self, rhs: Value) -> Value {
        match (self, rhs) {
            (Value::Real(a), Value::Real(b)) => Value::Real(a * b),
            (a, b) => Value::Complex(a.as_complex() * b.as_complex()),
        }
    }
}

impl Div for Value {
    type Output = Value;
    fn div(self, rhs: Value) -> Value {
        match (self, rhs) {
            (Value::Real(a), Value::Real(b)) => {
                if b.abs() < 1e-300 {
                    Value::Real(f64::NAN)
                } else {
                    Value::Real(a / b)
                }
            }
            (a, b) => {
                let denom = b.as_complex();
                if denom.norm() < 1e-300 {
                    Value::Complex(Complex64::new(f64::NAN, f64::NAN))
                } else {
                    Value::Complex(a.as_complex() / denom)
                }
            }
        }
    }
}

impl Neg for Value {
    type Output = Value;
    fn neg(self) -> Value {
        match self {
            Value::Real(r) => Value::Real(-r),
            Value::Complex(c) => Value::Complex(-c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_plus_real_is_real() {
        let r = Value::Real(2.0) + Value::Real(3.0);
        assert_eq!(r, Value::Real(5.0));
    }

    #[test]
    fn real_plus_complex_promotes_to_complex() {
        let r = Value::Real(2.0) + Value::complex(3.0, 4.0);
        assert_eq!(r, Value::Complex(Complex64::new(5.0, 4.0)));
    }

    #[test]
    fn complex_plus_real_promotes_to_complex() {
        let r = Value::complex(2.0, 3.0) + Value::Real(4.0);
        assert_eq!(r, Value::Complex(Complex64::new(6.0, 3.0)));
    }

    #[test]
    fn complex_plus_complex_is_complex() {
        let r = Value::complex(1.0, 2.0) + Value::complex(3.0, 4.0);
        assert_eq!(r, Value::Complex(Complex64::new(4.0, 6.0)));
    }

    #[test]
    fn division_by_near_zero_real_is_nan() {
        let r = Value::Real(1.0) / Value::Real(0.0);
        assert!(matches!(r, Value::Real(v) if v.is_nan()));
    }

    #[test]
    fn division_by_near_zero_complex_is_nan_complex() {
        let r = Value::complex(1.0, 1.0) / Value::complex(0.0, 0.0);
        assert!(matches!(r, Value::Complex(c) if c.re.is_nan() && c.im.is_nan()));
    }

    #[test]
    fn complex_with_zero_imaginary_is_real() {
        let v = Value::complex(3.0, 0.0);
        assert_eq!(v, Value::Real(3.0));
        assert_eq!(v.as_real(), Some(3.0));
    }

    #[test]
    fn as_real_returns_none_for_pure_imaginary() {
        let v = Value::complex(0.0, 5.0);
        assert_eq!(v.as_real(), None);
    }

    #[test]
    fn neg_real() {
        assert_eq!(-Value::Real(3.5), Value::Real(-3.5));
    }

    #[test]
    fn neg_complex() {
        assert_eq!(-Value::complex(1.0, 2.0), Value::complex(-1.0, -2.0));
    }
}
