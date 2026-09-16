//! Frente P1 — listas de primera clase + estadística/probabilidad (GeoGebra).
//!
//! Motor puro (sin I/O, sin `Document`): opera sobre rebanadas `&[f64]`
//! resueltas por `resolve_list_arg` en `grafito-command` (literales `{…}`
//! o columnas `DataTable.xs|.ys`). Todo es determinista: el azar usa
//! [`DeterministicRng`] sembrado por `hash(document.version, canonical, args)`;
//! jamás `thread_rng` sin semilla.
//!
//! Alcance honesto de esta ola:
//! - Las listas que producen los comandos se devuelven como texto `{…}` o
//!   escalares (`CommandOutcome::Message`); `GeoObject::List` queda diferido
//!   (`grafito_core::ListObj` ya existe como tipo de datos, sin variante
//!   persistente todavía porque los matches exhaustivos de `document.rs` /
//!   `validation.rs` y la piel están fuera del alcance permitido).
//! - `Selections` se descartó: sin listas persistentes sería idéntico a
//!   `KeepIf` (el brief lo prohíbe expresamente).
//! - `ChiSquared` (estadística) se descartó: colisiona con la distribución
//!   `ChiSquared` existente y con `ChiSqTest`; el conteo final manda.
//! - `resolve_list_arg` NO se extendió (fuera del alcance permitido en
//!   `commands.rs`): las etiquetas de lista persistente quedan diferidas.
//! - `MAX_LIST_DEPTH`/`MAX_LIST_LENGTH` viven aquí porque `core::validation`
//!   está fuera del alcance permitido; `ListObj::validate` los referencia.

use crate::expr::evaluate;
use crate::statistics::{
    chi_squared_cdf, linear_regression, mean, normal_cdf, normal_quantile, pearson_correlation,
    quantile, std_dev, student_t_quantile,
};

/// Profundidad máxima de anidamiento de `ListItem` (GeoGebra-razonable).
pub const MAX_LIST_DEPTH: usize = 8;
/// Longitud máxima de una lista P1 (muy por debajo de `MAX_ARRAY_LENGTH` 200k).
pub const MAX_LIST_LENGTH: usize = 10_000;
/// Iteraciones máximas de la inversión numérica de CDF (Newton-bisección).
pub const MAX_CDF_INVERSION_ITERS: usize = 200;
/// Tolerancia de la inversión numérica de CDF.
pub const CDF_INVERSION_TOL: f64 = 1e-12;

/// Probabilidad válida en `(0, 1)` (forma positiva para guardas mínimas).
fn is_unit_prob(p: f64) -> bool {
    p.is_finite() && 0.0 < p && p < 1.0
}

/// RNG determinista xorshift64* con semilla FNV-1a.
///
/// Documentado a propósito en vez de `rand` (cero dependencias nuevas):
/// mismo `(version, canonical, args)` → misma secuencia, siempre.
#[derive(Debug, Clone)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    /// Siembra con `FNV-1a64(version ‖ canonical ‖ args)`. Nunca deja estado 0.
    pub fn seed_from_parts(version: u64, canonical: &str, args: &[&str]) -> Self {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        for byte in version
            .to_le_bytes()
            .iter()
            .chain(canonical.bytes().collect::<Vec<u8>>().iter())
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for arg in args {
            hash ^= 0xff;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in arg.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        if hash == 0 {
            hash = 0x9e37_79b9_7f4a_7c15;
        }
        Self { state: hash }
    }

    /// Siguiente `u64` (xorshift64*, periodo 2⁶⁴−1 sobre estados no nulos).
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Entero uniforme en `[0, n)`. `None` si `n == 0`.
    pub fn below(&mut self, n: usize) -> Option<usize> {
        if n == 0 {
            return None;
        }
        Some((self.next_u64() % n as u64) as usize)
    }

    /// Permutación Fisher-Yates in-situ.
    pub fn shuffle(&mut self, data: &mut [f64]) {
        let n = data.len();
        if n < 2 {
            return;
        }
        for i in (1..n).rev() {
            if let Some(j) = self.below(i + 1) {
                data.swap(i, j);
            }
        }
    }
}

fn require_finite_slice(data: &[f64], cmd: &str) -> Result<(), String> {
    if data.iter().any(|v| !v.is_finite()) {
        return Err(format!("{cmd}: la lista contiene valores no finitos"));
    }
    Ok(())
}

/// Valida longitud contra `MAX_LIST_LENGTH`.
pub fn validate_flat_len(len: usize, cmd: &str) -> Result<(), String> {
    if len > MAX_LIST_LENGTH {
        return Err(format!(
            "{cmd}: longitud {len} excede el máximo {MAX_LIST_LENGTH}"
        ));
    }
    Ok(())
}

/// Valida profundidad de anidamiento contra `MAX_LIST_DEPTH`.
pub fn check_depth(depth: usize, cmd: &str) -> Result<(), String> {
    if depth > MAX_LIST_DEPTH {
        return Err(format!(
            "{cmd}: profundidad {depth} excede el máximo {MAX_LIST_DEPTH}"
        ));
    }
    Ok(())
}

fn require_flat_len(data: &[f64], cmd: &str) -> Result<(), String> {
    validate_flat_len(data.len(), cmd)?;
    require_finite_slice(data, cmd)
}

/// `Element[lista, n]` (1-based, GeoGebra). Error si fuera de rango.
pub fn element(data: &[f64], n: i64) -> Result<f64, String> {
    require_flat_len(data, "Element")?;
    if n < 1 || n as usize > data.len() {
        return Err(format!(
            "Element: índice {n} fuera de rango [1, {}]",
            data.len()
        ));
    }
    let idx = n as usize - 1;
    data.get(idx)
        .copied()
        .ok_or_else(|| "Element: índice fuera de rango".to_string())
}

/// `Unique[lista]`: orden determinista (total) sin duplicados exactos.
pub fn unique_sorted(data: &[f64]) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Unique")?;
    let mut out = data.to_vec();
    out.sort_by(|a, b| a.total_cmp(b));
    out.dedup();
    Ok(out)
}

/// `Union[a, b]` como conjuntos, orden determinista.
pub fn union_sorted(a: &[f64], b: &[f64]) -> Result<Vec<f64>, String> {
    require_flat_len(a, "Union")?;
    require_flat_len(b, "Union")?;
    let mut out = Vec::with_capacity(a.len().saturating_add(b.len()));
    out.extend_from_slice(a);
    out.extend_from_slice(b);
    validate_flat_len(out.len(), "Union")?;
    out.sort_by(|x, y| x.total_cmp(y));
    out.dedup();
    Ok(out)
}

