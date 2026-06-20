//! Special mathematical functions.
//!
//! This module provides implementations of special functions commonly used
//! in mathematics, physics, and engineering.

/// Compute the Gamma function Γ(x) using Lanczos approximation.
///
/// The Gamma function is a generalization of the factorial function:
/// Γ(n) = (n-1)! for positive integers
/// Γ(x) = ∫₀^∞ t^(x-1) e^(-t) dt for complex numbers
///
/// # Arguments
/// * `x` - Input value
///
/// # Returns
/// Γ(x)
pub fn gamma(x: f64) -> f64 {
    if x <= 0.0 && x.fract() == 0.0 {
        return f64::INFINITY; // Poles at non-positive integers
    }

    // Use reflection formula for negative values
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x));
    }

    // Lanczos approximation
    let g = 7.0;
    #[allow(clippy::inconsistent_digit_grouping)]
    let c = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    let x = x - 1.0;
    let mut sum = c[0];
    for (i, &ci) in c.iter().enumerate().skip(1) {
        sum += ci / (x + i as f64);
    }

    let t = x + g + 0.5;
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * sum
}

/// Compute the natural logarithm of the Gamma function.
///
/// This is more numerically stable than computing ln(Γ(x)) directly.
///
/// # Arguments
/// * `x` - Input value
///
/// # Returns
/// ln(Γ(x))
pub fn ln_gamma(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    // Use the Lanczos approximation directly on ln(Γ(x)).
    if x < 0.5 {
        // Reflection formula.
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }

    let g = 7.0;
    #[allow(clippy::inconsistent_digit_grouping)]
    let c = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    let x = x - 1.0;
    let mut sum = c[0];
    for (i, &ci) in c.iter().enumerate().skip(1) {
        sum += ci / (x + i as f64);
    }

    let t = x + g + 0.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

/// Compute the Beta function B(a, b).
///
/// The Beta function is related to the Gamma function:
/// B(a, b) = Γ(a)Γ(b) / Γ(a+b)
///
/// # Arguments
/// * `a` - First parameter
/// * `b` - Second parameter
///
/// # Returns
/// B(a, b)
pub fn beta(a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 {
        return f64::NAN;
    }
    (ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)).exp()
}

/// Compute the Bessel function of the first kind J_n(x) using series expansion.
///
/// J_n(x) = Σ_{m=0}^∞ (-1)^m / (m! Γ(m+n+1)) * (x/2)^(2m+n)
///
/// **Warning**: This series expansion is only accurate for small values of x (x < ~15.0).
/// For larger x, it will suffer from catastrophic cancellation and return incorrect results or NaN.
///
/// # Arguments
/// * `n` - Order (integer)
/// * `x` - Input value
///
/// # Returns
/// J_n(x)
pub fn bessel_j(n: i32, x: f64) -> f64 {
    if n < 0 {
        // For integer orders: J_{-n}(x) = (-1)^n J_n(x)
        let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
        return sign * bessel_j(-n, x);
    }
    let n = n as f64;
    let mut sum = 0.0;
    let mut term = (x / 2.0).powf(n) / gamma(n + 1.0);

    for m in 0..100 {
        sum += term;
        term *= -x * x / (4.0 * (m as f64 + 1.0) * (m as f64 + n + 1.0));

        if term.abs() < 1e-15 {
            break;
        }
    }

    sum
}

/// Compute the Bessel function of the second kind Y_n(x) using the relation:
/// Y_n(x) = (J_n(x) cos(nπ) - J_{-n}(x)) / sin(nπ)
///
/// For integer n, use the limit form.
///
/// **Warning**: This implementation relies on the series expansion of J_n(x) and Y_0(x),
/// which is only accurate for small x (x < ~10.0). For larger x, it diverges.
///
/// # Arguments
/// * `n` - Order (integer)
/// * `x` - Input value (must be positive)
///
/// # Returns
/// Y_n(x)
fn bessel_y0(x: f64) -> f64 {
    // Series expansion for Y_0(x) for x > 0.
    let j0 = bessel_j(0, x);
    let gamma_euler = 0.5772156649015329;
    let z = x * x / 4.0;
    let mut sum = 0.0;
    let mut harmonic = 0.0;
    let mut fact2 = 1.0; // (k!)^2
    let mut z_pow = z;
    let mut sign = 1.0; // (-1)^(k-1)
    for k in 1..100 {
        harmonic += 1.0 / k as f64;
        fact2 *= (k * k) as f64;
        let term = sign * harmonic * z_pow / fact2;
        sum += term;
        if term.abs() < 1e-15 {
            break;
        }
        sign = -sign;
        z_pow *= z;
    }
    (2.0 / std::f64::consts::PI) * (j0 * ((x / 2.0).ln() + gamma_euler) + sum)
}

fn bessel_y1_numerical(x: f64) -> f64 {
    // Y_1(x) = -d/dx Y_0(x) computed via central difference.
    let h = 1e-7 * x.max(1e-6);
    let y0_plus = bessel_y0(x + h);
    let y0_minus = bessel_y0(x - h);
    -(y0_plus - y0_minus) / (2.0 * h)
}

