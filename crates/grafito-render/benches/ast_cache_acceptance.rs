#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Aceptación B4: ¿cuánto del build en FRÍO de 10 funciones es parseo?
//!
//! - `cold_10_funcs`: documento fresco por iteración (muestras frías:
//!   parse + eval + build). Es el costo del primer frame / pan lejos.
//! - `prepare_ast_10x`: solo el parseo (`prepare_function_ast`) de las
//!   10 expresiones del bench existente. Aísla el numerador.
//! - Referencia caliente: `build_geometry_with_functions` en
//!   `render_scenarios.rs` (~220µs, muestras cacheadas).
//!
//! Gate B4: implementar AST cache en `render_2d.rs` solo si
//! `prepare_ast_10x / cold_10_funcs >= 15%`.
//!
//! Frente P3 (H3): `ImplicitCurveObj::get_cached_asts` clona ambos AST por
//! objeto por frame + hashea exprs y variables en cada llamada.
//!
//! - `h3_asts_cache_miss`: primer acceso (parsea).
//! - `h3_asts_cache_hit`: misma clave repetida (hash + clones, sin parseo).
//!
//! Gate P3: pasar a `Arc` + hash solo en cambio de versión solo si
//! `(miss - hit)` no explica el costo y el `hit` por sí solo es ≥10% del
//! frame (medido contra `cold_10_funcs` como referencia de orden).
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{Document, FunctionObj, GeoObject, ImplicitCurveObj, RelationOperator};
use grafito_geometry::ViewTransform;
use grafito_render::Renderer;
use std::collections::HashMap;

fn view_800x600() -> ViewTransform {
    ViewTransform::new(800.0, 600.0)
}

fn bench_cold_10_funcs(c: &mut Criterion) {
    let view = view_800x600();
    c.bench_function("cold_10_funcs", |b| {
        b.iter(|| {
            let mut doc = Document::new();
            for i in 0..10 {
                doc.add_object(GeoObject::Function(FunctionObj::new(format!(
                    "sin({}*x)",
                    i + 1
                ))));
            }
            let (vertices, indices) =
                Renderer::build_geometry_static(black_box(&doc), black_box(&view), false, true);
            black_box((vertices.len(), indices.len()));
        })
    });
}

fn bench_prepare_ast_10x(c: &mut Criterion) {
    let variables: HashMap<String, f64> = HashMap::new();
    let exprs: Vec<String> = (1..=10).map(|i| format!("sin({i}*x)")).collect();
    c.bench_function("prepare_ast_10x", |b| {
        b.iter(|| {
            for expr in &exprs {
                let ast = grafito_geometry::expr::prepare_function_ast(
                    black_box(expr),
                    black_box(&variables),
                    &["x"],
                )
                .unwrap();
                black_box(ast);
            }
        })
    });
}

fn curve_obj() -> ImplicitCurveObj {
    ImplicitCurveObj::new("sin(x)*cos(y)+x*y", "x*x+y*y-1", RelationOperator::Eq)
}

fn bench_h3_asts_cache_miss(c: &mut Criterion) {
    let variables: HashMap<String, f64> = HashMap::new();
    c.bench_function("h3_asts_cache_miss", |b| {
        b.iter(|| {
            let obj = curve_obj();
            let pair = obj
                .get_cached_asts(black_box(&variables), &["x", "y"])
                .unwrap();
            black_box(pair);
        })
    });
}

fn bench_h3_asts_cache_hit(c: &mut Criterion) {
    let variables: HashMap<String, f64> = HashMap::new();
    let obj = curve_obj();
    let _ = obj.get_cached_asts(&variables, &["x", "y"]).unwrap();
    c.bench_function("h3_asts_cache_hit", |b| {
        b.iter(|| {
            let pair = obj
                .get_cached_asts(black_box(&variables), &["x", "y"])
                .unwrap();
            black_box(pair);
        })
    });
}

criterion_group!(
    benches,
    bench_cold_10_funcs,
    bench_prepare_ast_10x,
    bench_h3_asts_cache_miss,
    bench_h3_asts_cache_hit
);
criterion_main!(benches);