/// `Intersection[a, b]` como conjuntos, orden determinista.
pub fn intersection_sorted(a: &[f64], b: &[f64]) -> Result<Vec<f64>, String> {
    require_flat_len(a, "Intersection")?;
    require_flat_len(b, "Intersection")?;
    let mut sa = a.to_vec();
    let mut sb = b.to_vec();
    sa.sort_by(|x, y| x.total_cmp(y));
    sa.dedup();
    sb.sort_by(|x, y| x.total_cmp(y));
    sb.dedup();
    let mut out = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < sa.len() && j < sb.len() {
        match sa[i].total_cmp(&sb[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(sa[i]);
                i += 1;
                j += 1;
            }
        }
    }
    Ok(out)
}

/// `Insert[lista, pos, valor]` con posición 1-based (`1..=len+1`).
pub fn insert_at(data: &[f64], pos: i64, value: f64) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Insert")?;
    if !value.is_finite() {
        return Err("Insert: valor no finito".to_string());
    }
    if pos < 1 || pos as usize > data.len().saturating_add(1) {
        return Err(format!(
            "Insert: posición {pos} fuera de rango [1, {}]",
            data.len() + 1
        ));
    }
    validate_flat_len(data.len().saturating_add(1), "Insert")?;
    let mut out = Vec::with_capacity(data.len().saturating_add(1));
    let idx = pos as usize - 1;
    out.extend_from_slice(data.get(..idx).unwrap_or(&[]));
    out.push(value);
    out.extend_from_slice(data.get(idx..).unwrap_or(&[]));
    Ok(out)
}

/// `Remove[lista, pos]` con posición 1-based.
pub fn remove_at(data: &[f64], pos: i64) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Remove")?;
    if data.is_empty() {
        return Err("Remove: lista vacía".to_string());
    }
    if pos < 1 || pos as usize > data.len() {
        return Err(format!(
            "Remove: posición {pos} fuera de rango [1, {}]",
            data.len()
        ));
    }
    let mut out = Vec::with_capacity(data.len().saturating_sub(1));
    let idx = pos as usize - 1;
    out.extend_from_slice(data.get(..idx).unwrap_or(&[]));
    out.extend_from_slice(data.get(idx.saturating_add(1)..).unwrap_or(&[]));
    Ok(out)
}

/// `IndexOf[lista, valor]`: primera posición 1-based (igualdad exacta).
pub fn index_of(data: &[f64], value: f64) -> Result<usize, String> {
    require_flat_len(data, "IndexOf")?;
    if !value.is_finite() {
        return Err("IndexOf: valor no finito".to_string());
    }
    for (i, v) in data.iter().enumerate() {
        if *v == value {
            return Ok(i.saturating_add(1));
        }
    }
    Err(format!("IndexOf: {value} no está en la lista"))
}

fn check_var_name(var: &str, cmd: &str) -> Result<(), String> {
    let ok = !var.is_empty()
        && var.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && var
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!("{cmd}: variable '{var}' inválida"))
    }
}

/// `IterationList[f, var, semilla, n]`: itera `var ← f(var)` desde `semilla`.
pub fn iteration_list(
    expr: &str,
    var: &str,
    seed: f64,
    n: i64,
    vars: &[(String, f64)],
) -> Result<Vec<f64>, String> {
    check_var_name(var, "IterationList")?;
    if !seed.is_finite() {
        return Err("IterationList: semilla no finita".to_string());
    }
    if n < 1 || n as usize > MAX_LIST_LENGTH {
        return Err(format!(
            "IterationList: n={n} fuera de rango [1, {MAX_LIST_LENGTH}]"
        ));
    }
    let mut out = Vec::with_capacity(n as usize);
    let mut current = seed;
    for _ in 0..n {
        let mut scope: Vec<(String, f64)> = vars.to_vec();
        scope.push((var.to_string(), current));
        let next = evaluate(expr, &scope)
            .map_err(|e| format!("IterationList: no se pudo evaluar '{expr}': {e}"))?;
        if !next.is_finite() {
            return Err("IterationList: la iteración produjo un valor no finito".to_string());
        }
        out.push(next);
        current = next;
    }
    Ok(out)
}

/// `Map[f, lista]`: aplica la expresión con `x` ligada a cada elemento.
pub fn map_list(expr: &str, data: &[f64], vars: &[(String, f64)]) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Map")?;
    let mut out = Vec::with_capacity(data.len());
    for v in data {
        let mut scope: Vec<(String, f64)> = vars.to_vec();
        scope.push(("x".to_string(), *v));
        let y = evaluate(expr, &scope)
            .map_err(|e| format!("Map: no se pudo evaluar '{expr}' con x={v}: {e}"))?;
        if !y.is_finite() {
            return Err(format!("Map: resultado no finito para x={v}"));
        }
        out.push(y);
    }
    Ok(out)
}

/// `Shuffle[lista]` determinista con el RNG dado.
pub fn shuffle_with_seed(data: &[f64], rng: &mut DeterministicRng) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Shuffle")?;
    let mut out = data.to_vec();
    rng.shuffle(&mut out);
    Ok(out)
}

/// `Sample[lista, k]` sin reposición, determinista. `0 ≤ k ≤ len`.
pub fn sample_with_seed(
    data: &[f64],
    k: i64,
    rng: &mut DeterministicRng,
) -> Result<Vec<f64>, String> {
    require_flat_len(data, "Sample")?;
    if k < 0 || k as usize > data.len() {
        return Err(format!("Sample: k={k} fuera de rango [0, {}]", data.len()));
    }
    let mut idx: Vec<usize> = (0..data.len()).collect();
    for i in 0..k as usize {
        let j = i.saturating_add(rng.below(data.len().saturating_sub(i)).unwrap_or(0));
        idx.swap(i, j);
    }
    Ok(idx
        .iter()
        .take(k as usize)
        .filter_map(|i| data.get(*i).copied())
        .collect())
}

/// `RandomElement[lista]`: un elemento uniforme, determinista.
pub fn random_element_with_seed(data: &[f64], rng: &mut DeterministicRng) -> Result<f64, String> {
    require_flat_len(data, "RandomElement")?;
    if data.is_empty() {
        return Err("RandomElement: lista vacía".to_string());
    }
    let i = rng.below(data.len()).unwrap_or(0);
    data.get(i)
        .copied()
        .ok_or_else(|| "RandomElement: índice fuera de rango".to_string())
}