pub fn bessel_y(n: i32, x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }

    // Use identities for negative integer orders.
    if n < 0 {
        let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
        return sign * bessel_y(-n, x);
    }

    let y0 = bessel_y0(x);
    if n == 0 {
        return y0;
    }

    let y1 = bessel_y1_numerical(x);
    if n == 1 {
        return y1;
    }

    // Forward recurrence: Y_{m+1}(x) = (2m/x) Y_m(x) - Y_{m-1}(x).
    let mut y_m1 = y0;
    let mut y_0 = y1;
    for m in 1..n {
        let y_p1 = (2.0 * m as f64 / x) * y_0 - y_m1;
        y_m1 = y_0;
        y_0 = y_p1;
    }
    y_0
}

/// Compute the modified Bessel function of the first kind I_n(x).
///
/// I_n(x) = i^(-n) J_n(ix)
///
/// # Arguments
/// * `n` - Order (integer)
/// * `x` - Input value
///
/// # Returns
/// I_n(x)
pub fn bessel_i(n: i32, x: f64) -> f64 {
    if n < 0 {
        // For integer orders: I_{-n}(x) = I_n(x)
        return bessel_i(-n, x);
    }
    let n = n as f64;
    let mut sum = 0.0;
    let mut term = (x / 2.0).powf(n) / gamma(n + 1.0);

    for m in 0..100 {
        sum += term;
        term *= x * x / (4.0 * (m as f64 + 1.0) * (m as f64 + n + 1.0));

        if term.abs() < 1e-15 {
            break;
        }
    }

    sum
}

/// Compute the error function erf(x).
///
/// erf(x) = (2/√π) ∫₀^x e^(-t²) dt
///
/// # Arguments
/// * `x` - Input value
///
/// # Returns
/// erf(x)
pub fn erf(x: f64) -> f64 {
    // Approximation using Horner's method
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

    sign * y
}

/// Compute the complementary error function erfc(x) = 1 - erf(x).
///
/// # Arguments
/// * `x` - Input value
///
/// # Returns
/// erfc(x)
pub fn erfc(x: f64) -> f64 {
    1.0 - erf(x)
}

/// Compute the digamma function ψ(x) = d/dx ln(Γ(x)).
///
/// # Arguments
/// * `x` - Input value
///
/// # Returns
/// ψ(x)
pub fn digamma(x: f64) -> f64 {
    if x <= 0.0 && x.fract() == 0.0 {
        return f64::NAN;
    }

    // Use recurrence relation to shift x to large values
    let mut result = 0.0;
    let mut x = x;

    while x < 6.0 {
        result -= 1.0 / x;
        x += 1.0;
    }

    // Asymptotic expansion for large x
    let x2 = 1.0 / (x * x);
    result += x.ln() - 0.5 / x - x2 * (1.0 / 12.0 - x2 * (1.0 / 120.0 - x2 * (1.0 / 252.0)));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gamma_integers() {
        // Γ(1) = 0! = 1
        assert!((gamma(1.0) - 1.0).abs() < 0.001);

        // Γ(2) = 1! = 1
        assert!((gamma(2.0) - 1.0).abs() < 0.001);

        // Γ(3) = 2! = 2
        assert!((gamma(3.0) - 2.0).abs() < 0.001);

        // Γ(4) = 3! = 6
        assert!((gamma(4.0) - 6.0).abs() < 0.001);

        // Γ(5) = 4! = 24
        assert!((gamma(5.0) - 24.0).abs() < 0.01);
    }

    #[test]
    fn test_gamma_half() {
        // Γ(1/2) = √π
        let expected = std::f64::consts::PI.sqrt();
        assert!((gamma(0.5) - expected).abs() < 0.001);
    }

    #[test]
    fn test_beta() {
        // B(1, 1) = 1
        assert!((beta(1.0, 1.0) - 1.0).abs() < 0.001);

        // B(2, 2) = 1/6
        assert!((beta(2.0, 2.0) - 1.0 / 6.0).abs() < 0.001);
    }

    #[test]
    fn test_bessel_j0() {
        // J_0(0) = 1
        assert!((bessel_j(0, 0.0) - 1.0).abs() < 0.001);

        // J_0(2.4048) ≈ 0 (first zero)
        assert!(bessel_j(0, 2.4048).abs() < 0.01);
    }

    #[test]
    fn test_bessel_j1() {
        // J_1(0) = 0
        assert!(bessel_j(1, 0.0).abs() < 0.001);

        // J_1(3.8317) ≈ 0 (first zero)
        assert!(bessel_j(1, 3.8317).abs() < 0.01);
    }

    #[test]
    fn test_erf() {
        // erf(0) = 0
        assert!(erf(0.0).abs() < 0.001);

        // erf(∞) = 1
        assert!((erf(10.0) - 1.0).abs() < 0.001);

        // erf(-∞) = -1
        assert!((erf(-10.0) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_erfc() {
        // erfc(0) = 1
        assert!((erfc(0.0) - 1.0).abs() < 0.001);

        // erfc(∞) = 0
        assert!(erfc(10.0).abs() < 0.001);
    }

    #[test]
    fn test_digamma() {
        // ψ(1) = -γ (Euler-Mascheroni constant)
        let gamma_euler = 0.5772156649015329;
        assert!((digamma(1.0) - (-gamma_euler)).abs() < 0.001);
    }
}
