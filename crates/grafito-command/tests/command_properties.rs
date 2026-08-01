use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;
use grafito_geometry::Point2;

fn snapshot(document: &Document) -> (serde_json::Value, u64) {
    (
        serde_json::to_value(document).expect("seed document should serialize"),
        document.version,
    )
}

fn failed_command_corpus() -> Vec<String> {
    let mut commands = vec![
        "FooBar[ghost]".to_string(),
        "Circle[(0,0)]".to_string(),
        "Function[]".to_string(),
        "Script[(9,9); FooBar[ghost]]".to_string(),
        "Point[(1,2]".to_string(),
    ];
    let mut state = 0xD1CE_CAFE_1234_5678_u64;

    for _ in 0..64 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let label = state % 1_000_000;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let first = state % 1_000;
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let second = state % 1_000;
        commands.push(format!("FuzzUnknown{label}[{first}, {second}]"));
    }

    commands
}

#[test]
fn failed_bounded_command_corpus_is_atomic() {
    for command in failed_command_corpus() {
        let mut document = Document::new();
        document.add_point(Point2::new(3.0, -4.0));
        document.set_variable("baseline".to_string(), 7.0);
        let before = snapshot(&document);
        let mut input = command.clone();

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "expected a rejected command for {command:?}, got {outcome:?}"
        );
        assert_eq!(
            snapshot(&document),
            before,
            "rejected command changed the document: {command:?}"
        );
    }
}
