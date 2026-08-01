use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{pencil::MAX_PENCIL_POINTS, Document, GeoObject};

fn run(document: &mut Document, text: String) -> CommandOutcome {
    let mut input = text;
    process_input(document, &mut input)
}

#[test]
fn ode_rejects_step_counts_that_cannot_fit_the_trajectory_point_limit() {
    let mut document = Document::new();
    let outcome = run(
        &mut document,
        format!("ODE[0, 0, 0, 1, {MAX_PENCIL_POINTS}]"),
    );

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("8191")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
}

#[test]
fn ode_system_rejects_step_counts_that_cannot_fit_the_trajectory_point_limit() {
    let mut document = Document::new();
    let outcome = run(
        &mut document,
        format!("ODESystem[0, 0, 0, 0, 0, 1, {MAX_PENCIL_POINTS}]"),
    );

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("8191")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
}

#[test]
fn ode_accepts_the_exact_step_count_that_fills_the_trajectory_point_limit() {
    let mut document = Document::new();
    let outcome = run(
        &mut document,
        format!("ODE[0, 0, 0, 1, {}]", MAX_PENCIL_POINTS - 1),
    );

    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "got {outcome:?}"
    );
    assert!(document.objects_iter().any(|(_, object)| matches!(
        object,
        GeoObject::Pencil(pencil) if pencil.points.len() == MAX_PENCIL_POINTS
    )));
}

#[test]
fn ode_trajectories_are_open_pencils_even_with_one_step() {
    let mut document = Document::new();
    let scalar = run(&mut document, "ODE[0, 0, 0, 1, 1, euler]".to_string());
    assert!(
        matches!(scalar, CommandOutcome::Message(_)),
        "got {scalar:?}"
    );
    assert!(document.objects_iter().any(|(_, object)| matches!(
        object,
        GeoObject::Pencil(pencil)
            if pencil.label == "ODE(euler)" && pencil.len() == 2 && pencil.segment_count() == 1
    )));

    let phase = run(
        &mut document,
        "ODESystem[1, 0, 0, 0, 0, 1, 1, euler]".to_string(),
    );
    assert!(matches!(phase, CommandOutcome::Message(_)), "got {phase:?}");
    assert!(document.objects_iter().any(|(_, object)| matches!(
        object,
        GeoObject::Pencil(pencil)
            if pencil.label == "Phase(euler)" && pencil.len() == 2 && pencil.segment_count() == 1
    )));
}

#[test]
fn ode_rejects_fixed_steps_that_cannot_advance_time() {
    let mut document = Document::new();
    let outcome = run(
        &mut document,
        "ODE[1, 10000000000000000, 0, 10000000000000002, 2, euler]".to_string(),
    );

    assert!(matches!(outcome, CommandOutcome::Error(message) if message.contains("avanzar")));
    assert_eq!(document.object_count(), 0);
}
