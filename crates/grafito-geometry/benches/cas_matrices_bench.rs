//! Bench CAS + matrices (Ola 2 B14): Buchberger, Gruntz y `Matrix::mul`.
//!
//! Informativo (sin gate): testigos de costo de los motores simbólicos y
//! del álgebra densa. Objetivos del plan: Buchberger chico < 100 ms,
//! `mul` 256² medido para guiar el rayon por filas (B8).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_buchberger_2x2(c: &mut Criterion) {
    let polys = vec!["x + y - 1".to_string(), "x - y - 3".to_string()];
    let vars = vec!["x".to_string(), "y".to_string()];
    c.bench_function("cas_buchberger_2x2_lineal", |b| {
        b.iter(|| {
            let out = grafito_geometry::cas::buchberger_basis(black_box(&polys), black_box(&vars))
                .expect("base 2x2");
            black_box(out);
        })
    });
}

fn bench_gruntz(c: &mut Criterion) {
    c.bench_function("cas_gruntz_sin_x_sobre_x", |b| {
        b.iter(|| {
            let out = grafito_geometry::cas::gruntz_limit(
                black_box("sin(x)/x"),
                black_box("x"),
                black_box(0.0),
            )
            .expect("límite clásico");
            black_box(out);
        })
    });
}

fn bench_matrix_mul_256(c: &mut Criterion) {
    let n = 256usize;
    let data: Vec<f64> = (0..n * n).map(|i| (i % 97) as f64 * 0.5).collect();
    let a = grafito_geometry::matrices::Matrix::new(n, n, data.clone()).expect("matriz A");
    let b = grafito_geometry::matrices::Matrix::new(n, n, data).expect("matriz B");
    c.bench_function("matrices_mul_256", |c| {
        // `iter_batched`: el clon de setup no contamina la medición del `mul`.
        c.iter_batched(
            || (a.clone(), b.clone()),
            |(a, b)| a.mul(&b).expect("producto"),
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_buchberger_2x2,
    bench_gruntz,
    bench_matrix_mul_256
);
criterion_main!(benches);