/// `RandomDiscrete[min, max]`: entero uniforme inclusivo, determinista.
pub fn random_discrete_range(lo: f64, hi: f64, rng: &mut DeterministicRng) -> Result<f64, String> {
    if !lo.is_finite() || !hi.is_finite() {
        return Err("RandomDiscrete: extremos no finitos".to_string());
    }
    if lo.fract() != 0.0 || hi.fract() != 0.0 {
        return Err("RandomDiscrete: los extremos deben ser enteros".to_string());
    }
    if lo > hi {
        return Err(format!("RandomDiscrete: min {lo} > max {hi}"));
    }
    let span = hi - lo + 1.0;
    if !(1.0..=4_294_967_295.0).contains(&span) {
        return Err("RandomDiscrete: rango excesivo".to_string());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let draw = lo + (rng.next_u64() % span as u64) as f64;
    Ok(draw)
}

/// `Min[lista]` / `Max[lista]` / `Sum[lista]` / `Product[lista]`.
pub fn list_min(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "Min")?;
    if data.is_empty() {
        return Err("Min: lista vacía".to_string());
    }
    Ok(data.iter().copied().fold(f64::INFINITY, f64::min))
}

/// `Max[lista]`.
pub fn list_max(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "Max")?;
    if data.is_empty() {
        return Err("Max: lista vacía".to_string());
    }
    Ok(data.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}

/// `Sum[lista]`.
pub fn list_sum(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "Sum")?;
    if data.is_empty() {
        return Err("Sum: lista vacía".to_string());
    }
    let s: f64 = data.iter().sum();
    if s.is_finite() {
        Ok(s)
    } else {
        Err("Sum: resultado no finito".to_string())
    }
}

/// `Product[lista]`.
pub fn list_product(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "Product")?;
    if data.is_empty() {
        return Err("Product: lista vacía".to_string());
    }
    let mut p = 1.0;
    for v in data {
        p *= *v;
        if !p.is_finite() {
            return Err("Product: resultado no finito".to_string());
        }
    }
    Ok(p)
}

/// Formatea escalares como `{a, b, c}` (texto que devuelven los comandos P1).
pub fn format_scalars(data: &[f64]) -> String {
    if data.is_empty() {
        return "{}".to_string();
    }
    let inner: Vec<String> = data.iter().map(|v| format!("{v}")).collect();
    format!("{{{}}}", inner.join(", "))
}

// ── Estadística descriptiva P1 ──────────────────────────────────────────

/// `TiedRank[lista]`: rangos promedio 1-based (empates promediados).
pub fn tied_rank(data: &[f64]) -> Result<Vec<f64>, String> {
    require_flat_len(data, "TiedRank")?;
    if data.is_empty() {
        return Err("TiedRank: lista vacía".to_string());
    }
    let mut order: Vec<usize> = (0..data.len()).collect();
    order.sort_by(|a, b| {
        data.get(*a)
            .unwrap_or(&f64::NAN)
            .total_cmp(data.get(*b).unwrap_or(&f64::NAN))
    });
    let mut ranks = vec![0.0; data.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len()
            && data.get(order[j + 1]).copied().unwrap_or(f64::NAN)
                == data.get(order[i]).copied().unwrap_or(f64::NAN)
        {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            if let Some(slot) = order.get(k).and_then(|o| ranks.get_mut(*o)) {
                *slot = avg;
            }
        }
        i = j.saturating_add(1);
    }
    Ok(ranks)
}

/// `OrdinalRank[lista]`: rangos 1..n (empates por orden de aparición).
pub fn ordinal_rank(data: &[f64]) -> Result<Vec<f64>, String> {
    require_flat_len(data, "OrdinalRank")?;
    if data.is_empty() {
        return Err("OrdinalRank: lista vacía".to_string());
    }
    let mut order: Vec<usize> = (0..data.len()).collect();
    order.sort_by(|a, b| {
        data.get(*a)
            .unwrap_or(&f64::NAN)
            .total_cmp(data.get(*b).unwrap_or(&f64::NAN))
            .then_with(|| a.cmp(b))
    });
    let mut ranks = vec![0.0; data.len()];
    for (rank, pos) in order.iter().enumerate() {
        if let Some(slot) = ranks.get_mut(*pos) {
            *slot = rank as f64 + 1.0;
        }
    }
    Ok(ranks)
}

/// `Spearman[xs, ys]`: Pearson sobre rangos promedio.
pub fn spearman(xs: &[f64], ys: &[f64]) -> Result<f64, String> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return Err("Spearman: se requieren dos listas del mismo largo (≥2)".to_string());
    }
    validate_flat_len(xs.len(), "Spearman")?;
    let rx = tied_rank(xs)?;
    let ry = tied_rank(ys)?;
    pearson_correlation(&rx, &ry)
        .filter(|v| v.is_finite())
        .ok_or_else(|| "Spearman: no computable (varianza nula?)".to_string())
}

/// `MAD[lista]`: mediana de |x − mediana|.
pub fn mad(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "MAD")?;
    let m = mean(data).ok_or_else(|| "MAD: lista vacía o no finita".to_string())?;
    let _ = m;
    let med = crate::statistics::median(data)
        .ok_or_else(|| "MAD: lista vacía o no finita".to_string())?;
    let mut dev: Vec<f64> = data.iter().map(|v| (v - med).abs()).collect();
    dev.sort_by(|a, b| a.total_cmp(b));
    let n = dev.len();
    if n.is_multiple_of(2) {
        Ok((dev[n / 2 - 1] + dev[n / 2]) / 2.0)
    } else {
        dev.get(n / 2)
            .copied()
            .ok_or_else(|| "MAD: lista vacía".to_string())
    }
}

/// `Percentile[lista, p]` con `p` en `[0, 100]` (interpola como `quantile`).
pub fn percentile(data: &[f64], p: f64) -> Result<f64, String> {
    require_flat_len(data, "Percentile")?;
    if !p.is_finite() || !(0.0..=100.0).contains(&p) {
        return Err(format!("Percentile: p={p} fuera de [0, 100]"));
    }
    quantile(data, p / 100.0).ok_or_else(|| "Percentile: lista vacía".to_string())
}

/// Desvío poblacional (÷n). `SampleSD*` usa `statistics::std_dev` (÷n−1).
pub fn population_sd(data: &[f64], cmd: &str) -> Result<f64, String> {
    require_flat_len(data, cmd)?;
    let m = mean(data).ok_or_else(|| format!("{cmd}: lista vacía o no finita"))?;
    if data.len() < 2 {
        return Err(format!("{cmd}: se requieren al menos 2 datos"));
    }
    let sxx: f64 = data.iter().map(|v| (v - m).powi(2)).sum();
    Ok((sxx / data.len() as f64).sqrt())
}

/// `MeanX[xs]` o `MeanX[xs, ys]` (media de abscisas).
pub fn mean_of(data: &[f64], cmd: &str) -> Result<f64, String> {
    require_flat_len(data, cmd)?;
    mean(data).ok_or_else(|| format!("{cmd}: lista vacía o no finita"))
}

