#![no_main]

use libfuzzer_sys::fuzz_target;

// Document / spreadsheet domain: larger budget, still bounded.
const MAX_INPUT_BYTES: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(&data[..data.len().min(MAX_INPUT_BYTES)]);
    // Try to deserialize any fuzzed JSON as a Document — validations must not panic.
    if let Ok(mut doc) = grafito_core::deserialize_document(&input) {
        // Spreadsheet recompute must be bounded and must not panic on mutated state.
        let _ = doc.recompute_spreadsheet_variables();
        // Also exercise cell access with arbitrary indices derived from input.
        let len = input.len();
        if len >= 2 {
            let row = (input.as_bytes()[0] as usize) % 20;
            let col = (input.as_bytes()[1] as usize) % 20;
            let _ = doc.eval_spreadsheet_cell(row, col);
        }
    } else {
        // Also fuzz spreadsheet JSON validation directly.
        let _ = grafito_core::validation::parse_document_json(&input);
    }
});
