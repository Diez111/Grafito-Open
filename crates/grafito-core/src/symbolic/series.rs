//! Series de autorrelleno para la planilla (frente C2).
//!
//! `FillColumn`/`FillRow`/`FillCells` rellenan con un valor constante;
//! este módulo aporta la serie (lineal `inicio + paso·i` o geométrica
//! `inicio · paso^i`) sobre un rango 1D (`"A1:A10"`, horizontal o
//! vertical). Función pura del cerebro: sin egui, sin I/O.
//!
//! Presupuestos (fuente única, sin duplicar constantes):
//! - filas/columnas: [`Document::MAX_SPREADSHEET_ROWS`]/
//!   [`Document::MAX_SPREADSHEET_COLS`] (400×400).
//! - celdas por serie: [`Document::MAX_SPREADSHEET_RECOMPUTE_CELLS`]
//!   (10 000).
//!
//! El llamador (comando `FillSeries` o panel de planilla) aplica el
//! resultado con `stage_spreadsheet_cell_edits` de forma atómica: si
//! alguna celda desborda a no-finito, no se escribe nada.

use crate::document::Document;

/// Modo de la serie de autorrelleno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesKind {
    /// `valor[i] = inicio + paso * i`.
    Linear,
    /// `valor[i] = inicio * paso^i` (`paso` es la razón).
    Geometric,
}

/// Interpreta el modo textual del comando/panel (`"lineal"` por defecto).
///
/// Acepta `lineal`/`linear` y `geom`/`geometric`/`geometrica`/`geométrica`
/// (con o sin tildes, con comillas simples o dobles).
pub fn parse_series_mode(raw: &str) -> Result<SeriesKind, String> {
    let normalized: String = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let ascii: String = normalized
        .chars()
        .map(|c| match c {
            'é' => 'e',
            c => c,
        })
        .collect();
    match ascii.as_str() {
        "" | "lineal" | "linear" | "aritmetica" => Ok(SeriesKind::Linear),
        "geom" | "geometric" | "geometrica" | "geometrico" => Ok(SeriesKind::Geometric),
        other => Err(format!(
            "FillSeries: modo '{other}' inválido; usa \"lineal\" o \"geom\""
        )),
    }
}

/// Parsea una etiqueta de celda tipo `A1` a `(fila, columna)` 0-based.
///
/// Reglas idénticas al resto de la planilla: letras + fila ≥ 1 sin cero
/// inicial, validación canónica por reconstrucción y cotas 400×400.
fn parse_series_cell(cell: &str) -> Option<(usize, usize)> {
    let trimmed = cell
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_ascii_uppercase();
    if trimmed.is_empty() {
        return None;
    }
    let letter_count = trimmed
        .bytes()
        .take_while(|b| b.is_ascii_uppercase())
        .count();
    if letter_count == 0 || letter_count == trimmed.len() {
        return None;
    }
    let (letters, row_text) = trimmed.split_at(letter_count);
    if row_text.starts_with('0') {
        return None;
    }
    let row = row_text.parse::<usize>().ok()?.checked_sub(1)?;
    if row >= Document::MAX_SPREADSHEET_ROWS {
        return None;
    }
    let mut col: usize = 0;
    for letter in letters.bytes() {
        col = col
            .checked_mul(26)?
            .checked_add((letter - b'A' + 1) as usize)?;
    }
    let col = col.checked_sub(1)?;
    if col >= Document::MAX_SPREADSHEET_COLS {
        return None;
    }
    let mut rest = col;
    let mut rev = String::new();
    loop {
        rev.push(char::from(b'A' + (rest % 26) as u8));
        if rest < 26 {
            break;
        }
        rest = rest / 26 - 1;
    }
    let canonical = format!("{}{}", rev.chars().rev().collect::<String>(), row + 1);
    if canonical != trimmed {
        return None;
    }
    Some((row, col))
}

/// Parsea un rango `"A1:A10"` (separadores `:`, `,` o espacio) a celdas
/// 0-based en orden de recorrido.
///
/// Solo rangos 1D (una fila o una columna): un rectángulo 2D se rechaza
/// con error honesto. El conteo se acota por
/// [`Document::MAX_SPREADSHEET_RECOMPUTE_CELLS`].
pub fn parse_series_range(range: &str) -> Result<Vec<(usize, usize)>, String> {
    let trimmed = range.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if trimmed.is_empty() {
        return Err("FillSeries: rango vacío".to_string());
    }
    let endpoints: Option<((usize, usize), (usize, usize))> = [':', ',', ' ']
        .iter()
        .filter(|sep| trimmed.contains(**sep))
        .find_map(|sep| {
            let parts: Vec<&str> = trimmed
                .split(*sep)
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() == 2 {
                match (parse_series_cell(parts[0]), parse_series_cell(parts[1])) {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => None,
                }
            } else {
                None
            }
        })
        .or_else(|| parse_series_cell(trimmed).map(|cell| (cell, cell)));
    let Some(((r1, c1), (r2, c2))) = endpoints else {
        return Err(format!("FillSeries: rango inválido '{trimmed}'"));
    };
    let (row_min, row_max) = if r1 <= r2 { (r1, r2) } else { (r2, r1) };
    let (col_min, col_max) = if c1 <= c2 { (c1, c2) } else { (c2, c1) };
    let rows = row_max - row_min + 1;
    let cols = col_max - col_min + 1;
    if rows > 1 && cols > 1 {
        return Err(
            "FillSeries: el rango debe ser 1D (una fila o una columna), no un rectángulo"
                .to_string(),
        );
    }
    let count = rows
        .checked_mul(cols)
        .ok_or_else(|| "FillSeries: rango desborda".to_string())?;
    if count > Document::MAX_SPREADSHEET_RECOMPUTE_CELLS {
        return Err(format!(
            "FillSeries: {count} celdas excede máximo {}",
            Document::MAX_SPREADSHEET_RECOMPUTE_CELLS
        ));
    }
    let mut cells = Vec::with_capacity(count);
    for row in row_min..=row_max {
        for col in col_min..=col_max {
            cells.push((row, col));
        }
    }
    Ok(cells)
}

