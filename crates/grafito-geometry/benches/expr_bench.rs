use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_geometry::expr::{evaluate, CompiledExpr};
use std::collections::HashMap;

fn bench_expr_evaluation(c: &mut Criterion) {
    let expr = "sin(x) + x^2 - cos(2*x)";
    let vars = vec![("x".to_string(), 2.0)];
    let compiled = CompiledExpr::new(expr, &HashMap::new()).unwrap();

    // Clear the thread-local cache so the interpreted path does real work each
    // iteration (it re-parses/preprocesses on every call).
    c.bench_function("interpreted_eval", |b| {
        b.iter(|| evaluate(black_box(expr), black_box(&vars)).unwrap())
    });

    c.bench_function("compiled_eval", |b| {
        b.iter(|| compiled.eval(black_box(&vars)).unwrap())
    });
}

criterion_group!(benches, bench_expr_evaluation);
criterion_main!(benches);
