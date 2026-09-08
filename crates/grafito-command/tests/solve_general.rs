#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Frente B1 — `Solve` GENERAL a comandos de paleta.
//!
//! Decisión documentada: `Solve` ya existía (numérico 1-raíz + intervalo) y
//! `SolveSystem` lineal existe (dispatch `LinearSolve`); NO se duplican ni
//! renombran. `Solve` se extiende (sin intervalo → general), `NSolve` y
//! `SolveNlSystem` son comandos nuevos.

use grafito_command::command_registry;
use grafito_command::commands::{parse_cas_command, process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn message(outcome: &CommandOutcome) -> &str {
    match outcome {
        CommandOutcome::Message(text) => text,
        other => panic!("se esperaba Message, llegó {other:?}"),
    }
}

fn raiz_points(document: &Document) -> Vec<(f64, f64)> {
    let mut points: Vec<(f64, f64)> = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Point(point) if point.label.starts_with("Raíz") => {
                Some((point.position.x, point.position.y))
            }
            _ => None,
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    points
}

#[test]
fn solve_general_cubic_returns_all_three_roots() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[x^3-6x^2+11x-6,x]");
    let text = message(&outcome).to_string();
    assert!(text.contains("{1, 2, 3}"), "got {text}");
    let points = raiz_points(&document);
    assert_eq!(points.len(), 3, "got {points:?}");
    for (got, want) in points.iter().map(|p| p.0).zip([1.0, 2.0, 3.0]) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }
}

#[test]
fn solve_general_quadratic_without_reals_warns_complex() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[x^2+1,x]");
    let text = message(&outcome).to_string();
    assert!(text.contains("{}"), "got {text}");
    assert!(text.contains("complej"), "got {text}");
    assert!(raiz_points(&document).is_empty());
}

#[test]
fn solve_with_interval_keeps_legacy_numeric_behavior() {
    let mut document = Document::new();
    let outcome = run(&mut document, "Solve[x^2-4,x,-3,3]");
    let text = message(&outcome).to_string();
    assert!(
        text.contains("2.000000") || text.contains("x = "),
        "got {text}"
    );
    assert_eq!(raiz_points(&document).len(), 2);
}

#[test]
fn nsolve_returns_single_numeric_root() {
    let mut document = Document::new();
    let outcome = run(&mut document, "NSolve[x^2-4,x,0,5]");
    let text = message(&outcome).to_string();
    assert!(text.contains("2.000000"), "got {text}");
    let points = raiz_points(&document);
    assert_eq!(points.len(), 1, "got {points:?}");
    assert!((points[0].0 - 2.0).abs() < 1e-9);

    let mut trig = Document::new();
    let outcome = run(&mut trig, "NSolve[sin(x),x,-1,1]");
    let text = message(&outcome).to_string();
    assert!(text.contains("0.000000"), "got {text}");
}

#[test]
fn nsolve_without_root_is_honest_error() {
    let mut document = Document::new();
    let outcome = run(&mut document, "NSolve[x^2+1,x,-2,2]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
}

#[test]
fn solve_nl_system_circle_line_returns_points() {
    let mut document = Document::new();
    let outcome = run(&mut document, "SolveNlSystem[x^2+y^2-25, x-y-1, x, y]");
    let text = message(&outcome).to_string();
    assert!(text.contains("(4, 3)"), "got {text}");
    assert!(text.contains("(-3, -4)"), "got {text}");
    let points = raiz_points(&document);
    assert_eq!(points.len(), 2, "got {points:?}");
}

#[test]
fn solve_nl_system_dependent_is_honest_error() {
    let mut document = Document::new();
    let outcome = run(&mut document, "SolveNlSystem[x+y-1, 2*x+2*y-2, x, y]");
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "got {outcome:?}"
    );
}

#[test]
fn solve_nl_system_rejects_bad_arity_and_vars() {
    for command in [
        "SolveNlSystem[x+y-1, x, y]",
        "SolveNlSystem[x+y-1, x-y, x, x]",
        "SolveNlSystem[x+y-1, x-y, 1x, y]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: got {outcome:?}"
        );
    }
}

#[test]
fn registry_routes_nsolve_without_duplicating_solve() {
    assert_eq!(command_registry::canonicalize("Solve"), Some("Solve"));
    assert_eq!(command_registry::canonicalize("solve"), Some("Solve"));
    assert_eq!(command_registry::canonicalize("nsolve"), Some("NSolve"));
    assert_eq!(
        command_registry::canonicalize("resolver"),
        Some("Solve"),
        "resolver sigue siendo alias de Solve"
    );
    assert_eq!(
        command_registry::canonicalize("solvenlsystem"),
        Some("SolveNlSystem"),
        "canónico case-insensitive sin alias redundante"
    );
    assert_eq!(
        command_registry::canonicalize("sistema_nolineal"),
        Some("SolveNlSystem")
    );
    // SolveSystem lineal intacto (dispatch LinearSolve, no duplicado).
    assert_eq!(
        command_registry::canonicalize("SolveSystem"),
        Some("LinearSolve")
    );
    let parsed = parse_cas_command("NSolve[x^2,x,0,1]").expect("parse NSolve");
    assert_eq!(parsed.command, "NSolve");
    let parsed = parse_cas_command("SolveNlSystem[x+y-1,x-y,x,y]").expect("parse sistema");
    assert_eq!(parsed.command, "SolveNlSystem");
    let solve = command_registry::resolve("Solve").expect("spec Solve");
    assert!(
        !solve.aliases.contains(&"nsolve"),
        "nsolve migró a NSolve canónico"
    );
    assert!(solve.signatures.len() == 2, "Solve general + intervalo");
}
