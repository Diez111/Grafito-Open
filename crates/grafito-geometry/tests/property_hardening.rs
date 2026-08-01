use grafito_geometry::ast::parse_ast;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn exact_and_approximate_ast_inputs_evaluate_without_panicking() {
    let cases = [
        ("0", 0.0),
        ("-0", -0.0),
        ("2^53", 9_007_199_254_740_992.0),
        ("1 / 3", 1.0 / 3.0),
        ("sqrt(2)", 2.0_f64.sqrt()),
    ];

    for (input, expected) in cases {
        let result = catch_unwind(AssertUnwindSafe(|| {
            parse_ast(input).map(|expr| expr.eval_at("x", f64::EPSILON))
        }));
        let value = result
            .expect("AST evaluation must not panic")
            .expect("edge expression should parse");

        assert!(
            (value - expected).abs() <= f64::EPSILON * expected.abs().max(1.0),
            "unexpected result for {input:?}: {value}"
        );
    }
}

#[test]
fn ast_rejects_unknown_characters_and_extra_function_arguments() {
    for input in [
        "1@+2",
        "sin(0, 1)",
        "atan2(1, 2, 3)",
        "clamp(1, 0, 2, 3)",
        "sum(x, x, 0, 2, 4)",
    ] {
        assert!(parse_ast(input).is_err(), "{input:?} must be rejected");
    }

    for input in ["sin(0)", "atan2(1, 2)", "clamp(1, 0, 2)", "sum(x, x, 0, 2)"] {
        assert!(parse_ast(input).is_ok(), "{input:?} must remain supported");
    }

    let error = parse_ast("π@").expect_err("unsupported input must report its source offset");
    assert!(error.contains("byte offset 2"), "{error}");
}

#[test]
fn bounded_ast_corpus_never_panics() {
    let mut corpus = vec![
        "1 / 0".to_string(),
        "sqrt(-1)".to_string(),
        "(".repeat(257),
        "1+".to_string(),
        "1e-300".to_string(),
    ];
    let mut state = 0x1234_5678_ABCD_EF01_u64;

    for _ in 0..96 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let numerator = (state % 10_000) as i64 - 5_000;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let denominator = (state % 997) + 1;
        corpus.push(format!("(({numerator} / {denominator}) + sin(x))"));
    }

    for input in corpus {
        let result = catch_unwind(AssertUnwindSafe(|| {
            parse_ast(&input).map(|expr| expr.eval_at("x", 1.0 / 3.0))
        }));
        assert!(result.is_ok(), "AST input panicked: {input:?}");
    }
}
