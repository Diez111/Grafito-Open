#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

#[test]
fn rose_rejects_a_zero_denominator_before_creating_geometry() {
    let mut document = Document::new();
    let mut input = "Rose[1, 3, 0]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("denominador")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
}

#[test]
fn bessely_rejects_orders_above_the_recurrence_budget() {
    let mut document = Document::new();
    let mut input = "BesselY[1001, 1]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("orden")),
        "unexpected outcome: {outcome:?}"
    );
}

#[test]
fn rkf45_command_does_not_create_a_partial_scalar_solution() {
    let mut document = Document::new();
    let mut input =
        "ODE[100000000000000000000000000000000000000*t^5, 0, 0, 1, 20, rk45, 1e-12]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("RKF45")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
}

#[test]
fn rkf45_command_does_not_create_a_partial_system_solution() {
    let mut document = Document::new();
    let mut input = "ODESystem[10000000000*x, 0, 0, 1, 0, 1, 20, rk45, 1e-12]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("RKF45")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
}
