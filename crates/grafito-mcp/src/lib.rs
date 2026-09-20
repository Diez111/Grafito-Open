//! Núcleo puro del servidor MCP de laboratorio (`grafito-mcp`).
//!
//! Cerebro puro: sin egui, sin wgpu, sin red. Todo lo que toca disco o
//! procesos vive en `ledger.rs` / `sat.rs` / `main.rs`; acá solo hay
//! validación, generación determinista, medición escalable y IDs.
//!
//! Filosofía "sin presupuesto artificial, con resistencia real":
//! los topes bajos de Fase A (2000 puntos, 4096 seeds) se reemplazan por
//! topes altos configurables por entorno. Lo que es combinatoriamente
//! imposible (distancias distintas O(n²) gigante, halving O(n³)) sigue dando
//! error honesto en español con guía, jamás OOM en silencio.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use grafito_geometry::Point2;
use std::collections::BTreeMap;

pub mod bridge;
pub mod colab;
pub mod gpu;
pub mod lean;
pub mod ledger;
pub mod policy;
pub mod protocol;
pub mod sat;
pub mod tools;

// ── Identidad del server ─────────────────────────────────────────────

/// Versión del protocolo MCP que hablamos (spec 2024-11-05, compatible 2025).
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
/// Nombre del server para `initialize`.
pub const SERVER_NAME: &str = "grafito-mcp";
/// Versión del server (sigue al workspace).
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Topes resistentes (altos, configurables) ─────────────────────────

/// Puntos por defecto: 100k (50× la Fase A). O(n) con índice espacial.
pub const DEFAULT_MAX_POINTS: usize = 100_000;
/// Techo duro anti-OOM aunque el env pida más (500k puntos ≈ 8 MB en RAM).
pub const HARD_MAX_POINTS: usize = 500_000;
/// Aristas materializadas por defecto: 2M.
pub const DEFAULT_MAX_EDGES: usize = 2_000_000;
/// Techo duro de aristas (10M ≈ 160 MB como pares u64, no se pasa).
pub const HARD_MAX_EDGES: usize = 10_000_000;
/// Vars DIMACS por defecto: 500k (25× Fase A).
pub const DEFAULT_MAX_DIMACS_VARS: usize = 500_000;
/// Techo duro de vars DIMACS (2M).
pub const HARD_MAX_DIMACS_VARS: usize = 2_000_000;
/// Bytes de CNF por defecto: 50 MiB.
pub const DEFAULT_MAX_CNF_BYTES: usize = 50 * 1024 * 1024;
/// Techo duro de CNF (200 MiB).
pub const HARD_MAX_CNF_BYTES: usize = 200 * 1024 * 1024;
/// Seeds por loop por defecto: 100k.
pub const DEFAULT_MAX_SEEDS: usize = 100_000;
/// Techo duro de seeds (1M).
pub const HARD_MAX_SEEDS: usize = 1_000_000;
/// Timeout SAT por defecto: 30 s.
pub const DEFAULT_SAT_TIMEOUT_MS: u64 = 30_000;
/// Timeout SAT máximo: 24 h (0 = sin timeout, solo explícito).
pub const MAX_SAT_TIMEOUT_MS: u64 = 86_400_000;
/// Distancias distintas: tope O(n²) honesto (10k puntos = 50M pares).
pub const MAX_DISTINCT_N: usize = 10_000;

/// Topes efectivos resueltos desde el entorno (con techos duros).
#[derive(Debug, Clone, Copy)]
pub struct LabLimits {
    pub max_points: usize,
    pub max_edges: usize,
    pub max_dimacs_vars: usize,
    pub max_cnf_bytes: usize,
    pub max_seeds: usize,
}

impl Default for LabLimits {
    fn default() -> Self {
        Self {
            max_points: DEFAULT_MAX_POINTS,
            max_edges: DEFAULT_MAX_EDGES,
            max_dimacs_vars: DEFAULT_MAX_DIMACS_VARS,
            max_cnf_bytes: DEFAULT_MAX_CNF_BYTES,
            max_seeds: DEFAULT_MAX_SEEDS,
        }
    }
}

fn env_usize(name: &str, def: usize, hard: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map(|v| v.min(hard))
        .filter(|v| *v >= 1)
        .unwrap_or(def)
}

