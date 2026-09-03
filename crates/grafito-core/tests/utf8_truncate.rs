#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Property tests for UTF-8 safe truncation (emoji / CJK).
//!
//! Production code does `while !s.is_char_boundary(end) { end -= 1 }` before slicing
//! (see `crates/grafito-app/src/assistant.rs:3047` `assistant_correction_prompt`,
//!  `crates/grafito-ui/src/assistant.rs:5987`, `crates/grafito-command/src/commands.rs:14128`).
//! These tests guarantee that any truncation at an arbitrary byte budget never panics,
//! always lands on a char boundary, and preserves prefix semantics for emoji/CJK strings.

use proptest::prelude::*;

/// Mirrors the production truncate pattern.
fn truncate_to_boundary(s: &str, max_bytes: usize) -> &str {
    let mut end = s.len().min(max_bytes);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[test]
fn truncate_emoji_and_cjk_examples_are_char_boundaries() {
    let cases = [
        ("hello 😀 world 🌍", 10),
        ("hello 😀 world 🌍", 7),
        ("hello 😀 world 🌍", 6),
        ("日本語テスト", 5),
        ("日本語テスト", 9),
        ("a\u{0300}b😀c漢字d", 4), // combining + emoji + kanji
        ("𝄞𝄞𝄞abc", 5),             // 4-byte musical symbols
        ("", 10),
        ("abc", 0),
        ("abc", 100),
    ];
    for (s, max) in cases {
        let t = truncate_to_boundary(s, max);
        assert!(
            s.is_char_boundary(t.len()),
            "truncated len must be char boundary for {s:?} @ {max}"
        );
        assert!(
            t.len() <= max,
            "truncated len {} > max {max} for {s:?}",
            t.len()
        );
        assert!(
            s.starts_with(t),
            "truncated must be prefix of original for {s:?}"
        );
        // Re-slicing must not panic and must be valid UTF-8
        assert!(std::str::from_utf8(t.as_bytes()).is_ok());
    }
}

#[test]
fn truncate_zero_and_full_length_are_correct() {
    let s = "CJK: 漢字 emoji: 😀😀";
    assert_eq!(truncate_to_boundary(s, 0), "");
    assert_eq!(truncate_to_boundary(s, s.len()), s);
    assert_eq!(truncate_to_boundary(s, s.len() + 100), s);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any String + any byte budget must truncate to a char boundary without panic.
    #[test]
    fn proptest_truncate_is_always_char_boundary(
        s in any::<String>(),
        max_bytes in 0usize..8192
    ) {
        let t = truncate_to_boundary(&s, max_bytes);
        prop_assert!(s.is_char_boundary(t.len()), "not a char boundary: {:?} len {} max {}", s, t.len(), max_bytes);
        prop_assert!(t.len() <= max_bytes.min(s.len()));
        prop_assert!(s.starts_with(t));
        prop_assert!(std::str::from_utf8(t.as_bytes()).is_ok());
    }

    /// Specifically emoji / CJK heavy strings — ensure is_char_boundary invariant.
    #[test]
    fn proptest_truncate_emoji_cjk_is_char_boundary(
        // Mix ASCII, CJK (U+4E00..U+9FFF), emoji (😀/🌍/𝄞/漢)
        s in prop::collection::vec(
            prop_oneof![
                prop::char::range('a', 'z'),
                prop::char::range('\u{4E00}', '\u{9FFF}'),
                Just('😀'), Just('🌍'), Just('漢'), Just('あ'), Just('𝄞'), Just('\u{0300}')
            ],
            0..200
        ).prop_map(|chars| chars.into_iter().collect::<String>()),
        max_bytes in 0usize..1024
    ) {
        let t = truncate_to_boundary(&s, max_bytes);
        prop_assert!(s.is_char_boundary(t.len()));
        prop_assert!(t.len() <= max_bytes.min(s.len()));
        prop_assert!(s.starts_with(t));
    }

    /// Monotonicity: larger budget must not produce shorter truncation.
    #[test]
    fn proptest_truncate_monotonic(
        s in any::<String>(),
        a in 0usize..2048,
        b in 0usize..2048
    ) {
        let ta = truncate_to_boundary(&s, a);
        let tb = truncate_to_boundary(&s, b);
        if a <= b {
            prop_assert!(ta.len() <= tb.len(), "a={a} b={b} ta={} tb={}", ta.len(), tb.len());
        } else {
            prop_assert!(tb.len() <= ta.len());
        }
    }
}
