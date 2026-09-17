//! Cobertura del compilador plano sobre un corpus de aula (T4).
//!
//! Cada expresión del corpus debe parsear (gramática que la app acepta) y,
//! salvo excepciones documentadas, compilar a `ValidatedOps`. El piso
//! (`PISO_COBERTURA_FLAT`) se calibró midiendo en este box: si cae, el
//! sampler pierde el camino rápido en escenas reales y hay que mirar
//! `compile_ast` antes de hablar de SIMD.

use grafito_geometry::expr::{compile_flat_ops, prepare_function_ast};
use std::collections::BTreeMap;

/// (expresión, variables libres). `None` en vars = función de `x`.
fn corpus_1var() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("x^2+2*x+1", vec![]),
        ("x^3-3*x", vec![]),
        ("(x^2+1)/(x-2)", vec![]),
        ("1/(1+x^2)", vec![]),
        ("sin(x)", vec![]),
        ("cos(2*x)", vec![]),
        ("tan(x/4)", vec![]),
        ("sin(x)^2+cos(x)^2", vec![]),
        ("asin(x/2)", vec![]),
        ("atan(x)", vec![]),
        ("exp(-x^2)", vec![]),
        ("ln(x+3)", vec![]),
        ("sqrt(x+5)", vec![]),
        ("cbrt(x-1)", vec![]),
        ("2^x", vec![]),
        ("abs(x-1)", vec![]),
        ("abs(sin(x))", vec![]),
        ("sinh(x)", vec![]),
        ("cosh(x)-1", vec![]),
        ("tanh(x/2)", vec![]),
        ("sin(atan(x))", vec![]),
        ("exp(sin(x))", vec![]),
        ("ln(abs(x)+1)", vec![]),
        ("sin(sin(sin(sin(sin(x)))))", vec![]),
        ("x*sin(x)+cos(x^2)", vec![]),
        ("(x+1)*(x-1)*(x+2)", vec![]),
        ("x^4-5*x^2+4", vec![]),
        ("sec(x)", vec![]),
        ("csc(x+1)", vec![]),
        ("cot(x/2)", vec![]),
        ("x^2", vec!["p"]),
        ("sin(p*x)", vec!["p"]),
    ]
}

/// Pares paramétricos `(xt, yt)` sobre `t`.
fn corpus_parametrico() -> Vec<(&'static str, &'static str)> {
    vec![
        ("cos(t)", "sin(t)"),
        ("t*cos(t)", "t*sin(t)"),
        ("t^2", "t^3-t"),
        ("exp(t)*cos(t)", "exp(t)*sin(t)"),
        ("t", "t^2"),
        ("cosh(t)", "sinh(t)"),
    ]
}

/// Implícitas `f(x,y) = 0`.
fn corpus_implicito() -> Vec<&'static str> {
    vec![
        "x^2+y^2-25",
        "x^2/9+y^2/4-1",
        "y-x^2",
        "sin(x)-y",
        "x^3+y^3-6*x*y",
        "exp(-(x^2+y^2))",
    ]
}

fn compila(expr: &str, muestreo: &[&str], libres: &[&str], v1: &str, v2: &str, v3: &str) -> bool {
    let vars = BTreeMap::new();
    let mut ignorar: Vec<&str> = muestreo.to_vec();
    ignorar.extend_from_slice(libres);
    let Ok(ast) = prepare_function_ast(expr, &vars, &ignorar) else {
        return false;
    };
    compile_flat_ops(&ast, v1, v2, v3).is_some()
}

#[test]
fn flat_cubre_corpus_aula() {
    let mut ok = 0usize;
    let mut total = 0usize;
    let mut fallos = Vec::new();
    for (expr, libres) in corpus_1var() {
        total += 1;
        let (v2, v3) = match libres.as_slice() {
            [] => ("", ""),
            [a] => (*a, ""),
            [a, b] => (*a, *b),
            _ => ("", ""),
        };
        if libres.len() <= 2 && compila(expr, &["x"], &libres, "x", v2, v3) {
            ok += 1;
        } else {
            fallos.push(expr.to_string());
        }
    }
    for (xt, yt) in corpus_parametrico() {
        for expr in [xt, yt] {
            total += 1;
            if compila(expr, &["t"], &[], "t", "", "") {
                ok += 1;
            } else {
                fallos.push(expr.to_string());
            }
        }
    }
    for expr in corpus_implicito() {
        total += 1;
        if compila(expr, &["x", "y"], &[], "x", "y", "") {
            ok += 1;
        } else {
            fallos.push(expr.to_string());
        }
    }
    println!(
        "cobertura flat: {ok}/{total} ({:.1}%)",
        100.0 * ok as f64 / total as f64
    );
    for fallo in &fallos {
        println!("  sin flat: {fallo}");
    }
    // Piso calibrado 2026-09-17 en este box (ver salida del test). Si cae,
    // el sampler pierde el camino rápido: mirar `compile_ast` primero.
    const PISO_COBERTURA_FLAT: f64 = 100.0;
    assert!(
        100.0 * ok as f64 / total as f64 >= PISO_COBERTURA_FLAT,
        "cobertura flat bajo el piso ({fallos:?})"
    );
}