/// `SigmaXX[xs]` = Σx².
pub fn sigma_xx(xs: &[f64]) -> Result<f64, String> {
    require_flat_len(xs, "SigmaXX")?;
    let s: f64 = xs.iter().map(|v| v * v).sum();
    if s.is_finite() {
        Ok(s)
    } else {
        Err("SigmaXX: resultado no finito".to_string())
    }
}

/// `SigmaXY[xs, ys]` = Σxy.
pub fn sigma_xy(xs: &[f64], ys: &[f64]) -> Result<f64, String> {
    if xs.len() != ys.len() {
        return Err("SigmaXY: largos distintos".to_string());
    }
    require_flat_len(xs, "SigmaXY")?;
    let s: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| x * y).sum();
    if s.is_finite() {
        Ok(s)
    } else {
        Err("SigmaXY: resultado no finito".to_string())
    }
}

/// `SigmaYY[ys]` = Σy².
pub fn sigma_yy(ys: &[f64]) -> Result<f64, String> {
    sigma_xx(ys).map_err(|_| "SigmaYY: resultado no finito".to_string())
}

/// `Sxx[xs]` = Σ(x−x̄)².
pub fn sxx(xs: &[f64]) -> Result<f64, String> {
    require_flat_len(xs, "Sxx")?;
    let m = mean(xs).ok_or_else(|| "Sxx: lista vacía o no finita".to_string())?;
    let s: f64 = xs.iter().map(|v| (v - m).powi(2)).sum();
    if s.is_finite() {
        Ok(s)
    } else {
        Err("Sxx: resultado no finito".to_string())
    }
}

/// `Sxy[xs, ys]` = Σ(x−x̄)(y−ȳ).
pub fn sxy(xs: &[f64], ys: &[f64]) -> Result<f64, String> {
    if xs.len() != ys.len() || xs.is_empty() {
        return Err("Sxy: se requieren dos listas del mismo largo (≥1)".to_string());
    }
    require_flat_len(xs, "Sxy")?;
    let mx = mean(xs).ok_or_else(|| "Sxy: datos no finitos".to_string())?;
    let my = mean(ys).ok_or_else(|| "Sxy: datos no finitos".to_string())?;
    let s: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| (x - mx) * (y - my))
        .sum();
    if s.is_finite() {
        Ok(s)
    } else {
        Err("Sxy: resultado no finito".to_string())
    }
}

/// `Syy[ys]` = Σ(y−ȳ)².
pub fn syy(ys: &[f64]) -> Result<f64, String> {
    sxx(ys).map_err(|_| "Syy: lista vacía o no finita".to_string())
}

/// `GeometricMean[lista]` (exige x > 0).
pub fn geometric_mean(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "GeometricMean")?;
    if data.is_empty() {
        return Err("GeometricMean: lista vacía".to_string());
    }
    if data.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err("GeometricMean: exige valores finitos > 0".to_string());
    }
    let log_sum: f64 = data.iter().map(|v| v.ln()).sum();
    Ok((log_sum / data.len() as f64).exp())
}

/// `HarmonicMean[lista]` (exige x ≠ 0).
pub fn harmonic_mean(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "HarmonicMean")?;
    if data.is_empty() {
        return Err("HarmonicMean: lista vacía".to_string());
    }
    if data.iter().any(|v| !v.is_finite() || *v == 0.0) {
        return Err("HarmonicMean: exige valores finitos ≠ 0".to_string());
    }
    let inv_sum: f64 = data.iter().map(|v| 1.0 / v).sum();
    if inv_sum == 0.0 || !inv_sum.is_finite() {
        return Err("HarmonicMean: no computable".to_string());
    }
    Ok(data.len() as f64 / inv_sum)
}

/// `RootMeanSquare[lista]` = √(Σx²/n).
pub fn root_mean_square(data: &[f64]) -> Result<f64, String> {
    require_flat_len(data, "RootMeanSquare")?;
    if data.is_empty() {
        return Err("RootMeanSquare: lista vacía".to_string());
    }
    let s: f64 = data.iter().map(|v| v * v).sum();
    if !s.is_finite() {
        return Err("RootMeanSquare: resultado no finito".to_string());
    }
    Ok((s / data.len() as f64).sqrt())
}

/// `SumSquaredErrors[lista]` = Σ(x−x̄)² (igual que `Sxx`).
pub fn sum_squared_errors(data: &[f64]) -> Result<f64, String> {
    sxx(data).map_err(|e| e.replace("Sxx", "SumSquaredErrors"))
}

/// `RSquare[xs, ys]`: R² de la regresión lineal.
pub fn rsquare(xs: &[f64], ys: &[f64]) -> Result<f64, String> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return Err("RSquare: se requieren dos listas del mismo largo (≥2)".to_string());
    }
    linear_regression(xs, ys)
        .map(|(_, _, r2)| r2)
        .filter(|v| v.is_finite())
        .ok_or_else(|| "RSquare: no computable (x constante?)".to_string())
}

// ── Intervalos y pruebas Z/T ────────────────────────────────────────────

fn z_crit(conf: f64, cmd: &str) -> Result<f64, String> {
    if !is_unit_prob(conf) {
        return Err(format!("{cmd}: confianza debe estar en (0,1)"));
    }
    Ok(normal_quantile(1.0 - (1.0 - conf) / 2.0, 0.0, 1.0))
}

/// `(inferior, centro, superior)` para la media con σ conocida.
pub fn z_mean_estimate(data: &[f64], sigma: f64, conf: f64) -> Result<(f64, f64, f64), String> {
    require_flat_len(data, "ZMeanEstimate")?;
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err("ZMeanEstimate: sigma debe ser finito > 0".to_string());
    }
    let m = mean(data).ok_or_else(|| "ZMeanEstimate: lista vacía o no finita".to_string())?;
    let z = z_crit(conf, "ZMeanEstimate")?;
    let margin = z * sigma / (data.len() as f64).sqrt();
    Ok((m - margin, m, m + margin))
}

/// Intervalo Z para la diferencia de medias (σ₁, σ₂ conocidas).
pub fn z_mean_2_estimate(
    d1: &[f64],
    s1: f64,
    d2: &[f64],
    s2: f64,
    conf: f64,
) -> Result<(f64, f64, f64), String> {
    require_flat_len(d1, "ZMean2Estimate")?;
    require_flat_len(d2, "ZMean2Estimate")?;
    if !s1.is_finite() || s1 <= 0.0 || !s2.is_finite() || s2 <= 0.0 {
        return Err("ZMean2Estimate: sigmas finitos > 0 requeridos".to_string());
    }
    let m1 = mean(d1).ok_or_else(|| "ZMean2Estimate: lista1 vacía o no finita".to_string())?;
    let m2 = mean(d2).ok_or_else(|| "ZMean2Estimate: lista2 vacía o no finita".to_string())?;
    let z = z_crit(conf, "ZMean2Estimate")?;
    let se = ((s1 * s1) / d1.len() as f64 + (s2 * s2) / d2.len() as f64).sqrt();
    Ok((m1 - m2 - z * se, m1 - m2, m1 - m2 + z * se))
}

