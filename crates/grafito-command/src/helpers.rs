//! Helpers de manipulación de expresiones — extraídos de `commands.rs` para reducir el god file.
//!
//! Contiene `replace_variable` (seguro para UTF-8, respeta límites de palabra) y
//! `substitute_document_vars` (inyecta `Document.variables` con paréntesis para
//! preservar precedencia). Ambos son puros y testeables sin `Document`.

use grafito_core::Document;

/// Reemplaza una variable por otra solo en límites de palabra (identificadores completos).
/// Evita corromper nombres de funciones: `replace_variable("exp(e)", "e", "x")` → `"exp(x)"`, no `"xxp(x)"`.
pub fn replace_variable(expr: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return expr.to_string();
    }
    let mut result = String::with_capacity(expr.len());
    let bytes = expr.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + from_bytes.len() <= bytes.len() && &bytes[i..i + from_bytes.len()] == from_bytes {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok =
                i + from_bytes.len() == bytes.len() || !is_ident_char(bytes[i + from_bytes.len()]);
            if before_ok && after_ok {
                result.push_str(to);
                i += from_bytes.len();
                continue;
            }
        }
        // Handle multi-byte UTF-8 by pushing the whole character
        let ch_len = utf8_char_len(bytes[i]);
        result.push_str(&expr[i..i + ch_len]);
        i += ch_len;
    }
    result
}

/// Sustituye las variables de `document.variables` en la expresión, envolviendo
/// cada valor entre paréntesis para preservar la precedencia (p. ej. valores
/// negativos en exponentes). Las variables no finitas se ignoran.
///
/// Esto permite que las herramientas de análisis basadas en derivación
/// simbólica (que operan sobre una expresión pura en `x`) respeten el contexto
/// de variables del documento.
pub fn substitute_document_vars(expr: &str, document: &Document) -> String {
    let mut out = expr.to_string();
    for (k, v) in &document.variables {
        // `x` es la variable de la función: no se sustituye.
        if k == "x" || !v.is_finite() {
            continue;
        }
        out = replace_variable(&out, k, &format!("({})", v));
    }
    out
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::Document;

    #[test]
    fn replace_respects_word_boundaries() {
        assert_eq!(replace_variable("exp(e)", "e", "x"), "exp(x)");
        assert_eq!(replace_variable("e + e", "e", "x"), "x + x");
        assert_eq!(replace_variable("ex", "e", "x"), "ex");
    }

    #[test]
    fn substitute_ignores_non_finite_and_x() {
        let mut doc = Document::new();
        doc.variables.insert("a".into(), 2.0);
        doc.variables.insert("x".into(), 99.0);
        doc.variables.insert("b".into(), f64::NAN);
        let out = substitute_document_vars("a*x + b", &doc);
        assert!(out.contains("(2)"), "a debe sustituirse");
        assert!(out.contains("x"), "x no debe sustituirse");
        assert!(out.contains("b"), "b no finito no debe sustituirse");
    }
}
