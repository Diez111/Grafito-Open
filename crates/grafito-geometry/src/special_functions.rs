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

fn bessel_asymptotic_y(n: f64, x: f64) -> f64 {
    let chi = x - n * std::f64::consts::PI / 2.0 - std::f64::consts::PI / 4.0;
    let mu = 4.0 * n * n;
    let p = 1.0 - (mu - 1.0) * (mu - 9.0) / (2.0 * (8.0 * x).powi(2))
        + (mu - 1.0) * (mu - 9.0) * (mu - 25.0) * (mu - 49.0) / (24.0 * (8.0 * x).powi(4));
    let q =
        (mu - 1.0) / (8.0 * x) - (mu - 1.0) * (mu - 9.0) * (mu - 25.0) / (6.0 * (8.0 * x).powi(3));
    let amp = (2.0 / (std::f64::consts::PI * x)).sqrt();
    amp * (p * chi.sin() + q * chi.cos())
}

fn nonnegative_bessel_order(n: i32) -> Option<(i32, f64)> {
    if n >= 0 {
        return Some((n, 1.0));
    }

    let order = i32::try_from(n.unsigned_abs()).ok()?;
    let sign = if order % 2 == 0 { 1.0 } else { -1.0 };
    Some((order, sign))
}

/// Maximum absolute integer Bessel order accepted by all evaluators.
pub const MAX_BESSEL_ORDER: i32 = 1_000;

/// Convierte un orden de Bessel expresado como `f64` en un entero evaluable.
///
/// Los órdenes no finitos, no enteros o fuera del presupuesto son errores de
/// dominio. Los evaluadores de expresiones los representan con `NaN` en vez de
/// degradarlos silenciosamente a orden cero.
pub fn parse_bessel_order(order: f64) -> Option<i32> {
    if order.is_finite()
        && order.fract() == 0.0
        && (-(MAX_BESSEL_ORDER as f64)..=MAX_BESSEL_ORDER as f64).contains(&order)
    {
        Some(order as i32)
    } else {
        None
    }
}

/// Maximum absolute Bessel Y order evaluated by the forward recurrence.
pub const MAX_BESSEL_Y_ORDER: i32 = MAX_BESSEL_ORDER;

/// Maximum absolute Bessel J order evaluated by the bounded recurrence.
pub const MAX_BESSEL_J_ORDER: i32 = MAX_BESSEL_Y_ORDER;

/// Maximum absolute modified Bessel I order evaluated by the bounded series.
pub const MAX_BESSEL_I_ORDER: i32 = MAX_BESSEL_Y_ORDER;

/// Whether an integer order can be evaluated by [`bessel_j`].
pub fn bessel_j_order_is_supported(n: i32) -> bool {
    nonnegative_bessel_order(n).is_some_and(|(order, _)| order <= MAX_BESSEL_J_ORDER)
}

/// Whether an integer order can be evaluated by [`bessel_y`].
pub fn bessel_y_order_is_supported(n: i32) -> bool {
    nonnegative_bessel_order(n).is_some_and(|(order, _)| order <= MAX_BESSEL_Y_ORDER)
}

/// Whether an integer order can be evaluated by [`bessel_i`].
pub fn bessel_i_order_is_supported(n: i32) -> bool {
    nonnegative_bessel_order(n).is_some_and(|(order, _)| order <= MAX_BESSEL_I_ORDER)
}

/// Compute the Bessel function of the first kind J_n(x).
///
/// J_n(x) = Σ_{m=0}^∞ (-1)^m / (m! Γ(m+n+1)) * (x/2)^(2m+n)
///
/// Uses forward recurrence when the argument dominates the order, and a
/// continued fraction followed by normalized backward recurrence otherwise.
///
/// # Arguments
/// * `n` - Order (integer)
/// * `x` - Input value
///
/// # Returns
/// J_n(x)
pub fn bessel_j(n: i32, x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }

    let Some((n, order_sign)) = nonnegative_bessel_order(n) else {
        return f64::NAN;
    };
    if n > MAX_BESSEL_J_ORDER {
        return f64::NAN;
    }

    let (x, argument_sign) = if x < 0.0 {
        // J_n(-x) = (-1)^n J_n(x)
        let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
        (-x, sign)
    } else {
        (x, 1.0)
    };

    order_sign * argument_sign * libm::jn(n, x)
}

