use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{
    implicit_curve::{MAX_IMPLICIT_GRID_SIZE, MAX_MARCHING_SQUARES_WORK_UNITS},
    validation::MAX_CONTOUR_LEVELS,
    Document,
};

fn contour_command(level_count: usize) -> String {
    let levels = (0..level_count)
        .map(|level| level.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("Contour[x+y,-1,1,-1,1,{levels}]")
}

fn assert_rejected_without_mutation(level_count: usize, expected_message: &str) {
    let mut document = Document::new();
    let before = (serde_json::to_value(&document).unwrap(), document.version);
    let mut input = contour_command(level_count);

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains(expected_message)),
        "{outcome:?}"
    );
    assert_eq!(
        (serde_json::to_value(&document).unwrap(), document.version),
        before
    );
}

#[test]
fn contour_command_rejects_the_persisted_level_limit() {
    assert_rejected_without_mutation(MAX_CONTOUR_LEVELS + 1, "level count");
}

#[test]
fn contour_command_rejects_the_persisted_work_limit() {
    let cells_per_level = MAX_IMPLICIT_GRID_SIZE * MAX_IMPLICIT_GRID_SIZE;
    let excessive_levels = MAX_MARCHING_SQUARES_WORK_UNITS / cells_per_level + 1;

    assert_rejected_without_mutation(excessive_levels, "work budget");
}
