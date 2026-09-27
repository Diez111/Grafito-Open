//! `edge`: parseo de aristas `.edge` y grafo unitario.
//!
//! Port de `lab/hn_hunt.py` (`load_edge`, `numeric_edges`) más la verificación
//! de aristas de `lab/parse874.py` (conteo contra 874.edge).
//!
//! NO portado (a propósito): nada de SAT/`subprocess`/paths de `hn_hunt.py`.
//! La construcción por distancia unitaria reutiliza
//! `grafito-geometry::search::unit_graph_edges` (tolerancia `<= tol`; el `.py`
//! usaba `< 1e-9` estricto — borde irrelevante, paridad exacta en fixtures).
//!
//! El formato `.edge` es 1-based (`e a b`); acá se devuelve 0-based,
//! normalizado `(min, max)`, sin duplicados y ordenado.

use grafito_geometry::search::{self, SearchError};
use grafito_geometry::Point2;
use thiserror::Error;

/// Errores del parser de aristas, con línea (1-based) cuando aplica.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum EdgeError {
    /// Línea `e` sin exactamente 3 campos (`e a b`, como el unpack del `.py`).
    #[error("aristas línea {line}: se esperaba 'e a b', vino '{preview}'")]
    BadArity { line: usize, preview: String },
    /// Índices no enteros (el `.py` reventaba con `ValueError`).
    #[error("aristas línea {line}: índices no enteros")]
    BadInt { line: usize },
    /// Índice 0 o negativo (el formato es 1-based).
    #[error("aristas línea {line}: índice 1-based inválido")]
    BadOneBased { line: usize },
    /// Arista fuera de `[0, n)`.
    #[error("aristas línea {line}: índice {idx} fuera de [0, {n})")]
    OutOfBounds { line: usize, idx: usize, n: usize },
    /// Lazo `e k k` (el `.py` lo metía al set sin chistar).
    #[error("aristas: lazo en {idx} no admitido")]
    SelfLoop { idx: usize },
    /// Error del harness al construir aristas numéricas.
    #[error("aristas numéricas: {0}")]
    Numeric(#[from] SearchError),
}

/// Parsea un texto `.edge` a pares 0-based, fiel a `hn_hunt.load_edge`.
///
/// Ignora la cabecera `p ...`, comentarios y líneas en blanco; junta
/// duplicados en un set ordenado. Líneas `e` malformadas → `Err` (el `.py`
/// reventaba con traceback).
pub fn parse_edge_text(text: &str) -> Result<Vec<(usize, usize)>, EdgeError> {
    let mut edges: Vec<(usize, usize)> = Vec::with_capacity(text.len() / 6 + 8);
    for (idx, line) in text.lines().enumerate() {
        let nline = idx + 1;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let mut it = t.split_whitespace();
        if it.next() != Some("e") {
            continue;
        }
        let (Some(a_str), Some(b_str), None) = (it.next(), it.next(), it.next()) else {
            return Err(EdgeError::BadArity {
                line: nline,
                preview: t.chars().take(60).collect(),
            });
        };
        let a: i64 = a_str
            .parse()
            .map_err(|_| EdgeError::BadInt { line: nline })?;
        let b: i64 = b_str
            .parse()
            .map_err(|_| EdgeError::BadInt { line: nline })?;
        if a < 1 || b < 1 {
            return Err(EdgeError::BadOneBased { line: nline });
        }
        let (a, b) = (a as usize - 1, b as usize - 1);
        edges.push((a.min(b), a.max(b)));
    }
    edges.sort_unstable();
    edges.dedup();
    Ok(edges)
}

/// Valida aristas contra `n_points` vértices: rango y sin lazos.
pub fn validate_edges(edges: &[(usize, usize)], n_points: usize) -> Result<(), EdgeError> {
    for (a, b) in edges {
        if *a >= n_points {
            return Err(EdgeError::OutOfBounds {
                line: 0,
                idx: *a,
                n: n_points,
            });
        }
        if *b >= n_points {
            return Err(EdgeError::OutOfBounds {
                line: 0,
                idx: *b,
                n: n_points,
            });
        }
        if a == b {
            return Err(EdgeError::SelfLoop { idx: *a });
        }
    }
    Ok(())
}

/// Parsea y valida de una vez (rango contra `n_points`, sin lazos).
pub fn parse_edge_text_validated(
    text: &str,
    n_points: usize,
) -> Result<Vec<(usize, usize)>, EdgeError> {
    let edges = parse_edge_text(text)?;
    validate_edges(&edges, n_points)?;
    Ok(edges)
}

/// Aristas por distancia unitaria, espejo de `hn_hunt.numeric_edges`.
///
/// Delega en `search::unit_graph_edges` (no se duplica); el `Err` del harness
/// se mapea tal cual.
pub fn numeric_unit_edges(points: &[Point2], tol: f64) -> Result<Vec<(usize, usize)>, EdgeError> {
    Ok(search::unit_graph_edges(points, tol)?)
}
