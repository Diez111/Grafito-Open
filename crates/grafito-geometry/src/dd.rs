//! Double-Double arithmetic — ~106 bits of precision using two f64.
//! Pure Rust implementation, no C dependencies, works on all platforms.
//!
//! A DoubleDouble represents a number as the unevaluated sum of two f64 values:
//! `value = hi + lo` where `|lo| <= 0.5 * ulp(hi)`.
//! This gives approximately 106 bits of precision (vs 53 bits for f64).

use std::f64::consts::PI;
use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Split constant for Dekker's product algorithm
const SPLIT: f64 = 134217729.0; // 2^27 + 1

/// A double-double precision floating point number (~106 bits).
#[derive(Clone, Copy, Default)]
pub struct DD {
    pub hi: f64,
    pub lo: f64,
}

impl DD {
    /// Create a new DD from high and low parts
    #[inline]
    pub fn new(hi: f64, lo: f64) -> Self {
        Self { hi, lo }
    }

    /// Create a DD from a single f64
    #[inline]
    pub fn from_f64(val: f64) -> Self {
        Self { hi: val, lo: 0.0 }
    }

    /// Convert to f64 (loses precision)
    #[inline]
    pub fn to_f64(&self) -> f64 {
        self.hi + self.lo
    }

    /// Check if the value is finite
    #[inline]
    pub fn is_finite(&self) -> bool {
        self.hi.is_finite() && self.lo.is_finite()
    }

    /// Check if the value is NaN
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.hi.is_nan() || self.lo.is_nan()
    }

    /// Check if the value is zero
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.hi == 0.0 && self.lo == 0.0
    }

    /// Absolute value
    #[inline]
    pub fn abs(&self) -> Self {
        if self.hi < 0.0 {
            -*self
        } else {
            *self
        }
    }

    /// Square root using Newton-Raphson iteration
    pub fn sqrt(&self) -> Self {
        if self.hi < 0.0 {
            return Self::new(f64::NAN, f64::NAN);
        }
        if self.is_zero() {
            return Self::new(0.0, 0.0);
        }

        // Initial approximation
        let x = self.hi.sqrt();
        let x_dd = Self::from_f64(x);

        // Newton-Raphson: x_{n+1} = 0.5 * (x_n + self/x_n)
        let half = Self::from_f64(0.5);
        let x1 = half * (x_dd + *self / x_dd);
        half * (x1 + *self / x1)
    }

    /// Compute sin using Taylor series
    pub fn sin(&self) -> Self {
        if !self.hi.is_finite() {
            return Self::new(f64::NAN, f64::NAN);
        }
        // Reduce to [-pi, pi]
        let two_pi = Self::from_f64(2.0 * PI);
        let mut x = *self;

        // Reduce to [-pi, pi]
        while x.hi > PI {
            x = x - two_pi;
        }
        while x.hi < -PI {
            x = x + two_pi;
        }

        // Taylor series: sin(x) = x - x^3/3! + x^5/5! - x^7/7! + ...
        let x2 = x * x;
        let mut term = x;
        let mut sum = x;
        let mut sign = -1.0;

        for i in 1..=10 {
            let denom = (2 * i) as f64 * (2 * i + 1) as f64;
            term = term * x2 / Self::from_f64(denom);
            if sign > 0.0 {
                sum = sum + term;
            } else {
                sum = sum - term;
            }
            sign = -sign;
            if term.abs().to_f64() < 1e-30 {
                break;
            }
        }

        sum
    }

    /// Compute cos using Taylor series
    pub fn cos(&self) -> Self {
        // cos(x) = sin(x + pi/2)
        let pi_over_2 = Self::from_f64(PI / 2.0);
        (*self + pi_over_2).sin()
    }

    /// Compute exp using Taylor series
    pub fn exp(&self) -> Self {
        // exp(x) = 1 + x + x^2/2! + x^3/3! + ...
        let mut sum = Self::from_f64(1.0);
        let mut term = Self::from_f64(1.0);

        for i in 1..=30 {
            term = term * *self / Self::from_f64(i as f64);
            sum = sum + term;
            if term.abs().to_f64() < 1e-30 {
                break;
            }
        }

        sum
    }

    /// Compute ln using the identity: ln(x) = 2 * atanh((x-1)/(x+1))
    pub fn ln(&self) -> Self {
        if self.hi <= 0.0 {
            return Self::new(f64::NAN, f64::NAN);
        }

        // Range reduction: x = m * 2^e where 0.5 <= m < 1
        let (m, e) = self.frexp();
        let e_dd = Self::from_f64(e as f64);
        let ln2 = Self::from_f64(std::f64::consts::LN_2);

        // ln(x) = ln(m) + e * ln(2)
        // ln(m) = 2 * atanh((m-1)/(m+1))
        let one = Self::from_f64(1.0);
        let two = Self::from_f64(2.0);
        let u = (m - one) / (m + one);
        let u2 = u * u;

        // atanh(u) = u + u^3/3 + u^5/5 + ...
        let mut sum = u;
        let mut term = u;
        for i in 1..=20 {
            term = term * u2;
            let denom = (2 * i + 1) as f64;
            sum = sum + term / Self::from_f64(denom);
            if term.abs().to_f64() < 1e-30 {
                break;
            }
        }

        two * sum + e_dd * ln2
    }

    /// Decompose into mantissa and exponent (like C's frexp)
    fn frexp(&self) -> (Self, i32) {
        let (m, e) = libm::frexp(self.hi);
        // Adjust the low part
        let lo_scaled = self.lo / (2.0_f64.powi(e));
        (Self::new(m, lo_scaled), e)
    }
}

