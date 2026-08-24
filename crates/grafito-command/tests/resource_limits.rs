#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{
    execute_cas_command, expand_all_cas, process_input, CasCmd, CommandOutcome,
};
use grafito_core::Document;

fn document_snapshot(document: &Document) -> serde_json::Value {
    serde_json::to_value(document).expect("document should serialize")
}

#[test]
fn histogram_command_rejects_oversized_bin_count_without_mutation() {
    let mut document = Document::new();
    let mut input = "Histogram[{1,2,3},4097]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("bins 4097 exceeds maximum 4096")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
    assert!(document.variables.is_empty());
}

#[test]
fn mandelbrot_command_rejects_oversized_iteration_count_without_mutation() {
    let mut document = Document::new();
    let mut input = "Mandelbrot[10001]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("max_iter 10001 exceeds maximum 10000")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 0);
    assert!(document.variables.is_empty());
}

#[test]
fn fractal_commands_enforce_the_default_pixel_work_budget_before_mutation() {
    for max_iter in [1_599, 1_600] {
        let mut document = Document::new();
        let mut input = format!("Mandelbrot[{max_iter}]");

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "max_iter={max_iter} should fit the default fractal work budget: {outcome:?}"
        );
        assert_eq!(document.object_count(), 1);
    }

    for command in ["Mandelbrot[1601]", "Julia[-0.7,0.3,1601]"] {
        let mut document = Document::new();
        let before = document_snapshot(&document);
        let mut input = command.to_string();

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains("work")),
            "{command} should exceed the default fractal work budget: {outcome:?}"
        );
        assert_eq!(document_snapshot(&document), before);
    }
}

#[test]
fn taylor_commands_reject_orders_above_the_derivative_budget_without_mutation() {
    for order in [63, 64] {
        let mut document = Document::new();
        let mut input = format!("Taylor[x,x,0,{order}]");

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "Taylor order {order} should be accepted: {outcome:?}"
        );
        assert_eq!(document.object_count(), 1);
    }

    let mut document = Document::new();
    let before = document_snapshot(&document);
    let mut input = "Taylor[x,x,0,65]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("order 65 exceeds maximum 64")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);

    assert_eq!(
        expand_all_cas("Taylor[x,x,0,65]", &document),
        "Taylor[x,x,0,65]"
    );

    let command = CasCmd {
        command: "Taylor".to_string(),
        args: vec![
            "x".to_string(),
            "x".to_string(),
            "0".to_string(),
            "65".to_string(),
        ],
    };
    let outcome = execute_cas_command(&mut document, &command);
    assert!(
        matches!(outcome, Some(ref message) if message.contains("order 65 exceeds maximum 64")),
        "unexpected direct CAS outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn taylor_command_rejects_an_oversized_ast_before_expanding_its_first_derivative() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let group = "(x+x+x+x+x+x)";
    let expression = std::iter::repeat_n(group, 100)
        .collect::<Vec<_>>()
        .join("+");
    let mut input = format!("Taylor[{expression},x,0,1]");

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("budget")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn taylor_command_rejects_an_accepted_order_when_derivative_growth_exceeds_the_work_budget() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let expression = std::iter::repeat_n("sin(x)", 16)
        .collect::<Vec<_>>()
        .join("*");
    let mut input = format!("Taylor[{expression},x,0,64]");

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("budget")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn command_input_rejects_more_than_64_kib_before_mutation() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let mut input = format!("Unknown[{}]", "a".repeat(65_536));

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("65536 bytes")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn command_rejects_more_than_64_top_level_arguments_without_mutation() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let mut input = format!("Unknown[{}]", vec!["1"; 65].join(","));

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("64 arguments")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn command_rejects_unbalanced_delimiters_without_mutation() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let mut input = "Point[(1,2]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("unbalanced delimiters")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn script_accepts_100_statements_and_rejects_101_atomically() {
    let mut document = Document::new();
    let mut valid = format!("Script[{}]", vec!["(0,0)"; 100].join(";"));
    let outcome = process_input(&mut document, &mut valid);
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "unexpected outcome: {outcome:?}"
    );

    let before = document_snapshot(&document);
    let mut invalid = format!("Script[{}]", vec!["(0,0)"; 101].join(";"));
    let outcome = process_input(&mut document, &mut invalid);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("100 commands")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn script_limits_total_nesting_and_rejects_multiple_arguments() {
    let mut document = Document::new();
    let five_levels = "Script[".repeat(5) + "(0,0)" + &"]".repeat(5);
    let mut valid = five_levels;
    let outcome = process_input(&mut document, &mut valid);
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "unexpected outcome: {outcome:?}"
    );

    let before = document_snapshot(&document);
    let six_levels = "Script[".repeat(6) + "(0,0)" + &"]".repeat(6);
    let mut too_deep = six_levels;
    let outcome = process_input(&mut document, &mut too_deep);
    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("depth")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);

    let mut multiple_arguments = "Script[(0,0),(1,1)]".to_string();
    let outcome = process_input(&mut document, &mut multiple_arguments);
    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("exactly one argument")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn nested_script_semicolons_are_scoped_to_the_nested_script() {
    let mut document = Document::new();
    let mut input = "Script[(0,0);Script[(1,1);(2,2)];(3,3)]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document.object_count(), 4);
}

#[test]
fn script_rejects_an_oversized_nested_command_atomically() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let mut input = format!("Script[(0,0);Unknown[{}]]", vec!["1"; 65].join(","));

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("64 arguments")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}

#[test]
fn matrix_param_solve_rejects_matrices_too_large_for_symbolic_expansion() {
    let mut document = Document::new();
    let before = document_snapshot(&document);
    let size = 9;
    let rows = (0..size)
        .map(|row| {
            let entries = (0..size)
                .map(|column| if row == column { "p" } else { "0" })
                .collect::<Vec<_>>()
                .join(",");
            format!("[{entries}]")
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut input = format!("MatrixParamSolve[[{rows}], p]");

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("maximum 8")),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(document_snapshot(&document), before);
}
