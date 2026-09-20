//! Harness de búsqueda para problemas abiertos (Fase A, cerebro puro).
//!
//! Todo es determinista, acotado y sin I/O: el LLM propone configuraciones,
//! estos motores miden. Ninguna función inventa resultados: lo que no entra
//! en cota devuelve `Err` honesto en español.
//!
//! Cubre TOPP 39 (distancias), TOPP 57 (Hadwiger-Nelson vía grafos
//! unit-distance + DIMACS), k-sets/halving (TOPP 7), no-3-en-línea,
//! triángulo vacío y el loop de un problema (`run_topp39_scan`).

use crate::Point2;
use std::collections::{BTreeMap, BTreeSet};

/// Error del harness con mensaje listo para mostrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchError(pub String);

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SearchError {}

/// Máximo de puntos por corrida (el comando permite 10k; el harness acota).
pub const MAX_SEARCH_POINTS: usize = 2_000;
/// Máximo de aristas unitarias materializadas.
pub const MAX_UNIT_EDGES: usize = 200_000;
/// Máximo de variables CNF (n * k) para export DIMACS.
pub const MAX_DIMACS_VARS: usize = 20_000;
/// Máximo de vértices para backtracking exacto de k-coloración.
pub const MAX_BRUTE_N: usize = 24;
/// Máximo de puntos para conteo de halving edges O(n³).
pub const MAX_HALVING_N: usize = 400;
/// Máximo de puntos para test de triángulo vacío O(n⁴).
pub const MAX_EMPTY_N: usize = 80;

fn validate_points(points: &[Point2]) -> Result<(), SearchError> {
    if points.is_empty() {
        return Err(SearchError("harness: se requiere al menos 1 punto".into()));
    }
    if points.len() > MAX_SEARCH_POINTS {
        return Err(SearchError(format!(
            "harness: {} puntos excede el máximo {}",
            points.len(),
            MAX_SEARCH_POINTS
        )));
    }
    for (idx, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(SearchError(format!("harness: punto {idx} no finito")));
        }
    }
    Ok(())
}

fn validate_tol(tol: f64) -> Result<(), SearchError> {
    if !tol.is_finite() || !(1e-12..=1e-3).contains(&tol) {
        return Err(SearchError(format!(
            "harness: tolerancia '{tol}' fuera de [1e-12, 1e-3]"
        )));
    }
    Ok(())
}

#[inline]
fn dist(a: Point2, b: Point2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx.hypot(dy)
}

/// Cuenta pares a distancia unitaria (|d - 1| <= tol). O(n²).
///
/// # Errores
/// Puntos vacíos/no finitos, más de `MAX_SEARCH_POINTS` o `tol` fuera de rango.
pub fn unit_pairs(points: &[Point2], tol: f64) -> Result<usize, SearchError> {
    validate_points(points)?;
    validate_tol(tol)?;
    let mut count = 0usize;
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            if (dist(points[i], points[j]) - 1.0).abs() <= tol {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Cuenta distancias distintas cuantizando a `quant` (default 1e-9). O(n²).
pub fn distinct_distances(points: &[Point2], quant: f64) -> Result<usize, SearchError> {
    validate_points(points)?;
    if !quant.is_finite() || !(1e-12..=1e-3).contains(&quant) {
        return Err(SearchError(format!(
            "harness: cuanto '{quant}' fuera de [1e-12, 1e-3]"
        )));
    }
    let mut set = BTreeSet::new();
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let d = dist(points[i], points[j]);
            if !d.is_finite() {
                return Err(SearchError("harness: distancia no finita".into()));
            }
            let key = (d / quant).round() as i64;
            set.insert(key);
        }
    }
    Ok(set.len())
}

/// Aristas del grafo unit-distance como pares de índices ordenados.
pub fn unit_graph_edges(points: &[Point2], tol: f64) -> Result<Vec<(usize, usize)>, SearchError> {
    validate_points(points)?;
    validate_tol(tol)?;
    let mut edges = Vec::new();
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            if (dist(points[i], points[j]) - 1.0).abs() <= tol {
                edges.push((i, j));
                if edges.len() > MAX_UNIT_EDGES {
                    return Err(SearchError(format!(
                        "harness: aristas exceden el máximo {MAX_UNIT_EDGES}"
                    )));
                }
            }
        }
    }
    Ok(edges)
}

