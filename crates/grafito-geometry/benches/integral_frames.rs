#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! F5 — bench INFORMATIVO (sin gate) de la matematica de frames de integral.
//!
//! Los 48 frames nativos de integral (`render_integral_frames`, grafito-app,
//! `pub(crate)`) muestran la curva canonica fija `f(x)=x²` + area acumulada
//! por trapecios con el evaluador existente. Este bench costea exactamente esa
//! acumulacion (`eval_integral_hybrid` sobre `x²` en `[0, 2]`) a dos
//! resoluciones: 200 muestras (base) y 2000 (estres prespuestado).
//!
//! Valor exacto de referencia: ∫₀² x² dx = 8/3 ≈ 2.6667 (se pinnea con
//! tolerancia 1e-6 para que el bench tambien sea testigo de correccion).
//! BLOQUEADOR honesto del RGBA: ver `crates/grafito-anim/benches/native_frames.rs`.
//!
//! Criterio de aceptacion F5: corre con
//! `cargo bench -p grafito-geometry --bench integral_frames -- --test`,
//! imprime su numero base y NO bloquea CI. Sin `unwrap` en prod (solo bench).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_geometry::integral::eval_integral_hybrid;

fn bench_integral_frames(c: &mut Criterion) {
    // Numero base impreso + testigo de correccion.
    let base = eval_integral_hybrid(|x: f64| x * x, 0.0, 2.0, 200);
    println!("F5-base integral_x2_200: area={base:.6} (exacto 2.666667)");
    assert!(
        (base - 8.0 / 3.0).abs() < 1e-6,
        "x² en [0,2] debe dar 8/3, dio {base}"
    );

    c.bench_function("integral_x2_200", |b| {
        b.iter(|| {
            eval_integral_hybrid(
                black_box(|x: f64| x * x),
                black_box(0.0),
                black_box(2.0),
                black_box(200),
            )
        });
    });

    c.bench_function("integral_x2_2000", |b| {
        b.iter(|| {
            eval_integral_hybrid(
                black_box(|x: f64| x * x),
                black_box(0.0),
                black_box(2.0),
                black_box(2000),
            )
        });
    });
}

criterion_group!(benches, bench_integral_frames);
criterion_main!(benches);
