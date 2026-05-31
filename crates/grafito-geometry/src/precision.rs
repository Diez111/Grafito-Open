//! Grafito Precision Engine — Arbitrary precision math via MPFR (rug crate).
//! Provides HiFloat wrapper for guaranteed-precision computations.

use rug::Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    Fast,
    High(u32),
}

impl Default for Precision {
    fn default() -> Self { Precision::Fast }
}

impl Precision {
    pub fn bits(&self) -> u32 {
        match self {
            Precision::Fast => 53,
            Precision::High(b) => *b,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HiFloat {
    inner: Float,
    prec: Precision,
}

impl HiFloat {
    pub fn new(val: f64, prec: Precision) -> Self {
        let bits = prec.bits();
        Self { inner: Float::with_val(bits, val), prec }
    }

    pub fn fast(val: f64) -> Self { Self::new(val, Precision::Fast) }
    pub fn high(val: f64, bits: u32) -> Self { Self::new(val, Precision::High(bits)) }

    pub fn to_f64(&self) -> f64 { self.inner.to_f64() }

    pub fn sin(&self) -> Self {
        let bits = self.prec.bits();
        let mut result = Float::with_val(bits, &self.inner);
        result.sin_mut();
        Self { inner: result, prec: self.prec }
    }

    pub fn cos(&self) -> Self {
        let bits = self.prec.bits();
        let mut result = Float::with_val(bits, &self.inner);
        result.cos_mut();
        Self { inner: result, prec: self.prec }
    }

    pub fn exp(&self) -> Self {
        let bits = self.prec.bits();
        let mut result = Float::with_val(bits, &self.inner);
        result.exp_mut();
        Self { inner: result, prec: self.prec }
    }

    pub fn ln(&self) -> Self {
        let bits = self.prec.bits();
        let mut result = Float::with_val(bits, &self.inner);
        result.ln_mut();
        Self { inner: result, prec: self.prec }
    }

    pub fn abs(&self) -> Self {
        let bits = self.prec.bits();
        Self { inner: Float::with_val(bits, self.inner.clone().abs()), prec: self.prec }
    }

    pub fn sqrt(&self) -> Self {
        let bits = self.prec.bits();
        let mut result = Float::with_val(bits, &self.inner);
        result.sqrt_mut();
        Self { inner: result, prec: self.prec }
    }

    pub fn is_finite(&self) -> bool { self.inner.is_finite() }
    pub fn is_nan(&self) -> bool { self.inner.is_nan() }
}

impl std::ops::Add for HiFloat {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() { self.prec } else { rhs.prec };
        Self { inner: Float::with_val(prec.bits(), &self.inner + &rhs.inner), prec }
    }
}

impl std::ops::Sub for HiFloat {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() { self.prec } else { rhs.prec };
        Self { inner: Float::with_val(prec.bits(), &self.inner - &rhs.inner), prec }
    }
}

impl std::ops::Mul for HiFloat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() { self.prec } else { rhs.prec };
        Self { inner: Float::with_val(prec.bits(), &self.inner * &rhs.inner), prec }
    }
}

impl std::ops::Div for HiFloat {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() { self.prec } else { rhs.prec };
        Self { inner: Float::with_val(prec.bits(), &self.inner / &rhs.inner), prec }
    }
}

impl std::ops::Neg for HiFloat {
    type Output = Self;
    fn neg(self) -> Self {
        Self { inner: Float::with_val(self.prec.bits(), -&self.inner), prec: self.prec }
    }
}

impl std::cmp::PartialEq for HiFloat {
    fn eq(&self, other: &Self) -> bool { self.inner == other.inner }
}
