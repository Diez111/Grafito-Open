#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_complex::parse_complex;
use num_complex::Complex64;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn variables() -> HashMap<String, Complex64> {
    HashMap::from([("z".to_string(), Complex64::new(1.0 / 3.0, -f64::EPSILON))])
}

#[test]
fn exact_and_approximate_complex_inputs_evaluate_without_panicking() {
    let cases = ["0", "-0", "2^53", "1 / 3", "1e-300 + i"];

    for input in cases {
        let result = catch_unwind(AssertUnwindSafe(|| {
            parse_complex(input).and_then(|expr| expr.eval(&variables()))
        }));
        let value = result
            .expect("complex evaluation must not panic")
            .expect("edge expression should parse and evaluate");

        assert!(
            value.re.is_finite() && value.im.is_finite(),
            "edge expression should produce a finite value: {input:?} -> {value:?}"
        );
    }
}

#[test]
fn bounded_complex_corpus_never_panics() {
    let mut corpus = vec![
        "z / (z-z)".to_string(),
        "sqrt(-1)".to_string(),
        "(".repeat(257),
        "1e-".to_string(),
        "sin(".to_string(),
    ];
    let mut state = 0xBADC_0FFE_E0DD_F00D_u64;

    for _ in 0..96 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let real = (state % 10_000) as i64 - 5_000;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let imaginary = (state % 10_000) as i64 - 5_000;
        corpus.push(format!("({real} + {imaginary}i) / (1 + z)"));
    }

    for input in corpus {
        let result = catch_unwind(AssertUnwindSafe(|| {
            parse_complex(&input).and_then(|expr| expr.eval(&variables()))
        }));
        assert!(result.is_ok(), "complex input panicked: {input:?}");
    }
}
