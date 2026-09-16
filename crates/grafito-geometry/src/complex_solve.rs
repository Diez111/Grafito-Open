//! Raíces en ℂ: cuadrática exacta + Durand–Kerner acotado.
//!
//! `CSolve`/`CSolutions` necesitan todas las raíces complejas, no solo las
//! reales de `solve_all_real`. Grado 1–2 exactos (fórmula), 3–16 por
//! Durand–Kerner con tolerancia y cota de iteraciones; más allá, error
//! honesto. Todo coeficiente no finito → error (nunca NaN silencioso).

use num_complex::Complex64;

use crate::poly_tools::poly_coeffs;

/// Grado máximo para Durand–Kerner (igual que `Solve`).
pub const MAX_CSOLVE_DEGREE: usize = 16;
/// Iteraciones máximas de Durand–Kerner.
pub const MAX_DK_ITERS: usize = 100;
/// Tolerancia de convergencia de Durand–Kerner.
pub const DK_TOLERANCE: f64 = 1e-12;

/// Todas las raíces complejas de un polinomio univariado.
///
/// Devuelve las raíces ordenadas por (parte real, parte imaginaria) para
/// salida determinista. Multiplicidades colapsadas: raíces a menos de
/// `DK_TOLERANCE` se informan una vez (GeoGebra lista multiplicidades por
/// separado solo en vistas específicas; acá se declara el colapso).
pub fn csolve(expr: &str, var: &str) -> Result<Vec<Complex64>, String> {
    let coefs = poly_coeffs(expr, var)?;
    if coefs.is_empty() {
        return Err(format!("'{expr}' no es un polinomio en '{var}'"));
    }
    let degree = coefs.len() - 1;
    if degree == 0 {
        return Err(format!("'{expr}' es constante: sin raíces en '{var}'"));
    }
    if degree > MAX_CSOLVE_DEGREE {
        return Err(format!(
            "'{expr}' excede grado {MAX_CSOLVE_DEGREE} para CSolve; usa NSolve"
        ));
    }
    for (i, c) in coefs.iter().enumerate() {
        if !c.is_finite() {
            return Err(format!("coeficiente {i} no finito en '{expr}'"));
        }
    }
    let mut roots = if degree == 1 {
        // a + b·x = 0 → x = −a/b.
        let (a, b) = (coefs[0], coefs[1]);
        if b == 0.0 {
            return Err(format!("'{expr}' degenera: sin raíces en '{var}'"));
        }
        vec![Complex64::new(-a / b, 0.0)]
    } else if degree == 2 {
        quadratic_complex(coefs[2], coefs[1], coefs[0])
    } else {
        let z: Vec<Complex64> = coefs.iter().map(|&c| Complex64::new(c, 0.0)).collect();
        durand_kerner(&z)?
    };
    // Determinista: ordena por (re, im) y colapsa casi-duplicados.
    roots.sort_by(|a, b| a.re.total_cmp(&b.re).then_with(|| a.im.total_cmp(&b.im)));
    let mut unique: Vec<Complex64> = Vec::with_capacity(roots.len());
    for r in roots {
        let dup = unique
            .iter()
            .any(|u: &Complex64| (u - r).norm() <= DK_TOLERANCE * 64.0);
        if !dup {
            unique.push(r);
        }
    }
    Ok(unique)
}

/// Fórmula cuadrática en ℂ (discriminante negativo bienvenido).
fn quadratic_complex(a: f64, b: f64, c: f64) -> Vec<Complex64> {
    if a == 0.0 {
        if b == 0.0 {
            return Vec::new();
        }
        return vec![Complex64::new(-c / b, 0.0)];
    }
    let (ca, cb, cc) = (
        Complex64::new(a, 0.0),
        Complex64::new(b, 0.0),
        Complex64::new(c, 0.0),
    );
    let disc = cb * cb - Complex64::new(4.0, 0.0) * ca * cc;
    let sqrt_disc = disc.sqrt();
    let two_a = Complex64::new(2.0 * a, 0.0);
    vec![(-cb + sqrt_disc) / two_a, (-cb - sqrt_disc) / two_a]
}

