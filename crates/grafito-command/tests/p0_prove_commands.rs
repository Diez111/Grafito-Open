//! Frente P0.4 — demostración: Prove/ProveDetails/Relation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

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

#[test]
fn p04_prove_linear_consequence() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "Prove[x=2, {x+y-3, x-y-1}, {x, y}]");
    assert!(out.contains("verdadero"), "lineal demostrado: {out}");
}

#[test]
fn p04_prove_identity_and_false() {
    let mut document = Document::new();
    let ident = ok_in(&mut document, "Prove[(x+1)^2=x^2+2*x+1]");
    assert!(ident.contains("verdadero"), "identidad: {ident}");
    let fake = ok_in(&mut document, "Prove[1=2]");
    assert!(fake.contains("falso"), "falso constante: {fake}");
}

#[test]
fn p04_prove_unknown_is_honest() {
    let mut document = Document::new();
    // Trigonométrica: fuera del ideal polinómico → indefinido, no falso.
    let out = ok_in(&mut document, "Prove[sin(x)^2+cos(x)^2=1]");
    assert!(out.contains("indefinido"), "honesto: {out}");
}

#[test]
fn p04_prove_details_shows_basis() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "ProveDetails[x=2, {x+y-3, x-y-1}, {x, y}]");
    assert!(
        out.contains("verdadero") && out.contains("base"),
        "traza: {out}"
    );
}

#[test]
fn p04_relation_symbolic_numeric_counterexample() {
    let mut document = Document::new();
    let sym = ok_in(&mut document, "Relation[x-x, 0]");
    assert!(sym.contains("simbólico"), "simbólico: {sym}");
    let num = ok_in(&mut document, "Relation[(x+1)^2, x^2+2*x+1]");
    assert!(
        num.contains("puntos") && num.contains("Prove"),
        "sugiere: {num}"
    );
    let bad = ok_in(&mut document, "Relation[x^2, 2*x]");
    assert!(
        bad.contains("falso") && bad.contains("contraejemplo"),
        "refuta: {bad}"
    );
}

#[test]
fn p04_prove_budget_is_honest() {
    let mut document = Document::new();
    let out = run_in(&mut document, "Prove[x=0, {x}, {x}, {y}]");
    assert!(
        matches!(out, CommandOutcome::Error(_)),
        "4 args es error honesto: {out:?}"
    );
}