/// Prueba Z bilateral para la media: retorna `(z, p)`.
pub fn z_mean_test(data: &[f64], mu0: f64, sigma: f64) -> Result<(f64, f64), String> {
    require_flat_len(data, "ZMeanTest")?;
    if !mu0.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
        return Err("ZMeanTest: mu0 finito y sigma > 0 requeridos".to_string());
    }
    let m = mean(data).ok_or_else(|| "ZMeanTest: lista vacía o no finita".to_string())?;
    let z = (m - mu0) / (sigma / (data.len() as f64).sqrt());
    let p = 2.0 * (1.0 - normal_cdf(z.abs(), 0.0, 1.0));
    Ok((z, p.clamp(0.0, 1.0)))
}

/// Prueba Z bilateral para μ₁−μ₂ con σ conocidas: retorna `(z, p)`.
pub fn z_mean_2_test(d1: &[f64], s1: f64, d2: &[f64], s2: f64) -> Result<(f64, f64), String> {
    require_flat_len(d1, "ZMean2Test")?;
    require_flat_len(d2, "ZMean2Test")?;
    if !s1.is_finite() || s1 <= 0.0 || !s2.is_finite() || s2 <= 0.0 {
        return Err("ZMean2Test: sigmas finitos > 0 requeridos".to_string());
    }
    let m1 = mean(d1).ok_or_else(|| "ZMean2Test: lista1 vacía o no finita".to_string())?;
    let m2 = mean(d2).ok_or_else(|| "ZMean2Test: lista2 vacía o no finita".to_string())?;
    let se = ((s1 * s1) / d1.len() as f64 + (s2 * s2) / d2.len() as f64).sqrt();
    if se == 0.0 {
        return Err("ZMean2Test: error estándar nulo".to_string());
    }
    let z = (m1 - m2) / se;
    let p = 2.0 * (1.0 - normal_cdf(z.abs(), 0.0, 1.0));
    Ok((z, p.clamp(0.0, 1.0)))
}

/// Intervalo Z (Wald) para una proporción: `(inferior, p̂, superior)`.
pub fn z_proportion_estimate(x: f64, n: f64, conf: f64) -> Result<(f64, f64, f64), String> {
    if !x.is_finite() || !n.is_finite() || x.fract() != 0.0 || n.fract() != 0.0 {
        return Err("ZProportionEstimate: éxitos y n deben ser enteros".to_string());
    }
    if n < 1.0 || x < 0.0 || x > n {
        return Err("ZProportionEstimate: 0 ≤ éxitos ≤ n, n ≥ 1".to_string());
    }
    let p = x / n;
    let z = z_crit(conf, "ZProportionEstimate")?;
    let margin = z * (p * (1.0 - p) / n).sqrt();
    Ok(((p - margin).max(0.0), p, (p + margin).min(1.0)))
}

/// Intervalo Z para p₁−p₂ (no agrupado).
pub fn z_proportion_2_estimate(
    x1: f64,
    n1: f64,
    x2: f64,
    n2: f64,
    conf: f64,
) -> Result<(f64, f64, f64), String> {
    for (x, n) in [(x1, n1), (x2, n2)] {
        if !x.is_finite() || !n.is_finite() || x.fract() != 0.0 || n.fract() != 0.0 {
            return Err("ZProportion2Estimate: éxitos y n enteros".to_string());
        }
        if n < 1.0 || x < 0.0 || x > n {
            return Err("ZProportion2Estimate: 0 ≤ éxitos ≤ n, n ≥ 1".to_string());
        }
    }
    let p1 = x1 / n1;
    let p2 = x2 / n2;
    let z = z_crit(conf, "ZProportion2Estimate")?;
    let se = (p1 * (1.0 - p1) / n1 + p2 * (1.0 - p2) / n2).sqrt();
    let d = p1 - p2;
    Ok(((d - z * se).max(-1.0), d, (d + z * se).min(1.0)))
}

/// Prueba Z bilateral para una proporción contra p₀: retorna `(z, p)`.
pub fn z_proportion_test(x: f64, n: f64, p0: f64) -> Result<(f64, f64), String> {
    if !x.is_finite() || !n.is_finite() || x.fract() != 0.0 || n.fract() != 0.0 {
        return Err("ZProportionTest: éxitos y n deben ser enteros".to_string());
    }
    if n < 1.0 || x < 0.0 || x > n {
        return Err("ZProportionTest: 0 ≤ éxitos ≤ n, n ≥ 1".to_string());
    }
    if !is_unit_prob(p0) {
        return Err("ZProportionTest: p0 debe estar en (0,1)".to_string());
    }
    let se = (p0 * (1.0 - p0) / n).sqrt();
    if se == 0.0 {
        return Err("ZProportionTest: error estándar nulo".to_string());
    }
    let z = (x / n - p0) / se;
    let p = 2.0 * (1.0 - normal_cdf(z.abs(), 0.0, 1.0));
    Ok((z, p.clamp(0.0, 1.0)))
}

/// Prueba Z bilateral para p₁−p₂ (agrupada): retorna `(z, p)`.
pub fn z_proportion_2_test(x1: f64, n1: f64, x2: f64, n2: f64) -> Result<(f64, f64), String> {
    for (x, n) in [(x1, n1), (x2, n2)] {
        if !x.is_finite() || !n.is_finite() || x.fract() != 0.0 || n.fract() != 0.0 {
            return Err("ZProportion2Test: éxitos y n enteros".to_string());
        }
        if n < 1.0 || x < 0.0 || x > n {
            return Err("ZProportion2Test: 0 ≤ éxitos ≤ n, n ≥ 1".to_string());
        }
    }
    let pooled = (x1 + x2) / (n1 + n2);
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 + 1.0 / n2)).sqrt();
    if se == 0.0 {
        return Err("ZProportion2Test: error estándar nulo".to_string());
    }
    let z = (x1 / n1 - x2 / n2) / se;
    let p = 2.0 * (1.0 - normal_cdf(z.abs(), 0.0, 1.0));
    Ok((z, p.clamp(0.0, 1.0)))
}