/// Exporta la k-coloración del grafo a DIMACS CNF.
///
/// Variables `x(v,c) = v * k + c + 1` (1-based). Cláusulas: cada vértice toma
/// al menos un color, a lo sumo uno, y vecinos difieren por color.
pub fn export_dimacs_kcoloring(
    n: usize,
    edges: &[(usize, usize)],
    k: usize,
) -> Result<String, SearchError> {
    if n == 0 || n > MAX_SEARCH_POINTS {
        return Err(SearchError(format!(
            "harness: n={n} fuera de [1, {MAX_SEARCH_POINTS}]"
        )));
    }
    if !(1..=16).contains(&k) {
        return Err(SearchError(format!("harness: k={k} fuera de [1, 16]")));
    }
    let vars = n
        .checked_mul(k)
        .ok_or_else(|| SearchError("harness: desborde en n * k para DIMACS".into()))?;
    if vars > MAX_DIMACS_VARS {
        return Err(SearchError(format!(
            "harness: {vars} variables exceden el máximo {MAX_DIMACS_VARS}; probá con menos vértices o colores"
        )));
    }
    for (a, b) in edges {
        if *a >= n || *b >= n {
            return Err(SearchError(format!(
                "harness: arista ({a}, {b}) fuera de [0, {n})"
            )));
        }
        if *a == *b {
            return Err(SearchError(format!("harness: lazo en {a} no admitido")));
        }
    }
    // Conteo de cláusulas: n (al-menos-uno) + n * k*(k-1)/2 (a-lo-sumo-uno)
    // + edges * k (adyacencia).
    let at_most = k.saturating_mul(k.saturating_sub(1)) / 2;
    let clauses = n + n.saturating_mul(at_most) + edges.len().saturating_mul(k);
    let mut out = String::with_capacity(64 + clauses.saturating_mul(8).min(1_000_000));
    out.push_str(&format!("p cnf {vars} {clauses}\n"));
    let var = |v: usize, c: usize| v * k + c + 1;
    for v in 0..n {
        for c in 0..k {
            out.push_str(&format!("{} ", var(v, c)));
        }
        out.push_str("0\n");
        for c1 in 0..k {
            for c2 in (c1 + 1)..k {
                out.push_str(&format!("-{} -{} 0\n", var(v, c1), var(v, c2)));
            }
        }
    }
    for (a, b) in edges {
        for c in 0..k {
            out.push_str(&format!("-{} -{} 0\n", var(*a, c), var(*b, c)));
        }
    }
    Ok(out)
}

/// Backtracking exacto de k-coloración con orden por grado. Solo n <= 24.
///
/// Devuelve `Ok(true/false)`. Si el grafo es más grande, `Err` honesto:
/// exportá a DIMACS y usá un SAT externo (kissat/cadical).
pub fn is_k_colorable_bruteforce(
    n: usize,
    edges: &[(usize, usize)],
    k: usize,
) -> Result<bool, SearchError> {
    if n == 0 || k == 0 {
        return Err(SearchError("harness: n y k deben ser >= 1".into()));
    }
    if n > MAX_BRUTE_N {
        return Err(SearchError(format!(
            "harness: n={n} excede backtracking {MAX_BRUTE_N}; exportá DIMACS y usá kissat"
        )));
    }
    if k > 8 {
        return Err(SearchError(format!(
            "harness: k={k} excede backtracking 8; exportá DIMACS"
        )));
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, b) in edges {
        if *a >= n || *b >= n || *a == *b {
            return Err(SearchError("harness: arista inválida".into()));
        }
        adj[*a].push(*b);
        adj[*b].push(*a);
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|v| std::cmp::Reverse(adj[*v].len()));
    let mut color: Vec<Option<usize>> = vec![None; n];
    fn backtrack(
        pos: usize,
        order: &[usize],
        adj: &[Vec<usize>],
        color: &mut [Option<usize>],
        k: usize,
    ) -> bool {
        if pos == order.len() {
            return true;
        }
        let v = order[pos];
        for c in 0..k {
            let mut ok = true;
            for w in &adj[v] {
                if color[*w] == Some(c) {
                    ok = false;
                    break;
                }
            }
            if ok {
                color[v] = Some(c);
                if backtrack(pos + 1, order, adj, color, k) {
                    return true;
                }
                color[v] = None;
            }
        }
        false
    }
    Ok(backtrack(0, &order, &adj, &mut color, k))
}

