//! Motor faltante de estadística descriptiva, muestreo determinista y finanzas.
//!
//! Cubre lo que `statistics.rs` y `list_ops.rs` todavía no tenían, sin tocar
//! ninguna firma existente: solo agrega. Todo lo que ya existía se reutiliza
//! por delegación (cero lógica duplicada).
//!
//! - Descriptiva: `population_variance` (÷n; `variance` ya era muestral ÷n−1),
//!   `histogram_right` (bins `(izq, der]`, el `histogram` existente es `[izq, der)`),
//!   `contingency_table_2d` (fachada 2D sobre `list_ops::contingency_chi2`).
//! - Muestreo: `random_uniform` / `random_normal` / `random_binomial` /
//!   `random_poisson` con [`DeterministicRng`] (`list_ops.rs:55`): misma semilla
//!   → misma secuencia, siempre.
//! - Finanzas (convención de aula, todo positivo, pagos al final del período):
//!   `future_value` / `present_value` / `payment`.
//!
//! Errores honestos en español rioplatense vía `Result<_, String>`; `NaN`
//! jamás se devuelve en silencio donde hay `Result`.

use crate::list_ops::DeterministicRng;

/// Tope de observaciones para la descriptiva de este módulo (espeja `MAX_LIST_LENGTH`).
pub const MAX_STATS_EXTRA_LEN: usize = 10_000;
/// Tope de clases del histograma derecho (espeja `MAX_HISTOGRAM_BINS`).
pub const MAX_HISTOGRAM_RIGHT_BINS: usize = 4_096;
/// Lado máximo de una tabla de contingencia (filas y columnas).
pub const MAX_CONTINGENCY_DIM: usize = 1_024;
/// Tope de celdas de una tabla de contingencia.
pub const MAX_CONTINGENCY_CELLS: usize = 1_048_576;
/// Tope de ensayos de `random_binomial` (cota de trabajo lineal).
pub const MAX_RANDOM_TRIALS: u32 = 100_000;
/// Hasta este λ `random_poisson` usa Knuth exacto; arriba aproxima normal.
pub const MAX_POISSON_KNUTH_LAMBDA: f64 = 100.0;
/// λ máximo admitido por `random_poisson`.
pub const MAX_POISSON_LAMBDA: f64 = 1_000_000.0;
/// Pasos máximos del Knuth de Poisson antes del error honesto.
pub const MAX_POISSON_KNUTH_STEPS: usize = 1_000_000;
/// Períodos máximos de las funciones financieras.
pub const MAX_FINANCE_PERIODS: u32 = 100_000;
/// Denominador 2⁵³ del uniforme en `[0, 1)`.
const UNIT_DENOMINATOR: f64 = 9_007_199_254_740_992.0;

fn require_len(len: usize, cmd: &str) -> Result<(), String> {
    if len > MAX_STATS_EXTRA_LEN {
        return Err(format!(
            "{cmd}: longitud {len} excede el máximo {MAX_STATS_EXTRA_LEN}"
        ));
    }
    Ok(())
}

fn require_finite_slice(data: &[f64], cmd: &str) -> Result<(), String> {
    if data.iter().any(|v| !v.is_finite()) {
        return Err(format!("{cmd}: la lista contiene valores no finitos"));
    }
    Ok(())
}

// ── Descriptiva faltante ──────────────────────────────────────────────

/// `Variance[lista]` poblacional (÷n).
///
/// `statistics::variance` ya era la muestral (÷n−1); esta es la poblacional.
/// Con un solo dato da `0.0`; vacía o no finita → error.
pub fn population_variance(data: &[f64]) -> Result<f64, String> {
    require_len(data.len(), "Variance")?;
    if data.is_empty() {
        return Err("Variance: lista vacía".to_string());
    }
    require_finite_slice(data, "Variance")?;
    let mean = crate::statistics::mean(data)
        .ok_or_else(|| "Variance: media no representable".to_string())?;
    let mut sum = 0.0;
    for value in data {
        sum += (value - mean).powi(2);
        if !sum.is_finite() {
            return Err("Variance: suma de cuadrados no finita".to_string());
        }
    }
    Ok(sum / data.len() as f64)
}