/// Compute the Bessel function of the second kind Y_n(x) using the relation:
/// Y_n(x) = (J_n(x) cos(nπ) - J_{-n}(x)) / sin(nπ)
///
/// For integer n, use the limit form.
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

    let Some((n, order_sign)) = nonnegative_bessel_order(n) else {
        return f64::NAN;
    };
    if n > MAX_BESSEL_Y_ORDER {
        return f64::NAN;
    }

    if x > 15.0 {
        return order_sign * bessel_asymptotic_y(n as f64, x);
    }

    let y0 = bessel_y0(x);
    if n == 0 {
        return order_sign * y0;
    }

    let y1 = bessel_y1_numerical(x);
    if n == 1 {
        return order_sign * y1;
    }

    // Forward recurrence: Y_{m+1}(x) = (2m/x) Y_m(x) - Y_{m-1}(x).
    let mut y_m1 = y0;
    let mut y_0 = y1;
    for m in 1..n {
        let y_p1 = (2.0 * m as f64 / x) * y_0 - y_m1;
        y_m1 = y_0;
        y_0 = y_p1;
    }
    order_sign * y_0
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
    if !x.is_finite() {
        return f64::NAN;
    }

    let Some((n, _)) = nonnegative_bessel_order(n) else {
        return f64::NAN;
    };
    if n > MAX_BESSEL_I_ORDER {
        return f64::NAN;
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
    if x.is_nan() || x == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if x == f64::INFINITY {
        return f64::INFINITY;
    }
    if x <= 0.0 && x.fract() == 0.0 {
        return f64::NAN;
    }

    if x < 0.0 {
        let sin_pi_x = (std::f64::consts::PI * x).sin();
        return digamma(1.0 - x) - std::f64::consts::PI / sin_pi_x.tan();
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

/// Compute the trigamma function ψ₁(x) = d/dx ψ(x).
///
/// The recurrence shifts finite inputs to a range where the asymptotic series
/// converges rapidly. Non-positive integers are poles and return `NaN`.
pub fn trigamma(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY || (x <= 0.0 && x.fract() == 0.0) {
        return f64::NAN;
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    if x < 0.0 {
        let sin_pi_x = (std::f64::consts::PI * x).sin();
        return std::f64::consts::PI.powi(2) / (sin_pi_x * sin_pi_x) - trigamma(1.0 - x);
    }

    let mut result = 0.0;
    let mut x = x;
    while x < 10.0 {
        result += 1.0 / (x * x);
        x += 1.0;
    }

    let inv = 1.0 / x;
    let inv2 = inv * inv;
    result
        + inv
        + inv2
            * (0.5
                + inv
                    * (1.0 / 6.0
                        + inv2
                            * (-1.0 / 30.0
                                + inv2
                                    * (1.0 / 42.0 + inv2 * (-1.0 / 30.0 + inv2 * (5.0 / 66.0))))))
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

        // Large argument test (asymptotic)
        // J_0(15.0) ≈ -0.01422447
        assert!((bessel_j(0, 15.0) - (-0.01422447)).abs() < 1e-5);
        // J_0(20.0) ≈ 0.16702466
        assert!((bessel_j(0, 20.0) - 0.16702466).abs() < 1e-5);
    }

    #[test]
    fn test_bessel_j1() {
        // J_1(0) = 0
        assert!(bessel_j(1, 0.0).abs() < 0.001);

        // J_1(3.8317) ≈ 0 (first zero)
        assert!(bessel_j(1, 3.8317).abs() < 0.01);

        // Large argument test (asymptotic)
        // J_1(15.0) ≈ 0.205103
        assert!((bessel_j(1, 15.0) - 0.205103).abs() < 1e-5);
    }

    #[test]
    fn bessel_j_high_order_moderate_argument_matches_reference() {
        let expected = 3.961_755_094_336_252e-59;
        let actual = bessel_j(100, 20.0);

        assert!(actual.is_finite());
        assert!(
            ((actual - expected) / expected).abs() < 1e-12,
            "J_100(20) = {actual:e}, expected {expected:e}"
        );
    }

    #[test]
    fn bessel_j_matches_low_order_and_transition_references() {
        let cases = [
            (0, 1.0, 0.765_197_686_557_966_6),
            (1, 1.0, 0.440_050_585_744_933_5),
            (2, 5.0, 0.046_565_116_277_752_216),
            (10, 20.0, 0.186_482_558_023_945_1),
            (100, 100.0, 0.096_366_673_295_861_56),
        ];

        for (order, argument, expected) in cases {
            let actual = bessel_j(order, argument);
            assert!(
                (actual - expected).abs() <= 1e-13 * expected.abs().max(1.0),
                "J_{order}({argument}) = {actual:e}, expected {expected:e}"
            );
        }
    }

    #[test]
    fn bessel_j_preserves_integer_order_and_argument_parity() {
        for order in [3, 4, 100] {
            let positive = bessel_j(order, 7.0);
            let parity = if order % 2 == 0 { 1.0 } else { -1.0 };

            assert_eq!(bessel_j(-order, 7.0), parity * positive);
            assert_eq!(bessel_j(order, -7.0), parity * positive);
            assert_eq!(bessel_j(-order, -7.0), positive);
        }
    }

    #[test]
    fn bessel_j_handles_zero_and_rejects_nonfinite_arguments() {
        assert_eq!(bessel_j(0, 0.0), 1.0);
        assert_eq!(bessel_j(100, 0.0), 0.0);
        assert!(bessel_j(0, f64::NAN).is_nan());
        assert!(bessel_j(0, f64::INFINITY).is_nan());
        assert!(bessel_j(0, f64::NEG_INFINITY).is_nan());
    }

    #[test]
    fn test_bessel_y() {
        // Y_0(15.0) ≈ 0.205464
        assert!((bessel_y(0, 15.0) - 0.205464).abs() < 1e-5);
        // Y_0(20.0) ≈ 0.0626406
        assert!((bessel_y(0, 20.0) - 0.0626406).abs() < 1e-5);
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

    #[test]
    fn special_functions_reject_non_terminating_or_unrepresentable_orders() {
        assert!(digamma(f64::NEG_INFINITY).is_nan());
        assert!(bessel_j(i32::MIN, 1.0).is_nan());
        assert!(bessel_y(i32::MIN, 1.0).is_nan());
        assert!(bessel_i(i32::MIN, 1.0).is_nan());
    }

    #[test]
    fn bessel_y_rejects_orders_beyond_the_recurrence_budget() {
        assert!(bessel_y(MAX_BESSEL_Y_ORDER + 1, 1.0).is_nan());
        assert!(!bessel_y_order_is_supported(MAX_BESSEL_Y_ORDER + 1));
    }

    #[test]
    fn bessel_j_rejects_orders_beyond_the_recurrence_budget() {
        assert_eq!(MAX_BESSEL_J_ORDER, MAX_BESSEL_Y_ORDER);
        assert!(bessel_j(MAX_BESSEL_J_ORDER, 1.0).is_finite());
        assert!(bessel_j(MAX_BESSEL_J_ORDER + 1, 1.0).is_nan());
        assert!(bessel_j(-(MAX_BESSEL_J_ORDER + 1), 1.0).is_nan());
        assert!(bessel_j(i32::MAX, 1.0).is_nan());
    }

    #[test]
    fn bessel_i_rejects_orders_beyond_the_series_budget() {
        assert_eq!(MAX_BESSEL_I_ORDER, MAX_BESSEL_Y_ORDER);
        assert!(bessel_i_order_is_supported(MAX_BESSEL_I_ORDER));
        assert!(bessel_i(MAX_BESSEL_I_ORDER + 1, 1.0).is_nan());
        assert!(bessel_i(-(MAX_BESSEL_I_ORDER + 1), 1.0).is_nan());
        assert!(bessel_i(i32::MAX, 1.0).is_nan());
    }

    #[test]
    fn test_trigamma_standard_values_and_poles() {
        let pi_squared_over_six = std::f64::consts::PI.powi(2) / 6.0;
        assert!((trigamma(1.0) - pi_squared_over_six).abs() < 1e-12);
        assert!((trigamma(2.0) - (pi_squared_over_six - 1.0)).abs() < 1e-12);
        assert!(!trigamma(0.0).is_finite());
        assert!(!trigamma(-1.0).is_finite());
        assert!(trigamma(-1_000_000.5).is_finite());
    }
}