impl LabLimits {
    /// Resuelve desde `GRAFITO_LAB_MAX_*` con techos duros anti-OOM.
    pub fn from_env() -> Self {
        Self {
            max_points: env_usize(
                "GRAFITO_LAB_MAX_POINTS",
                DEFAULT_MAX_POINTS,
                HARD_MAX_POINTS,
            ),
            max_edges: env_usize("GRAFITO_LAB_MAX_EDGES", DEFAULT_MAX_EDGES, HARD_MAX_EDGES),
            max_dimacs_vars: env_usize(
                "GRAFITO_LAB_MAX_DIMACS_VARS",
                DEFAULT_MAX_DIMACS_VARS,
                HARD_MAX_DIMACS_VARS,
            ),
            max_cnf_bytes: env_usize(
                "GRAFITO_LAB_MAX_CNF_BYTES",
                DEFAULT_MAX_CNF_BYTES,
                HARD_MAX_CNF_BYTES,
            ),
            max_seeds: env_usize("GRAFITO_LAB_MAX_SEEDS", DEFAULT_MAX_SEEDS, HARD_MAX_SEEDS),
        }
    }
}

// ── Familias ─────────────────────────────────────────────────────────

/// Familia de construcción del conjunto de puntos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Seeded,
    Grid,
    Triangular,
    RegularPolygon,
}

impl Family {
    /// Parsea el nombre; `random` se rechaza con guía (no reproducible).
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_lowercase().as_str() {
            "" | "seeded" | "seed" => Ok(Self::Seeded),
            "grid" | "reticula" | "retícula" => Ok(Self::Grid),
            "triangular" | "triangular_lattice" | "hexagonal" => Ok(Self::Triangular),
            "regular_polygon" | "polygon" | "poligono" | "polígono" => Ok(Self::RegularPolygon),
            "random" | "azar" => Err(
                "familia 'random' no es reproducible: usá 'seeded' con una seed explícita".into(),
            ),
            otra => Err(format!(
                "familia '{otra}' desconocida: elegí seeded, grid, triangular o regular_polygon"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seeded => "seeded",
            Self::Grid => "grid",
            Self::Triangular => "triangular",
            Self::RegularPolygon => "regular_polygon",
        }
    }
}

// ── Hashes e IDs ─────────────────────────────────────────────────────

/// FNV-1a 64 (idéntico al del harness `search.rs` para compatibilidad).
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    h
}