#[inline]
fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Cuenta halving edges no dirigidas (n par): pares con (n-2)/2 puntos a cada lado.
pub fn halving_edges_count(points: &[Point2]) -> Result<usize, SearchError> {
    validate_points(points)?;
    if points.len() > MAX_HALVING_N {
        return Err(SearchError(format!(
            "harness: {} puntos excede halving {}",
            points.len(),
            MAX_HALVING_N
        )));
    }
    let n = points.len();
    if n < 2 || n % 2 == 1 {
        return Ok(0);
    }
    let want = (n - 2) / 2;
    let mut count = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            let mut left = 0usize;
            let mut right = 0usize;
            let mut deg = false;
            for k in 0..n {
                if k == i || k == j {
                    continue;
                }
                let c = cross(points[i], points[j], points[k]);
                if c == 0.0 {
                    deg = true;
                    break;
                } else if c > 0.0 {
                    left += 1;
                } else {
                    right += 1;
                }
            }
            if !deg && left == want && right == want {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Indica si hay tres puntos colineales exactos (cross == 0).
pub fn has_three_colinear(points: &[Point2]) -> Result<bool, SearchError> {
    validate_points(points)?;
    let n = points.len();
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                if cross(points[i], points[j], points[k]) == 0.0 {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn point_in_triangle_strict(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let s1 = cross(a, b, p);
    let s2 = cross(b, c, p);
    let s3 = cross(c, a, p);
    (s1 > 0.0 && s2 > 0.0 && s3 > 0.0) || (s1 < 0.0 && s2 < 0.0 && s3 < 0.0)
}

/// Indica si existe un triángulo vacío (sin puntos estrictamente dentro).
pub fn empty_triangle_exists(points: &[Point2]) -> Result<bool, SearchError> {
    validate_points(points)?;
    if points.len() > MAX_EMPTY_N {
        return Err(SearchError(format!(
            "harness: {} puntos excede vacío {}",
            points.len(),
            MAX_EMPTY_N
        )));
    }
    let n = points.len();
    if n < 3 {
        return Ok(false);
    }
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                if cross(points[i], points[j], points[k]) == 0.0 {
                    continue;
                }
                let mut empty = true;
                for m in 0..n {
                    if m == i || m == j || m == k {
                        continue;
                    }
                    if point_in_triangle_strict(points[m], points[i], points[j], points[k]) {
                        empty = false;
                        break;
                    }
                }
                if empty {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Conjunto determinista de `n` puntos en [-scale, scale]² desde `seed`.
pub fn seeded_point_set(seed: u64, n: usize, scale: f64) -> Result<Vec<Point2>, SearchError> {
    if n == 0 || n > MAX_SEARCH_POINTS {
        return Err(SearchError(format!(
            "harness: n={n} fuera de [1, {MAX_SEARCH_POINTS}]"
        )));
    }
    if !scale.is_finite() || scale <= 0.0 || scale > 1e6 {
        return Err(SearchError(format!(
            "harness: escala '{scale}' fuera de (0, 1e6]"
        )));
    }
    let mut state = seed;
    let mut pts = Vec::with_capacity(n);
    for _ in 0..n {
        let a = splitmix64(&mut state);
        let b = splitmix64(&mut state);
        let x = (a as f64 / u64::MAX as f64) * 2.0 * scale - scale;
        let y = (b as f64 / u64::MAX as f64) * 2.0 * scale - scale;
        if !x.is_finite() || !y.is_finite() {
            return Err(SearchError("harness: punto generado no finito".into()));
        }
        pts.push(Point2::new(x, y));
    }
    Ok(pts)
}

/// Retícula determinista rows×cols con paso fijo (ideal para unit-distance).
pub fn grid_point_set(rows: usize, cols: usize, spacing: f64) -> Result<Vec<Point2>, SearchError> {
    if rows == 0 || cols == 0 {
        return Err(SearchError("harness: filas y columnas >= 1".into()));
    }
    let n = rows
        .checked_mul(cols)
        .ok_or_else(|| SearchError("harness: desborde en filas * columnas".into()))?;
    if n > MAX_SEARCH_POINTS {
        return Err(SearchError(format!(
            "harness: retícula {n} excede el máximo {MAX_SEARCH_POINTS}"
        )));
    }
    if !spacing.is_finite() || spacing <= 0.0 || spacing > 1e6 {
        return Err(SearchError("harness: paso fuera de (0, 1e6]".into()));
    }
    let mut pts = Vec::with_capacity(n);
    for r in 0..rows {
        for c in 0..cols {
            pts.push(Point2::new(c as f64 * spacing, r as f64 * spacing));
        }
    }
    Ok(pts)
}

/// Retícula triangular (hexagonal) rows×cols con lado `spacing`.
///
/// Es la construcción clásica de muchos pares unitarios (TOPP 39): cada fila
/// se desplaza medio paso y la altura es `spacing * sqrt(3)/2`.
pub fn triangular_lattice(
    rows: usize,
    cols: usize,
    spacing: f64,
) -> Result<Vec<Point2>, SearchError> {
    if rows == 0 || cols == 0 {
        return Err(SearchError("harness: filas y columnas >= 1".into()));
    }
    let n = rows
        .checked_mul(cols)
        .ok_or_else(|| SearchError("harness: desborde en filas * columnas".into()))?;
    if n > MAX_SEARCH_POINTS {
        return Err(SearchError(format!(
            "harness: retícula triangular {n} excede el máximo {MAX_SEARCH_POINTS}"
        )));
    }
    if !spacing.is_finite() || spacing <= 0.0 || spacing > 1e6 {
        return Err(SearchError("harness: lado fuera de (0, 1e6]".into()));
    }
    let dy = spacing * 3.0_f64.sqrt() / 2.0;
    let mut pts = Vec::with_capacity(n);
    for r in 0..rows {
        for c in 0..cols {
            let x = c as f64 * spacing + if r % 2 == 1 { spacing / 2.0 } else { 0.0 };
            pts.push(Point2::new(x, r as f64 * dy));
        }
    }
    Ok(pts)
}

/// `n` puntos igualmente espaciados en una circunferencia de radio `radius`.
pub fn regular_polygon(n: usize, radius: f64) -> Result<Vec<Point2>, SearchError> {
    if !(3..=MAX_SEARCH_POINTS).contains(&n) {
        return Err(SearchError(format!(
            "harness: polígono n={n} fuera de [3, {MAX_SEARCH_POINTS}]"
        )));
    }
    if !radius.is_finite() || radius <= 0.0 || radius > 1e6 {
        return Err(SearchError("harness: radio fuera de (0, 1e6]".into()));
    }
    let mut pts = Vec::with_capacity(n);
    for k in 0..n {
        let theta = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        pts.push(Point2::new(radius * theta.cos(), radius * theta.sin()));
    }
    Ok(pts)
}

/// Medición compacta de un conjunto de puntos (TOPP 39) + hash del conjunto.
#[derive(Debug, Clone)]
pub struct PointSetMetrics {
    /// Puntos medidos.
    pub n: usize,
    /// Pares a distancia 1.
    pub unit: usize,
    /// Distancias distintas cuantizadas a 1e-9.
    pub distinct: usize,
    /// Hash FNV de las coordenadas.
    pub hash: u64,
}

/// Mide `points` (pares unitarios, distancias distintas y hash) sin generar nada.
pub fn measure_point_set(points: &[Point2]) -> Result<PointSetMetrics, SearchError> {
    let unit = unit_pairs(points, 1e-9)?;
    let distinct = distinct_distances(points, 1e-9)?;
    let mut bytes = Vec::with_capacity(points.len() * 16);
    for p in points {
        bytes.extend_from_slice(&p.x.to_bits().to_le_bytes());
        bytes.extend_from_slice(&p.y.to_bits().to_le_bytes());
    }
    Ok(PointSetMetrics {
        n: points.len(),
        unit,
        distinct,
        hash: fnv1a_64(&bytes),
    })
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    h
}

/// Registro verificable de una corrida TOPP 39 (anti-alucinación).
///
/// El LLM propone (seed, n); el motor mide. Solo este registro cuenta como
/// evidencia: si el hash no cierra, el resultado se descarta.
#[derive(Debug, Clone)]
pub struct SearchRun {
    /// Semilla del generador.
    pub seed: u64,
    /// Cantidad de puntos.
    pub n: usize,
    /// Escala del conjunto.
    pub scale: f64,
    /// Pares a distancia 1.
    pub unit: usize,
    /// Distancias distintas.
    pub distinct: usize,
    /// Hash FNV de la configuración + métricas.
    pub hash: u64,
    /// Distribución de grados del grafo unitario por grado.
    pub degree_hist: BTreeMap<usize, usize>,
}

impl SearchRun {
    /// Serializa a una línea JSONL estable (claves fijas, 6 decimales).
    pub fn to_jsonl(&self) -> String {
        let mut hist = String::from("{");
        let mut first = true;
        for (grado, cuenta) in &self.degree_hist {
            if !first {
                hist.push(',');
            }
            first = false;
            hist.push_str(&format!("\"{grado}\":{cuenta}"));
        }
        hist.push('}');
        format!(
            "{{\"kind\":\"topp39\",\"seed\":{},\"n\":{},\"scale\":{:.6},\"unit\":{},\"distinct\":{},\"hash\":{},\"degrees\":{}}}",
            self.seed, self.n, self.scale, self.unit, self.distinct, self.hash, hist
        )
    }
}

/// Corrida completa TOPP 39: genera, mide, hashea. Pura y determinista.
pub fn run_topp39_scan(
    seed: u64,
    n: usize,
    scale: f64,
    tol: f64,
) -> Result<SearchRun, SearchError> {
    let pts = seeded_point_set(seed, n, scale)?;
    let unit = unit_pairs(&pts, tol)?;
    let distinct = distinct_distances(&pts, 1e-9)?;
    let edges = unit_graph_edges(&pts, tol)?;
    let mut degree: BTreeMap<usize, usize> = BTreeMap::new();
    let mut deg_of: Vec<usize> = vec![0; pts.len()];
    for (a, b) in &edges {
        deg_of[*a] += 1;
        deg_of[*b] += 1;
    }
    for d in deg_of {
        *degree.entry(d).or_insert(0) += 1;
    }
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&seed.to_le_bytes());
    bytes.extend_from_slice(&n.to_le_bytes());
    bytes.extend_from_slice(&scale.to_bits().to_le_bytes());
    bytes.extend_from_slice(&unit.to_le_bytes());
    bytes.extend_from_slice(&distinct.to_le_bytes());
    let hash = fnv1a_64(&bytes);
    Ok(SearchRun {
        seed,
        n,
        scale,
        unit,
        distinct,
        hash,
        degree_hist: degree,
    })
}

/// Verifica un `SearchRun` re-ejecutando la medición (puerta anti-alucinación).
///
/// `Ok(true)` = el hash y las métricas cierran. `Ok(false)` = no cierran y el
/// resultado debe descartarse. `Err` = parámetros inválidos.
pub fn verify_search_run(run: &SearchRun, tol: f64) -> Result<bool, SearchError> {
    let fresh = run_topp39_scan(run.seed, run.n, run.scale, tol)?;
    Ok(fresh.hash == run.hash && fresh.unit == run.unit && fresh.distinct == run.distinct)
}

/// Loop de un problema (Fase B): barre `seeds` con el mismo (n, scale, tol),
/// re-verifica cada corrida y devuelve el mejor por pares unitarios.
///
/// Puertas anti-alucinación:
/// 1. el LLM solo propone seeds (números); el motor mide todo;
/// 2. cada corrida se re-verifica (`verify_search_run`); la que no cierra se
///    descarta sin romper el loop;
/// 3. solo los `SearchRun` verificados cuentan como evidencia.
///
/// `seeds` vacío o `seeds.len() > 4096` → `Err` honesto.
pub fn topp39_best_of(
    seeds: &[u64],
    n: usize,
    scale: f64,
    tol: f64,
) -> Result<(SearchRun, Vec<SearchRun>), SearchError> {
    if seeds.is_empty() {
        return Err(SearchError("harness: loop sin seeds".into()));
    }
    if seeds.len() > 4_096 {
        return Err(SearchError(format!(
            "harness: {} seeds excede el máximo 4096",
            seeds.len()
        )));
    }
    let mut verified = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let run = run_topp39_scan(*seed, n, scale, tol)?;
        if verify_search_run(&run, tol)? {
            verified.push(run);
        }
    }
    if verified.is_empty() {
        return Err(SearchError(
            "harness: ninguna corrida verificó; resultados descartados".into(),
        ));
    }
    let mut best = verified[0].clone();
    for run in &verified[1..] {
        if run.unit > best.unit || (run.unit == best.unit && run.distinct < best.distinct) {
            best = run.clone();
        }
    }
    Ok((best, verified))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuadrado_unidad_da_4_pares_y_2_distancias() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        assert_eq!(unit_pairs(&pts, 1e-9).unwrap(), 4);
        assert_eq!(distinct_distances(&pts, 1e-9).unwrap(), 2);
        let edges = unit_graph_edges(&pts, 1e-9).unwrap();
        assert_eq!(edges.len(), 4);
    }

    #[test]
    fn reticula_3x3_paso_1_tiene_12_pares() {
        let pts = grid_point_set(3, 3, 1.0).unwrap();
        assert_eq!(pts.len(), 9);
        assert_eq!(unit_pairs(&pts, 1e-9).unwrap(), 12);
    }

    #[test]
    fn reticulatriangular_y_poligono_miden_igual() {
        // Retícula triangular 2×2: 4 puntos, 5 aristas unitarias (rombo + fila).
        let tri = triangular_lattice(2, 2, 1.0).unwrap();
        assert_eq!(tri.len(), 4);
        let m = measure_point_set(&tri).unwrap();
        assert_eq!(m.n, 4);
        assert!(m.unit >= 4, "triangular unit={} esperado >= 4", m.unit);
        // Polígono regular n=4 radio 1/sqrt(2): cuadrado de lado 1, 4 pares.
        let poly = regular_polygon(4, 1.0 / 2.0_f64.sqrt()).unwrap();
        assert_eq!(unit_pairs(&poly, 1e-9).unwrap(), 4);
        assert!(triangular_lattice(0, 2, 1.0).is_err());
        assert!(regular_polygon(2, 1.0).is_err());
    }

    #[test]
    fn k4_abstracto_no_3_coloreable_y_dimacs_valido() {
        // K4 abstracto (no realizable como unit-distance plano, pero válido
        // para el backtracking y el export DIMACS).
        let n = 4usize;
        let edges = vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
        assert!(!is_k_colorable_bruteforce(n, &edges, 3).unwrap());
        assert!(is_k_colorable_bruteforce(n, &edges, 4).unwrap());
        let cnf = export_dimacs_kcoloring(n, &edges, 4).unwrap();
        assert!(cnf.starts_with("p cnf "));
        // Triángulo equilátero lado 1: 3 pares unitarios, no bipartito.
        let s = 3.0_f64.sqrt() / 2.0;
        let tri = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, s),
        ];
        let tri_edges = unit_graph_edges(&tri, 1e-9).unwrap();
        assert_eq!(tri_edges.len(), 3);
        assert!(!is_k_colorable_bruteforce(tri.len(), &tri_edges, 2).unwrap());
    }

    #[test]
    fn cuadrado_tiene_2_halving_y_triangulo_vacio() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        // Solo las 2 diagonales son halving (los lados dejan 2 puntos del
        // mismo lado); hay triángulo vacío y nada colineal.
        assert_eq!(halving_edges_count(&pts).unwrap(), 2);
        assert!(empty_triangle_exists(&pts).unwrap());
        assert!(!has_three_colinear(&pts).unwrap());
    }

    #[test]
    fn tres_colineales_se_detecta() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
        ];
        assert!(has_three_colinear(&pts).unwrap());
    }

    #[test]
    fn corrida_topp39_es_determinista_y_verifica() {
        let a = run_topp39_scan(42, 12, 5.0, 1e-9).unwrap();
        let b = run_topp39_scan(42, 12, 5.0, 1e-9).unwrap();
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.unit, b.unit);
        assert!(verify_search_run(&a, 1e-9).unwrap());
        let mut fraguada = a.clone();
        fraguada.unit += 1;
        assert!(!verify_search_run(&fraguada, 1e-9).unwrap());
        assert!(a.to_jsonl().contains("\"kind\":\"topp39\""));
    }

    #[test]
    fn loop_topp39_devuelve_mejor_verificado() {
        let seeds: Vec<u64> = (0..16).collect();
        let (best, all) = topp39_best_of(&seeds, 10, 5.0, 1e-9).unwrap();
        assert_eq!(all.len(), 16);
        let max_unit = all.iter().map(|r| r.unit).max().unwrap();
        assert_eq!(best.unit, max_unit);
        for run in &all {
            assert!(verify_search_run(run, 1e-9).unwrap());
        }
        assert!(topp39_best_of(&[], 10, 5.0, 1e-9).is_err());
    }

    #[test]
    fn cotas_dan_error_honesto() {
        assert!(seeded_point_set(1, 0, 1.0).is_err());
        assert!(seeded_point_set(1, MAX_SEARCH_POINTS + 1, 1.0).is_err());
        assert!(unit_pairs(&[], 1e-9).is_err());
        assert!(is_k_colorable_bruteforce(MAX_BRUTE_N + 1, &[], 3).is_err());
        assert!(export_dimacs_kcoloring(2000, &[], 16).is_err());
    }
}
