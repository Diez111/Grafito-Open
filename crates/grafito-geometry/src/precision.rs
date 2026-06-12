//! Grafito Precision Engine — Extended precision math using DoubleDouble arithmetic.
//! Provides HiFloat wrapper for guaranteed-precision computations (~106 bits).

use crate::dd::DD;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    #[default]
    Fast,
    High(u32),
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
    inner: DD,
    prec: Precision,
}

impl HiFloat {
    pub fn new(val: f64, prec: Precision) -> Self {
        Self {
            inner: DD::from_f64(val),
            prec,
        }
    }

    pub fn fast(val: f64) -> Self {
        Self::new(val, Precision::Fast)
    }

    pub fn high(val: f64, _bits: u32) -> Self {
        Self::new(val, Precision::High(106))
    }

    pub fn to_f64(&self) -> f64 {
        self.inner.to_f64()
    }

    pub fn sin(&self) -> Self {
        Self {
            inner: self.inner.sin(),
            prec: self.prec,
        }
    }

    pub fn cos(&self) -> Self {
        Self {
            inner: self.inner.cos(),
            prec: self.prec,
        }
    }

    pub fn exp(&self) -> Self {
        Self {
            inner: self.inner.exp(),
            prec: self.prec,
        }
    }

    pub fn ln(&self) -> Self {
        Self {
            inner: self.inner.ln(),
            prec: self.prec,
        }
    }

    pub fn abs(&self) -> Self {
        Self {
            inner: self.inner.abs(),
            prec: self.prec,
        }
    }

    pub fn sqrt(&self) -> Self {
        Self {
            inner: self.inner.sqrt(),
            prec: self.prec,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.inner.is_finite()
    }

    pub fn is_nan(&self) -> bool {
        self.inner.is_nan()
    }
}

impl std::ops::Add for HiFloat {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() {
            self.prec
        } else {
            rhs.prec
        };
        Self {
            inner: self.inner + rhs.inner,
            prec,
        }
    }
}

impl std::ops::Sub for HiFloat {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() {
            self.prec
        } else {
            rhs.prec
        };
        Self {
            inner: self.inner - rhs.inner,
            prec,
        }
    }
}

impl std::ops::Mul for HiFloat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() {
            self.prec
        } else {
            rhs.prec
        };
        Self {
            inner: self.inner * rhs.inner,
            prec,
        }
    }
}

impl std::ops::Div for HiFloat {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let prec = if self.prec.bits() >= rhs.prec.bits() {
            self.prec
        } else {
            rhs.prec
        };
        Self {
            inner: self.inner / rhs.inner,
            prec,
        }
    }
}

impl std::ops::Neg for HiFloat {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            inner: -self.inner,
            prec: self.prec,
        }
    }
}

impl PartialEq for HiFloat {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
