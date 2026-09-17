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

    // Ola 2 B1: walk recursivo vs loop plano, mismo AST preparado, 5000
    // puntos. Decide con datos si el SIMD (B10) vale la pena.
    {
        use grafito_geometry::expr::{compile_flat_ops, eval_opcodes_flat, prepare_function_ast};
        use std::collections::BTreeMap;
        for (nombre, texto) in [("sin_poly", "sin(x) + x^2 - cos(2*x)"), ("trivial", "x+1")] {
            let ast = prepare_function_ast(texto, &BTreeMap::new(), &["x"]).expect("parse");
            let ops = compile_flat_ops(&ast, "x", "", "").expect("opcodes");
            let xs: Vec<f64> = (0..5000)
                .map(|i| -3.0 + 6.0 * (i as f64) / 5000.0)
                .collect();
            c.bench_function(&format!("walk_{nombre}_5000"), |b| {
                b.iter(|| {
                    let mut acc = 0.0;
                    for &x in &xs {
                        let v = ast.eval_at("x", black_box(x));
                        if v.is_finite() {
                            acc += v;
                        }
                    }
                    black_box(acc)
                })
            });
            c.bench_function(&format!("flat_{nombre}_5000"), |b| {
                b.iter(|| {
                    let mut acc = 0.0;
                    for &x in &xs {
                        if let Some(v) = eval_opcodes_flat(&ops, black_box(x), 0.0, 0.0) {
                            acc += v;
                        }
                    }
                    black_box(acc)
                })
            });
        }
    }
}

criterion_group!(benches, bench_expr_evaluation);
criterion_main!(benches);
