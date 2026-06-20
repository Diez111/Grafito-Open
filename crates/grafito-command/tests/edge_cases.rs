//! Tests de edge case para el procesador de comandos de texto.
//!
//! Verifican que entradas degeneradas, inválidas o extremas no provoquen
//! pánicos y produzcan un `CommandOutcome` coherente.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

#[test]
fn empty_command_does_not_panic() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "");
    // La convención es que un comando vacío retorna Ok (no-op).
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "empty command should be a no-op Ok, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn unknown_bracketed_command_returns_error() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "FooBar[1,2]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "unknown command FooBar should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0, "no object should be created");
}

#[test]
fn function_with_empty_args_returns_error_no_panic() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Function[]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "Function with no expression should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn nan_in_point_input_does_not_panic() {
    let mut doc = Document::new();
    // Whatever the outcome, the key invariant is: no panic.
    let outcome = run(&mut doc, "Point[(nan, 0)]");
    assert!(
        !matches!(outcome, CommandOutcome::Ok) || doc.object_count() == 0,
        "NaN point should not silently create an object, got {:?}",
        outcome
    );
}

#[test]
fn division_by_zero_in_function_does_not_panic() {
    let mut doc = Document::new();
    // Creating the function must not evaluate it, so 1/0 must not panic here.
    let outcome = run(&mut doc, "Function[1/0]");
    match outcome {
        CommandOutcome::Message(_) | CommandOutcome::Ok => {}
        CommandOutcome::Error(e) => panic!("Function[1/0] should not error at creation: {}", e),
    }
}

#[test]
fn spanish_alias_analizar_works() {
    let mut doc = Document::new();
    // First create a function so Analizar has something to analyze.
    run(&mut doc, "Function[sin(x)]");
    assert!(doc.object_count() >= 1);

    let outcome = run(&mut doc, "Analizar[sin(x)]");
    assert!(
        matches!(
            outcome,
            CommandOutcome::Message(_) | CommandOutcome::Error(_)
        ),
        "Analizar (Spanish alias) should produce a message or error, got {:?}",
        outcome
    );
    // The key invariant: it does not panic and is recognized (not a silent Ok
    // for an unknown command).
    if let CommandOutcome::Error(e) = &outcome {
        assert!(
            !e.contains("no reconocido"),
            "Analizar should be recognized, got error: {}",
            e
        );
    }
}

#[test]
fn circle_with_insufficient_args_returns_error() {
    let mut doc = Document::new();
    // Circle[(0,0)] lacks the radius argument.
    let outcome = run(&mut doc, "Circle[(0,0)]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "Circle with insufficient args should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0, "no circle should be created");
}

#[test]
fn whitespace_only_command_does_not_panic() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "   \t  ");
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "whitespace-only command should be a no-op Ok, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn deeply_nested_brackets_do_not_panic() {
    let mut doc = Document::new();
    // Pathological nesting should be handled gracefully.
    let outcome = run(&mut doc, "Function[sin(sin(sin(sin(sin(x)))))]]]");
    // We only require that it does not panic; any non-panic outcome is fine.
    let _ = outcome;
}
