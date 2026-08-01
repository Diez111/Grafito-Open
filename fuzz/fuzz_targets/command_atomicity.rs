#![no_main]

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_BYTES: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    let mut document = Document::new();
    let before = serde_json::to_value(&document).expect("empty document serializes");
    let version = document.version;
    let mut input = String::from_utf8_lossy(&data[..data.len().min(MAX_INPUT_BYTES)]).into_owned();

    let outcome = process_input(&mut document, &mut input);
    if matches!(outcome, CommandOutcome::Error(_)) {
        assert_eq!(
            serde_json::to_value(&document).expect("document serializes after rejected command"),
            before
        );
        assert_eq!(document.version, version);
    }
});