/// SHA-256 en hex (para `cnf_hash` citable y nombres de archivo).
pub fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let out = hasher.finalize();
    let mut s = String::with_capacity(64);
    for byte in out {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

/// `run_id` estable: `t39-{familia}-n{n}-h{fnv:016x}`.
pub fn run_id_for(family: Family, n: usize, seed: u64, unit: usize, distinct: usize) -> String {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(family.as_str().as_bytes());
    bytes.extend_from_slice(&n.to_le_bytes());
    bytes.extend_from_slice(&seed.to_le_bytes());
    bytes.extend_from_slice(&unit.to_le_bytes());
    bytes.extend_from_slice(&distinct.to_le_bytes());
    let h = fnv1a_64(&bytes);
    format!("t39-{}-n{n}-h{h:016x}", family.as_str())
}

/// ¿`run_id` sintácticamente válido? (alfanumérico + `-`, ≤128 chars.)
pub fn is_valid_run_id(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 128
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// ¿`cnf_hash` válido? (64 hex chars.)
pub fn is_valid_cnf_hash(raw: &str) -> bool {
    raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit())
}

// ── Generación (delegando al motor, con topes altos) ─────────────────

/// Parámetros de generación (evita `too_many_arguments` y deja API estable).
#[derive(Debug, Clone, Copy)]
pub struct GenParams {
    pub family: Family,
    pub n: usize,
    pub seed: u64,
    pub scale: f64,
    pub rows: Option<usize>,
    pub cols: Option<usize>,
    pub spacing: Option<f64>,
    pub radius: Option<f64>,
}

/// Genera el conjunto según la familia. Puro, determinista, sin I/O.
pub fn generate_points(params: GenParams, limits: &LabLimits) -> Result<Vec<Point2>, String> {
    let GenParams {
        family,
        n,
        seed,
        scale,
        rows,
        cols,
        spacing,
        radius,
    } = params;
    if n == 0 || n > limits.max_points {
        return Err(format!(
            "n={n} fuera de [1, {}]; si necesitás más, subí GRAFITO_LAB_MAX_POINTS (techo {})",
            limits.max_points, HARD_MAX_POINTS
        ));
    }
    match family {
        Family::Seeded => {
            if !scale.is_finite() || scale <= 0.0 || scale > 1e6 {
                return Err(format!("escala '{scale}' fuera de (0, 1e6]"));
            }
            grafito_geometry::search::seeded_point_set(seed, n, scale).map_err(|e| e.to_string())
        }
        Family::Grid | Family::Triangular => {
            let spacing = spacing.unwrap_or(1.0);
            // Deriva filas×columnas desde n si no se dan (lo más cuadrado posible).
            let (r, c) = match (rows, cols) {
                (Some(r), Some(c)) => (r, c),
                _ => {
                    let c = (n as f64).sqrt().ceil() as usize;
                    let c = c.max(1);
                    let r = n.div_ceil(c);
                    (r, c)
                }
            };
            if r == 0 || c == 0 {
                return Err("filas y columnas deben ser >= 1".into());
            }
            let total = r.checked_mul(c).ok_or("desborde en filas * columnas")?;
            if total > limits.max_points {
                return Err(format!(
                    "retícula {total} excede el máximo {}",
                    limits.max_points
                ));
            }
            let mut pts = match family {
                Family::Grid => grafito_geometry::search::grid_point_set(r, c, spacing)
                    .map_err(|e| e.to_string())?,
                _ => grafito_geometry::search::triangular_lattice(r, c, spacing)
                    .map_err(|e| e.to_string())?,
            };
            pts.truncate(n);
            Ok(pts)
        }
        Family::RegularPolygon => {
            let radius = radius.unwrap_or(1.0);
            grafito_geometry::search::regular_polygon(n, radius).map_err(|e| e.to_string())
        }
    }
}

// ── Medición escalable (índice espacial O(n)) ────────────────────────

#[inline]
fn dist2(a: Point2, b: Point2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Cuenta pares unitarios con índice espacial (celda 1.0, vecindad 3×3).
///
/// Equivalente al conteo naive O(n²) pero O(n) en la práctica: permite
/// 100k puntos donde el naive tardaría horas. Determinista (el conteo no
/// depende del orden de inserción).
pub fn unit_pairs_spatial(points: &[Point2], tol: f64) -> Result<usize, String> {
    if points.is_empty() {
        return Err("harness: se requiere al menos 1 punto".into());
    }
    if !tol.is_finite() || !(1e-12..=1e-3).contains(&tol) {
        return Err(format!(
            "harness: tolerancia '{tol}' fuera de [1e-12, 1e-3]"
        ));
    }
    for (idx, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(format!("harness: punto {idx} no finito"));
        }
    }
    let lo = (1.0 - tol).max(0.0);
    let lo2 = lo * lo;
    let hi = 1.0 + tol;
    let hi2 = hi * hi;
    // Celda 1.0: un par unitario solo puede estar en celdas vecinas (3×3).
    let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (idx, p) in points.iter().enumerate() {
        let cx = p.x.floor() as i64;
        let cy = p.y.floor() as i64;
        grid.entry((cx, cy)).or_default().push(idx);
    }
    let mut count = 0usize;
    for (idx, p) in points.iter().enumerate() {
        let cx = p.x.floor() as i64;
        let cy = p.y.floor() as i64;
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                let Some(bucket) = grid.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &j in bucket {
                    if j >= idx {
                        continue; // cada par una sola vez
                    }
                    let q = points[j];
                    // Prefiltro por caja antes de la raíz (rápido y exacto).
                    if (p.x - q.x).abs() > hi || (p.y - q.y).abs() > hi {
                        continue;
                    }
                    let d2 = dist2(*p, q);
                    if d2 >= lo2 && d2 <= hi2 {
                        count += 1;
                    }
                }
            }
        }
    }
    Ok(count)
}