/// Histograma con bins cerrados a derecha `(izq, der]`.
///
/// El `histogram` existente asigna `[izq, der)` (el máximo cae en el último
/// bin por recorte); acá cada borde interior pertenece al bin de la izquierda.
/// El mínimo `lo` entra en el primer bin. Devuelve `(izq, der, conteo)` por bin.
/// Sin datos, sin bins o sin finitos → vacío (igual que `histogram`).
pub fn histogram_right(data: &[f64], bins: usize) -> Vec<(f64, f64, f64)> {
    if data.is_empty() || bins == 0 {
        return vec![];
    }
    let finite: Vec<f64> = data
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        return vec![];
    }
    let bins = bins.min(MAX_HISTOGRAM_RIGHT_BINS);
    let lo = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let width = if (hi - lo).abs() < 1e-15 {
        1.0
    } else {
        (hi - lo) / bins as f64
    };
    if !width.is_finite() || width <= 0.0 {
        return vec![];
    }
    let mut counts = vec![0usize; bins];
    for value in finite {
        let mut index = if value <= lo {
            0
        } else {
            (((value - lo) / width).ceil() as usize).saturating_sub(1)
        };
        if index >= bins {
            index = bins - 1;
        }
        counts[index] += 1;
    }
    counts
        .iter()
        .enumerate()
        .map(|(i, count)| {
            let left = lo + i as f64 * width;
            (left, left + width, *count as f64)
        })
        .collect()
}

/// `ContingencyTable` 2D: χ² de independencia sobre matriz `filas × columnas`.
///
/// Fachada rectangular sobre `list_ops::contingency_chi2` (aplana y delega,
/// sin duplicar la fórmula). Devuelve `(χ², grados_de_libertad, p_valor)`.
/// Tabla < 2×2, despareja, con negativos/no finitos o total nulo → error.
pub fn contingency_table_2d(table: &[Vec<f64>]) -> Result<(f64, f64, f64), String> {
    const CMD: &str = "ContingencyTable";
    if table.len() < 2 {
        return Err(format!("{CMD}: se requieren al menos 2 filas"));
    }
    if table.len() > MAX_CONTINGENCY_DIM {
        return Err(format!(
            "{CMD}: filas {} exceden el máximo {MAX_CONTINGENCY_DIM}",
            table.len()
        ));
    }
    let ncols = table.first().map_or(0, Vec::len);
    if ncols < 2 {
        return Err(format!("{CMD}: se requieren al menos 2 columnas"));
    }
    if ncols > MAX_CONTINGENCY_DIM {
        return Err(format!(
            "{CMD}: columnas {ncols} exceden el máximo {MAX_CONTINGENCY_DIM}"
        ));
    }
    let cells = table
        .len()
        .checked_mul(ncols)
        .ok_or_else(|| format!("{CMD}: tabla no representable"))?;
    if cells > MAX_CONTINGENCY_CELLS {
        return Err(format!(
            "{CMD}: {cells} celdas exceden el máximo {MAX_CONTINGENCY_CELLS}"
        ));
    }
    for row in table {
        if row.len() != ncols {
            return Err(format!("{CMD}: la tabla debe ser rectangular"));
        }
    }
    let mut flat = Vec::with_capacity(cells);
    for row in table {
        flat.extend_from_slice(row);
    }
    crate::list_ops::contingency_chi2(&flat, ncols)
}

// ── Muestreo determinista ─────────────────────────────────────────────

/// Uniforme en `[0, 1)` a partir del RNG (53 bits del `next_u64`).
fn next_unit(rng: &mut DeterministicRng) -> f64 {
    // 53 bits altos del xorshift64*: cae en [0, 1) sin sesgo medible.
    (rng.next_u64() >> 11) as f64 / UNIT_DENOMINATOR
}

