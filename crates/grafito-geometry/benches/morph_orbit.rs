#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! P2-perf — bench INFORMATIVO (sin gate) del morph puro + órbita de cámara.
//!
//! - `morph_64x48`: `morph_shapes` cuadrada→triángulo (64 muestras,
//!   48 frames = `MORPH_MAX_FRAMES`). DECISIÓN MEDIDA P2-perf: se probó
//!   `par_iter` por frames y EMPEORÓ (6.99 µs seq vs 22.5 µs par: el
//!   dispatch cuesta más que ~3k lerps); el bench queda como testigo de
//!   que secuencial es lo correcto en este presupuesto. Presupuestos
//!   intactos: samples 2..=512, frames 1..=48, tope 1 MiB.
//! - `orbit_10k`: `Camera3D::orbit` ×10 000 (puro, sin render). Testigo de
//!   que la órbita no mete NaN tras muchas vueltas (theta normalizado).
//!
//! Criterio de aceptación: corre con
//! `cargo bench -p grafito-geometry --bench morph_orbit -- --test`,
//! imprime sus números base y NO bloquea CI. Sin `unwrap` en prod
//! (solo bench).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_geometry::morph::{morph_shapes, MorphConfig, MorphEasing};
use grafito_geometry::types::Point2;
use grafito_geometry::types3d::Camera3D;

fn cuadrada() -> Vec<Point2> {
    vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ]
}

fn triangulo() -> Vec<Point2> {
    vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.5, 1.0),
    ]
}

fn bench_morph_orbit(c: &mut Criterion) {
    // Números base impresos (visibles con `-- --test --nocapture`).
    let cfg = MorphConfig::try_new(64, 48, false, true, MorphEasing::CubicInOut)
        .expect("config base válida");
    let base = morph_shapes(&cuadrada(), &triangulo(), &cfg).expect("morph base válido");
    println!(
        "P2-base morph_64x48: frames={} puntos_por_frame={}",
        base.len(),
        base.first().map_or(0, Vec::len)
    );
    assert_eq!(base.len(), 48);
    assert!(base.iter().all(|f| f.len() == 64));
    // Pineo de presupuestos (perf 2026-09-10): si cambian estos topes, el
    // bench falla honesto en vez de regresión silenciosa.
    {
        use grafito_geometry::morph::{
            MORPH_MAX_BYTES, MORPH_MAX_FRAMES, MORPH_MAX_INPUT_POINTS, MORPH_MAX_SAMPLES,
        };
        assert_eq!(MORPH_MAX_SAMPLES, 512);
        assert_eq!(MORPH_MAX_FRAMES, 48);
        assert_eq!(MORPH_MAX_INPUT_POINTS, 4096);
        assert_eq!(MORPH_MAX_BYTES, 1024 * 1024);
        println!("perf-presupuestos morph: samples=512 frames=48 inputs=4096 tope=1MiB OK");
    }

    let mut cam = Camera3D::new(16.0 / 9.0);
    for _ in 0..10_000 {
        cam.orbit(0.01, 0.005);
    }
    let pos = cam.position();
    println!(
        "P2-base orbit_10k: pos=({:.3},{:.3},{:.3}) finita={}",
        pos.x,
        pos.y,
        pos.z,
        pos.x.is_finite() && pos.y.is_finite() && pos.z.is_finite()
    );
    assert!(pos.x.is_finite() && pos.y.is_finite() && pos.z.is_finite());

    c.bench_function("morph_64x48", |b| {
        b.iter(|| {
            morph_shapes(
                black_box(&cuadrada()),
                black_box(&triangulo()),
                black_box(&cfg),
            )
            .expect("morph válido")
        })
    });

    c.bench_function("orbit_10k", |b| {
        b.iter(|| {
            let mut cam = Camera3D::new(black_box(16.0f32 / 9.0));
            for _ in 0..10_000 {
                cam.orbit(black_box(0.01), black_box(0.005));
            }
            black_box(cam.position());
        })
    });
}

criterion_group!(benches, bench_morph_orbit);
criterion_main!(benches);