/// Aristas unitarias con índice espacial, acotadas a `max_edges`.
pub fn unit_edges_spatial(
    points: &[Point2],
    tol: f64,
    max_edges: usize,
) -> Result<Vec<(usize, usize)>, String> {
    if points.is_empty() {
        return Err("harness: se requiere al menos 1 punto".into());
    }
    if !tol.is_finite() || !(1e-12..=1e-3).contains(&tol) {
        return Err(format!(
            "harness: tolerancia '{tol}' fuera de [1e-12, 1e-3]"
        ));
    }
    let lo = (1.0 - tol).max(0.0);
    let lo2 = lo * lo;
    let hi = 1.0 + tol;
    let hi2 = hi * hi;
    let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (idx, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(format!("harness: punto {idx} no finito"));
        }
        let cx = p.x.floor() as i64;
        let cy = p.y.floor() as i64;
        grid.entry((cx, cy)).or_default().push(idx);
    }
    let mut edges = Vec::new();
    for (idx, p) in points.iter().enumerate() {
        let cx = p.x.floor() as i64;
        let cy = p.y.floor() as i64;
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                let Some(bucket) = grid.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &j in bucket {
                    if j >= idx {
                        continue;
                    }
                    let q = points[j];
                    if (p.x - q.x).abs() > hi || (p.y - q.y).abs() > hi {
                        continue;
                    }
                    let d2 = dist2(*p, q);
                    if d2 >= lo2 && d2 <= hi2 {
                        edges.push((j, idx));
                        if edges.len() > max_edges {
                            return Err(format!(
                                "harness: aristas exceden el máximo {max_edges}; bajá escala/n o subí GRAFITO_LAB_MAX_EDGES (techo {HARD_MAX_EDGES})"
                            ));
                        }
                    }
                }
            }
        }
    }
    // Orden canónico para DIMACS determinista.
    edges.sort();
    Ok(edges)
}

/// Distancias distintas (O(n²) inherente): tope honesto `MAX_DISTINCT_N`.
pub fn distinct_count_bounded(points: &[Point2], quant: f64) -> Result<usize, String> {
    if points.len() > MAX_DISTINCT_N {
        return Err(format!(
            "distancias distintas con n={} excede el tope honesto {MAX_DISTINCT_N} (O(n²) = {} pares); usá una muestra o quedate con pares unitarios",
            points.len(),
            points.len().saturating_mul(points.len().saturating_sub(1)) / 2
        ));
    }
    grafito_geometry::search::distinct_distances(points, quant).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espacial_coincide_con_naive_en_cuadrado_y_r3x3() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        assert_eq!(unit_pairs_spatial(&pts, 1e-9).unwrap(), 4);
        let grid = grafito_geometry::search::grid_point_set(3, 3, 1.0).unwrap();
        let naive = grafito_geometry::search::unit_pairs(&grid, 1e-9).unwrap();
        assert_eq!(unit_pairs_spatial(&grid, 1e-9).unwrap(), naive);
        assert_eq!(naive, 12);
    }

    #[test]
    fn aristas_espaciales_ordenadas_y_acotadas() {
        let grid = grafito_geometry::search::grid_point_set(4, 4, 1.0).unwrap();
        let edges = unit_edges_spatial(&grid, 1e-9, 1_000_000).unwrap();
        let mut sorted = edges.clone();
        sorted.sort();
        assert_eq!(edges, sorted);
        assert!(unit_edges_spatial(&grid, 1e-9, 2).is_err());
    }

    #[test]
    fn ids_estables_y_validados() {
        let id = run_id_for(Family::Seeded, 12, 42, 3, 9);
        assert!(is_valid_run_id(&id));
        assert!(id.starts_with("t39-seeded-n12-h"));
        assert!(!is_valid_run_id(""));
        assert!(!is_valid_run_id("con espacios"));
        assert!(is_valid_cnf_hash(&"a".repeat(64)));
        assert!(!is_valid_cnf_hash("zz"));
        assert_eq!(sha256_hex("").len(), 64);
    }

    #[test]
    fn familia_random_se_rechaza_con_guia() {
        assert!(Family::parse("random").is_err());
        assert_eq!(Family::parse("GRID").unwrap(), Family::Grid);
        assert_eq!(Family::parse("").unwrap(), Family::Seeded);
    }
}