/// Normal estándar por Box-Muller (un solo uso del par; determinista).
fn box_muller_standard(rng: &mut DeterministicRng) -> f64 {
    let u1 = next_unit(rng).max(f64::MIN_POSITIVE);
    let u2 = next_unit(rng);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// `RandomUniform[a, b]`: continua uniforme en `[a, b)`. `a < b` finitos.
pub fn random_uniform(rng: &mut DeterministicRng, a: f64, b: f64) -> Result<f64, String> {
    if !a.is_finite() || !b.is_finite() {
        return Err("RandomUniform: extremos no finitos".to_string());
    }
    if a >= b {
        return Err(format!("RandomUniform: se requiere a < b (fue {a}, {b})"));
    }
    let draw = a + (b - a) * next_unit(rng);
    if draw.is_finite() {
        Ok(draw)
    } else {
        Err("RandomUniform: resultado no finito".to_string())
    }
}

/// `RandomNormal[mu, sigma]`: gaussiana por Box-Muller. `sigma > 0` finito.
pub fn random_normal(rng: &mut DeterministicRng, mu: f64, sigma: f64) -> Result<f64, String> {
    if !mu.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
        return Err("RandomNormal: mu finito y sigma > 0 requeridos".to_string());
    }
    let draw = mu + sigma * box_muller_standard(rng);
    if draw.is_finite() {
        Ok(draw)
    } else {
        Err("RandomNormal: resultado no finito".to_string())
    }
}

/// `RandomBinomial[n, p]`: conteo de éxitos en `n` ensayos (`p` en `[0, 1]`).
///
/// Suma exacta de Bernoulli hasta `MAX_RANDOM_TRIALS`; arriba → error honesto
/// (el cableado puede partir la tirada en vez de sesgarla con aproximaciones).
pub fn random_binomial(rng: &mut DeterministicRng, trials: u32, p: f64) -> Result<u32, String> {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return Err(format!("RandomBinomial: p={p} fuera de [0, 1]"));
    }
    if trials > MAX_RANDOM_TRIALS {
        return Err(format!(
            "RandomBinomial: n={trials} excede el máximo {MAX_RANDOM_TRIALS}"
        ));
    }
    let mut count: u32 = 0;
    for _ in 0..trials {
        if next_unit(rng) < p {
            count += 1;
        }
    }
    Ok(count)
}

/// `RandomPoisson[lambda]`: conteo de eventos (`lambda > 0`).
///
/// Knuth exacto hasta `MAX_POISSON_KNUTH_LAMBDA`; arriba aproxima con la normal
/// `N(lambda, sqrt(lambda))` redondeada (documentado, sin bucle eterno).
pub fn random_poisson(rng: &mut DeterministicRng, lambda: f64) -> Result<u32, String> {
    if !lambda.is_finite() || lambda <= 0.0 {
        return Err(format!(
            "RandomPoisson: lambda={lambda} debe ser finito > 0"
        ));
    }
    if lambda > MAX_POISSON_LAMBDA {
        return Err(format!(
            "RandomPoisson: lambda={lambda} excede el máximo {MAX_POISSON_LAMBDA}"
        ));
    }
    if lambda <= MAX_POISSON_KNUTH_LAMBDA {
        let threshold = (-lambda).exp();
        let mut product = 1.0;
        let mut steps: usize = 0;
        loop {
            steps += 1;
            if steps > MAX_POISSON_KNUTH_STEPS {
                return Err("RandomPoisson: Knuth no convergió en la cota".to_string());
            }
            product *= next_unit(rng).max(f64::MIN_POSITIVE);
            if product <= threshold {
                break;
            }
        }
        u32::try_from(steps - 1).map_err(|_| "RandomPoisson: conteo no representable".to_string())
    } else {
        let draw = (lambda + lambda.sqrt() * box_muller_standard(rng)).round();
        if !draw.is_finite() || draw < 0.0 {
            return Err("RandomPoisson: aproximación no finita".to_string());
        }
        u32::try_from(draw as u64).map_err(|_| "RandomPoisson: conteo no representable".to_string())
    }
}

// ── Finanzas (convención de aula) ─────────────────────────────────────

