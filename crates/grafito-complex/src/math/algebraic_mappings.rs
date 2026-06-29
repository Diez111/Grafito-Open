//! Mapeos algebraicos de primera clase para `ComplexMapping`.
//!
//! Representa los mapeos conformes más comunes como una enum cerrada,
//! evitando parsear la expresión cada vez que se evalúa. Esto resuelve
//! el bug de `ComplexMapping[1/z, ...]` donde el parser tokenizaba
//! `1/z` como `1 * z` por una inserción de `*` implícito.
//!
//! Cada variante usa la fórmula algebraica cerrada, no el AST. Esto
//! además es numéricamente más estable cerca de singularidades y permite
//! un manejo explícito de "valor no finito" (devolviendo `None` en
//! lugar de propagar `NaN`).
//!
//! # Uso
//!
//! ```ignore
//! use grafito_complex::algebraic_mappings::ConformalMap;
//! use num_complex::Complex64;
//!
//! let map = ConformalMap::from_expr_string("1/z").unwrap();
//! let z = Complex64::new(2.0, 0.0);
//! let w = map.apply(z).unwrap();  // 0.5
//! ```

use num_complex::Complex64;

/// Mapeo conforme algebraico de primera clase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConformalMap {
    /// Inversión simple `1/z`.
    Inversion,
    /// Potencia entera `z^n` con `n >= 1`.
    Power(i32),
    /// Exponencial `exp(z)`.
    Exponential,
    /// Logaritmo principal `log(z)` (rama con parte imaginaria en `(-π, π]`).
    Logarithm,
    /// Seno hiperbólico `sinh(z)`.
    Sinh,
    /// Coseno hiperbólico `cosh(z)`.
    Cosh,
    /// Seno `sin(z)`.
    Sine,
    /// Coseno `cos(z)`.
    Cosine,
    /// Tangente `tan(z)`.
    Tangent,
    /// Raíz cuadrada principal `sqrt(z)` (rama con parte real ≥ 0).
    Sqrt,
    /// Transformación de Joukowski `z + 1/z`.
    Joukowski,
    /// Transformación de Möbius general `(a*z + b) / (c*z + d)`.
    Mobius {
        a: Complex64,
        b: Complex64,
        c: Complex64,
        d: Complex64,
    },
    /// Inversión desplazada `1/(z - a)`.
    InversionShifted(Complex64),
}

/// Umbral para considerar una magnitud como singularidad.
const SINGULARITY_THRESHOLD: f64 = 1e-15;

