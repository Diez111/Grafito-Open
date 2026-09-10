#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_geometry::expr::{evaluate, CompiledExpr};
use std::collections::BTreeMap;

fn bench_expr_evaluation(c: &mut Criterion) {
    // Pineo de presupuesto (perf 2026-09-10): N=5000 toca el tope.
    assert_eq!(grafito_geometry::function_sampling::MAX_SAMPLE_POINTS, 5000);
    let expr = "sin(x) + x^2 - cos(2*x)";
    let vars = vec![("x".to_string(), 2.0)];
    let compiled = CompiledExpr::new(expr, &BTreeMap::new()).unwrap();

    // Clear the thread-local cache so the interpreted path does real work each
    // iteration (it re-parses/preprocesses on every call).
    c.bench_function("interpreted_eval", |b| {
        b.iter(|| evaluate(black_box(expr), black_box(&vars)).unwrap())
    });

    c.bench_function("compiled_eval", |b| {
        b.iter(|| compiled.eval(black_box(&vars)).unwrap())
    });

    // P2-perf: el camino que usa `sample_function` (parse-once + lote con
    // rayon ≥1024). N=5000 toca el tope `MAX_SAMPLE_POINTS`: mide el antes/
    // después del cambio de `evaluate`-por-punto a `eval_batch_1d`.
    c.bench_function("sample_function_5000", |b| {
        b.iter(|| {
            grafito_geometry::function_sampling::sample_function(
                black_box("sin(x) + x^2 - cos(2*x)"),
                black_box(-3.0),
                black_box(3.0),
                black_box(5000),
            )
            .unwrap()
        })
    });

    // P2-perf: curva CON polo (`1/x`): ejercita `classify_break_at`
    // (5 probes por candidato). Testigo del cambio de `evaluate` a
    // `evaluate_cached` en `eval_sample`: cada probe pasa de ~7.6µs
    // (re-parse) a ~90ns (hit LRU).
    c.bench_function("sample_function_polo_5000", |b| {
        b.iter(|| {
            grafito_geometry::function_sampling::sample_function(
                black_box("1/x"),
                black_box(-1.0),
                black_box(1.0),
                black_box(5000),
            )
            .unwrap()
        })
    });
}

criterion_group!(benches, bench_expr_evaluation);
criterion_main!(benches);
