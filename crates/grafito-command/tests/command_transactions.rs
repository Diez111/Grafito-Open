#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{
    find_object_by_label, process_cas_worksheet_cell, process_input, CommandOutcome,
};
use grafito_core::{
    deserialize_document, serialize_document, CasWorksheetStatus, CircleObj, Document, GeoObject,
    LineObj, PointObj, PolygonObj,
};
use grafito_geometry::Point2;
use std::collections::HashSet;

fn snapshot(document: &Document) -> (serde_json::Value, u64) {
    (serde_json::to_value(document).unwrap(), document.version)
}

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    process_input(document, &mut command.to_string())
}

#[test]
fn cas_worksheet_cell_commits_result_and_geometry_as_one_document_revision() {
    let mut document = Document::new();
    let version_before = document.version;

    let outcome = process_cas_worksheet_cell(&mut document, "Solve[x^2-4,x,-3,3]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert_eq!(document.cas_worksheet().len(), 1);
    assert_eq!(
        document.cas_worksheet()[0].status,
        CasWorksheetStatus::Success
    );
    assert_eq!(document.cas_worksheet()[0].input, "Solve[x^2-4,x,-3,3]");
    assert!(
        document.object_count() >= 2,
        "solve must retain its geometry"
    );
}

#[test]
fn cas_worksheet_error_cell_discards_partial_command_changes() {
    let mut document = Document::new();
    document.set_variable("baseline".to_string(), 7.0);
    let before = snapshot(&document);

    let outcome = process_cas_worksheet_cell(&mut document, "Solve[unknown_function(x),x]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(document.object_count(), 0);
    assert_eq!(document.get_variable("baseline"), Some(7.0));
    assert_eq!(document.cas_worksheet().len(), 1);
    assert_eq!(
        document.cas_worksheet()[0].status,
        CasWorksheetStatus::Error
    );
    assert!(document.cas_worksheet()[0].output.contains("Solve"));
    let after_without_cell = {
        let mut staged = document.detached_clone_for_staging();
        assert!(staged.clear_cas_worksheet());
        serde_json::to_value(&staged).expect("staged document serializes")
    };
    assert_eq!(
        after_without_cell, before.0,
        "failed parser work must not commit"
    );
}

#[test]
fn cas_worksheet_capacity_rejects_work_before_evaluating_the_command() {
    let mut document = Document::new();
    for index in 0..Document::MAX_CAS_WORKSHEET_CELLS {
        document
            .try_append_cas_worksheet_cell(
                format!("Simplify[x + {index}]"),
                "x".to_string(),
                CasWorksheetStatus::Success,
            )
            .expect("fixture cell is within the worksheet budget");
    }
    let before = snapshot(&document);

    let outcome = process_cas_worksheet_cell(&mut document, "Solve[x^2-4,x,-3,3]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
    assert_eq!(document.object_count(), 0);
}

#[test]
fn registered_primitive_commands_create_their_documented_geometry() {
    let mut document = Document::new();

    assert!(matches!(
        run(&mut document, "Point[(1,2)]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut document, "Circle[(0,0),2]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut document, "Polygon[(0,0),(2,0),(0,2)]"),
        CommandOutcome::Ok
    ));

    assert!(document.objects_iter().any(
        |(_, object)| matches!(object, GeoObject::Point(point) if point.position == Point2::new(1.0, 2.0))
    ));
    assert!(document.objects_iter().any(
        |(_, object)| matches!(object, GeoObject::Circle(circle) if circle.center == Point2::new(0.0, 0.0) && circle.radius == 2.0)
    ));
    assert!(document.objects_iter().any(
        |(_, object)| matches!(object, GeoObject::Polygon(polygon) if polygon.vertices.len() == 3)
    ));
}

#[test]
fn data_table_and_fit_commands_create_linked_local_analysis_atomically() {
    let mut document = Document::new();

    assert!(matches!(
        run(&mut document, "DataTable[{0,1,2},{1,3,5}]"),
        CommandOutcome::Message(_)
    ));
    let table_id = document
        .objects_iter()
        .find_map(|(id, object)| matches!(object, GeoObject::DataTable(_)).then_some(*id))
        .expect("DataTable creates a persistent source object");
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::ScatterPlot(plot) if plot.source_data == Some(table_id))
    }));

    assert!(matches!(
        run(&mut document, "FitLinear[D]"),
        CommandOutcome::Message(_)
    ));
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Function(function) if function.fit.as_ref().is_some_and(|fit| fit.source == table_id && fit.kind == grafito_geometry::statistics::FitKind::Linear && fit.diagnostics.rmse < 1e-12))
    }));

    let outcome = run(&mut document, "FitExp[D]");
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "positive fixture should fit: {outcome:?}"
    );

    let mut invalid = Document::new();
    assert!(matches!(
        run(&mut invalid, "DataTable[{0,1},{1,0}]"),
        CommandOutcome::Message(_)
    ));
    let before_invalid = snapshot(&invalid);
    let outcome = run(&mut invalid, "FitExp[D]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(
        snapshot(&invalid),
        before_invalid,
        "invalid fit must be atomic"
    );
}

