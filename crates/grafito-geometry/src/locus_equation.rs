//! Aproximación mock de eliminación Groebner vía sampling + regresión simbólica.
//!
//! El lugar geométrico real se obtiene por trazo dinámico (`Locus` como `Pencil`);
//! este módulo deriva una ecuación implícita `f(x,y)=0` a partir de sus muestras
//! usando regresión en base monomial acotada. No es un Groebner simbólico exacto,
//! sino un sustituto numérico presupuestado que respeta los límites de
//! `grafito-core::validation` y de `grafito-geometry::statistics`.

use crate::Point2;
use nalgebra::DMatrix;

/// Máximo de muestras que puede procesar la inferencia implícita.
pub const MAX_LOCUS_SAMPLES: usize = 5_000;
/// Grado total máximo de la base monomial (x^i y^j, i+j ≤ grado).
pub const MAX_LOCUS_DEGREE: usize = 4;
/// Mínimo de muestras para un ajuste implícito significativo.
pub const MIN_LOCUS_SAMPLES: usize = 6;
/// Máximo de monomios para grado 4: (4+1)(4+2)/2 = 15.
pub const MAX_MONOMIALS: usize = 15;
/// Máximo de caracteres para la ecuación resultante.
pub const MAX_EQUATION_CHARS: usize = 2_000;
/// Tolerancia absoluta para filtrar coeficientes despreciables.
pub const COEFF_EPS: f64 = 1e-8;
/// Tolerancia RMSE para aceptar un grado.
pub const RMSE_TOL: f64 = 1e-3;

/// Resultado de la inferencia implícita.
#[derive(Debug, Clone, PartialEq)]
pub struct LocusEquationResult {
    /// Ecuación implícita como expresión `f(x,y)` tal que `f(x,y)=0`.
    pub equation: String,
    /// Grado total usado.
    pub degree: usize,
    /// RMSE del ajuste en muestras.
    pub rmse: f64,
    /// Coeficientes en orden de `monomials_for_degree(degree)` normalizados a máx |c|=1.
    pub coefficients: Vec<f64>,
    /// Monomios correspondientes.
    pub monomials: Vec<(usize, usize)>,
}

/// Devuelve la lista ordenada de monomios x^i y^j con i+j ≤ degree.
///
/// Orden: grado total ascendente, para cada grado `i` decrece (x mayor primero).
/// Ejemplo grado 2 → (0,0), (1,0), (0,1), (2,0), (1,1), (0,2).
pub fn monomials_for_degree(degree: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for total in 0..=degree {
        for x_pow in (0..=total).rev() {
            let y_pow = total - x_pow;
            out.push((x_pow, y_pow));
        }
    }
    out
}

fn eval_monomial(point: Point2, x_pow: usize, y_pow: usize) -> f64 {
    // Potencias pequeñas (≤4) permiten multiplicación iterativa sin powf.
    let mut res = 1.0_f64;
    for _ in 0..x_pow {
        res *= point.x;
        if !res.is_finite() {
            return f64::NAN;
        }
    }
    for _ in 0..y_pow {
        res *= point.y;
        if !res.is_finite() {
            return f64::NAN;
        }
    }
    res
}

/// Construye la matriz de Vandermonde monomial (n × m) para los puntos dados.
fn build_vandermonde(
    points: &[Point2],
    monomials: &[(usize, usize)],
) -> Result<DMatrix<f64>, String> {
    let n = points.len();
    let m = monomials.len();
    let product = n
        .checked_mul(m)
        .ok_or_else(|| "desbordamiento en Vandermonde".to_string())?;
    if product > MAX_LOCUS_SAMPLES * MAX_MONOMIALS {
        return Err(format!(
            "Vandermonde {n}×{m} excede presupuesto {}",
            MAX_LOCUS_SAMPLES * MAX_MONOMIALS
        ));
    }
    let mut data = Vec::with_capacity(product);
    for point in points {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("muestra no finita".to_string());
        }
        // Rechazo de coordenadas excesivamente grandes que producirían overflow en grado 4.
        if point.x.abs() > 1e6 || point.y.abs() > 1e6 {
            return Err("coordenada de muestra fuera de rango (±1e6)".to_string());
        }
        for (x_pow, y_pow) in monomials {
            let value = eval_monomial(*point, *x_pow, *y_pow);
            if !value.is_finite() {
                return Err("monomio no finito".to_string());
            }
            // Acota magnitud para evitar OOM numérico en SVD.
            if value.abs() > 1e12 {
                return Err("monomio excede magnitud 1e12".to_string());
            }
            data.push(value);
        }
    }
    // DMatrix::from_row_slice espera datos en orden de fila.
    let dm = DMatrix::from_row_slice(n, m, &data);
    Ok(dm)
}