/// Intervalo t para la media (σ desconocida).
pub fn t_mean_estimate(data: &[f64], conf: f64) -> Result<(f64, f64, f64), String> {
    require_flat_len(data, "TMeanEstimate")?;
    if data.len() < 2 {
        return Err("TMeanEstimate: se requieren al menos 2 datos".to_string());
    }
    if !is_unit_prob(conf) {
        return Err("TMeanEstimate: confianza debe estar en (0,1)".to_string());
    }
    let m = mean(data).ok_or_else(|| "TMeanEstimate: datos no finitos".to_string())?;
    let s = std_dev(data).ok_or_else(|| "TMeanEstimate: datos no finitos".to_string())?;
    let df = data.len() as f64 - 1.0;
    let t = student_t_quantile(1.0 - (1.0 - conf) / 2.0, df);
    if !t.is_finite() {
        return Err("TMeanEstimate: cuantil t no computable".to_string());
    }
    let margin = t * s / (data.len() as f64).sqrt();
    Ok((m - margin, m, m + margin))
}

/// Intervalo t de Welch para μ₁−μ₂ (varianzas desconocidas).
pub fn t_mean_2_estimate(d1: &[f64], d2: &[f64], conf: f64) -> Result<(f64, f64, f64), String> {
    require_flat_len(d1, "TMean2Estimate")?;
    require_flat_len(d2, "TMean2Estimate")?;
    if d1.len() < 2 || d2.len() < 2 {
        return Err("TMean2Estimate: cada muestra necesita ≥2 datos".to_string());
    }
    if !is_unit_prob(conf) {
        return Err("TMean2Estimate: confianza debe estar en (0,1)".to_string());
    }
    let m1 = mean(d1).ok_or_else(|| "TMean2Estimate: muestra1 no finita".to_string())?;
    let m2 = mean(d2).ok_or_else(|| "TMean2Estimate: muestra2 no finita".to_string())?;
    let s1 = std_dev(d1).ok_or_else(|| "TMean2Estimate: muestra1 no finita".to_string())?;
    let s2 = std_dev(d2).ok_or_else(|| "TMean2Estimate: muestra2 no finita".to_string())?;
    let n1 = d1.len() as f64;
    let n2 = d2.len() as f64;
    let v1 = s1 * s1 / n1;
    let v2 = s2 * s2 / n2;
    if v1 + v2 == 0.0 {
        return Err("TMean2Estimate: varianza nula".to_string());
    }
    let df = (v1 + v2).powi(2) / (v1 * v1 / (n1 - 1.0) + v2 * v2 / (n2 - 1.0));
    let t = student_t_quantile(1.0 - (1.0 - conf) / 2.0, df);
    if !t.is_finite() {
        return Err("TMean2Estimate: cuantil t no computable".to_string());
    }
    let d = m1 - m2;
    let margin = t * (v1 + v2).sqrt();
    Ok((d - margin, d, d + margin))
}

// ── Clases, DotPlot, polígono de frecuencias, contingencia ────────────────

/// Fronteras de clase para `Class`/`Classes`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassBreaks {
    /// Cantidad de clases.
    pub classes: usize,
    /// Ancho de clase.
    pub width: f64,
    /// `classes + 1` fronteras.
    pub boundaries: Vec<f64>,
}

/// Calcula `k` clases de igual ancho sobre `[min, max]`.
pub fn class_breaks(data: &[f64], k: i64) -> Result<ClassBreaks, String> {
    require_flat_len(data, "Classes")?;
    if data.is_empty() {
        return Err("Classes: lista vacía".to_string());
    }
    if !(1..=1024).contains(&k) {
        return Err(format!("Classes: k={k} fuera de [1, 1024]"));
    }
    let k = k as usize;
    let lo = data.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let width = if hi > lo { (hi - lo) / k as f64 } else { 0.0 };
    let mut boundaries = Vec::with_capacity(k.saturating_add(1));
    for i in 0..=k {
        boundaries.push(lo + width * i as f64);
    }
    Ok(ClassBreaks {
        classes: k,
        width,
        boundaries,
    })
}

/// `DotPlot`: puntos `(x, altura)` con altura = ocurrencia acumulada exacta.
pub fn dot_plot_points(data: &[f64]) -> Result<(Vec<f64>, Vec<f64>), String> {
    require_flat_len(data, "DotPlot")?;
    if data.is_empty() {
        return Err("DotPlot: lista vacía".to_string());
    }
    let mut xs = Vec::with_capacity(data.len());
    let mut ys = Vec::with_capacity(data.len());
    for (i, v) in data.iter().enumerate() {
        let mut height = 1.0;
        for w in data.get(..i).unwrap_or(&[]) {
            if *w == *v {
                height += 1.0;
            }
        }
        xs.push(*v);
        ys.push(height);
    }
    Ok((xs, ys))
}

/// `FrequencyPolygon`: puntos medios de clase + frecuencias absolutas.
pub fn frequency_polygon_points(data: &[f64], k: i64) -> Result<(Vec<f64>, Vec<f64>), String> {
    let breaks = class_breaks(data, k).map_err(|e| e.replace("Classes", "FrequencyPolygon"))?;
    let mut freqs = vec![0.0; breaks.classes];
    let (lo, width) = (
        breaks.boundaries.first().copied().unwrap_or(0.0),
        breaks.width,
    );
    for v in data {
        let mut bin = if width > 0.0 {
            ((v - lo) / width).floor() as usize
        } else {
            0
        };
        if bin >= breaks.classes {
            bin = breaks.classes.saturating_sub(1);
        }
        if let Some(slot) = freqs.get_mut(bin) {
            *slot += 1.0;
        }
    }
    let mut mids = Vec::with_capacity(breaks.classes);
    for i in 0..breaks.classes {
        let a = breaks.boundaries.get(i).copied().unwrap_or(0.0);
        let b = breaks.boundaries.get(i + 1).copied().unwrap_or(a);
        mids.push((a + b) / 2.0);
    }
    Ok((mids, freqs))
}

