#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Tests de edge case para el procesador de comandos de texto.
//!
//! Verifican que entradas degeneradas, inválidas o extremas no provoquen
//! pánicos y produzcan un `CommandOutcome` coherente.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject, LineObj, PointObj};
use grafito_geometry::Point2;

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
fn simplify_uses_symbolic_rewrite_instead_of_numeric_evaluation() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Simplify[x + 0]");

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("x + 0 = x")),
        "got {outcome:?}"
    );
}

#[test]
fn limit_at_infinity_is_rejected_without_creating_graph_objects() {
    let mut doc = Document::new();
    let before = doc.object_count();
    let outcome = run(&mut doc, "Limit[1/x, x, inf]");

    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
    assert_eq!(doc.object_count(), before);
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

#[test]
fn statistical_commands_reject_non_finite_samples_without_mutation() {
    let cases = [
        "BoxPlot[{1, NaN, 3}]",
        "ScatterPlot[{1, inf}, {2, 3}]",
        "LinearRegression[{1, 2}, {-inf, 3}]",
    ];

    for command in cases {
        let mut doc = Document::new();
        let outcome = run(&mut doc, command);
        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(doc.object_count(), 0, "{command} must not create an object");
    }
}

#[test]
fn binomial_k_greater_than_n_returns_zero_no_panic() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Binomial[3, 0.5, 10]");
    assert!(
        matches!(outcome, CommandOutcome::Message(ref m) if m.contains("0.000000")),
        "k > n should produce zero probability, got {:?}",
        outcome
    );
}

#[test]
fn xintercept_is_recognized_despite_uppercase_x() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "XIntercept[x^2 - 1]");
    assert!(
        !matches!(outcome, CommandOutcome::Error(ref e) if e.contains("no reconocido")),
        "XIntercept should be recognized, got {:?}",
        outcome
    );
}

#[test]
fn solve_rejects_an_empty_variable_without_mutating_the_document() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Solve[1 + 1, ]");

    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn lhopital_rejects_a_quotient_that_is_not_indeterminate() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "LHopital[1+x, x, x, 0, 3]");

    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
}

#[test]
fn improper_integral_rejects_a_boundary_pole_instead_of_truncating_it() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "ImproperIntegral[1/x, x, 0, 1]");

    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
}

#[test]
fn solve_does_not_report_a_pole_as_a_root() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Solve[1/(x-0.0011), x, -1, 1]");

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("Sin raíces")),
        "got {outcome:?}"
    );
    assert_eq!(doc.object_count(), 1, "only the graph should be created");
}

#[test]
fn bolzano_check_is_inconclusive_across_a_discontinuity() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "BolzanoCheck[1/x, x, -1, 1]");

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("inconclusive")),
        "got {outcome:?}"
    );
}

#[test]
fn arc_creates_a_parametric_curve_instead_of_a_chord() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "Arc[(0,0), 1, 0, 180]");

    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "got {outcome:?}"
    );
    assert!(
        doc.objects_iter().any(|(_, object)| matches!(
            object,
            GeoObject::Arc(_) | GeoObject::ParametricCurve2D(_)
        )),
        "Arc should create an Arc or parametric curve"
    );
}

#[test]
fn point_on_object_creates_projected_point_without_stealing_original() {
    let mut doc = Document::new();
    let line_id = doc.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0)).with_label("L"),
    ));
    let point_id = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 3.0)).with_label("P"),
    ));
    let before = doc.object_count();

    let outcome = run(&mut doc, "PointOnObject[L, P]");
    assert!(matches!(outcome, CommandOutcome::Ok), "got {outcome:?}");
    assert_eq!(doc.object_count(), before + 1);
    assert!(doc.constraints.is_free(&point_id));
    let affected = doc.move_point(point_id, Point2::new(1.0, 4.0));
    assert!(affected.contains(&point_id));
    assert!(affected.len() >= 2, "projected point should depend on P");
    assert!(doc.get_object(line_id).is_some());
}

#[test]
fn ode_invalid_expression_returns_error() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "ODE[bad_fn(y), 0, 1, 2, 20]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "invalid ODE expression should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn ode_rejects_unbounded_step_counts() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "ODE[y, 0, 1, 2, 10000000]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "huge ODE step count should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn ode_system_rejects_unbounded_step_counts() {
    let mut doc = Document::new();
    let outcome = run(&mut doc, "ODESystem[y, -x, 0, 1, 0, 10, 10000000]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "huge ODE system step count should error, got {:?}",
        outcome
    );
    assert_eq!(doc.object_count(), 0);
}
