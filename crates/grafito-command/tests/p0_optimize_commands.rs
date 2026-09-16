//! Frente P0.5 — optimización, EDO numérica y gráficos discretos.

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

fn err_in(document: &mut Document, command: &str) -> String {
    match run_in(document, command) {
        CommandOutcome::Error(text) => text,
        other => panic!("{command} esperaba error honesto, llegó {other:?}"),
    }
}

#[test]
fn p05_minimize_maximize() {
    let mut document = Document::new();
    let lo = ok_in(&mut document, "Minimize[x^2-4*x+1,x,-10,10]");
    assert!(lo.contains("= -3") && lo.contains("x = 2"), "mínimo: {lo}");
    assert!(lo.contains("no garantizado"), "honestidad: {lo}");
    let hi = ok_in(&mut document, "Maximize[sin(x),x,0,6.2832]");
    assert!(hi.contains('1'), "máximo: {hi}");
    let def = ok_in(&mut document, "Minimize[x^2,x]");
    assert!(def.contains("= 0"), "defecto [-10,10]: {def}");
}

#[test]
fn p05_minimize_rejects_bad_interval() {
    let mut document = Document::new();
    assert!(err_in(&mut document, "Minimize[x,x,5,5]").contains("a < b"));
    assert!(err_in(&mut document, "Minimize[x+y,x]").contains("varias variables"));
}

#[test]
fn p05_nsolve_ode_exponential_with_graph() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "NSolveODE[y,0,1,1]");
    assert!(out.contains("2.718"), "e^1: {out}");
    assert!(out.contains("graficados"), "gráfico: {out}");
    // Tabla + scatter enlazado creados.
    assert!(document.object_count() >= 2, "objetos creados");
}

#[test]
fn p05_nsolve_ode_rejects() {
    let mut document = Document::new();
    assert!(err_in(&mut document, "NSolveODE[y,0,1,0]").contains("diferir"));
    assert!(err_in(&mut document, "NSolveODE[y,0,1,1,1]").contains('n'));
}

#[test]
fn p05_slope_field() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "SlopeField[x+y]");
    assert!(out.contains("SlopeField"), "creado: {out}");
    assert!(err_in(&mut document, "SlopeField[]").contains("SlopeField"));
}

#[test]
fn p05_step_stick_line_graphs() {
    let mut document = Document::new();
    for (cmd, name) in [
        ("StepGraph[{0,1,2},{0,1,4}]", "StepGraph"),
        ("StickGraph[{0,1,2},{0,1,4}]", "StickGraph"),
        ("LineGraph[{0,1,2},{0,1,4}]", "LineGraph"),
    ] {
        let out = ok_in(&mut document, cmd);
        assert!(out.contains(name), "{cmd}: {out}");
    }
    // Serie simple usa índices como x.
    let single = ok_in(&mut document, "StepGraph[{5,6,7}]");
    assert!(single.contains("StepGraph"), "serie simple: {single}");
    assert!(err_in(&mut document, "StepGraph[{1}]").contains("dos pares"));
}

#[test]
fn p05_normal_quantile_plot() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "NormalQuantilePlot[{1,2,3,4,5}]");
    assert!(out.contains("NormalQuantilePlot"), "creado: {out}");
    assert!(err_in(&mut document, "NormalQuantilePlot[{1,2}]").contains('3'));
}