/// Factor `(1 + tasa)^n` validado: tasa finita mayor que −1, `1 ≤ n ≤ cota`.
fn growth_factor(rate: f64, periods: u32) -> Result<f64, String> {
    if !rate.is_finite() {
        return Err("tasa no finita".to_string());
    }
    if rate <= -1.0 {
        return Err(format!("tasa={rate} inválida: debe ser mayor que -1"));
    }
    if periods == 0 || periods > MAX_FINANCE_PERIODS {
        return Err(format!(
            "períodos={periods} fuera de [1, {MAX_FINANCE_PERIODS}]"
        ));
    }
    let exponent = i32::try_from(periods).map_err(|_| "períodos no representables".to_string())?;
    let factor = (1.0 + rate).powi(exponent);
    if factor.is_finite() && factor > 0.0 {
        Ok(factor)
    } else {
        Err("factor de capitalización no representable".to_string())
    }
}

fn require_finite_amount(value: f64, cmd: &str, what: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{cmd}: {what} no finito"))
    }
}

/// `FutureValue[tasa, n, presente, cuota]`: valor futuro con pagos al final
/// de cada período. `FV = presente·(1+r)^n + cuota·(((1+r)^n − 1)/r)`;
/// con tasa 0 → `presente + cuota·n`.
///
/// La forma cerrada la implementa `cas_extra::finance_future_value`
/// (núcleo canónico compartido); acá solo quedan la validación de aula
/// (cota propia `MAX_FINANCE_PERIODS`, mensajes con prefijo `CMD`) y el
/// chequeo de resultado finito. El orden de args difiere de
/// [`crate::cas_extra::future_value`] (`(tasa, n, cuota, capital)`), así que
/// no se puede delegar la firma completa sin romper callers.
pub fn future_value(rate: f64, periods: u32, present: f64, payment: f64) -> Result<f64, String> {
    const CMD: &str = "FutureValue";
    require_finite_amount(present, CMD, "el capital")?;
    require_finite_amount(payment, CMD, "la cuota")?;
    let factor = growth_factor(rate, periods).map_err(|e| format!("{CMD}: {e}"))?;
    let value = crate::cas_extra::finance_future_value(factor, rate, periods, payment, present);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{CMD}: resultado no finito"))
    }
}

/// `PresentValue[tasa, n, futuro, cuota]`: inversa de `future_value`.
/// Con tasa 0 → `futuro − cuota·n`.
///
/// Como arriba: forma cerrada en `cas_extra::finance_present_value`,
/// validación y mensajes propios de este módulo.
pub fn present_value(rate: f64, periods: u32, future: f64, payment: f64) -> Result<f64, String> {
    const CMD: &str = "PresentValue";
    require_finite_amount(future, CMD, "el monto futuro")?;
    require_finite_amount(payment, CMD, "la cuota")?;
    let factor = growth_factor(rate, periods).map_err(|e| format!("{CMD}: {e}"))?;
    let value = crate::cas_extra::finance_present_value(factor, rate, periods, payment, future);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{CMD}: resultado no finito"))
    }
}

