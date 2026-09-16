//! Frente P0.3 — comandos complejos: CSolve/CSolutions/NSolutions/Solutions +
//! ToComplex/ToPolar/ToPoint.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_command::commands::{find_object_by_label, process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run_in(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn ok_in(document: &mut Document, command: &str) -> String {
    match run_in(document, command) {
        CommandOutcome::Message(text) => text,
        CommandOutcome::Ok => String::new(),
        other => panic!("{command} esperaba éxito, llegó {other:?}"),
    }
}

fn err_in(document: &mut Document, command: &str) -> String {
    match run_in(document, command) {
        CommandOutcome::Error(text) => text,
        other => panic!("{command} esperaba error honesto, llegó {other:?}"),
    }
}

#[test]
fn p03_csolve_quadratic_complex_pair() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "CSolve[x^2+1]");
    assert!(out.contains("1i") && out.contains("-1i"), "par ±i: {out}");
}

#[test]
fn p03_csolve_real_and_cubic() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "CSolve[x^2-4]");
    assert!(out.contains("-2") && out.contains('2'), "reales: {out}");
    let cubic = ok_in(&mut document, "CSolve[x^3-6*x^2+11*x-6]");
    assert!(
        cubic.contains('1') && cubic.contains('2') && cubic.contains('3'),
        "cúbica 1,2,3: {cubic}"
    );
}

#[test]
fn p03_csolve_honest_limits() {
    let mut document = Document::new();
    assert!(err_in(&mut document, "CSolve[x+y]").contains("varias variables"));
    assert!(err_in(&mut document, "CSolve[sin(x)]").contains("polinomio"));
}

#[test]
fn p03_csolutions_verifies_residual() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "CSolutions[x^2+1]");
    assert!(out.contains("residuo"), "verificación visible: {out}");
}

#[test]
fn p03_nsolutions_lists_all_real() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "NSolutions[x^3-6*x^2+11*x-6]");
    assert!(out.contains("{1, 2, 3}"), "todas las reales: {out}");
}

#[test]
fn p03_solutions_accepts_equation() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "Solutions[x^2=4]");
    assert!(out.contains("-2") && out.contains('2'), "ecuación: {out}");
}

#[test]
fn p03_to_complex_and_polar() {
    let mut document = Document::new();
    assert!(ok_in(&mut document, "ToComplex[3,4]").contains("3+4i"));
    assert!(ok_in(&mut document, "ToComplex[3-4i]").contains("3-4i"));
    let polar = ok_in(&mut document, "ToPolar[1,1]");
    assert!(
        polar.contains("1.4142") && polar.contains("0.7853"),
        "polar: {polar}"
    );
}

#[test]
fn p03_to_point_creates_point() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "ToPoint[3+4i]");
    assert!(out.contains("(3, 4)"), "creación: {out}");
    let found = document.objects_iter().any(
        |(_, o)| matches!(o, GeoObject::Point(p) if p.position.x == 3.0 && p.position.y == 4.0),
    );
    assert!(found, "el punto (3,4) debe existir");
    let _ = find_object_by_label(&document, "A");
}
