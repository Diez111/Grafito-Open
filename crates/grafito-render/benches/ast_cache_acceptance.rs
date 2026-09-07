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
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{Document, FunctionObj, GeoObject};
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

criterion_group!(benches, bench_cold_10_funcs, bench_prepare_ast_10x);
criterion_main!(benches);