#[test]
fn every_registered_local_fit_command_creates_a_linked_function() {
    let mut document = Document::new();
    assert!(matches!(
        run(&mut document, "DataTable[{1,2,3,4,5},{2,4,8,16,32}]"),
        CommandOutcome::Message(_)
    ));

    for (command, expected_kind) in [
        (
            "FitLinear[D]",
            grafito_geometry::statistics::FitKind::Linear,
        ),
        (
            "FitPoly[D,2]",
            grafito_geometry::statistics::FitKind::Polynomial { degree: 2 },
        ),
        (
            "FitExp[D]",
            grafito_geometry::statistics::FitKind::Exponential,
        ),
        (
            "FitLog[D]",
            grafito_geometry::statistics::FitKind::Logarithmic,
        ),
        ("FitPow[D]", grafito_geometry::statistics::FitKind::Power),
        (
            "FitSin[D]",
            grafito_geometry::statistics::FitKind::Sinusoidal,
        ),
    ] {
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "{command}: {outcome:?}"
        );
        assert!(document.objects_iter().any(|(_, object)| {
            matches!(object, GeoObject::Function(function) if function.fit.as_ref().is_some_and(|fit| fit.kind == expected_kind))
        }));
    }
}

#[test]
fn documented_point_constructions_transform_and_propagate() {
    let mut rotated = Document::new();
    let source = rotated.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("A"),
    ));
    let center = rotated.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("O"),
    ));
    assert!(matches!(
        run(&mut rotated, "Rotate[A,O,90]"),
        CommandOutcome::Ok
    ));
    let rotated_id = rotated
        .objects_iter()
        .find_map(|(id, object)| {
            matches!(object, GeoObject::Point(point) if point.label == "A'").then_some(*id)
        })
        .expect("Rotate must create a point");
    assert_eq!(
        rotated.point_position(rotated_id),
        Some(Point2::new(1.0, 1.0))
    );
    rotated.move_point(source, Point2::new(3.0, 0.0));
    let order = rotated.propagation_order(&[source, center]);
    rotated.re_evaluate_constraints(&order);
    let propagated = rotated.point_position(rotated_id).unwrap();
    assert!((propagated.x - 1.0).abs() < 1e-12);
    assert!((propagated.y - 2.0).abs() < 1e-12);

    let mut dilated = Document::new();
    let source = dilated.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("A"),
    ));
    let center = dilated.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("O"),
    ));
    assert!(matches!(
        run(&mut dilated, "Dilate[A,2,O]"),
        CommandOutcome::Ok
    ));
    let dilated_id = dilated
        .objects_iter()
        .find_map(|(id, object)| {
            matches!(object, GeoObject::Point(point) if point.label == "A'").then_some(*id)
        })
        .expect("Dilate must create a point");
    assert_eq!(
        dilated.point_position(dilated_id),
        Some(Point2::new(3.0, 0.0))
    );
    dilated.move_point(source, Point2::new(3.0, 0.0));
    let order = dilated.propagation_order(&[source, center]);
    dilated.re_evaluate_constraints(&order);
    assert_eq!(
        dilated.point_position(dilated_id),
        Some(Point2::new(5.0, 0.0))
    );

    let mut perpendicular = Document::new();
    perpendicular.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 2.0)).with_label("P"),
    ));
    perpendicular.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)).with_label("L"),
    ));
    assert!(matches!(
        run(&mut perpendicular, "Perpendicular[P,L]"),
        CommandOutcome::Ok
    ));
    assert!(perpendicular.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Line(line) if line.label == "perpendicular"
            && (line.start.x - 1.0).abs() < 1e-12
            && (line.end.x - 1.0).abs() < 1e-12)
    }));
}

#[test]
fn invalid_documented_constructions_fail_atomically() {
    for command in [
        "Point[(NaN,0)]",
        "Circle[(0,0),0]",
        "Polygon[(0,0),(1,0)]",
        "Rotate[Missing,(0,0),90]",
        "Dilate[Missing,2,(0,0)]",
        "Perpendicular[Missing,AlsoMissing]",
    ] {
        let mut document = Document::new();
        let before = snapshot(&document);
        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn strict_primitive_arguments_do_not_create_implicit_variables() {
    let mut document = Document::new();

    assert!(matches!(
        run(&mut document, "point[(1,2)]"),
        CommandOutcome::Ok
    ));
    assert!(!document.variables.contains_key("point"));

    let before = snapshot(&document);
    let outcome = run(&mut document, "Circle[(0,0),raduis]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
    assert!(!document.variables.contains_key("raduis"));
}

#[test]
fn literal_dilate_tracks_a_labeled_center() {
    let mut document = Document::new();
    let center = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("O"),
    ));

    assert!(matches!(
        run(&mut document, "Dilate[(2,0),2,O]"),
        CommandOutcome::Ok
    ));
    let output = document
        .objects_iter()
        .find_map(|(id, object)| {
            matches!(object, GeoObject::Point(point) if point.label == "D'").then_some(*id)
        })
        .expect("Dilate must create a point");
    assert_eq!(document.point_position(output), Some(Point2::new(3.0, 0.0)));

    assert!(document
        .try_move_point_and_re_evaluate(center, Point2::new(2.0, 0.0))
        .expect("moving the center should remain valid"));
    assert_eq!(document.point_position(output), Some(Point2::new(2.0, 0.0)));
}

#[test]
fn fixed_center_dilate_forms_have_the_expected_dependency_shape() {
    let mut labeled_source = Document::new();
    let source = labeled_source.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("A"),
    ));
    assert!(matches!(
        run(&mut labeled_source, "Dilate[A,2,(1,0)]"),
        CommandOutcome::Ok
    ));
    let output = labeled_source
        .objects_iter()
        .find_map(|(id, object)| {
            matches!(object, GeoObject::Point(point) if point.label == "A'").then_some(*id)
        })
        .expect("Dilate must create a point");
    assert!(!labeled_source.constraints.is_free(&output));
    assert!(labeled_source
        .try_move_point_and_re_evaluate(source, Point2::new(3.0, 0.0))
        .expect("moving the labeled source should remain valid"));
    assert_eq!(
        labeled_source.point_position(output),
        Some(Point2::new(5.0, 0.0))
    );

    let mut fixed = Document::new();
    assert!(matches!(
        run(&mut fixed, "Dilate[(2,0),2,(1,0)]"),
        CommandOutcome::Ok
    ));
    let output = fixed
        .objects_iter()
        .find_map(|(id, object)| matches!(object, GeoObject::Point(_)).then_some(*id))
        .expect("Dilate must create a point");
    assert_eq!(fixed.point_position(output), Some(Point2::new(3.0, 0.0)));
    assert!(fixed.constraints.is_free(&output));
}

