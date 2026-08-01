//! Aritmética racional exacta con enteros `i128` normalizados.
//!
//! [`ExactRational`] es exacto dentro del rango finito de `i128`; no pretende
//! ser una implementación de precisión arbitraria. Las operaciones que no
//! caben en ese rango devuelven [`ExactRationalError::Overflow`] en lugar de
//! degradarse silenciosamente a `f64`.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// Errores de construcción y aritmética de [`ExactRational`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactRationalError {
    /// El denominador de una fracción no puede ser cero.
    ZeroDenominator,
    /// Dividir por el racional cero no está definido.
    DivisionByZero,
    /// El resultado exacto no cabe en los enteros `i128` acotados.
    Overflow,
    /// La entrada no tiene la forma de entero o fracción `numerador/denominador`.
    InvalidFormat,
}

impl fmt::Display for ExactRationalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroDenominator => "el denominador no puede ser cero",
            Self::DivisionByZero => "división por el racional cero",
            Self::Overflow => "el resultado no cabe en un racional i128",
            Self::InvalidFormat => "se esperaba un entero o una fracción n/d",
        };
        f.write_str(message)
    }
}

impl Error for ExactRationalError {}

/// Racional exacto reducido con denominador estrictamente positivo.
///
/// La representación usa `i128`, por lo que los resultados fuera de ese rango
/// se rechazan con [`ExactRationalError::Overflow`]. Use esta clase cuando el
/// rango acotado sea aceptable; no sustituye una biblioteca de enteros grandes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExactRational {
    numerator: i128,
    denominator: i128,
}

impl ExactRational {
    /// Construye y normaliza `numerator / denominator`.
    pub fn new(numerator: i128, denominator: i128) -> Result<Self, ExactRationalError> {
        if denominator == 0 {
            return Err(ExactRationalError::ZeroDenominator);
        }
        if numerator == 0 {
            return Ok(Self::zero());
        }

        let divisor = gcd(numerator.unsigned_abs(), denominator.unsigned_abs());
        let (mut numerator, mut denominator) = if divisor == (1_u128 << 127) {
            // The only non-representable magnitude is |i128::MIN|. Reaching
            // this branch means both values are i128::MIN, so both reduce to -1.
            debug_assert_eq!(numerator, i128::MIN);
            debug_assert_eq!(denominator, i128::MIN);
            (-1, -1)
        } else {
            let divisor = divisor as i128;
            (numerator / divisor, denominator / divisor)
        };

        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?;
            denominator = denominator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?;
        }

        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// El racional cero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    /// El racional uno.
    #[must_use]
    pub const fn one() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }

    /// Numerador reducido.
    #[must_use]
    pub const fn numerator(self) -> i128 {
        self.numerator
    }

    /// Denominador reducido y siempre positivo.
    #[must_use]
    pub const fn denominator(self) -> i128 {
        self.denominator
    }

    /// Indica si el racional es exactamente cero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    /// Suma dos racionales verificando cada operación intermedia.
    pub fn checked_add(self, other: Self) -> Result<Self, ExactRationalError> {
        self.checked_add_or_sub(other, false)
    }

    /// Resta dos racionales verificando cada operación intermedia.
    pub fn checked_sub(self, other: Self) -> Result<Self, ExactRationalError> {
        self.checked_add_or_sub(other, true)
    }

    /// Multiplica dos racionales tras cancelar factores cruzados.
    pub fn checked_mul(self, other: Self) -> Result<Self, ExactRationalError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Self::zero());
        }

        let left_factor = gcd(self.numerator.unsigned_abs(), other.denominator as u128) as i128;
        let right_factor = gcd(other.numerator.unsigned_abs(), self.denominator as u128) as i128;
        let numerator = (self.numerator / left_factor)
            .checked_mul(other.numerator / right_factor)
            .ok_or(ExactRationalError::Overflow)?;
        let denominator = (self.denominator / right_factor)
            .checked_mul(other.denominator / left_factor)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    /// Divide dos racionales tras cancelar factores cruzados.
    pub fn checked_div(self, other: Self) -> Result<Self, ExactRationalError> {
        if other.is_zero() {
            return Err(ExactRationalError::DivisionByZero);
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }

        let numerator_factor = gcd(
            self.numerator.unsigned_abs(),
            other.numerator.unsigned_abs(),
        );
        let denominator_factor = gcd(other.denominator as u128, self.denominator as u128) as i128;
        let numerator = divide_by_gcd(self.numerator, numerator_factor)?
            .checked_mul(other.denominator / denominator_factor)
            .ok_or(ExactRationalError::Overflow)?;
        let denominator = (self.denominator / denominator_factor)
            .checked_mul(divide_by_gcd(other.numerator, numerator_factor)?)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    /// Niega el racional sin convertirlo a punto flotante.
    pub fn checked_neg(self) -> Result<Self, ExactRationalError> {
        Self::new(
            self.numerator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?,
            self.denominator,
        )
    }

    fn checked_add_or_sub(self, other: Self, subtract: bool) -> Result<Self, ExactRationalError> {
        let divisor = gcd(self.denominator as u128, other.denominator as u128) as i128;
        let left_multiplier = other.denominator / divisor;
        let right_multiplier = self.denominator / divisor;
        let left = self
            .numerator
            .checked_mul(left_multiplier)
            .ok_or(ExactRationalError::Overflow)?;
        let right = other
            .numerator
            .checked_mul(right_multiplier)
            .ok_or(ExactRationalError::Overflow)?;
        let numerator = if subtract {
            left.checked_sub(right)
        } else {
            left.checked_add(right)
        }
        .ok_or(ExactRationalError::Overflow)?;
        let denominator = right_multiplier
            .checked_mul(other.denominator)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    fn cmp_positive(
        mut left_numerator: u128,
        mut left_denominator: u128,
        mut right_numerator: u128,
        mut right_denominator: u128,
    ) -> Ordering {
        // Continued fractions compare fractions exactly without multiplying two
        // potentially large i128 values.
        let mut reversed = false;
        loop {
            let left_integer = left_numerator / left_denominator;
            let right_integer = right_numerator / right_denominator;
            if left_integer != right_integer {
                let ordering = left_integer.cmp(&right_integer);
                return if reversed {
                    ordering.reverse()
                } else {
                    ordering
                };
            }

            let left_remainder = left_numerator % left_denominator;
            let right_remainder = right_numerator % right_denominator;
            if left_remainder == 0 || right_remainder == 0 {
                let ordering = match (left_remainder == 0, right_remainder == 0) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => unreachable!("covered by the condition above"),
                };
                return if reversed {
                    ordering.reverse()
                } else {
                    ordering
                };
            }

            left_numerator = left_denominator;
            left_denominator = left_remainder;
            right_numerator = right_denominator;
            right_denominator = right_remainder;
            reversed = !reversed;
        }
    }
}

