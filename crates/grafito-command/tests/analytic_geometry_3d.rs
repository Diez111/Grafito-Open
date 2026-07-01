use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

fn point3d_solutions(doc: &Document) -> Vec<f64> {
    let mut ys = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point3D(p) if p.label == "Sol3D" => Some(p.position.y),
            _ => None,
        })
        .collect::<Vec<_>>();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys
}

#[test]
fn plane3d_and_line3d_commands_create_objects() {
    let mut doc = Document::new();
    assert!(matches!(
        run(&mut doc, "Plane3D[1, 0, 1, 4]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]"),
        CommandOutcome::Ok
    ));

    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Plane3D(_))));
    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Line3D(_))));
}

#[test]
fn equidistant_from_solves_university_problem_on_y_axis() {
    let mut doc = Document::new();
    run(&mut doc, "Plane3D[1, 0, 1, 4]");
    run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]");

    let outcome = run(&mut doc, "EquidistantFrom[P, L, \"y-axis\"]");
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "expected message, got {:?}",
        outcome
    );

    let ys = point3d_solutions(&doc);
    assert_eq!(ys.len(), 2, "expected two solutions, got {ys:?}");
    let expected = 2.0 * 2.0_f64.sqrt();
    assert!((ys[0] + expected).abs() < 1e-5, "ys={ys:?}");
    assert!((ys[1] - expected).abs() < 1e-5, "ys={ys:?}");
}

#[test]
fn solve3dgeometry_solves_dist_equality_with_point_constraint() {
    let mut doc = Document::new();
    run(&mut doc, "Plane3D[1, 0, 1, 4]");
    run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]");

    let outcome = run(
        &mut doc,
        "Solve3DGeometry[\"dist(P,P)=dist(P,L)\", y, \"P=(0,y,0)\"]",
    );
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "expected message, got {:?}",
        outcome
    );

    let ys = point3d_solutions(&doc);
    assert_eq!(ys.len(), 2, "expected two solutions, got {ys:?}");
    let expected = 2.0 * 2.0_f64.sqrt();
    assert!((ys[0] + expected).abs() < 1e-5, "ys={ys:?}");
    assert!((ys[1] - expected).abs() < 1e-5, "ys={ys:?}");
}

#[test]
fn plane3d_from_three_points_and_line3d_from_two_points() {
    let mut doc = Document::new();
    run(&mut doc, "Point3D[0, 0, 0]");
    run(&mut doc, "Point3D[1, 0, 0]");
    run(&mut doc, "Point3D[0, 1, 0]");
    run(&mut doc, "Point3D[0, 0, 1]");

    assert!(matches!(
        run(&mut doc, "Plane3D[P, P₁, P₂]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(run(&mut doc, "Line3D[P, P₃]"), CommandOutcome::Ok));

    let plane = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::Plane3D(p) => Some(p),
        _ => None,
    });
    let line = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::Line3D(l) => Some(l),
        _ => None,
    });

    let plane = plane.expect("Plane3D command should create a plane");
    assert!(plane.a.abs() < 1e-9);
    assert!(plane.b.abs() < 1e-9);
    assert!((plane.c.abs() - 1.0).abs() < 1e-9);

    let line = line.expect("Line3D command should create a line");
    assert!((line.direction.z - 1.0).abs() < 1e-9);
}