#[test]
fn generated_transformation_labels_are_unique() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("A"),
    ));
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 1.0)).with_label("P"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("L"),
    ));

    for command in [
        "Rotate[A,90]",
        "Rotate[A,180]",
        "Perpendicular[P,L]",
        "Perpendicular[P,L]",
    ] {
        assert!(matches!(run(&mut document, command), CommandOutcome::Ok));
    }

    let transformed_labels: Vec<_> = document
        .objects_iter()
        .map(|(_, object)| object.label().to_string())
        .filter(|label| label.starts_with("A'") || label.starts_with("perpendicular"))
        .collect();
    let unique_labels: HashSet<_> = transformed_labels.iter().collect();
    assert_eq!(transformed_labels.len(), 4);
    assert_eq!(unique_labels.len(), transformed_labels.len());
}

#[test]
fn generated_transformation_labels_stay_within_document_limits() {
    let mut document = Document::new();
    let source_label = "A".repeat(grafito_core::validation::MAX_STRING_LENGTH);
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label(&source_label),
    ));

    let outcome = run(&mut document, &format!("Rotate[{source_label},90]"));

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    let labels: Vec<_> = document
        .objects_iter()
        .map(|(_, object)| object.label())
        .collect();
    assert_eq!(labels.len(), 2);
    assert!(labels
        .iter()
        .all(|label| label.len() <= grafito_core::validation::MAX_STRING_LENGTH));
    assert_ne!(labels[0], labels[1]);
}

#[test]
fn perpendicular_repropagation_rejects_a_degenerate_source_atomically() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 1.0)).with_label("P"),
    ));
    let line_id = document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("L"),
    ));
    assert!(matches!(
        run(&mut document, "Perpendicular[P,L]"),
        CommandOutcome::Ok
    ));
    let Some(GeoObject::Line(line)) = document.get_object_mut(line_id) else {
        panic!("source line must exist");
    };
    line.end = line.start;
    let order = document.propagation_order(&[line_id]);
    let before = snapshot(&document);

    let error = document
        .try_re_evaluate_constraints(&order)
        .expect_err("a degenerate source line must be rejected");
    assert!(error.contains("degenerada"), "{error}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn perpendicular_rejects_unrepresentable_large_output_atomically() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0e308, 1.0e308)).with_label("P"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("L"),
    ));
    let before = snapshot(&document);

    let outcome = run(&mut document, "Perpendicular[P,L]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn scripts_defer_strict_argument_handling_to_nested_commands() {
    let mut valid = Document::new();
    assert!(matches!(
        run(&mut valid, "Script[point[(1,2)]]"),
        CommandOutcome::Message(_)
    ));
    assert!(!valid.variables.contains_key("point"));

    let mut invalid = Document::new();
    let before = snapshot(&invalid);
    let outcome = run(&mut invalid, "Script[Circle[(0,0),raduis]]");
    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&invalid), before);
}

#[test]
fn invalid_command_does_not_define_variables_or_change_document() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "FooBar[ghost]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Error(_)));
    assert_eq!(snapshot(&document), before);
}

#[test]
fn non_finite_geometry_is_rejected_without_committing_the_staged_document() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "Line[(NaN,0),(1,1)]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn overlong_function_expression_is_rejected_without_bypassing_document_limits() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = format!(
        "Function[{}]",
        "x".repeat(grafito_core::validation::MAX_EXPR_LENGTH + 1)
    );

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn successful_script_commits_its_staged_changes_as_one_revision() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "Script[(0,0);(1,1)]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_ne!(serde_json::to_value(&document).unwrap(), before.0);
    assert_eq!(document.version, before.1.wrapping_add(1));
}

#[test]
fn script_preserves_digit_suffixed_command_identifiers_for_nested_commands() {
    let mut document = Document::new();

    let outcome = run(
        &mut document,
        "Script[Segment3D[0,0,0,1,0,0];Segment3D[1,0,0,0.5,0.8660254037844386,0]]",
    );

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(
        document
            .objects_iter()
            .filter(|(_, object)| matches!(object, GeoObject::Segment3D(_)))
            .count(),
        2
    );
}

#[test]
fn failing_script_rolls_back_all_previous_commands() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "Script[(9,9); FooBar[ghost]]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Error(_)));
    assert_eq!(snapshot(&document), before);
}

#[test]
fn script_rolls_back_when_boolean_operand_lookup_fails() {
    for operation in [
        "PolygonUnion",
        "PolygonIntersection",
        "PolygonDifference",
        "PolygonXor",
    ] {
        let mut document = Document::new();
        let before = snapshot(&document);
        let mut input = format!("Script[(9,9);{operation}[MissingA,MissingB]]");

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{operation}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{operation} must be atomic");
    }
}

fn assert_error_is_atomic(command: &str, expected_message: &str) {
    let mut document = Document::new();
    document.set_variable("baseline".into(), 7.0);
    let before = snapshot(&document);
    let mut input = command.to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains(expected_message)),
        "{command}: {outcome:?}"
    );
    assert_eq!(snapshot(&document), before, "{command} must be atomic");
}

