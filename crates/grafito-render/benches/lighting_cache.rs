#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! perf-profiler 2026-09-10 — antes/después de:
//! a) `Light::shade` (Blinn-Phong) vs `calculate_lighting` legacy;
//! b) hit compartido `build_transformed_geometry_static_shared` (Arc,
//!    cero-copia) vs `build_transformed_geometry_static` (clona los Vecs).
//!
//! Pineo de presupuestos propios del render (corre en `-- --test` y full):
//! LRU 64, vértices 1M, índices 3M, prisma 64, celdas 250k. Los 48/64MiB
//! (anim) y 64/8M/5MB (GIF en app) viven fuera de scope y se verifican por
//! lectura — ver reporte del turno.
//!
//! RGBA por plantilla: BLOQUEADO honesto — `render_*_frames` vive tras
//! `pub(crate) mod anim_native` en grafito-app (sucio F1-F4, no tocable).
//! Bench exacto propuesto para el dueño en el reporte del turno.
//!
//! Criterio: `cargo bench -p grafito-render --bench lighting_cache -- --test`
//! imprime números base, NO bloquea CI. Sample chico (20) para quick.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{Document, GeoObject, TransformedObj};
use grafito_geometry::{Color, Point2, ViewTransform};
use grafito_render::{calculate_lighting, Light, Renderer};

fn bench_budgets() {
    // Solo lo público (el resto —Lru 64, verts 1M, idx 3M, celdas 250k— lo
    // pinea el unit test `presupuestos_render_pineados` dentro del crate).
    assert_eq!(grafito_render::MAX_PRISM_BASE_VERTICES, 64);
    println!("perf-presupuestos render (bench): prisma=64 OK (resto en unit test)");
}

fn bench_lighting_legacy_vs_shade(c: &mut Criterion) {
    let base = Color::new(0.9, 0.2, 0.2, 1.0);
    let normal = glam::Vec3::new(0.0, 0.0, 1.0);
    let light = Light::DEFAULT;
    // Canon dorado (debe coincidir con `lighting_golden_pinea_canon_con_specular`).
    let legacy = calculate_lighting(base, normal, Light::DEFAULT_DIR);
    let nuevo = light.shade(base, normal);
    assert!((legacy.r - 0.5566089).abs() < 1e-6);
    assert!((nuevo.r - 0.5567734).abs() < 1e-6);
    println!(
        "perf-base lighting: legacy_r={:.7} shade_r={:.7}",
        legacy.r, nuevo.r
    );

    c.bench_function("lighting_legacy", |b| {
        b.iter(|| {
            calculate_lighting(
                black_box(base),
                black_box(normal),
                black_box(Light::DEFAULT_DIR),
            )
        })
    });
    c.bench_function("light_shade_blinn_phong", |b| {
        b.iter(|| light.shade(black_box(base), black_box(normal)))
    });
}

fn transformed_fixture() -> (Document, TransformedObj, ViewTransform) {
    // Polígono de 2000 vértices: el hit clonaba ~64 KiB por llamada en el
    // path `Vec`; el path `Arc` solo clona el puntero. Con 4 vértices el
    // delta se pierde en el ruido del hash de la clave (~1.3µs).
    let ring: Vec<Point2> = (0..2000)
        .map(|i| {
            let a = i as f64 / 2000.0 * std::f64::consts::TAU;
            Point2::new(a.cos() * 10.0, a.sin() * 10.0)
        })
        .collect();
    let transformed = TransformedObj::new(
        GeoObject::Polygon(grafito_core::PolygonObj::new(ring)),
        "z + 2",
    );
    let mut document = Document::new();
    document.add_object(GeoObject::Transformed(transformed.clone()));
    (document, transformed, ViewTransform::new(800.0, 600.0))
}

fn bench_transformed_clone_vs_shared(c: &mut Criterion) {
    let (document, transformed, view) = transformed_fixture();
    // Precalienta el LRU thread-local: desde aquí todo es HIT.
    let (warm_v, _) =
        Renderer::build_transformed_geometry_static_shared(&document, &transformed, &view, false);
    println!(
        "perf-base transformed_hit: verts={} idx=calentado",
        warm_v.len()
    );

    c.bench_function("transformed_hit_clona_vecs", |b| {
        b.iter(|| {
            let (v, i) = Renderer::build_transformed_geometry_static(
                black_box(&document),
                black_box(&transformed),
                black_box(&view),
                false,
            );
            black_box((v.len(), i.len()))
        })
    });
    c.bench_function("transformed_hit_shared_arc", |b| {
        b.iter(|| {
            let (v, i) = Renderer::build_transformed_geometry_static_shared(
                black_box(&document),
                black_box(&transformed),
                black_box(&view),
                false,
            );
            black_box((v.len(), i.len()))
        })
    });
}

fn perf_benches(c: &mut Criterion) {
    bench_budgets();
    bench_lighting_legacy_vs_shade(c);
    bench_transformed_clone_vs_shared(c);
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(std::time::Duration::from_secs(2));
    targets = perf_benches
);
criterion_main!(benches);
