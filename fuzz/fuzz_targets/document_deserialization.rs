#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_INPUT_BYTES: usize = 65_536;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(&data[..data.len().min(MAX_INPUT_BYTES)]);
    let _ = grafito_core::deserialize_document(&input);
});