#[test]
fn graph_commands_that_cannot_create_geometry_fail_atomically() {
    for (command, expected_message) in [
        (
            "AngleBisector[(0,0),(0,0),(1,0)]",
            "AngleBisector: el vértice debe ser distinto",
        ),
        (
            "Reflect[(1,1),(0,0),(0,0)]",
            "Reflect: el eje requiere dos puntos distintos",
        ),
        (
            "Parabola[(0,0),1/0]",
            "Parabola: el parámetro p debe ser un número finito distinto de cero",
        ),
        (
            "SampledGraph[1/0,1]",
            "SampledGraph: la expresión debe producir al menos dos puntos finitos",
        ),
        (
            "Contour[x+y,-1,1,-1,1,1/0]",
            "Contour: cada nivel debe ser un número finito",
        ),
        (
            "Tetrahedron[0,0,0,0]",
            "Tetrahedron: la arista debe ser positiva",
        ),
        (
            "Tetrahedron[0,0,0,-1]",
            "Tetrahedron: la arista debe ser positiva",
        ),
        (
            "Tetrahedron[0,0,0,1/0]",
            "Tetrahedron: edge debe ser un número finito",
        ),
        (
            "Tetrahedron[1/0,0,0,1]",
            "Tetrahedron: x debe ser un número finito",
        ),
        ("Tetrahedron[0,0,0]", "cantidad de argumentos inválida"),
        (
            "Tetrahedron[1000000000000,0,0,2]",
            "Tetrahedron: los vértices exceden el límite de coordenadas renderizables",
        ),
    ] {
        assert_error_is_atomic(command, expected_message);
    }
}

#[test]
fn regular_polytope_commands_reject_invalid_input_and_projection_bounds_atomically() {
    for (command, expected_message) in [
        ("Tesseract4D[1,{0,1}]", "exactamente 6 angulos de rotacion"),
        ("Tesseract4D[1,[0,0,0,0,0,0]]", "se esperaba una lista"),
        ("Tesseract4D[1,{0,0,0,0,0,nan}]", "rotaciones invalidas"),
        ("Tesseract4D[0]", "scale debe ser finito y positivo"),
        ("Tesseract4D[-1]", "scale debe ser finito y positivo"),
        ("Tesseract4D[1e13]", "projection bound exceeds"),
        ("SimplexND[2]", "n debe ser un entero entre 3 y 10"),
        ("SimplexND[11]", "n debe ser un entero entre 3 y 10"),
        ("SimplexND[3.0]", "n debe ser un entero entre 3 y 10"),
        (
            "HypercubeND[4,1,{0,0,0,0,0}]",
            "exactamente 6 angulos de rotacion",
        ),
        ("HypercubeND[4,1,{0,0,0,0,0,nan}]", "rotaciones invalidas"),
        ("CrossPolytopeND[5,0]", "scale debe ser finito y positivo"),
        ("HypercubeND[10,1e13]", "projection bound exceeds"),
    ] {
        assert_error_is_atomic(command, expected_message);
    }
}

#[test]
fn legacy_placeholder_commands_are_unavailable_and_atomic() {
    assert_error_is_atomic("Image[\"missing.png\"]", "no está disponible");
}

#[test]
fn locus_command_creates_a_persistent_dynamic_trace_and_rejects_invalid_inputs_atomically() {
    let mut document = Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let target = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 2.0)).with_label("B"),
    ));

    let outcome = run(&mut document, "Locus[A,B]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Pencil(pencil)
            if pencil.is_dynamic_locus()
                && pencil.locus_binding().is_some_and(|binding| binding.driver == driver && binding.target == target))
    }));

    for (command, expected) in [
        ("Locus[A,A]", "distintos"),
        ("Locus[A,missing]", "no encontrado"),
        ("Locus[A,C]", "punto"),
    ] {
        if command == "Locus[A,C]" {
            document
                .try_add_object(GeoObject::Circle(
                    CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C"),
                ))
                .expect("circle fixture should be valid");
        }
        let before = snapshot(&document);
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains(expected)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn rejected_variable_update_reports_an_error_and_keeps_bound_geometry_atomic() {
    let mut document = Document::new();
    document.set_variable("phase".to_string(), 1.0);
    let mut circle = CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C");
    circle.radius_expr = Some("phase".to_string());
    let circle = document.add_object(GeoObject::Circle(circle));
    let probe = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("P"),
    ));
    document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0)).with_label("Q")),
            "PointOnObject",
            &[circle, probe],
        )
        .expect("valid positive-radius fixture should be constructible");
    let before = snapshot(&document);

    let outcome = run(&mut document, "SetValue[phase,0]");

    assert!(matches!(outcome, CommandOutcome::Error(ref message) if message.contains("SetValue")));
    assert_eq!(snapshot(&document), before);
}

#[test]
fn sampled_graph_creates_only_the_documented_static_polygon() {
    let mut document = Document::new();
    let mut input = "SampledGraph[x^2,1]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    let polygons = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            grafito_core::GeoObject::Polygon(polygon) => Some(polygon),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(polygons.len(), 1);
    assert_eq!(polygons[0].vertices.len(), 201);
    assert_eq!(
        polygons[0].vertices.first().map(|point| point.x),
        Some(-1.0)
    );
    assert_eq!(polygons[0].vertices.last().map(|point| point.x), Some(1.0));
    assert!(polygons[0].label.starts_with("sampled_graph"));
}

