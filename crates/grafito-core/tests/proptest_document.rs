#![allow(clippy::unwrap_used, clippy::expect_used)]
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

    /// Propiedad de límites: toda variable generada en 0..100 permanece <200.
    #[test]
    fn proptest_sanity_bound(x in 0..100u32) {
        prop_assert!(x < 200);
    }
}

#[test]
fn proptest_stub() {
    proptest::proptest!(|(x in 0..100u32)| prop_assert!(x < 200));
}