/// `Payment[tasa, n, presente]`: cuota de un préstamo (`presente` financiado).
/// `cuota = presente·r / (1 − (1+r)^−n)`; con tasa 0 → `presente / n`.
///
/// Equivale a `-cas_extra::finance_target_payment(factor, tasa,
/// n, presente, futuro = 0)` (convención de aula: cuota siempre ≥ 0, sin
/// signo de flujo). No se unifica con `cas_extra::payment` (4 args, con
/// signo) para no romper ninguna de las dos firmas.
pub fn payment(rate: f64, periods: u32, present: f64) -> Result<f64, String> {
    const CMD: &str = "Payment";
    require_finite_amount(present, CMD, "el capital")?;
    let factor = growth_factor(rate, periods).map_err(|e| format!("{CMD}: {e}"))?;
    let value = if rate == 0.0 {
        present / f64::from(periods)
    } else {
        let signed = crate::cas_extra::finance_target_payment(factor, rate, periods, present, 0.0)
            .ok_or_else(|| format!("{CMD}: denominador nulo o no finito"))?;
        -signed
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{CMD}: resultado no finito"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list_ops::DeterministicRng;

    fn rng(canonical: &str) -> DeterministicRng {
        DeterministicRng::seed_from_parts(3, canonical, &["stats_extra"])
    }

    #[test]
    fn population_variance_usa_divisor_n() {
        // Datos clásicos: media 5, suma de cuadrados 32 → poblacional 4.0.
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert!((population_variance(&data).unwrap() - 4.0).abs() < 1e-12);
        // La muestral existente da 32/7; la nueva difiere a propósito.
        let sample = crate::statistics::variance(&data).unwrap();
        assert!((sample - 32.0 / 7.0).abs() < 1e-12);
        assert_eq!(population_variance(&[7.0]).unwrap(), 0.0);
        assert!(population_variance(&[]).is_err());
        assert!(population_variance(&[1.0, f64::NAN]).is_err());
        assert!(population_variance(&[1.0, f64::INFINITY]).is_err());
    }

    #[test]
    fn histogram_right_cierra_a_derecha() {
        // Bordes [1, 2.5] y (2.5, 4]: el 2.5 cae en el primer bin.
        let hist = histogram_right(&[1.0, 2.0, 2.5, 3.0, 4.0], 2);
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].2, 3.0);
        assert_eq!(hist[1].2, 2.0);
        // El `histogram` viejo lo pone en el segundo (contraste documentado).
        let old = crate::statistics::histogram(&[1.0, 2.0, 2.5, 3.0, 4.0], 2);
        assert_eq!(old[1].2, 3.0);
        // Totales y casos borde.
        let total: f64 = hist.iter().map(|(_, _, c)| c).sum();
        assert_eq!(total, 5.0);
        assert!(histogram_right(&[], 4).is_empty());
        assert!(histogram_right(&[1.0], 0).is_empty());
        assert!(histogram_right(&[f64::NAN], 4).is_empty());
        assert_eq!(histogram_right(&[1.0, 2.0, 3.0], 4_097).len(), 4_096);
    }

    #[test]
    fn contingency_2d_delega_en_la_chi2_plana() {
        let (chi2, df, p) = contingency_table_2d(&[vec![10.0, 10.0], vec![10.0, 10.0]]).unwrap();
        assert!(chi2.abs() < 1e-9);
        assert_eq!(df, 1.0);
        assert!((p - 1.0).abs() < 1e-9);
        // Misma cuenta que la plana existente.
        let flat = crate::list_ops::contingency_chi2(&[10.0, 10.0, 10.0, 10.0], 2).unwrap();
        assert_eq!((chi2, df, p), flat);
        assert!(contingency_table_2d(&[vec![1.0, 2.0]]).is_err());
        assert!(contingency_table_2d(&[vec![1.0], vec![2.0]]).is_err());
        assert!(contingency_table_2d(&[vec![1.0, 2.0], vec![3.0]]).is_err());
        assert!(contingency_table_2d(&[vec![-1.0, 2.0], vec![3.0, 4.0]]).is_err());
        assert!(contingency_table_2d(&[vec![0.0, 0.0], vec![0.0, 0.0]]).is_err());
    }

    #[test]
    fn muestreo_reproduce_misma_semilla() {
        let mut a = rng("RandomUniform");
        let mut b = rng("RandomUniform");
        for _ in 0..8 {
            assert_eq!(
                random_uniform(&mut a, -2.0, 5.0).unwrap(),
                random_uniform(&mut b, -2.0, 5.0).unwrap()
            );
        }
        let mut a = rng("RandomNormal");
        let mut b = rng("RandomNormal");
        for _ in 0..8 {
            assert_eq!(
                random_normal(&mut a, 10.0, 2.0).unwrap(),
                random_normal(&mut b, 10.0, 2.0).unwrap()
            );
        }
        let mut a = rng("RandomBinomial");
        let mut b = rng("RandomBinomial");
        for _ in 0..8 {
            assert_eq!(
                random_binomial(&mut a, 20, 0.3).unwrap(),
                random_binomial(&mut b, 20, 0.3).unwrap()
            );
        }
        let mut a = rng("RandomPoisson");
        let mut b = rng("RandomPoisson");
        for _ in 0..8 {
            assert_eq!(
                random_poisson(&mut a, 4.0).unwrap(),
                random_poisson(&mut b, 4.0).unwrap()
            );
        }
    }

    #[test]
    fn muestreo_respeta_cotas_y_rangos() {
        let mut r = rng("ranges");
        for _ in 0..64 {
            let u = random_uniform(&mut r, -2.0, 5.0).unwrap();
            assert!((-2.0..5.0).contains(&u));
            assert!(random_normal(&mut r, 0.0, 1.0).unwrap().is_finite());
            assert!(random_binomial(&mut r, 20, 0.3).unwrap() <= 20);
        }
        assert!(random_uniform(&mut r, 5.0, 5.0).is_err());
        assert!(random_uniform(&mut r, 6.0, 5.0).is_err());
        assert!(random_uniform(&mut r, f64::NAN, 1.0).is_err());
        assert!(random_normal(&mut r, 0.0, 0.0).is_err());
        assert!(random_normal(&mut r, f64::INFINITY, 1.0).is_err());
        assert!(random_binomial(&mut r, 10, -0.1).is_err());
        assert!(random_binomial(&mut r, 10, 1.1).is_err());
        assert!(random_binomial(&mut r, MAX_RANDOM_TRIALS + 1, 0.5).is_err());
        assert_eq!(random_binomial(&mut r, 10, 0.0).unwrap(), 0);
        assert_eq!(random_binomial(&mut r, 10, 1.0).unwrap(), 10);
        assert!(random_poisson(&mut r, 0.0).is_err());
        assert!(random_poisson(&mut r, -2.0).is_err());
        assert!(random_poisson(&mut r, MAX_POISSON_LAMBDA + 1.0).is_err());
        // Rama normal (λ > 100) determinista y finita.
        let mut a = rng("poisson-grande");
        let mut b = rng("poisson-grande");
        assert_eq!(
            random_poisson(&mut a, 500.0).unwrap(),
            random_poisson(&mut b, 500.0).unwrap()
        );
    }

    #[test]
    fn finanzas_valores_de_referencia() {
        // 1000 al 5% diez años: 1000·1.05^10 ≈ 1628.8946.
        let fv = future_value(0.05, 10, 1000.0, 0.0).unwrap();
        assert!((fv - 1_628.894_626_777_442).abs() < 1e-6);
        // Cuota 100 por período: 100·((1.05^10 − 1)/0.05) ≈ 1257.789.
        let fv_cuotas = future_value(0.05, 10, 0.0, 100.0).unwrap();
        assert!((fv_cuotas - 1_257.789_253_554_884).abs() < 1e-6);
        // La inversa recupera el capital.
        let pv = present_value(0.05, 10, fv, 0.0).unwrap();
        assert!((pv - 1000.0).abs() < 1e-6);
        let pv_cuotas = present_value(0.05, 10, fv_cuotas, 100.0).unwrap();
        assert!(pv_cuotas.abs() < 1e-6);
        // Préstamo 1000 al 1% mensual a 12 meses: cuota ≈ 88.8488.
        let cuota = payment(0.01, 12, 1000.0).unwrap();
        assert!((cuota - 88.848_788_678_342).abs() < 1e-6);
        // Tasa cero: reparto lineal.
        assert_eq!(future_value(0.0, 10, 1000.0, 100.0).unwrap(), 2000.0);
        assert_eq!(present_value(0.0, 10, 2000.0, 100.0).unwrap(), 1000.0);
        assert_eq!(payment(0.0, 4, 1000.0).unwrap(), 250.0);
    }

    #[test]
    fn finanzas_rechaza_tasa_menos_uno_y_cotas() {
        assert!(future_value(-1.0, 10, 1000.0, 0.0).is_err());
        assert!(present_value(-2.0, 10, 1000.0, 0.0).is_err());
        assert!(payment(-1.0, 10, 1000.0).is_err());
        assert!(future_value(0.05, 0, 1000.0, 0.0).is_err());
        assert!(future_value(0.05, MAX_FINANCE_PERIODS + 1, 1000.0, 0.0).is_err());
        assert!(present_value(0.05, 10, f64::NAN, 0.0).is_err());
        assert!(payment(0.05, 10, f64::INFINITY).is_err());
        assert!(future_value(f64::NAN, 10, 1000.0, 0.0).is_err());
    }
}