impl From<i128> for ExactRational {
    fn from(value: i128) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }
}

impl FromStr for ExactRational {
    type Err = ExactRationalError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ExactRationalError::InvalidFormat);
        }

        match input.split_once('/') {
            Some((numerator, denominator)) if !denominator.contains('/') => {
                let numerator = numerator
                    .trim()
                    .parse::<i128>()
                    .map_err(|_| ExactRationalError::InvalidFormat)?;
                let denominator = denominator
                    .trim()
                    .parse::<i128>()
                    .map_err(|_| ExactRationalError::InvalidFormat)?;
                Self::new(numerator, denominator)
            }
            Some(_) => Err(ExactRationalError::InvalidFormat),
            None => input
                .parse::<i128>()
                .map(Self::from)
                .map_err(|_| ExactRationalError::InvalidFormat),
        }
    }
}

impl fmt::Display for ExactRational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(f, "{}", self.numerator)
        } else {
            write!(f, "{}/{}", self.numerator, self.denominator)
        }
    }
}

impl Ord for ExactRational {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.numerator.is_negative(), other.numerator.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Self::cmp_positive(
                self.numerator as u128,
                self.denominator as u128,
                other.numerator as u128,
                other.denominator as u128,
            ),
            (true, true) => Self::cmp_positive(
                other.numerator.unsigned_abs(),
                other.denominator as u128,
                self.numerator.unsigned_abs(),
                self.denominator as u128,
            ),
        }
    }
}

impl PartialOrd for ExactRational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn divide_by_gcd(value: i128, divisor: u128) -> Result<i128, ExactRationalError> {
    if divisor <= i128::MAX as u128 {
        Ok(value / divisor as i128)
    } else if value == i128::MIN && divisor == (1_u128 << 127) {
        Ok(-1)
    } else {
        Err(ExactRationalError::Overflow)
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_large_fractions_without_cross_multiplication() {
        let left = ExactRational::new(i128::MAX, i128::MAX - 1).unwrap();
        let right = ExactRational::one();
        assert!(left > right);
    }

    #[test]
    fn division_handles_the_minimum_i128_factor() {
        let value = ExactRational::new(i128::MIN, 1).unwrap();
        let divisor = ExactRational::new(i128::MIN, 1).unwrap();
        assert_eq!(value.checked_div(divisor), Ok(ExactRational::one()));
    }
}