impl ConformalMap {
    /// Aplica el mapeo al valor `z`. Devuelve `None` si la evaluación
    /// cae en una singularidad (división por cero, raíz de cero, etc.).
    pub fn apply(self, z: Complex64) -> Option<Complex64> {
        match self {
            Self::Inversion => {
                if z.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(z.inv())
                }
            }
            Self::Power(n) => {
                if n == 0 {
                    Some(Complex64::new(1.0, 0.0))
                } else {
                    Some(z.powi(n))
                }
            }
            Self::Exponential => Some(z.exp()),
            Self::Logarithm => {
                if z.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(z.ln())
                }
            }
            Self::Sinh => Some(z.sinh()),
            Self::Cosh => Some(z.cosh()),
            Self::Sine => Some(z.sin()),
            Self::Cosine => Some(z.cos()),
            Self::Tangent => {
                let cos = z.cos();
                if cos.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(z.tan())
                }
            }
            Self::Sqrt => {
                // sqrt(0) = 0 está bien definido (no es singularidad). Sólo
                // devolvemos 0 explícito para evitar cualquier NaN numérico.
                if z.norm() < SINGULARITY_THRESHOLD {
                    Some(Complex64::new(0.0, 0.0))
                } else {
                    Some(z.sqrt())
                }
            }
            Self::Joukowski => {
                if z.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(z + z.inv())
                }
            }
            Self::Mobius { a, b, c, d } => {
                let numer = a * z + b;
                let denom = c * z + d;
                if denom.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(numer / denom)
                }
            }
            Self::InversionShifted(a) => {
                let diff = z - a;
                if diff.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(diff.inv())
                }
            }
        }
    }

    /// Aplica la **inversa** del mapeo: dado un valor `w` en el output,
    /// devuelve un `z` tal que `self.apply(z) == w`. Devuelve `None` si
    /// la inversa no está definida en `w` o si el mapeo no es
    /// analíticamente invertible.
    ///
    /// Esto se usa para el **relleno de área** del `ComplexMapping`:
    /// para cada celda `w` en el output, computamos `z = inverse(w)` y
    /// evaluamos la curva original `f(z)`; según el signo determinamos
    /// si la celda está dentro o fuera de la región transformada.
    pub fn inverse_apply(self, w: Complex64) -> Option<Complex64> {
        match self {
            // 1/z → z (es su propia inversa)
            Self::Inversion => {
                if w.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(w.inv())
                }
            }
            // z^n → w^(1/n) (rama principal)
            Self::Power(n) => {
                if n == 0 {
                    Some(Complex64::new(1.0, 0.0))
                } else {
                    Some(w.powf(1.0 / n as f64))
                }
            }
            // exp(z) = w → z = log(w) (rama principal)
            Self::Exponential => {
                if w.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(w.ln())
                }
            }
            // log(z) = w → z = exp(w)
            Self::Logarithm => Some(w.exp()),
            // sinh, cosh, sin, cos, tan: inversa numérica
            Self::Sinh | Self::Cosh | Self::Sine | Self::Cosine | Self::Tangent => None,
            // sqrt(z) = w → z = w^2
            Self::Sqrt => Some(w * w),
            // Joukowski z + 1/z = w → ecuación cuadrática
            // z^2 - w*z + 1 = 0 → z = (w ± sqrt(w^2 - 4)) / 2
            Self::Joukowski => {
                let disc = w * w - Complex64::new(4.0, 0.0);
                let sqrt_disc = disc.sqrt();
                // Tomamos la rama con |z| <= 1 (interior del disco unitario
                // mapea a Joukowski clásico, elipse con focos en ±2).
                let z1 = (w + sqrt_disc) * 0.5;
                let z2 = (w - sqrt_disc) * 0.5;
                if z1.norm() <= 1.0 {
                    Some(z1)
                } else if z2.norm() <= 1.0 {
                    Some(z2)
                } else {
                    Some(z1) // fallback
                }
            }
            // Möbius (az+b)/(cz+d) = w → z = (dw - b) / (-cw + a)
            Self::Mobius { a, b, c, d } => {
                let numer = d * w - b;
                let denom = -c * w + a;
                if denom.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(numer / denom)
                }
            }
            // 1/(z - a) = w → z = a + 1/w
            Self::InversionShifted(a) => {
                if w.norm() < SINGULARITY_THRESHOLD {
                    None
                } else {
                    Some(a + w.inv())
                }
            }
        }
    }

    /// Intenta reconocer una cadena como uno de los mapeos algebraicos
    /// soportados. La cadena se normaliza quitando espacios. Devuelve
    /// `None` si no se reconoce.
    ///
    /// # Sintaxis reconocida
    ///
    /// - `1/z`, `1 / z`
    /// - `z^2`, `z^3`, ..., `z^10` (potencias enteras)
    /// - `exp(z)`, `exp z`
    /// - `log(z)`, `ln(z)`, `log z`, `ln z`
    /// - `sin(z)`, `cos(z)`, `tan(z)`
    /// - `sinh(z)`, `cosh(z)`
    /// - `sqrt(z)`
    /// - `z+1/z`, `z + 1/z`
    /// - `1/(z-a)` con `a` numérico (inversión desplazada)
    /// - `(a*z+b)/(c*z+d)` con coeficientes numéricos (Möbius)
    pub fn from_expr_string(s: &str) -> Option<Self> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if s.is_empty() {
            return None;
        }
        // Casos directos primero
        match s.as_str() {
            "1/z" => return Some(Self::Inversion),
            "z+1/z" => return Some(Self::Joukowski),
            _ => {}
        }
        // log / ln
        if s == "log(z)" || s == "ln(z)" {
            return Some(Self::Logarithm);
        }
        // exp, sin, cos, tan, sinh, cosh, sqrt
        if let Some(inner) = s.strip_prefix("exp(").and_then(|x| x.strip_suffix(')')) {
            if inner == "z" {
                return Some(Self::Exponential);
            }
        }
        for (prefix, variant) in [
            ("sin(", Self::Sine),
            ("cos(", Self::Cosine),
            ("tan(", Self::Tangent),
            ("sinh(", Self::Sinh),
            ("cosh(", Self::Cosh),
            ("sqrt(", Self::Sqrt),
        ] {
            if let Some(inner) = s.strip_prefix(prefix).and_then(|x| x.strip_suffix(')')) {
                if inner == "z" {
                    return Some(variant);
                }
            }
        }
        // z^n con n natural
        if let Some(rest) = s.strip_prefix("z^") {
            if let Ok(n) = rest.parse::<i32>() {
                if (1..=32).contains(&n) {
                    return Some(Self::Power(n));
                }
            }
        }
        // 1/(z-a) con a numérico
        if let Some(inner) = s.strip_prefix("1/(z-").and_then(|x| x.strip_suffix(')')) {
            if let Some(a) = parse_complex_literal(inner) {
                return Some(Self::InversionShifted(a));
            }
        }
        // (a*z+b)/(c*z+d) Möbius
        if let Some(mobius) = parse_mobius(&s) {
            return Some(mobius);
        }
        None
    }
}

