#![no_main]

use libfuzzer_sys::fuzz_target;

// Expr domain: small, bounded before AST parsing.
const MAX_INPUT_BYTES: usize = 4_096;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(&data[..data.len().min(MAX_INPUT_BYTES)]);
    // Expr parsing with budget — must not panic on arbitrary UTF-8.
    let _ = grafito_geometry::ast::parse_ast(&input);
    // Validation layer must not panic and must reject over-long exprs.
    let _ = grafito_core::validation::parse_document_json(&input);
    // Direct document validation path (empty doc is always cheap).
    let doc = grafito_core::Document::new();
    let _ = grafito_core::validation::validate_document(&doc);
});
