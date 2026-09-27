#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Regresión de performance de los hot paths de caché del render.
//!
//! - `cachekey_debug_format` vs `cachekey_estructural`: la cola actual de
//!   `transformed_cache_key` (`lib.rs`) hashea `format!("{:?}", inner)`; la
//!   referencia estructural hashea discriminante + bits sin alocaciones.
//!   Mismo fixture (polígono de 2000 vértices, como `lighting_cache.rs`) y
//!   mismo poder discriminante (asertado una vez en el setup).
//! - `aabb_frio_muestreo_4000` vs `aabb_cacheado_hit`: `object_world_aabb`
//!   muestrea curvas paramétricas/polares con 4000 pts en frío; el segundo
//!   bench mide el path cacheado tal cual lo deje el dueño del hot path
//!   (muestras cacheadas o AABB memoizado — hoy ~20 ns, ver tabla base).
//!
//! Config corta para CI: 15 muestras, 1 s de medición.
//!
//! Re-correr: `cargo bench -p grafito-render --bench cache_keys --locked -- --quick`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{
    Document, GeoObject, ParametricCurve2DObj, PolarCurveObj, PolygonObj, TransformedObj,
};
use grafito_geometry::{Point2, ViewTransform};
use grafito_render::object_world_aabb;
use std::collections::hash_map::DefaultHasher;
use std::f64::consts::TAU;
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// Cola Debug actual de `transformed_cache_key`: `format!("{:?}", inner)` +
/// hash. Réplica el COSTO, no el valor (la función real es privada).
fn debug_key_inner(inner: &GeoObject) -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{inner:?}").hash(&mut hasher);
    hasher.finish()
}

/// Clave estructural de referencia (sin alocaciones): discriminante + bits de
/// los campos que definen la geometría. Coordinada por nombre con los agentes
/// que optimizan el hot path; el resto de variantes cae al discriminante.
fn structural_key_inner(inner: &GeoObject) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(inner).hash(&mut hasher);
    match inner {
        GeoObject::Polygon(polygon) => {
            polygon.vertices.len().hash(&mut hasher);
            for vertex in &polygon.vertices {
                vertex.x.to_bits().hash(&mut hasher);
                vertex.y.to_bits().hash(&mut hasher);
            }
        }
        GeoObject::ParametricCurve2D(curve) => {
            curve.expr_x.hash(&mut hasher);
            curve.expr_y.hash(&mut hasher);
            curve.t_min.to_bits().hash(&mut hasher);
            curve.t_max.to_bits().hash(&mut hasher);
        }
        GeoObject::PolarCurve(polar) => {
            polar.expr_r.hash(&mut hasher);
            polar.t_min.to_bits().hash(&mut hasher);
            polar.t_max.to_bits().hash(&mut hasher);
        }
        GeoObject::Transformed(transformed) => {
            transformed.complex_expr.hash(&mut hasher);
            structural_key_inner(&transformed.inner).hash(&mut hasher);
        }
        _ => {}
    }
    hasher.finish()
}

/// Polígono de 2000 vértices tras `Transformed("z + 2")`: el `format!("{:?}")`
/// serializa los 2000 puntos en cada llamada (mismo fixture que
/// `bench_transformed_clone_vs_shared` en `lighting_cache.rs`).
fn transformed_fixture_2000gon() -> TransformedObj {
    let ring: Vec<Point2> = (0..2000)
        .map(|i| {
            let angle = i as f64 / 2000.0 * TAU;
            Point2::new(angle.cos() * 10.0, angle.sin() * 10.0)
        })
        .collect();
    TransformedObj::new(GeoObject::Polygon(PolygonObj::new(ring)), "z + 2")
}

/// Curva trig densa: el AABB frío la muestrea con 4000 pts + eval trigonométrica.
fn trig_curve() -> ParametricCurve2DObj {
    ParametricCurve2DObj::new("t", "sin(37*t)+0.5*sin(71*t)", 0.0, TAU * 4.0)
}