/// Construye las ediciones `(fila, columna, valor)` de una serie.
///
/// Lineal: `inicio + paso·i`. Geométrica: `inicio · paso^i`.
///
/// Todo finito o nada: si algún término desborda a no-finito se devuelve
/// `Err` sin celdas parciales (el llamador no escribe nada).
pub fn build_fill_series(
    range: &str,
    start: f64,
    step: f64,
    kind: SeriesKind,
) -> Result<Vec<(usize, usize, String)>, String> {
    if !start.is_finite() {
        return Err("FillSeries: inicio debe ser finito".to_string());
    }
    if !step.is_finite() {
        return Err("FillSeries: paso debe ser finito".to_string());
    }
    let cells = parse_series_range(range)?;
    let mut edits = Vec::with_capacity(cells.len());
    for (index, (row, col)) in cells.iter().enumerate() {
        let value = match kind {
            SeriesKind::Linear => start + step * index as f64,
            SeriesKind::Geometric => start * step.powi(index as i32),
        };
        if !value.is_finite() {
            return Err(format!(
                "FillSeries: la serie desborda a no-finito en la celda {} (término {index})",
                cell_label(*row, *col)
            ));
        }
        edits.push((*row, *col, value.to_string()));
    }
    edits.sort_unstable_by_key(|(row, col, _)| (*row, *col));
    Ok(edits)
}

/// Etiqueta canónica `A1` para mensajes de error.
fn cell_label(row: usize, col: usize) -> String {
    let mut rest = col;
    let mut rev = String::new();
    loop {
        rev.push(char::from(b'A' + (rest % 26) as u8));
        if rest < 26 {
            break;
        }
        rest = rest / 26 - 1;
    }
    format!("{}{}", rev.chars().rev().collect::<String>(), row + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_series_over_column() {
        let edits = build_fill_series("A1:A5", 1.0, 2.0, SeriesKind::Linear).expect("serie lineal");
        let values: Vec<&str> = edits.iter().map(|(_, _, v)| v.as_str()).collect();
        assert_eq!(values, vec!["1", "3", "5", "7", "9"]);
        assert_eq!(
            edits.iter().map(|(r, _, _)| *r).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn geometric_series_over_row() {
        let edits =
            build_fill_series("B2:D2", 2.0, 3.0, SeriesKind::Geometric).expect("serie geométrica");
        let values: Vec<&str> = edits.iter().map(|(_, _, v)| v.as_str()).collect();
        assert_eq!(values, vec!["2", "6", "18"]);
    }

    #[test]
    fn single_cell_range_is_constant() {
        let edits = build_fill_series("C3", 7.0, 99.0, SeriesKind::Linear).expect("una celda");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].2, "7");
    }

    #[test]
    fn rectangle_range_rejected_honestly() {
        let err = build_fill_series("A1:B2", 1.0, 1.0, SeriesKind::Linear).expect_err("2D");
        assert!(err.contains("1D"), "{err}");
    }
    #[test]
    fn row_bound_enforced() {
        let edits =
            build_fill_series("A1:A400", 1.0, 1.0, SeriesKind::Linear).expect("400 filas ok");
        assert_eq!(edits.len(), 400);
        let over = format!("A1:A{}", Document::MAX_SPREADSHEET_ROWS + 1);
        let err = build_fill_series(&over, 1.0, 1.0, SeriesKind::Linear).expect_err("fila 401");
        assert!(err.contains("inválido") || err.contains("excede"), "{err}");
    }

    #[test]
    fn non_finite_inputs_rejected() {
        assert!(build_fill_series("A1:A3", f64::NAN, 1.0, SeriesKind::Linear).is_err());
        assert!(build_fill_series("A1:A3", 1.0, f64::INFINITY, SeriesKind::Linear).is_err());
    }

    #[test]
    fn overflow_yields_no_partial_cells() {
        let err =
            build_fill_series("A1:A3", 1e308, 1e308, SeriesKind::Linear).expect_err("desborda");
        assert!(err.contains("no-finito"), "{err}");
        let err = build_fill_series("A1:A310", 1.0, 10.0, SeriesKind::Geometric)
            .expect_err("10^309 desborda");
        assert!(err.contains("no-finito"), "{err}");
    }

    #[test]
    fn mode_parsing_accepts_es_and_en() {
        assert_eq!(
            parse_series_mode("lineal").expect("lineal"),
            SeriesKind::Linear
        );
        assert_eq!(
            parse_series_mode("\"geom\"").expect("geom"),
            SeriesKind::Geometric
        );
        assert_eq!(
            parse_series_mode("geométrica").expect("geométrica"),
            SeriesKind::Geometric
        );
        assert!(parse_series_mode("cuadratica").is_err());
    }

    #[test]
    fn invalid_cells_rejected() {
        assert!(parse_series_cell("A0").is_none());
        assert!(parse_series_cell("1A").is_none());
        assert!(parse_series_cell("").is_none());
        assert!(parse_series_range("").is_err());
        assert!(parse_series_range("A1:B").is_err());
    }
}