/// Durand–Kerner simultáneo sobre coeficientes ascendentes.
///
/// Arranque en círculo unitario con ángulos irracionales (determinista).
/// Devuelve error honesto si no converge en `MAX_DK_ITERS`.
fn durand_kerner(coefs: &[Complex64]) -> Result<Vec<Complex64>, String> {
    let degree = coefs.len() - 1;
    let lead = coefs[degree];
    if lead.norm() == 0.0 {
        return Err("coeficiente líder nulo".to_string());
    }
    // Normaliza a mónico para estabilidad.
    let monic: Vec<Complex64> = coefs.iter().map(|c| c / lead).collect();
    let eval = |z: Complex64| -> Complex64 {
        let mut acc = Complex64::new(0.0, 0.0);
        for c in monic.iter().rev() {
            acc = acc * z + c;
        }
        acc
    };
    let mut roots: Vec<Complex64> = (0..degree)
        .map(|k| {
            // Radio distinto por índice: arranques conjugados simétricos se
            // repelen y ciclan sin converger (trampa clásica de D–K).
            let angle = 2.0 * std::f64::consts::PI * (k as f64 + 0.5) / degree as f64;
            Complex64::from_polar(0.4 + 0.05 * k as f64, angle)
        })
        .collect();
    for _ in 0..MAX_DK_ITERS {
        let mut max_step = 0.0f64;
        let mut next = roots.clone();
        for i in 0..degree {
            let mut denom = Complex64::new(1.0, 0.0);
            for j in 0..degree {
                if i != j {
                    denom *= roots[i] - roots[j];
                }
            }
            if denom.norm() < 1e-300 {
                continue;
            }
            let step = eval(roots[i]) / denom;
            max_step = max_step.max(step.norm());
            next[i] = roots[i] - step;
        }
        roots = next;
        if !roots.iter().all(|z| z.re.is_finite() && z.im.is_finite()) {
            return Err("Durand–Kerner divergió a no finito".to_string());
        }
        if max_step <= DK_TOLERANCE {
            return Ok(roots);
        }
    }
    Err(format!(
        "Durand–Kerner no convergió en {MAX_DK_ITERS} iteraciones"
    ))
}