/// Polar trig densa: el otro path de `object_world_aabb` con 4000 pts.
fn trig_polar() -> PolarCurveObj {
    PolarCurveObj::new("3+sin(9*t)+0.5*cos(23*t)", 0.0, TAU * 2.0)
}

fn bench_cachekey(c: &mut Criterion) {
    let transformed = transformed_fixture_2000gon();
    // Mismo poder discriminante en ambos esquemas (una vez, no por iter).
    assert_eq!(
        debug_key_inner(&transformed.inner),
        debug_key_inner(&transformed.inner)
    );
    assert_eq!(
        structural_key_inner(&transformed.inner),
        structural_key_inner(&transformed.inner)
    );
    let distinta = TransformedObj::new(
        GeoObject::Polygon(PolygonObj::new(vec![Point2::new(0.0, 0.0)])),
        "z + 2",
    );
    assert_ne!(
        debug_key_inner(&transformed.inner),
        debug_key_inner(&distinta.inner),
        "Debug debe distinguir geometrías distintas"
    );
    assert_ne!(
        structural_key_inner(&transformed.inner),
        structural_key_inner(&distinta.inner),
        "la clave estructural debe distinguir geometrías distintas"
    );
    println!(
        "perf-base cachekey: debug_hash={:016x} structural_hash={:016x}",
        debug_key_inner(&transformed.inner),
        structural_key_inner(&transformed.inner)
    );

    c.bench_function("cachekey_debug_format", |b| {
        b.iter(|| black_box(debug_key_inner(black_box(&transformed.inner))))
    });
    c.bench_function("cachekey_estructural", |b| {
        b.iter(|| black_box(structural_key_inner(black_box(&transformed.inner))))
    });
}

fn bench_aabb(c: &mut Criterion) {
    let view = ViewTransform::new(800.0, 600.0);
    let document = Document::new();

    let curva = GeoObject::ParametricCurve2D(trig_curve());
    let aabb_curva = object_world_aabb(&view, &document, &curva).expect("aabb curva trig");
    println!(
        "perf-base aabb curva: min=({:.3},{:.3}) max=({:.3},{:.3})",
        aabb_curva.min.x, aabb_curva.min.y, aabb_curva.max.x, aabb_curva.max.y
    );
    let polar = GeoObject::PolarCurve(trig_polar());
    let aabb_polar = object_world_aabb(&view, &document, &polar).expect("aabb polar trig");
    println!(
        "perf-base aabb polar: min=({:.3},{:.3}) max=({:.3},{:.3})",
        aabb_polar.min.x, aabb_polar.min.y, aabb_polar.max.x, aabb_polar.max.y
    );

    c.bench_function("aabb_frio_muestreo_4000", |b| {
        b.iter(|| {
            let objeto = GeoObject::ParametricCurve2D(trig_curve());
            black_box(object_world_aabb(
                black_box(&view),
                black_box(&document),
                black_box(&objeto),
            ))
        })
    });
    c.bench_function("aabb_cacheado_hit", |b| {
        b.iter(|| {
            black_box(object_world_aabb(
                black_box(&view),
                black_box(&document),
                black_box(&curva),
            ))
        })
    });
    c.bench_function("aabb_polar_frio_muestreo_4000", |b| {
        b.iter(|| {
            let objeto = GeoObject::PolarCurve(trig_polar());
            black_box(object_world_aabb(
                black_box(&view),
                black_box(&document),
                black_box(&objeto),
            ))
        })
    });
    c.bench_function("aabb_polar_cacheado_hit", |b| {
        b.iter(|| {
            black_box(object_world_aabb(
                black_box(&view),
                black_box(&document),
                black_box(&polar),
            ))
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(15)
        .measurement_time(Duration::from_secs(1))
        .warm_up_time(Duration::from_millis(500));
    targets = bench_cachekey, bench_aabb
);
criterion_main!(benches);
