#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente P3 (H1/H2): superficie implícita — caché de malla y costo del
//! fallback por muestra.
//!
//! - `h1_snapshot_same_key_hit`: 2ª llamada con la misma clave (debe ser
//!   clon barato, sin re-marching). Referencia: `h1_compute_fresh`.
//! - `h1_alternating_keys_thrash`: alternar dos juegos de variables (A/B)
//!   con `mesh_snapshot`. Con caché de un solo slot cada llamada recomputa;
//!   con multi-slot debería hittear.
//! - `h2_fallback_clone_per_sample`: cota superior del costo de
//!   `fallback_vars.clone()` por muestra a 32³ (32768 clones). Solo corre en
//!   la rama fallback (`prepare_function_ast` falló); con AST válido es cero.
//!
//! Gate P3: optimizar solo si el bench avala ≥10%.
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::ImplicitSurface3DObj;
use std::collections::HashMap;
use std::time::Duration;

fn sphere(cells: usize) -> ImplicitSurface3DObj {
    ImplicitSurface3DObj::new("x*x+y*y+z*z-1", (-2.0, 2.0, -2.0, 2.0, -2.0, 2.0), cells)
}

fn vars_a() -> HashMap<String, f64> {
    HashMap::from([("a".to_string(), 1.0)])
}

fn vars_b() -> HashMap<String, f64> {
    HashMap::from([("a".to_string(), 2.0)])
}

/// Malla fresca sin caché (referencia del costo de marching).
fn bench_h1_compute_fresh(c: &mut Criterion) {
    let obj = sphere(8);
    let vars = vars_a();
    c.bench_function("h1_compute_fresh", |b| {
        b.iter(|| {
            let mesh = obj.compute_mesh(black_box(&vars)).unwrap();
            black_box(mesh.triangle_count());
        })
    });
}

/// 2ª llamada misma clave (caché de un slot ya la evita re-marching hoy).
fn bench_h1_snapshot_same_key_hit(c: &mut Criterion) {
    let obj = sphere(8);
    let vars = vars_a();
    let _ = obj.mesh_snapshot(&vars).unwrap();
    c.bench_function("h1_snapshot_same_key_hit", |b| {
        b.iter(|| {
            let mesh = obj.mesh_snapshot(black_box(&vars)).unwrap();
            black_box(mesh.triangle_count());
        })
    });
}

/// Alternancia A/B: con un solo slot cada llamada re-marcha.
fn bench_h1_alternating_keys_thrash(c: &mut Criterion) {
    let obj = sphere(8);
    let a = vars_a();
    let b = vars_b();
    let mut flip = false;
    c.bench_function("h1_alternating_keys_thrash", |bencher| {
        bencher.iter(|| {
            flip = !flip;
            let vars = if flip { &a } else { &b };
            let mesh = obj.mesh_snapshot(black_box(vars)).unwrap();
            black_box(mesh.triangle_count());
        })
    });
}

/// Expresión que el AST nativo rechaza (`if`/`>`) pero `evaluate`
/// resuelve vía evalexpr: fuerza la rama fallback de `compute_mesh`.
fn fallback_sphere(cells: usize) -> ImplicitSurface3DObj {
    ImplicitSurface3DObj::new(
        "if(x*x+y*y+z*z>4, 1.0, -1.0)",
        (-2.0, 2.0, -2.0, 2.0, -2.0, 2.0),
        cells,
    )
}

/// Malla completa por la rama fallback (parse evalexpr por muestra).
/// Referencia para acotar el ahorro máximo de H2.
fn bench_h2_compute_fallback_full(c: &mut Criterion) {
    let obj = fallback_sphere(8);
    let vars = vars_a();
    assert!(
        grafito_geometry::expr::prepare_function_ast(&obj.expr, &vars, &["x", "y", "z"]).is_err(),
        "el bench H2 debe correr por la rama fallback"
    );
    c.bench_function("h2_compute_fallback_full", |b| {
        b.iter(|| {
            let mesh = obj.compute_mesh(black_box(&vars)).unwrap();
            black_box(mesh.triangle_count());
        })
    });
}
/// Cota superior de H2: clonar `fallback_vars` (5 vars) 32768 veces
/// (= muestras de 32³) + 3 push. Sin parsear ni evaluar.
fn bench_h2_fallback_clone_per_sample(c: &mut Criterion) {
    let fallback_vars: Vec<(String, f64)> = (0..5).map(|i| (format!("v{i}"), i as f64)).collect();
    c.bench_function("h2_fallback_clone_per_sample", |b| {
        b.iter(|| {
            let mut acc = 0.0;
            for _ in 0..32768 {
                let mut vars = black_box(fallback_vars.clone());
                vars.push(("x".to_string(), 1.0));
                vars.push(("y".to_string(), 2.0));
                vars.push(("z".to_string(), 3.0));
                acc += vars.len() as f64;
            }
            black_box(acc);
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(8));
    targets = bench_h1_compute_fresh,
        bench_h1_snapshot_same_key_hit,
        bench_h1_alternating_keys_thrash,
        bench_h2_fallback_clone_per_sample,
        bench_h2_compute_fallback_full
}
criterion_main!(benches);
