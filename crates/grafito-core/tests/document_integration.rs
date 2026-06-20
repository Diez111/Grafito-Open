use grafito_core::*;
use grafito_geometry::Point2;

#[test]
fn test_document_with_many_constraints() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(4.0, 0.0));
    let c = doc.add_point(Point2::new(2.0, 4.0));

    let line_ab = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(4.0, 0.0),
    )));
    let line_bc = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(4.0, 0.0),
        Point2::new(2.0, 4.0),
    )));

    let (mid, _) = doc.add_constructed_object(
        GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
        "Midpoint",
        &[a, b],
    );
    let (perp, _) = doc.add_constructed_object(
        GeoObject::Line(
            LineObj::new_with_kind(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), LineKind::Line)
                .with_label("P"),
        ),
        "Perpendicular",
        &[line_ab, c],
    );
    let (parallel, _) = doc.add_constructed_object(
        GeoObject::Line(
            LineObj::new_with_kind(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), LineKind::Line)
                .with_label("L"),
        ),
        "Parallel",
        &[line_bc, a],
    );

    // Distance matches the initial geometry so the solver does not move the free points.
    doc.add_distance_constraint(a, b, 4.0);

    let order = doc.propagation_order(&[a, b, c, line_ab, line_bc]);
    doc.re_evaluate_constraints(&order);

    // Midpoint of (0,0) and (4,0) should be (2,0).
    if let GeoObject::Point(m) = doc.get_object(mid).unwrap() {
        assert!((m.position.x - 2.0).abs() < 1e-6);
        assert!((m.position.y).abs() < 1e-6);
    } else {
        panic!("expected midpoint point");
    }

    // Perpendicular to a horizontal line through point c must be vertical.
    if let GeoObject::Line(l) = doc.get_object(perp).unwrap() {
        let dx = l.end.x - l.start.x;
        assert!(dx.abs() < 1e-6, "perpendicular line should be vertical");
        assert!((l.start.x - 2.0).abs() < 1e-6 || (l.end.x - 2.0).abs() < 1e-6);
    } else {
        panic!("expected perpendicular line");
    }

    // Parallel to line_bc through point a should preserve the slope of line_bc.
    if let GeoObject::Line(l) = doc.get_object(parallel).unwrap() {
        let dx = l.end.x - l.start.x;
        let dy = l.end.y - l.start.y;
        let slope = dy / dx;
        let expected_slope = (4.0 - 0.0) / (2.0 - 4.0); // -2
        assert!((slope - expected_slope).abs() < 1e-6);
    } else {
        panic!("expected parallel line");
    }

    // Move a free point and verify dependent objects are updated.
    doc.move_point(a, Point2::new(1.0, 0.0));
    let order = doc.propagation_order(&[a]);
    doc.re_evaluate_constraints(&order);

    if let GeoObject::Point(m) = doc.get_object(mid).unwrap() {
        assert!((m.position.x - 2.5).abs() < 1e-6);
        assert!((m.position.y).abs() < 1e-6);
    } else {
        panic!("expected midpoint point after move");
    }
}

#[test]
fn test_serialize_complex_document() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(4.0, 0.0));
    let line = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(4.0, 0.0),
    )));
    let circle = doc.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(2.0, 1.0),
        1.5,
    )));
    let _poly = doc.add_object(GeoObject::Polygon(PolygonObj::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(1.0, 2.0),
    ])));
    let func = doc.add_object(GeoObject::Function(FunctionObj::new("sin(x)")));
    let (_mid, _) = doc.add_constructed_object(
        GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
        "Midpoint",
        &[a, b],
    );
    doc.add_distance_constraint(a, b, 4.0);

    let object_count = doc.object_count();
    let constraint_count = doc.constraints.constraint_count();

    let json = serde_json::to_string(&doc).unwrap();
    let doc2: Document = serde_json::from_str(&json).unwrap();

    assert_eq!(doc2.object_count(), object_count);
    assert_eq!(doc2.constraints.constraint_count(), constraint_count);

    // Spot-check that specific objects and their properties survived.
    assert!(doc2.get_object(a).is_some());
    assert!(doc2.get_object(line).is_some());
    if let GeoObject::Circle(c) = doc2.get_object(circle).unwrap() {
        assert!((c.radius - 1.5).abs() < 1e-9);
    } else {
        panic!("expected circle after roundtrip");
    }
    if let GeoObject::Function(f) = doc2.get_object(func).unwrap() {
        assert_eq!(f.expr, "sin(x)");
    } else {
        panic!("expected function after roundtrip");
    }
}

#[test]
fn test_numeric_solver_with_multiple_constraints() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(3.0, 0.0));
    let l1 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let l2 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 1.0),
    )));

    // Combine distance, angle and horizontal constraints.
    doc.add_distance_constraint(a, b, 5.0);
    doc.add_angle_constraint(l1, l2, 90.0);
    doc.add_horizontal_constraint(l1);

    doc.re_evaluate_constraints(&[]);

    // Distance constraint.
    let pa = doc.point_position(a).unwrap();
    let pb = doc.point_position(b).unwrap();
    let distance = pa.distance(&pb);
    assert!(
        (distance - 5.0).abs() < 1e-6,
        "distance should be 5.0, got {}",
        distance
    );

    // Horizontal constraint on l1.
    let GeoObject::Line(line1) = doc.get_object(l1).unwrap() else {
        panic!("expected line");
    };
    assert!(
        (line1.start.y - line1.end.y).abs() < 1e-6,
        "line1 should be horizontal"
    );

    // Angle constraint between l1 and l2.
    let GeoObject::Line(line2) = doc.get_object(l2).unwrap() else {
        panic!("expected line");
    };
    let d1 = Point2::new(line1.end.x - line1.start.x, line1.end.y - line1.start.y);
    let d2 = Point2::new(line2.end.x - line2.start.x, line2.end.y - line2.start.y);
    let len1 = (d1.x * d1.x + d1.y * d1.y).sqrt();
    let len2 = (d2.x * d2.x + d2.y * d2.y).sqrt();
    assert!(len1 > 1e-6 && len2 > 1e-6);
    let dot = d1.x * d2.x + d1.y * d2.y;
    let cos_angle = dot / (len1 * len2);
    let angle = cos_angle.clamp(-1.0, 1.0).acos().to_degrees();
    assert!(
        (angle - 90.0).abs() < 1e-4,
        "angle should be 90°, got {}",
        angle
    );
}
