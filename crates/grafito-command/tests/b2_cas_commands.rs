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