#[test]
fn graph_commands_reject_reversed_or_degenerate_domains_atomically() {
    for (command, expected_message) in [
        (
            "ParametricCurve2D[t,t,1,0]",
            "ParametricCurve2D: se requiere t_min < t_max",
        ),
        ("PolarCurve[1,1,0]", "PolarCurve: se requiere t_min < t_max"),
        (
            "ComplexGrid[z,1,-1,-1,1]",
            "ComplexGrid: se requiere x_min < x_max e y_min < y_max",
        ),
        (
            "DomainColoring[z,1,-1,-1,1]",
            "DomainColoring: se requiere x_min < x_max e y_min < y_max",
        ),
        (
            "HeatMap[x+y,1,-1,-1,1]",
            "HeatMap: se requiere x_min < x_max e y_min < y_max",
        ),
        (
            "Surface3D[x+y,1,-1,-1,1]",
            "Surface3D: se requiere x_min < x_max e y_min < y_max",
        ),
        (
            "Surface3D[u,v,u+v,1,-1,-1,1]",
            "Surface3D: se requiere u_min < u_max y v_min < v_max",
        ),
    ] {
        assert_error_is_atomic(command, expected_message);
    }
}

#[test]
fn curve3d_rejects_invalid_domains_in_every_supported_syntax_atomically() {
    for (command, expected_message) in [
        (
            "Curve3D[(t,t,t),0,0]",
            "Curve3D: se requiere t_min < t_max con ambos valores finitos",
        ),
        (
            "Curve3D[(t,t,t),1,0]",
            "Curve3D: se requiere t_min < t_max con ambos valores finitos",
        ),
        (
            "Curve3D[(t,t,t),nan,1]",
            "Curve3D: t_min debe ser un número finito",
        ),
        (
            "Curve3D[(t,t,t),0,nan]",
            "Curve3D: t_max debe ser un número finito",
        ),
        (
            "Curve3D[(t,t,t),t,0,0]",
            "Curve3D: se requiere t_min < t_max con ambos valores finitos",
        ),
        (
            "Curve3D[(t,t,t),t,1,0]",
            "Curve3D: se requiere t_min < t_max con ambos valores finitos",
        ),
        (
            "Curve3D[(t,t,t),t,nan,1]",
            "Curve3D: t_min debe ser un número finito",
        ),
        (
            "Curve3D[(t,t,t),t,0,nan]",
            "Curve3D: t_max debe ser un número finito",
        ),
    ] {
        assert_error_is_atomic(command, expected_message);
    }
}

#[test]
fn curve3d_rejects_invalid_components_and_parameter_before_mutation() {
    for (command, expected_message) in [
        (
            "Curve3D[(unknown(s),s,s),s,0,1]",
            "Curve3D: expresión x inválida",
        ),
        (
            "Curve3D[(s,unknown(s),s),s,0,1]",
            "Curve3D: expresión y inválida",
        ),
        (
            "Curve3D[(s,s,unknown(s)),s,0,1]",
            "Curve3D: expresión z inválida",
        ),
        (
            "Curve3D[(s,s,s),s+1,0,1]",
            "Curve3D: el parámetro debe ser un identificador válido",
        ),
    ] {
        assert_error_is_atomic(command, expected_message);
    }
}

#[test]
fn accepted_function_commands_create_function_objects() {
    for expression in ["1000000", "sin(100*x)"] {
        let mut document = Document::new();
        let before = snapshot(&document);
        let mut input = format!("Function[{expression}]");

        let outcome = process_input(&mut document, &mut input);

        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "{expression}: {outcome:?}"
        );
        assert!(
            document.objects_iter().any(|(_, object)| {
                matches!(object, GeoObject::Function(function) if function.expr == expression)
            }),
            "{expression} must create a Function object"
        );
        let after = snapshot(&document);
        assert_ne!(after.0, before.0, "{expression} must change the document");
        assert!(
            after.1 > before.1,
            "{expression} must advance the document version"
        );
    }
}

#[test]
fn setvalue_reports_when_it_creates_an_undefined_variable() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "SetValue[missing,1]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("se creó la variable 'missing'")),
        "{outcome:?}"
    );
    assert_eq!(document.variables.get("missing"), Some(&1.0));
    let after = snapshot(&document);
    assert_ne!(
        after.0, before.0,
        "SetValue must define the missing variable"
    );
    assert!(
        after.1 > before.1,
        "SetValue must advance the document version"
    );
}

#[test]
fn animate_commands_create_a_looping_phase_and_configure_named_parameters() {
    let mut document = Document::new();

    let phase_outcome = run(&mut document, "Animate[]");
    assert!(
        matches!(phase_outcome, CommandOutcome::Message(_)),
        "{phase_outcome:?}"
    );
    assert_eq!(document.variables.get("phase"), Some(&0.0));
    let phase = document
        .variable_meta("phase")
        .expect("phase metadata exists");
    assert!(phase.animating);
    assert_eq!(phase.animation_mode, grafito_core::AnimationMode::Loop);
    assert!((phase.max - std::f64::consts::TAU).abs() < 1e-12);

    let named_outcome = run(&mut document, "Animate[amplitude,-2,2,0.5]");
    assert!(
        matches!(named_outcome, CommandOutcome::Message(_)),
        "{named_outcome:?}"
    );
    let amplitude = document
        .variable_meta("amplitude")
        .expect("amplitude metadata exists");
    assert!(amplitude.animating);
    assert_eq!(
        amplitude.animation_mode,
        grafito_core::AnimationMode::PingPong
    );
    assert_eq!(amplitude.animation_speed, 0.5);
    assert_eq!(document.variables.get("amplitude"), Some(&0.0));
}