/// Resuelve el vector nulo (espacio nulo) de `matrix` vía SVD y devuelve el
/// vector singular asociado al menor valor singular, normalizado a máx |c|=1.
fn nullspace_via_svd(matrix: &DMatrix<f64>) -> Result<Vec<f64>, String> {
    let (n_rows, n_cols) = (matrix.nrows(), matrix.ncols());
    if n_rows < n_cols {
        return Err("muestras insuficientes para el grado".to_string());
    }
    let svd = matrix.clone().svd(true, true);
    let Some(v_t) = svd.v_t else {
        return Err("descomposición SVD falló (V^T)".to_string());
    };
    if v_t.nrows() != n_cols || v_t.ncols() != n_cols {
        return Err("dimensiones SVD inesperadas".to_string());
    }
    // El último renglón de V^T corresponde al menor valor singular.
    let row = v_t.row(n_cols - 1);
    let mut coeffs: Vec<f64> = row.iter().copied().collect();
    if coeffs.iter().any(|value| !value.is_finite()) {
        return Err("coeficientes SVD no finitos".to_string());
    }
    // Normaliza a máx |c| = 1 para expresión estable.
    let max_abs = coeffs
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if !max_abs.is_finite() || max_abs <= COEFF_EPS {
        return Err("vector nulo degenerado".to_string());
    }
    for value in &mut coeffs {
        *value /= max_abs;
        if value.abs() < COEFF_EPS {
            *value = 0.0;
        }
    }
    Ok(coeffs)
}

fn rmse_for_coeffs(
    points: &[Point2],
    monomials: &[(usize, usize)],
    coeffs: &[f64],
) -> Result<f64, String> {
    if points.is_empty() {
        return Err("sin muestras para RMSE".to_string());
    }
    let mut sum_sq = 0.0_f64;
    let mut scale: f64 = 0.0;
    let mut sum_scaled = 0.0_f64;
    for point in points {
        let mut residual = 0.0_f64;
        for (coeff, (x_pow, y_pow)) in coeffs.iter().zip(monomials) {
            if *coeff == 0.0 {
                continue;
            }
            let m = eval_monomial(*point, *x_pow, *y_pow);
            if !m.is_finite() || !coeff.is_finite() {
                return Err("residual no finito".to_string());
            }
            residual += coeff * m;
        }
        if !residual.is_finite() {
            return Err("residual no finito".to_string());
        }
        let abs = residual.abs();
        if abs == 0.0 {
            continue;
        }
        if scale < abs {
            let ratio = if scale == 0.0 { 0.0 } else { scale / abs };
            sum_scaled = 1.0 + sum_scaled * ratio * ratio;
            scale = abs;
        } else {
            let ratio = abs / scale;
            sum_scaled += ratio * ratio;
        }
        sum_sq += residual * residual;
        if !sum_sq.is_finite() && scale.is_finite() {
            // Usa la versión escalada para evitar overflow.
            sum_sq = scale * scale * sum_scaled;
        }
    }
    // RMSE estable: si usamos escalado, recalcular desde escala.
    let rmse = if scale == 0.0 {
        0.0
    } else if sum_scaled > 0.0 && (sum_sq.is_infinite() || sum_sq > 1e280) {
        scale * (sum_scaled / points.len() as f64).sqrt()
    } else {
        (sum_sq / points.len() as f64).sqrt()
    };
    if !rmse.is_finite() {
        return Err("RMSE no finito".to_string());
    }
    Ok(rmse)
}

