//! CSV RFC 4180 puro para paridad GeoGebra (frente F10-C).
//!
//! Sin dependencias: serializa y parsea tablas de dos columnas
//! (`DataTableObj`) y listas de datos. Cotas coherentes con
//! `grafito_geometry::statistics::MAX_FIT_DATA_POINTS` (20 000 pares).

use thiserror::Error;

/// Máximo de filas por documento CSV (cabeza + datos).
pub const MAX_CSV_ROWS: usize = 20_000;
/// Máximo de bytes de entrada/salida por documento CSV (10 MiB como el doc).
pub const MAX_CSV_BYTES: usize = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CsvError {
    #[error("CSV supera el máximo de {MAX_CSV_ROWS} filas (recibidas {got})")]
    TooManyRows { got: usize },
    #[error("CSV supera el máximo de {MAX_CSV_BYTES} bytes (recibidos {got})")]
    TooManyBytes { got: usize },
    #[error("CSV con comilla sin cerrar en la fila {row}")]
    UnclosedQuote { row: usize },
}

/// Escapa un campo según RFC 4180 §2.7: si contiene coma, comilla, CR o LF
/// se envuelve en comillas y cada comilla se duplica.
pub fn escape_field(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        let mut out = String::with_capacity(field.len() + 2);
        out.push('"');
        for ch in field.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
        out
    } else {
        field.to_string()
    }
}

/// Serializa filas a CSV con terminador CRLF (RFC 4180 §2.1).
pub fn to_csv(rows: &[Vec<String>]) -> Result<String, CsvError> {
    if rows.len() > MAX_CSV_ROWS {
        return Err(CsvError::TooManyRows { got: rows.len() });
    }
    let mut out = String::new();
    for row in rows {
        for (index, field) in row.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&escape_field(field));
        }
        out.push_str("\r\n");
    }
    if out.len() > MAX_CSV_BYTES {
        return Err(CsvError::TooManyBytes { got: out.len() });
    }
    Ok(out)
}

/// Parsea CSV aceptando CRLF y LF; implementa comillas y `""` → `"`.
/// Devuelve error honesto ante comilla sin cerrar o exceso de cotas.
pub fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, CsvError> {
    if text.len() > MAX_CSV_BYTES {
        return Err(CsvError::TooManyBytes { got: text.len() });
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut record_has_content = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        record_has_content = true;
        match ch {
            '"' if !in_quotes => {
                in_quotes = true;
            }
            '"' => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                current_row.push(std::mem::take(&mut current));
            }
            '\r' if !in_quotes => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                current_row.push(std::mem::take(&mut current));
                rows.push(std::mem::take(&mut current_row));
                record_has_content = false;
            }
            '\n' if !in_quotes => {
                current_row.push(std::mem::take(&mut current));
                rows.push(std::mem::take(&mut current_row));
                record_has_content = false;
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            row: rows.len() + 1,
        });
    }
    if record_has_content || !current.is_empty() || !current_row.is_empty() {
        current_row.push(current);
        rows.push(current_row);
    }
    if rows.len() > MAX_CSV_ROWS {
        return Err(CsvError::TooManyRows { got: rows.len() });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_plain_field_is_identity() {
        assert_eq!(escape_field("hola"), "hola");
    }

    #[test]
    fn escape_quotes_and_commas() {
        assert_eq!(escape_field("a,b"), "\"a,b\"");
        assert_eq!(escape_field("dice \"x\""), "\"dice \"\"x\"\"\"");
        assert_eq!(escape_field("l1\nl2"), "\"l1\nl2\"");
    }

    #[test]
    fn roundtrip_with_quotes() {
        let rows = vec![
            vec!["x".to_string(), "y".to_string()],
            vec!["1,5".to_string(), "dice \"si\"".to_string()],
        ];
        let csv = to_csv(&rows).expect("csv fixture");
        assert!(csv.contains("\r\n"));
        let back = parse_csv(&csv).expect("parse fixture");
        assert_eq!(back, rows);
    }

    #[test]
    fn parse_accepts_lf_and_crlf() {
        let rows = parse_csv("a,b\n1,2\r\n3,4\n").expect("lf/crlf fixture");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn unclosed_quote_is_honest_error() {
        let err = parse_csv("\"abc,def").expect_err("debe fallar");
        assert!(matches!(err, CsvError::UnclosedQuote { .. }));
    }

    #[test]
    fn too_many_rows_is_rejected() {
        let rows = vec![vec!["a".to_string()]; MAX_CSV_ROWS + 1];
        assert!(matches!(to_csv(&rows), Err(CsvError::TooManyRows { .. })));
    }
}