#[test]
fn scalar_command_creates_validated_variable_metadata() {
    let mut document = Document::new();

    let outcome = run(&mut document, "2 + 2");

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    assert_eq!(document.get_variable("a"), Some(4.0));
    let metadata = document
        .variable_meta("a")
        .expect("scalar commands create slider metadata through the core API");
    assert_eq!(metadata.min, -5.0);
    assert_eq!(metadata.max, 5.0);
    grafito_core::validation::validate_document(&document)
        .expect("command-created metadata keeps the document valid");
}

#[test]
fn invalid_animate_command_is_atomic() {
    let mut document = Document::new();
    document.set_variable("stable".into(), 2.0);
    let before = snapshot(&document);

    let outcome = run(&mut document, "Animate[stable,1,1,1]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn setvalue_point_move_propagates_constraint_errors_without_committing() {
    let mut document = Document::new();
    let a = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let b = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(4.0, 0.0)).with_label("B"),
    ));
    document
        .try_add_distance_constraint(a, b, 1.0)
        .expect("first distance constraint is structurally valid");
    document
        .try_add_distance_constraint(a, b, 2.0)
        .expect("conflicting distance constraint is structurally valid");
    let before = snapshot(&document);
    let mut input = "SetValue[A,(3,0)]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("Numeric constraint")),
        "{outcome:?}"
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn setvalue_accepts_an_unchanged_free_point() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(3.0, -2.0)).with_label("A"),
    ));
    let before = snapshot(&document);
    let mut input = "SetValue[A,(3,-2)]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn setvalue_rejects_spreadsheet_coordinate_points_without_mutation() {
    let mut document = Document::new()
        .stage_spreadsheet_cell_edits(&[(0, 0, "(1, 2)".to_string())])
        .expect("coordinate cell should stage");
    let before = snapshot(&document);

    let outcome = run(&mut document, "SetValue[A1,(3,4)]");

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("spreadsheet")),
        "{outcome:?}"
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn setvalue_rejects_unresolved_spreadsheet_cell_labels_without_mutation() {
    let mut document = Document::new();
    document
        .set_spreadsheet_cell(0, 0, "(".to_string())
        .expect("invalid formula source is retained");
    document
        .recompute_spreadsheet_variables()
        .expect("invalid formula leaves no scalar value");
    let before = snapshot(&document);

    let outcome = run(&mut document, "SetValue[A1,2]");

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("Spreadsheet")),
        "{outcome:?}"
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn extrude_rejects_an_entire_operation_when_all_outputs_do_not_fit() {
    let mut document = Document::new();
    let mut polygon = PolygonObj::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.0, 1.0),
    ]);
    polygon.label = "P".to_string();
    document.add_object(GeoObject::Polygon(polygon));
    while document.object_count() < grafito_core::validation::MAX_OBJECT_COUNT {
        document.add_point(Point2::new(0.0, 0.0));
    }
    let before = snapshot(&document);
    let mut input = "Extrude[P,1]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("Extrude")),
        "{outcome:?}"
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn extrude_repropagation_preserves_every_side_edge() {
    let mut document = Document::new();
    let mut polygon = PolygonObj::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(0.0, 3.0),
    ]);
    polygon.label = "P".to_string();
    let polygon_id = document.add_object(GeoObject::Polygon(polygon));
    let mut input = "Extrude[P,2]".to_string();

    let outcome = process_input(&mut document, &mut input);
    assert!(
        matches!(outcome, CommandOutcome::Ok),
        "Extrude must succeed: {outcome:?}"
    );
    let order = document.propagation_order(&[polygon_id]);
    document
        .try_re_evaluate_constraints(&order)
        .expect("Extrude outputs must re-evaluate");

    let vertices = [
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(0.0, 3.0),
    ];
    for (side, constraints) in order.chunks_exact(3).enumerate() {
        let output = |constraint_id| {
            let constraint = document
                .constraints
                .get_constraint(constraint_id)
                .expect("Extrude constraint must exist");
            let output_id = *constraint
                .outputs
                .first()
                .expect("Extrude output must exist");
            match document.get_object(output_id) {
                Some(GeoObject::Segment3D(segment)) => (segment.a, segment.b),
                other => panic!("expected Segment3D output, got {other:?}"),
            }
        };

        let current = vertices[side];
        let next = vertices[(side + 1) % vertices.len()];
        let base = grafito_geometry::Point3D::new(current.x, 0.0, current.y);
        let top = grafito_geometry::Point3D::new(current.x, 2.0, current.y);
        let next_base = grafito_geometry::Point3D::new(next.x, 0.0, next.y);
        let next_top = grafito_geometry::Point3D::new(next.x, 2.0, next.y);

        assert_eq!(output(constraints[0]), (base, top));
        assert_eq!(output(constraints[1]), (base, next_base));
        assert_eq!(output(constraints[2]), (top, next_top));
    }
}

#[test]
fn informational_math_command_returns_message_instead_of_silent_ok() {
    let mut document = Document::new();
    let before = snapshot(&document);
    let mut input = "Derivative[1000000,x]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Message(ref message) if message.contains("= 0")),
        "{outcome:?}"
    );
    assert!(
        document.objects_iter().any(
            |(_, object)| matches!(object, GeoObject::Function(function) if function.expr == "0")
        ),
        "the derivative result must be represented by a Function object"
    );
    let after = snapshot(&document);
    assert_ne!(after.0, before.0, "Derivative must update the document");
    assert!(
        after.1 > before.1,
        "Derivative must advance the document version"
    );
}