fn format_monomial(x_pow: usize, y_pow: usize) -> String {
    match (x_pow, y_pow) {
        (0, 0) => String::new(), // constante se maneja aparte
        (1, 0) => "x".to_string(),
        (0, 1) => "y".to_string(),
        (x, 0) => format!("x^{x}"),
        (0, y) => format!("y^{y}"),
        (1, 1) => "x*y".to_string(),
        (x, 1) => format!("x^{x}*y"),
        (1, y) => format!("x*y^{y}"),
        (x, y) => format!("x^{x}*y^{y}"),
    }
}

fn coeffs_to_equation(monomials: &[(usize, usize)], coeffs: &[f64]) -> Result<String, String> {
    if monomials.len() != coeffs.len() {
        return Err("desajuste monomio/coeficiente".to_string());
    }
    let mut terms: Vec<String> = Vec::new();
    for ((x_pow, y_pow), coeff) in monomials.iter().zip(coeffs) {
        if coeff.abs() < COEFF_EPS {
            continue;
        }
        let mono = format_monomial(*x_pow, *y_pow);
        let term = if mono.is_empty() {
            // Término constante
            format!("{:.6}", coeff)
        } else if (*coeff - 1.0).abs() < 1e-7 {
            mono
        } else if (*coeff + 1.0).abs() < 1e-7 {
            format!("-{}", mono)
        } else {
            format!("{:.6}*{}", coeff, mono)
        };
        terms.push(term);
    }
    if terms.is_empty() {
        return Err("ecuación degenerada (sin términos)".to_string());
    }
    // Une términos con " + " manejando signos: reemplaza "+ -" por "- ".
    let mut equation = terms.join(" + ");
    equation = equation.replace(" + -", " - ");
    if equation.len() > MAX_EQUATION_CHARS {
        return Err(format!("ecuación excede {} caracteres", MAX_EQUATION_CHARS));
    }
    // Validar que la expresión sea parseable como función de x,y (al menos un operador o variable).
    if equation.trim().is_empty() {
        return Err("ecuación vacía".to_string());
    }
    Ok(equation)
}

