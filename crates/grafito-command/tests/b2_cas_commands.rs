#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente B2 — los 7 comandos del kernel pragmático responden por paleta.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

fn run(command: &str) -> CommandOutcome {
    let mut document = Document::new();
    let mut input = command.to_string();
    process_input(&mut document, &mut input)
}

fn ok_text(command: &str) -> String {
    match run(command) {
        CommandOutcome::Message(text) => text,
        CommandOutcome::Ok => String::new(),
        other => panic!("{command} esperaba éxito, llegó {other:?}"),
    }
}

fn err_text(command: &str) -> String {
    match run(command) {
        CommandOutcome::Error(text) => text,
        other => panic!("{command} esperaba error honesto, llegó {other:?}"),
    }
}

#[test]
fn b2_commands_happy_path() {
    assert!(ok_text("SolveODEN[{1,0,1}, 0]").contains("cos"));
    assert!(ok_text("EulerODE[1, 0, 0]").contains("C1"));
    assert!(ok_text("FrobeniusSeries[0, 0]").contains("y1"));
    assert!(ok_text("LaplaceDeriv[1, y, t, s, {0}]").contains('s'));
    assert!(ok_text("LaplaceInt[1]").contains('s'));
    assert!(ok_text("GroebnerOrdered[{x+y-3,x-y-1}, {x,y}, lex]").contains("x"));
    assert!(ok_text("Eliminate[{x+y-3,x-y-1}, {x,y}, {y}]").contains("x"));
}

#[test]
fn b2_commands_honest_errors() {
    assert!(!err_text("SolveODEN[{1}, 0, 0, 0, 0, 0, 0, 0, 0, 0]").is_empty());
    assert!(!err_text("GroebnerOrdered[{x}, {x}, foo]").is_empty());
    assert!(!err_text("FrobeniusSeries[x]").is_empty());
}

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
fn assume_refines_simplify_with_conditions() {
    let mut document = Document::new();
    // Sin hipótesis: x/x queda intacto y sin coletilla.
    let plain = ok_in(&mut document, "Simplify[x/x]");
    assert!(
        !plain.contains("(si:"),
        "sin hipótesis no hay coletilla: {plain}"
    );
    // Con x≠0 la hipótesis descarga la condición: colapsa sin coletilla.
    let _ = run_in(&mut document, "Assume[x!=0]");
    let refined = ok_in(&mut document, "Simplify[x/x]");
    assert!(refined.contains('1'), "debe colapsar a 1: {refined}");
    assert!(!refined.contains("(si:"), "hipótesis descargada: {refined}");
    // Con hipótesis ajena (y>0) la condición de x sí se declara.
    let mut other = Document::new();
    let _ = run_in(&mut other, "Assume[y>0]");
    let cond = ok_in(&mut other, "Simplify[x/x]");
    assert!(
        cond.contains("(si: x ≠ 0)"),
        "debe declarar la condición: {cond}"
    );
}

#[test]
fn assume_positive_unlocks_abs_and_exp_log() {
    let mut document = Document::new();
    let _ = run_in(&mut document, "Assume[x>0]");
    // Descargado: abs(x) = x sin coletilla.
    let abs = ok_in(&mut document, "Simplify[abs(x)]");
    assert!(abs.ends_with("= x"), "abs descargado: {abs}");
    let back = ok_in(&mut document, "Simplify[exp(ln(x))]");
    assert!(back.ends_with("= x"), "exp-ln descargado: {back}");
}

#[test]
fn assume_refines_limit_input() {
    let mut document = Document::new();
    let _ = run_in(&mut document, "Assume[x!=0]");
    // (x^2/x con x≠0 llega como x al motor: límite en 2 vale 2.
    let out = ok_in(&mut document, "Limit[x^2/x,x,2]");
    assert!(out.contains('2'), "límite refinado: {out}");
}
