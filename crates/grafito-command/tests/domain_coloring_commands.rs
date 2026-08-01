use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

#[test]
fn domain_coloring_creates_a_bounded_complex_grid() {
    let mut document = Document::new();

    let outcome = run(&mut document, "DomainColoring[1/z, -2, 2, -2, 2, 160]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::ComplexGrid(grid)
            if grid.render_mode == 1
                && grid.expr == "1/z"
                && grid.density == 160
                && grid.x_min == -2.0
                && grid.x_max == 2.0
                && grid.y_min == -2.0
                && grid.y_max == 2.0)
    }));
}

#[test]
fn domain_coloring_accepts_its_documented_optional_bounds_and_resolution() {
    for command in [
        "DomainColoring[1/z]",
        "DomainColoring[1/z, -2]",
        "DomainColoring[1/z, -2, 2]",
        "DomainColoring[1/z, -2, 2, -3]",
        "DomainColoring[1/z, -2, 2, -3, 3]",
        "DomainColoring[1/z, -2, 2, -3, 3, 160]",
    ] {
        let mut document = Document::new();
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "{command}: {outcome:?}"
        );
        assert!(document
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::ComplexGrid(_))));
    }
}

#[test]
fn domain_coloring_rejects_invalid_expression_density_and_bounds_atomically() {
    for command in [
        "DomainColoring[not valid, -2, 2, -2, 2, 160]",
        "DomainColoring[1/z, -2, 2, -2, 2, 0]",
        "DomainColoring[1/z, -2, 2, -2, 2, 301]",
        "DomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, r]",
        "DomainColoring[1/z, 2, -2, -2, 2, 160]",
    ] {
        let mut document = Document::new();
        let before = serde_json::to_value(&document).unwrap();

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            serde_json::to_value(&document).unwrap(),
            before,
            "{command}"
        );
    }
}

#[test]
fn heat_map_and_complex_surface_reject_unbounded_visualization_inputs_atomically() {
    for command in [
        "HeatMap[x+y, -2, 2, -2, 2, 0]",
        "HeatMap[x+y, -2, 2, -2, 2, 301]",
        "HeatMap[x+y, 2, -2, -2, 2, 64]",
        "ComplexSurface[not valid, -2, 2, -2, 2, 40]",
        "ComplexSurface[1/z, -2, 2, -2, 2, 0]",
        "ComplexSurface[1/z, 2, -2, -2, 2, 40]",
    ] {
        let mut document = Document::new();
        let before = serde_json::to_value(&document).unwrap();

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(
            serde_json::to_value(&document).unwrap(),
            before,
            "{command}"
        );
    }
}