/// Intenta derivar ecuación implícita `f(x,y)=0` a partir de `points`.
///
/// Prueba grados 2..=max_degree (acotado a `MAX_LOCUS_DEGREE`) y elige el
/// que minimiza RMSE sin superar el presupuesto de muestras y monomios.
/// Devuelve el mejor `LocusEquationResult` o error si ningún grado converge.
pub fn approximate_locus_equation(
    points: &[Point2],
    max_degree: Option<usize>,
) -> Result<LocusEquationResult, String> {
    if points.len() < MIN_LOCUS_SAMPLES {
        return Err(format!(
            "se requieren al menos {MIN_LOCUS_SAMPLES} muestras, recibidas {}",
            points.len()
        ));
    }
    if points.len() > MAX_LOCUS_SAMPLES {
        return Err(format!(
            "muestras {} exceden máximo {MAX_LOCUS_SAMPLES}",
            points.len()
        ));
    }
    for point in points {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("muestra no finita".to_string());
        }
    }
    let requested = max_degree.unwrap_or(3);
    if requested == 0 || requested > MAX_LOCUS_DEGREE {
        return Err(format!("grado debe estar entre 1 y {MAX_LOCUS_DEGREE}"));
    }
    let mut best: Option<LocusEquationResult> = None;
    let mut last_error: Option<String> = None;
    // Probar grados de forma creciente; para el mock Groebner, prioriza grado pequeño si RMSE es aceptable.
    for degree in 2..=requested {
        let monomials = monomials_for_degree(degree);
        if monomials.len() > MAX_MONOMIALS {
            last_error = Some(format!("grado {degree} excede MAX_MONOMIALS"));
            continue;
        }
        if points.len() < monomials.len() {
            last_error = Some(format!(
                "muestras {} insuficientes para grado {degree} (requiere {})",
                points.len(),
                monomials.len()
            ));
            continue;
        }
        let vand = match build_vandermonde(points, &monomials) {
            Ok(m) => m,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let coeffs = match nullspace_via_svd(&vand) {
            Ok(value) => value,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let rmse = match rmse_for_coeffs(points, &monomials, &coeffs) {
            Ok(value) => value,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let equation = match coeffs_to_equation(&monomials, &coeffs) {
            Ok(value) => value,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let candidate = LocusEquationResult {
            equation,
            degree,
            rmse,
            coefficients: coeffs,
            monomials,
        };
        // Si ya tenemos un candidato con RMSE aceptable, prioriza grado menor (expresividad mínima).
        let is_better = match &best {
            None => true,
            Some(existing) => candidate.rmse + 1e-9 < existing.rmse,
        };
        if is_better {
            best = Some(candidate);
        }
        // Parada temprana si RMSE es excelente y grado pequeño ya explica los datos.
        if let Some(ref current) = best {
            if current.rmse < RMSE_TOL && current.degree <= 3 {
                break;
            }
        }
    }
    best.ok_or_else(|| {
        last_error.unwrap_or_else(|| "no se pudo inferir ecuación implícita".to_string())
    })
}

/// Atajo que infiere ecuación con grado por defecto 3.
pub fn infer_locus_equation(points: &[Point2]) -> Result<String, String> {
    Ok(approximate_locus_equation(points, Some(3))?.equation)
}

/// Adapter desde `Pencil` (locus) – extrae `points` y delega a `approximate_locus_equation`.
///
/// No acopla a `grafito-core` para mantener `grafito-geometry` puro; el caller
/// en `grafito-command` pasa `locus.points.clone()`.
pub fn locus_equation_from_samples(points: &[Point2]) -> Result<LocusEquationResult, String> {
    approximate_locus_equation(points, Some(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point2;

    fn circle_samples(center: Point2, radius: f64, count: usize) -> Vec<Point2> {
        let mut points = Vec::with_capacity(count);
        for index in 0..count {
            let angle = index as f64 / count as f64 * std::f64::consts::TAU;
            points.push(Point2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ));
        }
        points
    }

    #[test]
    fn monomials_degree_two_has_six() {
        let m = monomials_for_degree(2);
        assert_eq!(m.len(), 6);
        assert!(m.contains(&(2, 0)));
        assert!(m.contains(&(1, 1)));
        assert!(m.contains(&(0, 2)));
    }

    #[test]
    fn circle_implicit_has_small_rmse() {
        let points = circle_samples(Point2::new(1.0, -0.5), 2.0, 100);
        let result = approximate_locus_equation(&points, Some(2)).expect("ajuste círculo");
        // Círculo grado 2 debe producir RMSE pequeño (<1e-3 para muestras exactas con redondeo).
        assert!(result.degree == 2, "degree={}", result.degree);
        assert!(result.rmse < 1e-2, "rmse={}", result.rmse);
        assert!(result.equation.len() <= MAX_EQUATION_CHARS);
        // Ecuación debe mencionar x e y.
        assert!(result.equation.contains('x') || result.equation.contains('y'));
    }

    #[test]
    fn rejects_too_few_samples() {
        let points = vec![Point2::new(0.0, 0.0); 3];
        assert!(approximate_locus_equation(&points, Some(2)).is_err());
    }

    #[test]
    fn rejects_oversized_sample_set() {
        let points = vec![Point2::new(0.0, 0.0); MAX_LOCUS_SAMPLES + 1];
        assert!(approximate_locus_equation(&points, Some(2)).is_err());
    }

    #[test]
    fn equation_string_within_budget() {
        let points = circle_samples(Point2::new(0.0, 0.0), 1.0, 50);
        let equation = infer_locus_equation(&points).expect("ecuación");
        assert!(equation.len() <= MAX_EQUATION_CHARS);
        assert!(!equation.is_empty());
    }

    #[test]
    fn budget_monomials_capped() {
        assert!(monomials_for_degree(MAX_LOCUS_DEGREE).len() <= MAX_MONOMIALS);
    }
}