#[test]
fn incompatible_numeric_constraint_commands_fail_atomically() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("P"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("L"),
    ));
    document.add_object(GeoObject::Circle(
        CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("C"),
    ));

    for (command, expected) in [
        ("Distance[P,L,1]", "Distance: requiere dos puntos"),
        ("Angle[P,L,90]", "Angle: requiere dos rectas"),
        ("Tangent[P,L]", "Tangent: requiere un círculo y una recta"),
        ("Horizontal[P]", "Horizontal: requiere una recta"),
        ("EqualLength[L,C]", "EqualLength: requiere dos segmentos"),
        (
            "Symmetry[P,L,C]",
            "Symmetry: requiere dos puntos y una recta",
        ),
    ] {
        let before = snapshot(&document);
        let mut input = command.to_string();
        let outcome = process_input(&mut document, &mut input);
        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains(expected)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn numeric_solver_failures_are_command_errors_and_leave_no_constraint_behind() {
    let mut document = Document::new();
    document.add_object(GeoObject::Circle(
        CircleObj::new(Point2::new(0.0, 1.0), 1.0).with_label("C"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(0.0, 0.0)).with_label("L"),
    ));
    let before = snapshot(&document);
    let mut input = "Tangent[C,L]".to_string();

    let outcome = process_input(&mut document, &mut input);

    assert!(
        matches!(outcome, CommandOutcome::Error(ref message) if message.contains("Numeric constraint solver failed")),
        "{outcome:?}"
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn malformed_command_numbers_and_data_never_define_variables_or_mutate() {
    for (command, unexpected_variables) in [
        ("Gamma[banana]", &["banana"][..]),
        ("BesselJ[banana,orange]", &["banana", "orange"][..]),
        ("Julia[banana,orange]", &["banana", "orange"][..]),
        ("Mean[{1,banana,3}]", &["banana"][..]),
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
        for variable in unexpected_variables {
            assert!(
                !document.variables.contains_key(*variable),
                "{command} must not define {variable}"
            );
        }
    }
}

#[test]
fn malformed_optional_constraint_number_is_not_replaced_by_a_default() {
    let mut document = Document::new();
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("L1"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0)).with_label("L2"),
    ));
    let before = snapshot(&document);

    let outcome = run(&mut document, "Angle[L1,L2,oops]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
    assert!(!document.variables.contains_key("oops"));
}

#[test]
fn unsupported_non_finite_math_results_are_errors() {
    for command in ["Gamma[-1]", "Uniform[1,1,1]"] {
        let mut document = Document::new();
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn unsupported_bessel_orders_are_explicit_atomic_errors() {
    for (command, unexpected_variable) in [
        ("BesselJ[1001,1]", "unused_j"),
        ("BesselJ[2147483647,1]", "unused_j_max"),
        ("BesselJ[1001,ghost_j]", "ghost_j"),
        ("BesselJ[2147483647,ghost_j_max]", "ghost_j_max"),
        ("BesselJ[-2147483648,ghost_j_min]", "ghost_j_min"),
        ("BesselY[-1001,ghost_y]", "ghost_y"),
        ("BesselY[-2147483648,ghost_y_min]", "ghost_y_min"),
        ("BesselI[1001,ghost_i]", "ghost_i"),
        ("BesselI[-1001,ghost_i_neg]", "ghost_i_neg"),
        ("BesselI[2147483647,ghost_i_max]", "ghost_i_max"),
        ("BesselI[-2147483648,ghost_i_min]", "ghost_i_min"),
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains("orden")),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
        assert!(!document.variables.contains_key(unexpected_variable));
    }
}

#[test]
fn function_rejects_static_invalid_bessel_orders_atomically_but_preserves_dynamic_orders() {
    for command in [
        "Function[besselj(0/0,x)]",
        "Function[bessely(1.5,x)]",
        "Function[besseli(1/0,x)]",
        "Function[besselj(1001,x)]",
        "Function[bessely(-2147483648,x)]",
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }

    let mut document = Document::new();
    document.set_variable("n".into(), 1.5);
    assert!(matches!(
        run(&mut document, "Function[besselj(n,x)]"),
        CommandOutcome::Message(_)
    ));
    assert_eq!(document.object_count(), 1);
}

#[test]
fn non_finite_special_function_results_are_atomic_errors() {
    for command in [
        "Gamma[-1]",
        "LnGamma[-1]",
        "Beta[0,1]",
        "BesselY[0,0]",
        "BesselY[1000,1]",
        "BesselI[0,1e308]",
        "Digamma[0]",
    ] {
        let mut document = Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(ref message) if message.contains("no es finito")),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
        assert_eq!(document.variables.len(), 1, "{command} defined a variable");
    }
}

fn persisted_document_with_duplicate_label(reverse_positions: bool) -> Document {
    let mut document = Document::new();
    let positions = if reverse_positions {
        [Point2::new(2.0, 0.0), Point2::new(1.0, 0.0)]
    } else {
        [Point2::new(1.0, 0.0), Point2::new(2.0, 0.0)]
    };
    document.add_object(GeoObject::Point(
        PointObj::new(positions[0]).with_label("A"),
    ));
    let second = document.add_object(GeoObject::Point(
        PointObj::new(positions[1]).with_label("B"),
    ));
    document
        .get_object_mut(second)
        .expect("second point exists")
        .set_label("A".into());
    let saved = serialize_document(&document).expect("legacy duplicate labels remain saveable");
    deserialize_document(&saved).expect("legacy duplicate labels remain loadable")
}

#[test]
fn label_commands_reject_persisted_ambiguity_independent_of_object_order() {
    for reverse_positions in [false, true] {
        for command in [
            "SetValue[A,(9,9)]",
            "Length[A]",
            "Root[A]",
            "Integral[A,x]",
            "Parallel[A,A]",
            "PolygonUnion[A,A]",
            "ComplexMapping[1/z,A]",
            "Intersection3D[A,A]",
            "Erase[A]",
            "Rotate[A,90]",
            "Midpoint[A,(0,0)]",
            "Plane3D[A,A,A]",
            "ConicByFivePoints[A,A,A,A,A]",
            "A(x)=x",
            "A(x): ∫xdx",
            "Derivative[Integral[A,x],x]",
        ] {
            let mut document = persisted_document_with_duplicate_label(reverse_positions);
            assert_eq!(find_object_by_label(&document, "A"), None);
            let before = snapshot(&document);

            let outcome = run(&mut document, command);

            assert!(
                matches!(outcome, CommandOutcome::Error(ref message) if message.contains("ambiguous")),
                "{command}: {outcome:?}"
            );
            assert_eq!(snapshot(&document), before, "{command} must be atomic");
        }
    }
}

#[test]
fn non_label_arguments_can_share_an_ambiguous_object_label() {
    let mut document = persisted_document_with_duplicate_label(false);
    document.set_variable("A".into(), 2.0);
    let before = snapshot(&document);

    let outcome = run(&mut document, "Gamma[A]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn limit_is_read_only_on_success() {
    let mut document = Document::new();
    document.set_variable("baseline".into(), 7.0);
    let before = snapshot(&document);

    let outcome = run(&mut document, "Limit[x^2,x,1]");

    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn commands_reject_extra_arguments_atomically() {
    for command in [
        "Gamma[1,2]",
        "Julia[-0.7,0.3,100,extra]",
        "Function[x,extra]",
        "Limit[x,x,0,extra]",
    ] {
        let mut document = Document::new();
        let before = snapshot(&document);

        let outcome = run(&mut document, command);

        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn script_rolls_back_on_strict_numeric_validation_failure() {
    let mut document = Document::new();
    let before = snapshot(&document);

    let outcome = run(&mut document, "Script[(9,9);Gamma[banana]]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
    assert!(!document.variables.contains_key("banana"));
}

#[test]
fn tangent_from_circle_data_creates_two_distinct_lines() {
    let mut document = Document::new();

    let outcome = run(&mut document, "Tangent[(0,0),1,(2,0)]");

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    let tangents = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Line(line) => Some(line),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tangents.len(), 2);
    assert_ne!(tangents[0].label, tangents[1].label);
    assert!(tangents
        .iter()
        .all(|line| line.start == Point2::new(2.0, 0.0)));
}

#[test]
fn repeated_literal_line_commands_create_uniquely_labeled_objects() {
    let mut document = Document::new();

    for command in ["Line[(0,0),(1,0)]", "Line[(0,1),(1,1)]"] {
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Ok),
            "{command}: {outcome:?}"
        );
    }

    let labels = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Line(line) => Some(line.label.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    assert_eq!(document.object_count(), 2);
    assert_eq!(labels.len(), 2);
}

#[test]
fn malformed_construction_never_reports_success_without_an_output() {
    let mut document = Document::new();
    let before = snapshot(&document);

    let outcome = run(&mut document, "Midpoint[Missing,AlsoMissing]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
}

#[test]
fn script_rolls_back_its_prefix_when_object_capacity_rejects_an_insertion() {
    let mut document = Document::new();
    while document.object_count() < grafito_core::validation::MAX_OBJECT_COUNT {
        document.add_point(Point2::new(0.0, 0.0));
    }
    let before = snapshot(&document);

    let outcome = run(&mut document, "Script[SetValue[temp,1];Point[(9,9)]]");

    assert!(matches!(outcome, CommandOutcome::Error(_)), "{outcome:?}");
    assert_eq!(snapshot(&document), before);
    assert!(!document.variables.contains_key("temp"));
}

#[test]
fn parallel_uses_the_documented_point_then_line_order() {
    let mut document = Document::new();
    let point_id = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 2.0)).with_label("P"),
    ));
    let line_id = document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)).with_label("L"),
    ));

    let outcome = run(&mut document, "Parallel[P,L]");

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    let parallel = document
        .objects_iter()
        .find_map(|(id, object)| {
            (*id != point_id && *id != line_id)
                .then_some(object)
                .and_then(|object| match object {
                    GeoObject::Line(line) => Some(line),
                    _ => None,
                })
        })
        .expect("Parallel must create a line");
    let dx = parallel.end.x - parallel.start.x;
    let dy = parallel.end.y - parallel.start.y;
    assert!(dx.abs() > 1e-12);
    assert!(dy.abs() < 1e-12);
    let cross = (Point2::new(1.0, 2.0).x - parallel.start.x) * dy
        - (Point2::new(1.0, 2.0).y - parallel.start.y) * dx;
    assert!(cross.abs() < 1e-12, "parallel line must pass through P");
}

#[test]
fn parallel_rejects_reversed_or_missing_operands_atomically() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 2.0)).with_label("P"),
    ));
    document.add_object(GeoObject::Line(
        LineObj::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)).with_label("L"),
    ));

    for command in ["Parallel[L,P]", "Parallel[P,Missing]"] {
        let before = snapshot(&document);
        let outcome = run(&mut document, command);
        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "{command}: {outcome:?}"
        );
        assert_eq!(snapshot(&document), before, "{command} must be atomic");
    }
}

#[test]
fn reflect_accepts_the_documented_object_label() {
    let mut document = Document::new();
    document.add_object(GeoObject::Circle(
        CircleObj::new(Point2::new(2.0, 1.0), 3.0).with_label("C"),
    ));

    let outcome = run(&mut document, "Reflect[C,(0,0),(0,1)]");

    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Circle(circle)
            if circle.label == "C'"
                && (circle.center.x + 2.0).abs() < 1e-12
                && (circle.center.y - 1.0).abs() < 1e-12
                && (circle.radius - 3.0).abs() < 1e-12)
    }));
}
