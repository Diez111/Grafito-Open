#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use std::ops::Range;

const FORBIDDEN_INSERTION_CALLS: &[&str] = &[
    ".add_object(",
    ".add_point(",
    "Document::add_object(",
    "Document::add_point(",
];

fn cfg_test_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut cursor = 0;

    while let Some(relative_marker) = source[cursor..].find("#[cfg(test)]") {
        let marker = cursor + relative_marker;
        let item = marker + "#[cfg(test)]".len();
        let open = source[item..].find('{').map(|offset| item + offset);
        let semicolon = source[item..].find(';').map(|offset| item + offset);

        if semicolon.is_some_and(|semicolon| open.is_none_or(|open| semicolon < open)) {
            let end = semicolon.expect("semicolon was checked") + 1;
            ranges.push(marker..end);
            cursor = end;
            continue;
        }

        let Some(open) = open else {
            break;
        };
        let mut depth = 0usize;
        let mut end = source.len();
        for (relative, byte) in source.as_bytes()[open..].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + relative + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        ranges.push(marker..end);
        cursor = end;
    }

    ranges
}

#[test]
fn production_app_code_uses_only_fallible_insertion_apis() {
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let entries = std::fs::read_dir(&source_dir).expect("read grafito-app source directory");

    for entry in entries {
        let path = entry.expect("read source entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            || path.file_name().and_then(|name| name.to_str()) == Some("tests.rs")
        {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("read Rust source");
        let test_ranges = cfg_test_ranges(&source);
        for forbidden in FORBIDDEN_INSERTION_CALLS {
            for (offset, _) in source.match_indices(forbidden) {
                if test_ranges.iter().any(|range| range.contains(&offset)) {
                    continue;
                }
                let line_number = source[..offset]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1;
                let line = source[..offset]
                    .rsplit_once('\n')
                    .map_or(&source[..offset], |(_, line)| line)
                    .trim_start();
                if line.starts_with("//") {
                    continue;
                }
                panic!(
                    "{}:{line_number} uses deprecated insertion call {forbidden}",
                    path.display()
                );
            }
        }
    }
}