/// Parsea un literal numérico (entero o decimal) como `Complex64`.
fn parse_complex_literal(s: &str) -> Option<Complex64> {
    s.parse::<f64>().ok().map(|r| Complex64::new(r, 0.0))
}

/// Intenta parsear una expresión Möbius `(a*z+b)/(c*z+d)`.
/// Solo reconoce coeficientes numéricos constantes.
fn parse_mobius(s: &str) -> Option<ConformalMap> {
    // Formato esperado: "(<num>*z+<num>)/(<num>*z+<num>)" o variantes con espacios
    // removidos. Sin negaciones compuestas (el caller ya normalizó espacios).
    //
    // No podemos strip_prefix('(')/strip_suffix(')') sobre el string entero:
    // una Möbius canónica tiene paréntesis anidados. En su lugar, dividimos
    // por el primer '/' que esté a profundidad 0 de paréntesis.
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut split_at = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'/' if depth == 0 => {
                split_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    let split_at = split_at?;
    let num_str = s[..split_at].trim();
    let den_str = s[split_at + 1..].trim();
    // Strip outer parens from num/den si los hay (case "(...)/(...)").
    let num_str = num_str
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .map(|s| s.trim())
        .unwrap_or(num_str);
    let den_str = den_str
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .map(|s| s.trim())
        .unwrap_or(den_str);

    let (a, b) = parse_linear_in_z(num_str)?;
    let (c, d) = parse_linear_in_z(den_str)?;
    // Detectar caso degenerado: a == 0, c == 0
    if a.norm() < SINGULARITY_THRESHOLD && c.norm() < SINGULARITY_THRESHOLD {
        return None;
    }
    Some(ConformalMap::Mobius { a, b, c, d })
}

/// Parsea `<coef>*z+<const>` o `z+<const>` o `<coef>*z` o `<const>`.
fn parse_linear_in_z(s: &str) -> Option<(Complex64, Complex64)> {
    // Caso: solo una constante
    if !s.contains('z') {
        let c = s.parse::<f64>().ok()?;
        return Some((Complex64::new(0.0, 0.0), Complex64::new(c, 0.0)));
    }
    // Caso: z solo
    if s == "z" {
        return Some((Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)));
    }
    // Buscar el signo + o - (que no sea el inicial) que separa coef*z de const
    let chars: Vec<char> = s.chars().collect();
    let mut split_idx = None;
    for (i, &c) in chars.iter().enumerate().skip(1) {
        if c == '+' || c == '-' {
            split_idx = Some(i);
            break;
        }
    }
    let (coef_str, const_str) = match split_idx {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    // coef_str puede ser "z", "<num>*z"
    let a = if coef_str == "z" {
        Complex64::new(1.0, 0.0)
    } else if let Some(rest) = coef_str.strip_suffix("*z") {
        let v = rest.parse::<f64>().ok()?;
        Complex64::new(v, 0.0)
    } else {
        return None;
    };
    let b = if const_str.is_empty() {
        Complex64::new(0.0, 0.0)
    } else {
        let v = const_str.parse::<f64>().ok()?;
        Complex64::new(v, 0.0)
    };
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: Complex64, b: Complex64, tol: f64, msg: &str) {
        let d = (a - b).norm();
        assert!(d < tol, "{}: a={}, b={}, |a-b|={} > {}", msg, a, b, d, tol);
    }

    // ----- Inversion -----

    #[test]
    fn inversion_of_two_is_half() {
        let m = ConformalMap::from_expr_string("1/z").unwrap();
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.5, 0.0), 1e-12, "1/2");
    }

    #[test]
    fn inversion_of_unit_circle_returns_unit_circle() {
        let m = ConformalMap::Inversion;
        for i in 0..32 {
            let theta = i as f64 / 32.0 * std::f64::consts::TAU;
            let z = Complex64::new(theta.cos(), theta.sin());
            let w = m.apply(z).unwrap();
            assert!(
                (w.norm() - 1.0).abs() < 1e-9,
                "|1/z| should be 1 on unit circle"
            );
        }
    }

    #[test]
    fn inversion_of_zero_is_singular() {
        let m = ConformalMap::Inversion;
        assert!(m.apply(Complex64::new(0.0, 0.0)).is_none());
    }

    #[test]
    fn inversion_with_spaces() {
        let m = ConformalMap::from_expr_string("1 / z").unwrap();
        assert_eq!(m, ConformalMap::Inversion);
    }

    // ----- Power -----

    #[test]
    fn z_squared_of_two_is_four() {
        let m = ConformalMap::from_expr_string("z^2").unwrap();
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(4.0, 0.0), 1e-12, "2^2");
    }

    #[test]
    fn z_squared_on_unit_circle_returns_unit_circle() {
        let m = ConformalMap::Power(2);
        for i in 0..32 {
            let theta = i as f64 / 32.0 * std::f64::consts::TAU;
            let z = Complex64::new(theta.cos(), theta.sin());
            let w = m.apply(z).unwrap();
            assert!(
                (w.norm() - 1.0).abs() < 1e-9,
                "|z^2| should be 1 on unit circle"
            );
        }
    }

    #[test]
    fn z_to_zero_is_one() {
        let m = ConformalMap::Power(0);
        let w = m.apply(Complex64::new(5.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "z^0");
    }

    // ----- Exponential -----

    #[test]
    fn exp_of_zero_is_one() {
        let m = ConformalMap::Exponential;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "exp(0)");
    }

    #[test]
    fn exp_of_i_pi_is_minus_one() {
        // e^(iπ) = -1 + 0i
        let m = ConformalMap::Exponential;
        let w = m.apply(Complex64::new(0.0, std::f64::consts::PI)).unwrap();
        assert_close(w, Complex64::new(-1.0, 0.0), 1e-9, "exp(i*pi)");
    }

    // ----- Logarithm -----

    #[test]
    fn log_of_one_is_zero() {
        let m = ConformalMap::Logarithm;
        let w = m.apply(Complex64::new(1.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.0, 0.0), 1e-12, "log(1)");
    }

    #[test]
    fn log_of_e_is_one() {
        let m = ConformalMap::Logarithm;
        let w = m.apply(Complex64::new(std::f64::consts::E, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-9, "log(e)");
    }

    #[test]
    fn log_of_unit_circle_is_imaginary_axis_segment() {
        // log(e^(iθ)) = iθ usando la rama principal con im ∈ (-π, π].
        // Para θ en (π, 2π), el resultado es i(θ - 2π) por el branch cut.
        let m = ConformalMap::Logarithm;
        for i in 0..32 {
            let theta = i as f64 / 32.0 * std::f64::consts::TAU;
            let z = Complex64::new(theta.cos(), theta.sin());
            let w = m.apply(z).unwrap();
            assert!(
                w.re.abs() < 1e-9,
                "log(e^(iθ)).re should be 0, got {}",
                w.re
            );
            let expected_im = if theta > std::f64::consts::PI {
                theta - std::f64::consts::TAU
            } else {
                theta
            };
            assert!(
                (w.im - expected_im).abs() < 1e-9,
                "log(e^(iθ)).im should be {}, got {}",
                expected_im,
                w.im
            );
        }
    }

    #[test]
    fn log_of_zero_is_singular() {
        let m = ConformalMap::Logarithm;
        assert!(m.apply(Complex64::new(0.0, 0.0)).is_none());
    }

    // ----- Sine / Cosine / Tangent -----

    #[test]
    fn sin_of_zero_is_zero() {
        let m = ConformalMap::Sine;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.0, 0.0), 1e-12, "sin(0)");
    }

    #[test]
    fn cos_of_zero_is_one() {
        let m = ConformalMap::Cosine;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "cos(0)");
    }

    #[test]
    fn sinh_of_zero_is_zero() {
        let m = ConformalMap::Sinh;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.0, 0.0), 1e-12, "sinh(0)");
    }

    #[test]
    fn cosh_of_zero_is_one() {
        let m = ConformalMap::Cosh;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "cosh(0)");
    }

    #[test]
    fn tan_of_zero_is_zero() {
        let m = ConformalMap::Tangent;
        let w = m.apply(Complex64::new(0.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.0, 0.0), 1e-12, "tan(0)");
    }

    #[test]
    fn tan_of_pi_over_2_is_singular() {
        let m = ConformalMap::Tangent;
        let w = m.apply(Complex64::new(std::f64::consts::FRAC_PI_2, 0.0));
        assert!(w.is_none(), "tan(pi/2) should be singular");
    }

    // ----- Sqrt -----

    #[test]
    fn sqrt_of_four_is_two() {
        let m = ConformalMap::Sqrt;
        let w = m.apply(Complex64::new(4.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(2.0, 0.0), 1e-12, "sqrt(4)");
    }

    #[test]
    fn sqrt_of_minus_one_is_i() {
        let m = ConformalMap::Sqrt;
        let w = m.apply(Complex64::new(-1.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.0, 1.0), 1e-12, "sqrt(-1)");
    }

    // ----- Joukowski -----

    #[test]
    fn joukowski_of_two_is_2_5() {
        // 2 + 1/2 = 2.5
        let m = ConformalMap::Joukowski;
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(2.5, 0.0), 1e-12, "2 + 1/2");
    }

    #[test]
    fn joukowski_of_unit_circle_maps_to_ellipse() {
        // Para |z|=1, z + 1/z = 2*cos(θ), así que x ∈ [-2, 2], y = 0
        let m = ConformalMap::Joukowski;
        for i in 0..32 {
            let theta = i as f64 / 32.0 * std::f64::consts::TAU;
            let z = Complex64::new(theta.cos(), theta.sin());
            let w = m.apply(z).unwrap();
            assert!((w.re - 2.0 * theta.cos()).abs() < 1e-9);
            assert!(
                w.im.abs() < 1e-9,
                "Joukowski on unit circle should have im=0"
            );
        }
    }

    // ----- InversionShifted -----

    #[test]
    fn inversion_shifted_works() {
        let m = ConformalMap::InversionShifted(Complex64::new(1.0, 0.0));
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "1/(2-1)");
    }

    #[test]
    fn inversion_shifted_at_pole_is_none() {
        let m = ConformalMap::InversionShifted(Complex64::new(1.0, 0.0));
        assert!(m.apply(Complex64::new(1.0, 0.0)).is_none());
    }

    #[test]
    fn from_string_1_over_z_minus_1() {
        let m = ConformalMap::from_expr_string("1/(z-1)").unwrap();
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(1.0, 0.0), 1e-12, "1/(2-1)");
    }

    // ----- Mobius -----

    #[test]
    fn mobius_identity() {
        // z → (1*z + 0) / (0*z + 1) = z
        let m = ConformalMap::Mobius {
            a: Complex64::new(1.0, 0.0),
            b: Complex64::new(0.0, 0.0),
            c: Complex64::new(0.0, 0.0),
            d: Complex64::new(1.0, 0.0),
        };
        let w = m.apply(Complex64::new(3.0, 4.0)).unwrap();
        assert_close(w, Complex64::new(3.0, 4.0), 1e-12, "identity");
    }

    #[test]
    fn mobius_inversion_via_formula() {
        // (0*z + 1) / (1*z + 0) = 1/z
        let m = ConformalMap::Mobius {
            a: Complex64::new(0.0, 0.0),
            b: Complex64::new(1.0, 0.0),
            c: Complex64::new(1.0, 0.0),
            d: Complex64::new(0.0, 0.0),
        };
        let w = m.apply(Complex64::new(2.0, 0.0)).unwrap();
        assert_close(w, Complex64::new(0.5, 0.0), 1e-12, "1/z via Möbius");
    }

    // ----- Recognition -----

    #[test]
    fn from_string_recognizes_all() {
        assert_eq!(
            ConformalMap::from_expr_string("1/z"),
            Some(ConformalMap::Inversion)
        );
        assert_eq!(
            ConformalMap::from_expr_string("z^2"),
            Some(ConformalMap::Power(2))
        );
        assert_eq!(
            ConformalMap::from_expr_string("z^3"),
            Some(ConformalMap::Power(3))
        );
        assert_eq!(
            ConformalMap::from_expr_string("exp(z)"),
            Some(ConformalMap::Exponential)
        );
        assert_eq!(
            ConformalMap::from_expr_string("log(z)"),
            Some(ConformalMap::Logarithm)
        );
        assert_eq!(
            ConformalMap::from_expr_string("ln(z)"),
            Some(ConformalMap::Logarithm)
        );
        assert_eq!(
            ConformalMap::from_expr_string("sin(z)"),
            Some(ConformalMap::Sine)
        );
        assert_eq!(
            ConformalMap::from_expr_string("cos(z)"),
            Some(ConformalMap::Cosine)
        );
        assert_eq!(
            ConformalMap::from_expr_string("tan(z)"),
            Some(ConformalMap::Tangent)
        );
        assert_eq!(
            ConformalMap::from_expr_string("sinh(z)"),
            Some(ConformalMap::Sinh)
        );
        assert_eq!(
            ConformalMap::from_expr_string("cosh(z)"),
            Some(ConformalMap::Cosh)
        );
        assert_eq!(
            ConformalMap::from_expr_string("sqrt(z)"),
            Some(ConformalMap::Sqrt)
        );
        assert_eq!(
            ConformalMap::from_expr_string("z+1/z"),
            Some(ConformalMap::Joukowski)
        );
    }

    #[test]
    fn from_string_rejects_unknown() {
        assert_eq!(ConformalMap::from_expr_string("z^2 + 1"), None);
        assert_eq!(ConformalMap::from_expr_string(""), None);
        assert_eq!(ConformalMap::from_expr_string("garbage"), None);
    }
}
