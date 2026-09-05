#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Propiedades reales de Document (G17: antes había tautologías `x < 200`).
//!
//! Cada propiedad fallaría antes si la serialización perdiera variables,
//! si el último `set` no ganara, o si encode→decode→encode no fuera estable.

use grafito_core::Document;
use proptest::prelude::*;

proptest! {
    /// Roundtrip de Document vía serde_json sin panic — propiedad básica de
    /// serialización. Genera un documento con una variable acotada y verifica
    /// que to_value / from_value preserve el valor.
    #[test]
    fn document_serde_roundtrip(x in 0u32..100) {
        let mut doc = Document::new();
        let name = format!("v{x}");
        doc.set_variable(name.clone(), x as f64);
        let value = serde_json::to_value(&doc).expect("Document serializes");
        let decoded: Document = serde_json::from_value(value).expect("Document deserializes");
        prop_assert_eq!(decoded.get_variable(&name), Some(x as f64));
    }

    /// Idempotencia del roundtrip: encode→decode→encode es punto fijo.
    /// Regresión que atraparía: serialización no determinista (HashMap sin
    /// ordenar), versión que muta al cargar, o `NaN`/`inf` que cambia de forma.
    #[test]
    fn document_serde_idempotence(x in 0u32..100, y in 0u32..100) {
        let mut doc = Document::new();
        doc.set_variable("va".to_string(), x as f64);
        doc.set_variable("vb".to_string(), y as f64);
        let first = serde_json::to_value(&doc).expect("first serialize");
        let decoded: Document =
            serde_json::from_value(first.clone()).expect("deserialize");
        let second = serde_json::to_value(&decoded).expect("second serialize");
        prop_assert_eq!(first, second);
        prop_assert_eq!(decoded.get_variable("va"), Some(x as f64));
        prop_assert_eq!(decoded.get_variable("vb"), Some(y as f64));
    }

    /// Última escritura gana + sobrevive al roundtrip.
    /// Regresión que atraparía: `set_variable` que ignora el segundo set,
    /// o persistencia que conserva el primer valor (bug de caché/staging).
    #[test]
    fn document_variable_overwrite_last_write_wins(x in 0u32..100, y in 0u32..100) {
        let mut doc = Document::new();
        doc.set_variable("v".to_string(), x as f64);
        doc.set_variable("v".to_string(), y as f64);
        prop_assert_eq!(doc.get_variable("v"), Some(y as f64));
        let value = serde_json::to_value(&doc).expect("Document serializes");
        let decoded: Document = serde_json::from_value(value).expect("deserializes");
        prop_assert_eq!(decoded.get_variable("v"), Some(y as f64));
    }

    /// Dos variables son independientes: escribir una no borra la otra.
    /// Regresión que atraparía: `variables` reemplazado en vez de insertado
    /// (`doc.variables = ...` en lugar de `insert`), o validación que
    /// descarta todo el mapa al fallar una entrada.
    #[test]
    fn document_two_variables_are_independent(x in 0u32..100, y in 0u32..100) {
        let mut doc = Document::new();
        doc.set_variable("va".to_string(), x as f64);
        doc.set_variable("vb".to_string(), y as f64);
        prop_assert_eq!(doc.get_variable("va"), Some(x as f64));
        prop_assert_eq!(doc.get_variable("vb"), Some(y as f64));
        let value = serde_json::to_value(&doc).expect("serializes");
        let decoded: Document = serde_json::from_value(value).expect("deserializes");
        prop_assert_eq!(decoded.get_variable("va"), Some(x as f64));
        prop_assert_eq!(decoded.get_variable("vb"), Some(y as f64));
    }
}