/// χ² de una tabla de contingencia plana (`filas × ncols`): `(χ², gl, p)`.
pub fn contingency_chi2(obs: &[f64], ncols: usize) -> Result<(f64, f64, f64), String> {
    require_flat_len(obs, "ContingencyTable")?;
    if ncols < 2 || obs.len() < 4 || !obs.len().is_multiple_of(ncols) {
        return Err(
            "ContingencyTable: se requiere matriz ≥2×2 (largo múltiplo de ncols)".to_string(),
        );
    }
    if obs.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("ContingencyTable: frecuencias finitas ≥ 0".to_string());
    }
    let nrows = obs.len() / ncols;
    if nrows < 2 {
        return Err("ContingencyTable: se requieren ≥2 filas".to_string());
    }
    let total: f64 = obs.iter().sum();
    if total <= 0.0 {
        return Err("ContingencyTable: total nulo".to_string());
    }
    let mut chi2 = 0.0;
    for r in 0..nrows {
        let row_sum: f64 = (0..ncols).filter_map(|c| obs.get(r * ncols + c)).sum();
        for c in 0..ncols {
            let col_sum: f64 = (0..nrows).filter_map(|rr| obs.get(rr * ncols + c)).sum();
            let expected = row_sum * col_sum / total;
            let observed = obs.get(r * ncols + c).copied().unwrap_or(0.0);
            if expected <= 0.0 {
                if observed != 0.0 {
                    return Err(
                        "ContingencyTable: frecuencia esperada nula con observada > 0".to_string(),
                    );
                }
                continue;
            }
            chi2 += (observed - expected).powi(2) / expected;
        }
    }
    let df = ((nrows - 1) * (ncols - 1)) as f64;
    let p = 1.0 - chi_squared_cdf(chi2, df);
    Ok((chi2, df, p.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng() -> DeterministicRng {
        DeterministicRng::seed_from_parts(1, "Test", &["{1,2,3}"])
    }

    #[test]
    fn rng_is_deterministic_for_same_seed() {
        let mut a = DeterministicRng::seed_from_parts(7, "Shuffle", &["{3,1,2}"]);
        let mut b = DeterministicRng::seed_from_parts(7, "Shuffle", &["{3,1,2}"]);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn rng_differs_across_commands() {
        let mut a = DeterministicRng::seed_from_parts(7, "Shuffle", &["{3,1,2}"]);
        let mut b = DeterministicRng::seed_from_parts(7, "Sample", &["{3,1,2}"]);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn element_is_one_based() {
        assert_eq!(element(&[10.0, 20.0, 30.0], 1), Ok(10.0));
        assert_eq!(element(&[10.0, 20.0, 30.0], 3), Ok(30.0));
        assert!(element(&[10.0], 0).is_err());
        assert!(element(&[10.0], 2).is_err());
        assert!(element(&[], 1).is_err());
    }

    #[test]
    fn unique_sorts_and_dedups() {
        assert_eq!(
            unique_sorted(&[3.0, 1.0, 2.0, 1.0]),
            Ok(vec![1.0, 2.0, 3.0])
        );
        assert!(unique_sorted(&[1.0, f64::NAN]).is_err());
    }

    #[test]
    fn union_and_intersection_behave_as_sets() {
        assert_eq!(
            union_sorted(&[3.0, 1.0], &[2.0, 3.0]),
            Ok(vec![1.0, 2.0, 3.0])
        );
        assert_eq!(intersection_sorted(&[3.0, 1.0], &[2.0, 3.0]), Ok(vec![3.0]));
        assert_eq!(intersection_sorted(&[1.0], &[2.0]), Ok(Vec::<f64>::new()));
    }

    #[test]
    fn insert_and_remove_use_one_based_positions() {
        assert_eq!(insert_at(&[1.0, 3.0], 2, 2.0), Ok(vec![1.0, 2.0, 3.0]));
        assert_eq!(insert_at(&[], 1, 5.0), Ok(vec![5.0]));
        assert!(insert_at(&[1.0], 0, 2.0).is_err());
        assert!(insert_at(&[1.0], 3, 2.0).is_err());
        assert_eq!(remove_at(&[1.0, 2.0, 3.0], 2), Ok(vec![1.0, 3.0]));
        assert!(remove_at(&[], 1).is_err());
        assert!(remove_at(&[1.0], 2).is_err());
    }

    #[test]
    fn index_of_reports_one_based_positions() {
        assert_eq!(index_of(&[5.0, 7.0, 9.0], 7.0), Ok(2));
        assert!(index_of(&[5.0], 6.0).is_err());
    }

    #[test]
    fn iteration_list_applies_the_map() {
        let out = iteration_list("2*x", "x", 1.0, 4, &[]).unwrap();
        assert_eq!(out, vec![2.0, 4.0, 8.0, 16.0]);
        assert!(iteration_list("x", "x", 1.0, 0, &[]).is_err());
        assert!(iteration_list("x", "9bad", 1.0, 3, &[]).is_err());
        assert!(iteration_list("1/0", "x", 1.0, 3, &[]).is_err());
    }

    #[test]
    fn map_list_binds_x() {
        assert_eq!(
            map_list("x^2", &[1.0, 2.0, 3.0], &[]),
            Ok(vec![1.0, 4.0, 9.0])
        );
        assert!(map_list("1/0", &[1.0], &[]).is_err());
    }

    #[test]
    fn shuffle_preserves_multiset_deterministically() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut sorted = shuffle_with_seed(&data, &mut rng()).unwrap();
        sorted.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(sorted, data);
        let a = shuffle_with_seed(&data, &mut rng()).unwrap();
        let b = shuffle_with_seed(&data, &mut rng()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn sample_draws_without_replacement() {
        let data = [1.0, 2.0, 3.0, 4.0];
        let s = sample_with_seed(&data, 2, &mut rng()).unwrap();
        assert_eq!(s.len(), 2);
        assert_ne!(s[0], s[1]);
        assert!(sample_with_seed(&data, 5, &mut rng()).is_err());
        assert!(sample_with_seed(&data, -1, &mut rng()).is_err());
        assert_eq!(
            sample_with_seed(&data, 0, &mut rng()).unwrap(),
            Vec::<f64>::new()
        );
    }

    #[test]
    fn random_element_and_discrete_ranges() {
        let data = [10.0, 20.0, 30.0];
        let v = random_element_with_seed(&data, &mut rng()).unwrap();
        assert!(data.contains(&v));
        assert!(random_element_with_seed(&[], &mut rng()).is_err());
        let d = random_discrete_range(1.0, 6.0, &mut rng()).unwrap();
        assert!((1.0..=6.0).contains(&d) && d.fract() == 0.0);
        assert!(random_discrete_range(6.0, 1.0, &mut rng()).is_err());
        assert!(random_discrete_range(1.5, 6.0, &mut rng()).is_err());
    }

    #[test]
    fn aggregations_reject_empty_and_nonfinite() {
        assert_eq!(list_min(&[3.0, 1.0, 2.0]), Ok(1.0));
        assert_eq!(list_max(&[3.0, 1.0, 2.0]), Ok(3.0));
        assert_eq!(list_sum(&[1.0, 2.0, 3.0]), Ok(6.0));
        assert_eq!(list_product(&[2.0, 3.0, 4.0]), Ok(24.0));
        assert!(list_min(&[]).is_err());
        assert!(list_max(&[]).is_err());
        assert!(list_sum(&[]).is_err());
        assert!(list_product(&[]).is_err());
        assert!(list_sum(&[1.0, f64::INFINITY]).is_err());
    }

    #[test]
    fn ranks_handle_ties() {
        assert_eq!(tied_rank(&[30.0, 10.0, 20.0]), Ok(vec![3.0, 1.0, 2.0]));
        assert_eq!(
            tied_rank(&[1.0, 2.0, 2.0, 3.0]),
            Ok(vec![1.0, 2.5, 2.5, 4.0])
        );
        assert_eq!(
            ordinal_rank(&[1.0, 2.0, 2.0, 3.0]),
            Ok(vec![1.0, 2.0, 3.0, 4.0])
        );
        assert!(tied_rank(&[]).is_err());
    }

    #[test]
    fn spearman_detects_monotonicity() {
        let r = spearman(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]).unwrap();
        assert!((r - 1.0).abs() < 1e-12);
        let r = spearman(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]).unwrap();
        assert!((r + 1.0).abs() < 1e-12);
        assert!(spearman(&[1.0], &[1.0]).is_err());
        assert!(spearman(&[1.0, 1.0], &[1.0, 2.0]).is_err());
    }

    #[test]
    fn mad_percentile_and_means() {
        assert_eq!(mad(&[1.0, 1.0, 2.0, 2.0, 4.0]), Ok(1.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 50.0), Ok(2.5));
        assert!(percentile(&[1.0], 101.0).is_err());
        assert_eq!(mean_of(&[2.0, 4.0], "MeanX"), Ok(3.0));
        assert_eq!(
            population_sd(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], "SDX").unwrap(),
            2.0
        );
        assert_eq!(geometric_mean(&[1.0, 4.0]), Ok(2.0));
        assert!(geometric_mean(&[-1.0]).is_err());
        assert_eq!(harmonic_mean(&[1.0, 2.0, 4.0]).unwrap(), 12.0 / 7.0);
        assert!(harmonic_mean(&[0.0]).is_err());
        assert!((root_mean_square(&[3.0, 4.0]).unwrap() - 12.5_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn sigma_and_s_moments() {
        assert_eq!(sigma_xx(&[1.0, 2.0]), Ok(5.0));
        assert_eq!(sigma_xy(&[1.0, 2.0], &[3.0, 4.0]), Ok(11.0));
        assert!(sigma_xy(&[1.0], &[1.0, 2.0]).is_err());
        assert_eq!(sxx(&[1.0, 2.0, 3.0]), Ok(2.0));
        assert_eq!(sxy(&[1.0, 2.0], &[2.0, 4.0]), Ok(1.0));
        assert_eq!(sum_squared_errors(&[1.0, 2.0, 3.0]), Ok(2.0));
        assert_eq!(rsquare(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), Ok(1.0));
        assert!(rsquare(&[1.0, 1.0], &[1.0, 2.0]).is_err());
    }

    #[test]
    fn z_and_t_intervals_cover_the_mean() {
        let (lo, m, hi) = z_mean_estimate(&[1.0, 2.0, 3.0], 1.0, 0.95).unwrap();
        assert!(lo < m && m < hi && (m - 2.0).abs() < 1e-12);
        assert!(z_mean_estimate(&[1.0], 0.0, 0.95).is_err());
        assert!(z_mean_estimate(&[1.0], 1.0, 1.5).is_err());
        let (z, p) = z_mean_test(&[1.0, 2.0, 3.0], 2.0, 1.0).unwrap();
        assert!(z.abs() < 1e-12 && (p - 1.0).abs() < 1e-6);
        let (lo, _, hi) = t_mean_estimate(&[1.0, 2.0, 3.0], 0.95).unwrap();
        assert!(lo < 2.0 && 2.0 < hi);
        assert!(t_mean_estimate(&[1.0], 0.95).is_err());
        let (lo, d, hi) = z_mean_2_estimate(&[1.0, 2.0], 1.0, &[3.0, 4.0], 1.0, 0.95).unwrap();
        assert!(lo < d && d < hi);
        let (_, p) = z_mean_2_test(&[1.0, 2.0], 1.0, &[1.0, 2.0], 1.0).unwrap();
        assert!((p - 1.0).abs() < 1e-6);
    }

    #[test]
    fn proportion_intervals_and_tests() {
        let (lo, p, hi) = z_proportion_estimate(5.0, 10.0, 0.95).unwrap();
        assert!(lo < p && p < hi && (p - 0.5).abs() < 1e-12);
        assert!(z_proportion_estimate(11.0, 10.0, 0.95).is_err());
        let (_, p) = z_proportion_test(5.0, 10.0, 0.5).unwrap();
        assert!((p - 1.0).abs() < 1e-6);
        assert!(z_proportion_test(5.0, 10.0, 0.0).is_err());
        let (lo, _, hi) = z_proportion_2_estimate(5.0, 10.0, 5.0, 10.0, 0.95).unwrap();
        assert!(lo < 0.0 && 0.0 < hi);
        let (_, p) = z_proportion_2_test(5.0, 10.0, 5.0, 10.0).unwrap();
        assert!((p - 1.0).abs() < 1e-6);
        let (lo, _, hi) = t_mean_2_estimate(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0], 0.95).unwrap();
        assert!(lo < 0.0 && 0.0 < hi);
    }

    #[test]
    fn classes_dotplot_polygon_and_contingency() {
        let b = class_breaks(&[1.0, 2.0, 3.0, 4.0], 2).unwrap();
        assert_eq!(b.classes, 2);
        assert_eq!(b.boundaries.len(), 3);
        assert!(class_breaks(&[], 2).is_err());
        assert!(class_breaks(&[1.0], 0).is_err());
        let (xs, ys) = dot_plot_points(&[1.0, 2.0, 1.0]).unwrap();
        assert_eq!(xs, vec![1.0, 2.0, 1.0]);
        assert_eq!(ys, vec![1.0, 1.0, 2.0]);
        assert!(dot_plot_points(&[]).is_err());
        let (mids, freqs) = frequency_polygon_points(&[1.0, 2.0, 3.0, 4.0], 2).unwrap();
        assert_eq!(mids.len(), 2);
        assert_eq!(freqs.iter().sum::<f64>(), 4.0);
        let (chi2, df, p) = contingency_chi2(&[10.0, 10.0, 10.0, 10.0], 2).unwrap();
        assert!(chi2.abs() < 1e-9 && df == 1.0 && (p - 1.0).abs() < 1e-9);
        assert!(contingency_chi2(&[1.0, 2.0, 3.0], 2).is_err());
        assert!(contingency_chi2(&[-1.0, 2.0, 3.0, 4.0], 2).is_err());
    }

    #[test]
    fn format_scalars_renders_braces() {
        assert_eq!(format_scalars(&[]), "{}");
        assert_eq!(format_scalars(&[1.0, 2.0]), "{1, 2}");
    }

    #[test]
    fn length_and_depth_caps_hold() {
        assert!(validate_flat_len(MAX_LIST_LENGTH + 1, "X").is_err());
        assert!(check_depth(MAX_LIST_DEPTH + 1, "X").is_err());
        assert!(check_depth(MAX_LIST_DEPTH, "X").is_ok());
    }
}