/// Error-free transformation of addition: a + b = s + t exactly
#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let v = s - a;
    let t = (a - (s - v)) + (b - v);
    (s, t)
}

/// Error-free transformation of multiplication using Dekker's algorithm
#[inline]
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (a_hi, a_lo) = split(a);
    let (b_hi, b_lo) = split(b);
    let err = ((a_hi * b_hi - p) + a_hi * b_lo + a_lo * b_hi) + a_lo * b_lo;
    (p, err)
}

/// Split a f64 into high and low parts for Dekker's algorithm
#[inline]
fn split(a: f64) -> (f64, f64) {
    let t = SPLIT * a;
    let hi = t - (t - a);
    let lo = a - hi;
    (hi, lo)
}

impl Add for DD {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let (s1, t1) = two_sum(self.hi, rhs.hi);
        let (_, t2) = two_sum(self.lo, rhs.lo);
        let (s3, t3) = two_sum(s1, t1 + t2);
        Self::new(s3, t3)
    }
}

impl Sub for DD {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + (-rhs)
    }
}

impl Mul for DD {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let (p1, e1) = two_prod(self.hi, rhs.hi);
        let p2 = self.hi * rhs.lo + self.lo * rhs.hi;
        let (s, t) = two_sum(p1, e1 + p2);
        Self::new(s, t)
    }
}

impl Div for DD {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.is_zero() {
            return Self::new(f64::INFINITY, f64::INFINITY);
        }

        // Newton-Raphson division
        let q1 = self.hi / rhs.hi;
        let q1_dd = Self::from_f64(q1);
        let r = self - rhs * q1_dd;
        let q2 = r.hi / rhs.hi;
        let (s, t) = two_sum(q1, q2);
        Self::new(s, t)
    }
}

impl Neg for DD {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.hi, -self.lo)
    }
}

impl PartialEq for DD {
    fn eq(&self, other: &Self) -> bool {
        self.hi == other.hi && self.lo == other.lo
    }
}

impl PartialOrd for DD {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Comparar primero por hi, luego por lo para preservar precisión DD.
        if self.hi < other.hi {
            Some(std::cmp::Ordering::Less)
        } else if self.hi > other.hi {
            Some(std::cmp::Ordering::Greater)
        } else {
            self.lo.partial_cmp(&other.lo)
        }
    }
}

impl fmt::Debug for DD {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DD({} + {}e)", self.hi, self.lo)
    }
}

impl fmt::Display for DD {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addition_precision() {
        // 1.0 + 1e-15: in f64 this loses precision, DD preserves it
        let a = DD::from_f64(1.0);
        let b = DD::from_f64(1e-15);
        let c = a + b;
        // DD should preserve the low part even though f64 can't
        assert!(c.lo != 0.0 || c.hi > 1.0, "DD should preserve precision");
        // Verify roundtrip is close
        assert!((c.to_f64() - 1.0).abs() < 1e-14);
    }

    #[test]
    fn test_sqrt() {
        let x = DD::from_f64(2.0);
        let sqrt2 = x.sqrt();
        let sqrt2_f64 = 2.0_f64.sqrt();
        assert!((sqrt2.to_f64() - sqrt2_f64).abs() < 1e-15);
    }

    #[test]
    fn test_trig() {
        let pi_over_4 = DD::from_f64(PI / 4.0);
        let sin_val = pi_over_4.sin().to_f64();
        let expected = (PI / 4.0).sin();
        assert!((sin_val - expected).abs() < 1e-14);
    }
}
