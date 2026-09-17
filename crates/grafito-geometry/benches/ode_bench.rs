//! Bench ODE (Ola 2 B2/B14): integrador con campo preparado una vez.
//!
//! Informativo (sin gate): mide `runge_kutta_4` + `euler` sobre `y' = -2·t·y`
//! con el mismo esquema del comando `ODE` (prepare + opcodes una vez, loop
//! plano por paso). Objetivo del plan: 8191 pasos < 5 ms.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_geometry::expr::{
    compile_flat_ops, eval_opcodes_flat, prepare_function_ast, ValidatedOps,
};
use std::collections::BTreeMap;

fn prepared_field(expr: &str) -> (ValidatedOps, grafito_geometry::ast::Expr) {
    let vars = BTreeMap::new();
    let ast = prepare_function_ast(expr, &vars, &["t", "y"]).expect("parse");
    let ops = compile_flat_ops(&ast, "t", "y", "").expect("opcodes");
    (ops, ast)
}

fn bench_ode_rk4(c: &mut Criterion) {
    let (ops, ast) = prepared_field("-2*t*y");
    c.bench_function("ode_rk4_2000_pasos_campo_plano", |b| {
        b.iter(|| {
            let f = |t: f64, y: f64| -> f64 {
                eval_opcodes_flat(&ops, t, y, 0.0).unwrap_or_else(|| {
                    let r = ast.eval_2d("t", t, "y", y);
                    if r.is_finite() {
                        r
                    } else {
                        f64::NAN
                    }
                })
            };
            let sol = grafito_geometry::ode::runge_kutta_4(
                f,
                black_box(0.0),
                black_box(1.0),
                black_box(2.0),
                black_box(2000),
            );
            assert_eq!(sol.len(), 2001);
            black_box(sol);
        })
    });
}

fn bench_ode_euler(c: &mut Criterion) {
    let (ops, ast) = prepared_field("-2*t*y");
    c.bench_function("ode_euler_2000_pasos_campo_plano", |b| {
        b.iter(|| {
            let f = |t: f64, y: f64| -> f64 {
                eval_opcodes_flat(&ops, t, y, 0.0).unwrap_or_else(|| {
                    let r = ast.eval_2d("t", t, "y", y);
                    if r.is_finite() {
                        r
                    } else {
                        f64::NAN
                    }
                })
            };
            let sol = grafito_geometry::ode::euler(
                f,
                black_box(0.0),
                black_box(1.0),
                black_box(2.0),
                black_box(2000),
            );
            assert_eq!(sol.len(), 2001);
            black_box(sol);
        })
    });
}

criterion_group!(benches, bench_ode_rk4, bench_ode_euler);
criterion_main!(benches);