/// Redondeo de display: 10 decimales recortando ceros (`1.0000000002`→`1`).
///
/// Solo presentación: el valor numérico conserva precisión completa y
/// `CSolutions` verifica el residuo por separado. El umbral 1e-9 aplasta
/// ruido de convergencia (`2.2e-31i` → `0`).
fn fmt_num(value: f64) -> String {
    const DISPLAY_SNAP: f64 = 1e-9;
    let v = if value.abs() < DISPLAY_SNAP {
        0.0
    } else {
        value
    };
    if v == 0.0 {
        return "0".to_string();
    }
    let full = format!("{v:.10}");
    let trimmed = full.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Formato canónico `a+bi` (GeoGebra escribe `a + bi`; acá sin espacios
/// para re-parseo, con `i` explícita y `-0` normalizado a `0`).
pub fn format_complex(z: Complex64) -> String {
    let re = if z.re == 0.0 { 0.0 } else { z.re };
    let im = if z.im == 0.0 { 0.0 } else { z.im };
    if im == 0.0 {
        return fmt_num(re);
    }
    if re == 0.0 {
        return format!("{}i", fmt_num(im));
    }
    if im < 0.0 {
        format!("{}{}i", fmt_num(re), fmt_num(im))
    } else {
        format!("{}+{}i", fmt_num(re), fmt_num(im))
    }
}

/// Conjunto de raíces `{z₁, z₂}` para mensajes.
pub fn format_complex_roots(roots: &[Complex64]) -> String {
    let parts: Vec<String> = roots.iter().map(|&z| format_complex(z)).collect();
    format!("{{{}}}", parts.join(", "))
}

/// Parsea un literal complejo: `a+bi`, `a-bi`, `bi`, `a`, `i`, `-i`, `3 i`.
///
/// Acepta la forma con multiplicación explícita (`3+4*i`) que produce el
/// normalizador de entrada antes del dispatch.
pub fn parse_complex_arg(text: &str) -> Result<Complex64, String> {
    let s: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace("*i", "i");
    if s.is_empty() {
        return Err("número complejo vacío".to_string());
    }
    // Sin `i`: real puro.
    if !s.contains('i') {
        let re: f64 = s
            .parse()
            .map_err(|_| format!("'{text}' no es un número complejo"))?;
        if !re.is_finite() {
            return Err(format!("'{text}' no es finito"));
        }
        return Ok(Complex64::new(re, 0.0));
    }
    // Debe terminar en `i` (formas `a+bi`, `bi`, `i`).
    let body = s
        .strip_suffix('i')
        .ok_or_else(|| format!("'{text}' no es un número complejo"))?;
    if body.is_empty() || body == "+" {
        return Ok(Complex64::new(0.0, 1.0));
    }
    if body == "-" {
        return Ok(Complex64::new(0.0, -1.0));
    }
    // Busca el último `+`/`-` no inicial como separador re/im.
    let mut split = None;
    for (i, c) in body.char_indices().skip(1) {
        if c == '+' || c == '-' {
            split = Some(i);
        }
    }
    let (re, im) = match split {
        Some(i) => {
            let (rs, is) = body.split_at(i);
            let re: f64 = rs
                .parse()
                .map_err(|_| format!("'{text}' no es un número complejo"))?;
            let im: f64 = if is == "+" {
                1.0
            } else if is == "-" {
                -1.0
            } else {
                is.parse()
                    .map_err(|_| format!("'{text}' no es un número complejo"))?
            };
            (re, im)
        }
        None => {
            // Solo parte imaginaria (`3i`, `-2.5i`).
            let im: f64 = body
                .parse()
                .map_err(|_| format!("'{text}' no es un número complejo"))?;
            (0.0, im)
        }
    };
    if !re.is_finite() || !im.is_finite() {
        return Err(format!("'{text}' no es finito"));
    }
    Ok(Complex64::new(re, im))
}

/// Forma polar `(r; θ)` con θ en radianes normalizado a (−π, π].
pub fn to_polar(z: Complex64) -> (f64, f64) {
    (z.norm(), z.arg())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_real_roots_exact() {
        let roots = csolve("x^2-4", "x").expect("cuadrática real");
        assert_eq!(roots.len(), 2);
        assert!((roots[0].re + 2.0).abs() < 1e-9);
        assert!((roots[1].re - 2.0).abs() < 1e-9);
    }

    #[test]
    fn quadratic_complex_pair() {
        // x²+1 = 0 → ±i (Solve real diría "sin raíces").
        let roots = csolve("x^2+1", "x").expect("par complejo");
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|z| z.re.abs() < 1e-9));
        assert!(roots.iter().any(|z| (z.im - 1.0).abs() < 1e-9));
        assert!(roots.iter().any(|z| (z.im + 1.0).abs() < 1e-9));
    }

    #[test]
    fn cubic_all_roots_durand_kerner() {
        // (x-1)(x-2)(x-3) = x³-6x²+11x-6.
        let roots = csolve("x^3-6*x^2+11*x-6", "x").expect("cúbica");
        assert_eq!(roots.len(), 3);
        for (root, expected) in roots.iter().zip([1.0, 2.0, 3.0]) {
            assert!((root.re - expected).abs() < 1e-6, "got {root:?}");
            assert!(root.im.abs() < 1e-6);
        }
    }

    #[test]
    fn degree_cap_is_honest() {
        let big = (0..=17)
            .map(|k| format!("x^{k}"))
            .collect::<Vec<_>>()
            .join("+");
        let err = csolve(&big, "x").expect_err("grado 17 excede");
        assert!(err.contains("16"), "got {err}");
    }

    #[test]
    fn parse_complex_forms() {
        assert_eq!(parse_complex_arg("3+4i").unwrap(), Complex64::new(3.0, 4.0));
        assert_eq!(
            parse_complex_arg("3-4i").unwrap(),
            Complex64::new(3.0, -4.0)
        );
        assert_eq!(parse_complex_arg("4i").unwrap(), Complex64::new(0.0, 4.0));
        assert_eq!(parse_complex_arg("i").unwrap(), Complex64::new(0.0, 1.0));
        assert_eq!(parse_complex_arg("-i").unwrap(), Complex64::new(0.0, -1.0));
        assert_eq!(parse_complex_arg("5").unwrap(), Complex64::new(5.0, 0.0));
        assert!(parse_complex_arg("x").is_err());
    }

    #[test]
    fn format_roundtrip() {
        assert_eq!(format_complex(Complex64::new(3.0, -4.0)), "3-4i");
        assert_eq!(format_complex(Complex64::new(0.0, 2.0)), "2i");
        assert_eq!(format_complex(Complex64::new(5.0, 0.0)), "5");
    }
}
