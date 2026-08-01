use grafito_core::*;
use grafito_geometry::{
    Color, Point2, RegularPolychoron, RegularPolytopeFamily, MAX_WORLD_COORDINATE,
};

fn variable_meta_candidate() -> VariableMeta {
    VariableMeta {
        position: Point2::new(1.0, -2.0),
        min: -5.0,
        max: 5.0,
        step: 0.1,
        visible: true,
        animating: false,
        animation_speed: 1.0,
        animation_mode: AnimationMode::PingPong,
    }
}

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
                .with_label("perpendicular"),
        ),
        "Perpendicular",
        &[line_ab, c],
    );
    let (parallel, _) = doc.add_constructed_object(
        GeoObject::Line(
            LineObj::new_with_kind(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), LineKind::Line)
                .with_label("parallel"),
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

#[test]
fn constructed_geometry_is_rebound_before_numeric_solving() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(10.0, 0.0));
    let (midpoint, _) = doc.add_constructed_object(
        // The intentionally stale initial value exposes equations that capture
        // construction geometry before propagation.
        GeoObject::Point(PointObj::new(Point2::new(100.0, 0.0)).with_label("M")),
        "Midpoint",
        &[a, b],
    );
    doc.add_distance_constraint(a, midpoint, 5.0);

    let order = doc.propagation_order(&[a, b]);
    doc.re_evaluate_constraints(&order);

    let a_position = doc.point_position(a).expect("A remains a point");
    let midpoint_position = doc
        .point_position(midpoint)
        .expect("constructed midpoint remains a point");
    assert!(
        (a_position.distance(&midpoint_position) - 5.0).abs() < 1e-6,
        "numeric constraints must be checked against the final constructed geometry"
    );
}

#[test]
fn numeric_constraint_creation_rejects_incompatible_object_types_without_registering_them() {
    let mut doc = Document::new();
    let point = doc.add_point(Point2::new(0.0, 0.0));
    let other_point = doc.add_point(Point2::new(1.0, 0.0));
    let line = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let circle = doc.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(0.0, 0.0),
        1.0,
    )));

    let initial_count = doc.constraints.constraint_count();
    for result in [
        doc.try_add_distance_constraint(point, line, 1.0),
        doc.try_add_angle_constraint(point, line, 90.0),
        doc.try_add_tangent_constraint(point, line),
        doc.try_add_coincident_constraint(point, line),
        doc.try_add_horizontal_constraint(point),
        doc.try_add_vertical_constraint(point),
        doc.try_add_equal_length_constraint(line, circle),
        doc.try_add_symmetry_constraint(point, line, other_point),
    ] {
        assert!(
            result.is_err(),
            "incompatible numeric inputs must be rejected"
        );
    }
    assert_eq!(doc.constraints.constraint_count(), initial_count);
}

#[test]
fn bound_point_rebuilds_the_spatial_index_after_a_variable_update() {
    let mut doc = Document::new();
    doc.set_variable("x".into(), 0.0);
    doc.set_variable("y".into(), 0.0);
    let mut point = PointObj::new(Point2::new(0.0, 0.0));
    point.x_expr = Some("x".into());
    point.y_expr = Some("y".into());
    let point_id = doc.add_object(GeoObject::Point(point));
    doc.add_object(GeoObject::Function(FunctionObj::new("x^2")));

    assert_eq!(doc.pick_object(Point2::new(0.0, 0.0), 0.1), Some(point_id));
    doc.set_variable("x".into(), 5.0);
    doc.set_variable("y".into(), 5.0);

    assert_eq!(doc.pick_object(Point2::new(5.0, 5.0), 0.1), Some(point_id));
}

#[test]
fn moving_a_point_invalidates_spatial_bounds_only_when_its_position_changes() {
    let mut doc = Document::new();
    let point_id = doc.add_point(Point2::new(0.0, 0.0));
    let version_before = doc.version;

    assert_eq!(doc.pick_object(Point2::new(0.0, 0.0), 0.1), Some(point_id));
    assert!(!doc.spatial_dirty);

    assert!(doc.move_point(point_id, Point2::new(0.0, 0.0)).is_empty());
    assert_eq!(doc.version, version_before);
    assert!(!doc.spatial_dirty);

    assert_eq!(
        doc.move_point(point_id, Point2::new(5.0, 5.0)),
        vec![point_id]
    );
    assert_eq!(doc.version, version_before.wrapping_add(1));
    assert!(doc.spatial_dirty);
    assert_eq!(doc.pick_object(Point2::new(5.0, 5.0), 0.1), Some(point_id));
}

#[test]
fn failed_point_move_with_dependents_restores_the_entire_document() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(4.0, 0.0));
    let (midpoint, _) = doc.add_constructed_object(
        GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
        "Midpoint",
        &[a, b],
    );
    let order = doc.propagation_order(&[a, b]);
    doc.try_re_evaluate_constraints(&order)
        .expect("initial constructive geometry should be valid");

    doc.try_add_distance_constraint(a, b, 1.0)
        .expect("first distance constraint");
    doc.try_add_distance_constraint(a, b, 2.0)
        .expect("conflicting distance constraint can be represented");
    let before = serde_json::to_value(&doc).expect("document should serialize");
    let version_before = doc.version;
    let midpoint_before = doc.point_position(midpoint).expect("midpoint exists");

    let error = doc
        .try_move_point_and_re_evaluate(a, Point2::new(3.0, 0.0))
        .expect_err("conflicting numeric constraints must reject the drag");

    assert!(error.contains("Numeric constraint"));
    assert_eq!(doc.version, version_before);
    assert_eq!(
        serde_json::to_value(&doc).expect("document should serialize"),
        before
    );
    assert_eq!(doc.point_position(a), Some(Point2::new(0.0, 0.0)));
    assert_eq!(doc.point_position(midpoint), Some(midpoint_before));
}

#[test]
fn point_move_rejects_non_finite_input_without_committing() {
    let mut doc = Document::new();
    let point = doc.add_point(Point2::new(0.0, 0.0));
    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;

    let error = doc
        .try_move_point_and_re_evaluate(point, Point2::new(f64::NAN, 0.0))
        .expect_err("non-finite drag input must be rejected");

    assert!(error.contains("finite"));
    assert_eq!(doc.version, version_before);
    assert_eq!(
        serde_json::to_value(&doc).expect("document serializes"),
        before
    );
}

#[test]
fn constructed_insertion_rejects_unknown_constraints_atomically() {
    let mut doc = Document::new();
    let source = doc.add_point(Point2::new(0.0, 0.0));
    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;

    let error = doc
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(f64::NAN, 0.0)).with_label("M")),
            "UnknownConstruction",
            &[source],
        )
        .expect_err("unknown construction semantics must be rejected");

    assert!(error.contains("construcción desconocida"));
    assert_eq!(doc.version, version_before);
    assert_eq!(
        serde_json::to_value(&doc).expect("document serializes"),
        before
    );
}

#[test]
fn atomic_point_update_propagates_constructed_geometry() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(4.0, 0.0));
    let (midpoint, _) = doc
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        )
        .expect("midpoint construction is valid");
    let order = doc.propagation_order(&[a, b]);
    doc.try_re_evaluate_constraints(&order)
        .expect("initial midpoint evaluates");

    assert!(doc
        .try_update_point_and_re_evaluate(a, |point| {
            point.position = Point2::new(2.0, 0.0);
            Ok(())
        })
        .expect("atomic point update succeeds"));

    assert_eq!(doc.point_position(a), Some(Point2::new(2.0, 0.0)));
    assert_eq!(doc.point_position(midpoint), Some(Point2::new(3.0, 0.0)));
}

#[test]
fn conic_by_five_collinear_points_rejects_atomically_before_output_creation() {
    let mut doc = Document::new();
    let points: Vec<ObjectId> = (0..5)
        .map(|x| doc.add_point(Point2::new(x as f64, 0.0)))
        .collect();
    let before = serde_json::to_value(&doc).expect("document should serialize");
    let version_before = doc.version;
    let object_count = doc.object_count();
    let constraint_count = doc.constraints.constraint_count();

    let error = doc
        .try_add_conic_by_five_points_constraint(&points)
        .expect_err("five collinear points do not define a representable conic");

    assert!(error.contains("ConicByFivePoints"));
    assert_eq!(doc.version, version_before);
    assert_eq!(doc.object_count(), object_count);
    assert_eq!(doc.constraints.constraint_count(), constraint_count);
    assert_eq!(
        serde_json::to_value(&doc).expect("document should serialize"),
        before
    );
}

#[test]
fn conic_by_five_circular_points_creates_a_finite_isotropic_ellipse() {
    let mut doc = Document::new();
    let points: Vec<ObjectId> = (0..5)
        .map(|index| {
            let angle = index as f64 * std::f64::consts::TAU / 5.0;
            doc.add_point(Point2::new(angle.cos(), angle.sin()))
        })
        .collect();

    let constraint = doc
        .try_add_conic_by_five_points_constraint(&points)
        .expect("five points on a circle define an ellipse");
    let output = doc
        .constraints
        .get_constraint(constraint)
        .expect("conic constraint is registered")
        .outputs[0];

    match doc.get_object(output) {
        Some(GeoObject::Ellipse(ellipse)) => {
            assert!(ellipse.angle.is_finite());
            assert!(ellipse.rx.is_finite() && ellipse.rx > 0.0);
            assert!(ellipse.ry.is_finite() && ellipse.ry > 0.0);
            assert!((ellipse.rx - ellipse.ry).abs() < 1e-6);
        }
        object => panic!("expected an ellipse, got {object:?}"),
    }
}

#[test]
fn constructed_output_is_evaluated_before_insertion_commits() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(4.0, 0.0));

    let (midpoint, _) = doc
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(100.0, 100.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        )
        .expect("valid midpoint construction");

    assert_eq!(doc.point_position(midpoint), Some(Point2::new(2.0, 0.0)));
}

#[test]
fn constructed_insertion_rejects_missing_wrong_arity_duplicate_and_wrong_typed_inputs() {
    let mut missing = Document::new();
    let point = missing.add_point(Point2::new(0.0, 0.0));
    let before = serde_json::to_value(&missing).expect("document serializes");
    assert!(missing
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, ObjectId::new()],
        )
        .is_err());
    assert_eq!(serde_json::to_value(&missing).unwrap(), before);

    let mut wrong_arity = Document::new();
    let point = wrong_arity.add_point(Point2::new(0.0, 0.0));
    let before = serde_json::to_value(&wrong_arity).expect("document serializes");
    assert!(wrong_arity
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point],
        )
        .is_err());
    assert_eq!(serde_json::to_value(&wrong_arity).unwrap(), before);

    let mut duplicate = Document::new();
    let point = duplicate.add_point(Point2::new(0.0, 0.0));
    let before = serde_json::to_value(&duplicate).expect("document serializes");
    assert!(duplicate
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, point],
        )
        .is_err());
    assert_eq!(serde_json::to_value(&duplicate).unwrap(), before);

    let mut wrong_type = Document::new();
    let point = wrong_type.add_point(Point2::new(0.0, 0.0));
    let line = wrong_type.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let before = serde_json::to_value(&wrong_type).expect("document serializes");
    assert!(wrong_type
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[point, line],
        )
        .is_err());
    assert_eq!(serde_json::to_value(&wrong_type).unwrap(), before);
}

#[test]
fn moving_circle_inputs_to_collinear_positions_is_rejected_atomically() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(2.0, 0.0));
    let c = doc.add_point(Point2::new(0.0, 2.0));
    let (circle, _) = doc
        .try_add_constructed_object(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("circle")),
            "CircleByThreePoints",
            &[a, b, c],
        )
        .expect("non-collinear points define a circle");
    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;
    let circle_before = doc.get_object(circle).cloned();

    let error = doc
        .try_move_point_and_re_evaluate(c, Point2::new(1.0, 0.0))
        .expect_err("collinear points must make the required circle undefined");

    assert!(error.contains("CircleByThreePoints"), "{error}");
    assert_eq!(doc.version, version_before);
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    assert_eq!(doc.get_object(circle), circle_before.as_ref());
}

#[test]
fn legacy_point_move_fails_closed_when_construction_becomes_undefined() {
    let mut doc = Document::new();
    let a = doc.add_point(Point2::new(0.0, 0.0));
    let b = doc.add_point(Point2::new(2.0, 0.0));
    let c = doc.add_point(Point2::new(0.0, 2.0));
    doc.try_add_constructed_object(
        GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("circle")),
        "CircleByThreePoints",
        &[a, b, c],
    )
    .expect("non-collinear points define a circle");
    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;

    let affected = doc.move_point(c, Point2::new(1.0, 0.0));

    assert!(affected.is_empty());
    assert_eq!(doc.version, version_before);
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn deleting_a_target_cascades_through_all_object_references() {
    let mut doc = Document::new();
    let target = doc.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
        "x^2 + y^2",
        "1",
        RelationOperator::Eq,
    )));
    let mapping = doc.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "z^2", target,
    )));
    let integral = doc.add_object(GeoObject::ComplexIntegral(ComplexIntegralObj::new(
        "1/z", target, false,
    )));

    assert!(doc.remove_object(target).is_some());

    assert!(doc.get_object(mapping).is_none());
    assert!(doc.get_object(integral).is_none());
    grafito_core::validation::validate_document(&doc)
        .expect("reference cascade must leave a valid document");
}

#[test]
fn duplicate_explicit_labels_are_not_inserted_by_the_legacy_wrapper() {
    let mut doc = Document::new();
    let first = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(1.0, 0.0)).with_label("A"),
        ));
    }));

    assert!(doc.get_object(first).is_some());
    assert!(rejected.is_err());
    assert_eq!(doc.object_count(), 1);
}

#[test]
fn legacy_insertion_cannot_exceed_the_document_object_limit() {
    let mut doc = Document::new();
    for index in 0..grafito_core::validation::MAX_OBJECT_COUNT {
        let id = doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(index as f64, 0.0)).with_label(format!("P{index}")),
        ));
        assert!(doc.get_object(id).is_some());
    }

    let error = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(-1.0, 0.0)).with_label("fallible-overflow"),
        ))
        .expect_err("fallible insertion must enforce the same object limit");
    assert!(error.contains("maximum"), "{error}");

    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(-1.0, 0.0)).with_label("overflow"),
        ));
    }));

    assert!(rejected.is_err());
    assert_eq!(
        doc.object_count(),
        grafito_core::validation::MAX_OBJECT_COUNT
    );
}

#[test]
fn infinite_lines_remain_pickable_beyond_their_defining_points() {
    let mut doc = Document::new();
    let line = doc.add_object(GeoObject::Line(
        LineObj::new_with_kind(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), LineKind::Line)
            .with_label("axis"),
    ));
    doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(5.0, 0.15)).with_label("nearby"),
    ));

    assert_eq!(doc.pick_object(Point2::new(5.0, 0.0), 0.1), Some(line));
}

#[test]
fn segment_spatial_bounds_include_the_visible_stroke_width() {
    let mut doc = Document::new();
    let mut segment = LineObj::new(Point2::new(-1.0, 0.0), Point2::new(1.0, 0.0));
    segment.label = "wide".to_string();
    segment.width = 20.0;
    let segment = doc.add_object(GeoObject::Line(segment));

    assert_eq!(doc.pick_object(Point2::new(0.0, 0.15), 0.01), Some(segment));
}

#[test]
fn detached_staging_clones_never_reuse_a_clean_spatial_index() {
    let mut doc = Document::new();
    doc.add_point(Point2::new(0.0, 0.0));
    assert!(doc.pick_object(Point2::new(0.0, 0.0), 0.1).is_some());
    assert!(!doc.spatial_dirty);

    let staged = doc.detached_clone_for_staging();

    assert!(staged.spatial_dirty);
    assert!(staged.spatial.is_empty());
}

#[test]
fn direct_variable_changes_cannot_leave_bound_point_picking_stale() {
    let mut doc = Document::new();
    doc.set_variable("a".to_string(), 0.0);
    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("bound");
    point.x_expr = Some("a".to_string());
    let point = doc.add_object(GeoObject::Point(point));
    assert_eq!(doc.pick_object(Point2::new(0.0, 0.0), 0.1), Some(point));
    assert!(!doc.spatial_dirty);

    doc.variables.insert("a".to_string(), 5.0);

    assert_eq!(doc.pick_object(Point2::new(5.0, 0.0), 0.1), Some(point));
}

#[test]
fn fallible_insertion_rejects_duplicate_labels_without_mutation() {
    let mut doc = Document::new();
    doc.try_add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ))
    .expect("first label is unique");
    let before = serde_json::to_value(&doc).expect("document serializes");

    let error = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(1.0, 0.0)).with_label("A"),
        ))
        .expect_err("duplicate labels must be rejected");

    assert!(error.contains("label"), "{error}");
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn fallible_insertion_rejects_invalid_primitives_without_mutating_runtime_state() {
    let mut doc = Document::new();
    let anchor = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(2.0, 3.0)).with_label("anchor"),
        ))
        .expect("valid anchor point");
    assert_eq!(doc.pick_object(Point2::new(2.0, 3.0), 0.1), Some(anchor));
    assert!(!doc.spatial_dirty);

    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;
    let spatial_len_before = doc.spatial.len();
    let candidates_before = doc.spatial.candidates(2.0, 3.0, 0.1);

    for (object, expected_error) in [
        (
            GeoObject::Circle(
                CircleObj::new(Point2::new(0.0, 0.0), 0.0).with_label("invalid-circle"),
            ),
            "Circle.radius must be positive",
        ),
        (
            GeoObject::Cube3D(
                Cube3DObj::new(grafito_geometry::Point3D::new(0.0, 0.0, 0.0), 0.0)
                    .with_label("invalid-cube"),
            ),
            "Cube3D.size must be positive",
        ),
        (
            GeoObject::Tetrahedron3D(
                Tetrahedron3DObj::new(grafito_geometry::Point3D::new(0.0, 0.0, 0.0), 0.0)
                    .with_label("invalid-tetrahedron"),
            ),
            "Tetrahedron3D.edge_length must be positive",
        ),
    ] {
        let rejected_id = object.id();
        let error = doc
            .try_add_object(object)
            .expect_err("invalid standalone primitive must be rejected");

        assert!(error.contains(expected_error), "{error}");
        assert!(doc.get_object(rejected_id).is_none());
        assert_eq!(doc.version, version_before);
        assert!(!doc.spatial_dirty);
        assert_eq!(doc.spatial.len(), spatial_len_before);
        assert_eq!(doc.spatial.candidates(2.0, 3.0, 0.1), candidates_before);
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
}

#[test]
fn fallible_insertion_rejects_invalid_regular_polytope_candidates_without_mutation() {
    let mut doc = Document::new();
    let anchor = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(2.0, 3.0)).with_label("anchor"),
        ))
        .expect("valid anchor point");
    assert_eq!(doc.pick_object(Point2::new(2.0, 3.0), 0.1), Some(anchor));
    assert!(!doc.spatial_dirty);

    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;
    let spatial_len_before = doc.spatial.len();
    let candidates_before = doc.spatial.candidates(2.0, 3.0, 0.1);

    let mut nonfinite_polychoron_scale = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    nonfinite_polychoron_scale.scale = f64::NAN;

    let mut zero_polychoron_scale = RegularPolychoron4DObj::new(RegularPolychoron::Pentachoron);
    zero_polychoron_scale.scale = 0.0;

    let mut nonfinite_polychoron_rotation =
        RegularPolychoron4DObj::new(RegularPolychoron::SixteenCell);
    nonfinite_polychoron_rotation.rotation_angles[4] = f64::INFINITY;

    let mut invalid_polychoron_color =
        RegularPolychoron4DObj::new(RegularPolychoron::TwentyFourCell);
    invalid_polychoron_color.color = Color::new(f32::NAN, 0.2, 0.3, 1.0);

    let mut zero_polychoron_width = RegularPolychoron4DObj::new(RegularPolychoron::SixHundredCell);
    zero_polychoron_width.width = 0.0;

    let mut huge_polychoron_scale = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    huge_polychoron_scale.scale = 1.0e13;

    let mut nonfinite_polytope_scale = RegularPolytopeNDObj::new(RegularPolytopeFamily::Simplex, 5);
    nonfinite_polytope_scale.scale = f64::INFINITY;

    let mut nonfinite_polytope_rotation =
        RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5);
    nonfinite_polytope_rotation.rotation_angles[7] = f64::NAN;

    let mut wrong_polytope_rotation_count =
        RegularPolytopeNDObj::new(RegularPolytopeFamily::CrossPolytope, 5);
    wrong_polytope_rotation_count.rotation_angles.pop();

    let mut zero_dimensional_polytope =
        RegularPolytopeNDObj::new(RegularPolytopeFamily::Simplex, 3);
    zero_dimensional_polytope.dimension = 0;

    let mut undersized_polytope = RegularPolytopeNDObj::new(RegularPolytopeFamily::Simplex, 3);
    undersized_polytope.dimension = 2;

    let mut oversized_polytope = RegularPolytopeNDObj::new(RegularPolytopeFamily::Simplex, 10);
    oversized_polytope.dimension = 11;

    let mut invalid_polytope_fill =
        RegularPolytopeNDObj::new(RegularPolytopeFamily::CrossPolytope, 5);
    invalid_polytope_fill.fill_color = Some(Color::new(0.2, 0.3, f32::INFINITY, 1.0));

    let mut huge_polytope_scale = RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5);
    huge_polytope_scale.scale = 1.0e13;

    for (object, expected_error) in [
        (
            GeoObject::RegularPolychoron4D(nonfinite_polychoron_scale),
            "RegularPolychoron4D.scale must be finite",
        ),
        (
            GeoObject::RegularPolychoron4D(zero_polychoron_scale),
            "RegularPolychoron4D.scale must be positive",
        ),
        (
            GeoObject::RegularPolychoron4D(nonfinite_polychoron_rotation),
            "RegularPolychoron4D.rotation_angles[4] must be finite",
        ),
        (
            GeoObject::RegularPolychoron4D(invalid_polychoron_color),
            "RegularPolychoron4D.color.r must be finite",
        ),
        (
            GeoObject::RegularPolychoron4D(zero_polychoron_width),
            "RegularPolychoron4D.width must be positive",
        ),
        (
            GeoObject::RegularPolychoron4D(huge_polychoron_scale),
            "RegularPolychoron4D.scale projection bound exceeds maximum renderable coordinate",
        ),
        (
            GeoObject::RegularPolytopeND(nonfinite_polytope_scale),
            "RegularPolytopeND.scale must be finite",
        ),
        (
            GeoObject::RegularPolytopeND(nonfinite_polytope_rotation),
            "RegularPolytopeND.rotation_angles[7] must be finite",
        ),
        (
            GeoObject::RegularPolytopeND(wrong_polytope_rotation_count),
            "RegularPolytopeND.rotation_angles must contain 10 angles for dimension 5",
        ),
        (
            GeoObject::RegularPolytopeND(zero_dimensional_polytope),
            "RegularPolytopeND.dimension 0 must be between 3 and 10",
        ),
        (
            GeoObject::RegularPolytopeND(undersized_polytope),
            "RegularPolytopeND.dimension 2 must be between 3 and 10",
        ),
        (
            GeoObject::RegularPolytopeND(oversized_polytope),
            "RegularPolytopeND.dimension 11 must be between 3 and 10",
        ),
        (
            GeoObject::RegularPolytopeND(invalid_polytope_fill),
            "RegularPolytopeND.fill_color.b must be finite",
        ),
        (
            GeoObject::RegularPolytopeND(huge_polytope_scale),
            "RegularPolytopeND.scale projection bound exceeds maximum renderable coordinate",
        ),
    ] {
        let rejected_id = object.id();
        let error = doc
            .try_add_object(object)
            .expect_err("invalid regular polytope must be rejected");

        assert!(error.contains(expected_error), "{error}");
        assert!(doc.get_object(rejected_id).is_none());
        assert_eq!(doc.version, version_before);
        assert!(!doc.spatial_dirty);
        assert_eq!(doc.spatial.len(), spatial_len_before);
        assert_eq!(doc.spatial.candidates(2.0, 3.0, 0.1), candidates_before);
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
}

#[test]
fn fallible_insertion_accepts_normal_regular_polytope_scales() {
    let mut doc = Document::new();

    for kind in [
        RegularPolychoron::Pentachoron,
        RegularPolychoron::Tesseract,
        RegularPolychoron::SixteenCell,
        RegularPolychoron::TwentyFourCell,
        RegularPolychoron::OneTwentyCell,
        RegularPolychoron::SixHundredCell,
    ] {
        doc.try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            kind,
        )))
        .expect("la escala normal 4D debe ser aceptada");
    }

    for family in [
        RegularPolytopeFamily::Simplex,
        RegularPolytopeFamily::Hypercube,
        RegularPolytopeFamily::CrossPolytope,
    ] {
        doc.try_add_object(GeoObject::RegularPolytopeND(RegularPolytopeNDObj::new(
            family, 5,
        )))
        .expect("la escala normal N-D debe ser aceptada");
    }

    assert_eq!(doc.object_count(), 9);
}

#[test]
fn oversized_one_twenty_cell_insertion_is_atomic() {
    let mut doc = Document::new();
    let anchor = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(2.0, 3.0)).with_label("anchor"),
        ))
        .expect("valid anchor point");
    assert_eq!(doc.pick_object(Point2::new(2.0, 3.0), 0.1), Some(anchor));
    assert!(!doc.spatial_dirty);

    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;
    let spatial_len_before = doc.spatial.len();
    let candidates_before = doc.spatial.candidates(2.0, 3.0, 0.1);
    let radius = RegularPolychoron::OneTwentyCell.canonical_radius_bound();
    let threshold_scale = MAX_WORLD_COORDINATE * 5.0 / (6.0 * radius);
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::OneTwentyCell);
    polychoron.scale = threshold_scale * 1.000_001;
    let rejected_id = polychoron.id;

    let error = doc
        .try_add_object(GeoObject::RegularPolychoron4D(polychoron))
        .expect_err("el 120-celda sobredimensionado debe rechazarse");

    assert!(error.contains("projection bound exceeds maximum renderable coordinate"));
    assert!(doc.get_object(rejected_id).is_none());
    assert_eq!(doc.version, version_before);
    assert!(!doc.spatial_dirty);
    assert_eq!(doc.spatial.len(), spatial_len_before);
    assert_eq!(doc.spatial.candidates(2.0, 3.0, 0.1), candidates_before);
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn regular_polytope_objects_are_excluded_from_the_2d_spatial_index() {
    let mut doc = Document::new();
    let polychoron = doc
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("valid regular polychoron");
    let polytope = doc
        .try_add_object(GeoObject::RegularPolytopeND(RegularPolytopeNDObj::new(
            RegularPolytopeFamily::Hypercube,
            5,
        )))
        .expect("valid regular N-D polytope");

    doc.rebuild_spatial_index();

    assert!(!doc.spatial_dirty);
    assert!(doc.spatial.is_empty());
    assert!(!doc.spatial.candidates(0.0, 0.0, 1.0).contains(&polychoron));
    assert!(!doc.spatial.candidates(0.0, 0.0, 1.0).contains(&polytope));
}

#[test]
fn fallible_insertion_accepts_a_valid_candidate_beside_legacy_duplicate_labels() {
    let mut doc = Document::new();
    doc.try_add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ))
    .expect("first legacy point");
    let second = doc
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
        ))
        .expect("second legacy point");
    doc.get_object_mut(second)
        .expect("second point exists")
        .set_label("A".to_string());

    let inserted = doc
        .try_add_object(GeoObject::Circle(
            CircleObj::new(Point2::new(3.0, 4.0), 2.0).with_label("valid-circle"),
        ))
        .expect("candidate validation must not reject compatible legacy duplicates");

    assert!(matches!(
        doc.get_object(inserted),
        Some(GeoObject::Circle(_))
    ));
    assert_eq!(doc.object_ids_by_label("A").len(), 2);
}

#[test]
fn deprecated_add_object_cannot_return_an_unattached_id_on_rejection() {
    let mut doc = Document::new();
    doc.try_add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ))
    .expect("first label is unique");
    let before = serde_json::to_value(&doc).expect("document serializes");
    let version_before = doc.version;

    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(1.0, 0.0)).with_label("A"),
        ));
    }));

    assert!(
        rejected.is_err(),
        "infallible compatibility API must fail loudly"
    );
    assert_eq!(doc.version, version_before);
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn legacy_ambiguous_labels_have_sorted_matches_and_no_unique_resolution() {
    let mut doc = Document::new();
    let first = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let second = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    doc.get_object_mut(second)
        .expect("second object exists")
        .set_label("A".to_string());
    let mut expected = vec![first, second];
    expected.sort_unstable();

    assert_eq!(doc.object_ids_by_label("A"), expected);
    let error = doc
        .try_find_object_by_label("A")
        .expect_err("ambiguous labels cannot resolve to an arbitrary object");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn persisted_duplicate_labels_are_preserved_but_never_resolved_arbitrarily() {
    let mut doc = Document::new();
    let first = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let second = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    doc.get_object_mut(second)
        .expect("second object exists")
        .set_label("A".to_string());
    let serialized = serialize_document(&doc).expect("legacy duplicate labels remain persistable");

    let loaded =
        deserialize_document(&serialized).expect("legacy duplicate labels remain loadable");

    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(loaded.object_ids_by_label("A"), expected);
    assert!(loaded.try_find_object_by_label("A").is_err());
}

#[test]
fn loop_animation_wraps_a_phase_parameter_without_leaving_its_domain() {
    let mut document = Document::new();
    document.set_variable("phase".into(), 0.0);
    document
        .configure_variable_animation(
            "phase",
            0.0,
            std::f64::consts::TAU,
            1.0,
            AnimationMode::Loop,
        )
        .expect("a finite nonempty loop range should be configurable");

    assert!(document.advance_variable_animations(std::f64::consts::TAU + 0.25));
    assert!((document.get_variable("phase").unwrap() - 0.25).abs() < 1e-9);
    assert_eq!(
        document
            .variable_meta("phase")
            .expect("phase metadata exists")
            .animation_mode,
        AnimationMode::Loop
    );
}

#[test]
fn ping_pong_animation_reverses_at_its_bounds() {
    let mut document = Document::new();
    document.set_variable("amplitude".into(), 0.75);
    document
        .configure_variable_animation("amplitude", 0.0, 1.0, 1.0, AnimationMode::PingPong)
        .expect("a finite nonempty ping-pong range should be configurable");

    assert!(document.advance_variable_animations(1.0));
    assert_eq!(document.get_variable("amplitude"), Some(1.0));
    assert_eq!(
        document
            .variable_meta("amplitude")
            .expect("amplitude metadata exists")
            .animation_speed,
        -1.0
    );
}

#[test]
fn invalid_animation_configuration_does_not_mutate_the_document() {
    let mut document = Document::new();
    document.set_variable("stable".into(), 2.0);
    let before = serde_json::to_value(&document).expect("document serializes");

    let error = document
        .configure_variable_animation("stable", 1.0, 1.0, 1.0, AnimationMode::Loop)
        .expect_err("equal animation bounds are invalid");

    assert!(error.contains("minimum"));
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn variable_meta_replacement_commits_a_valid_candidate_as_one_revision() {
    let mut document = Document::new();
    document
        .try_set_variable("t".to_string(), 2.0)
        .expect("fixture variable inserts");
    document.rebuild_spatial_index();
    let version_before = document.version;
    let mut candidate = variable_meta_candidate();
    candidate.min = -3.0;
    candidate.max = 7.0;

    let previous = document
        .try_replace_variable_meta_with_previous("t", candidate.clone())
        .expect("a valid metadata replacement commits")
        .expect("a changed metadata candidate returns the prior document");

    assert!(previous.variable_meta("t").is_none());
    assert_eq!(document.variable_meta("t"), Some(&candidate));
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert!(document.spatial_dirty);
}

#[test]
fn variable_meta_replacement_no_op_preserves_runtime_state() {
    let mut document = Document::new();
    document
        .try_set_variable("t".to_string(), 2.0)
        .expect("fixture variable inserts");
    let candidate = variable_meta_candidate();
    document
        .try_replace_variable_meta_with_previous("t", candidate.clone())
        .expect("fixture metadata inserts");
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();

    assert!(document
        .try_replace_variable_meta_with_previous("t", candidate)
        .expect("unchanged metadata is a no-op")
        .is_none());
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn variable_meta_replacement_rejects_invalid_candidates_atomically() {
    let mut document = Document::new();
    document
        .try_set_variable("t".to_string(), 2.0)
        .expect("fixture variable inserts");
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();

    let mut reversed = variable_meta_candidate();
    reversed.min = 3.0;
    reversed.max = -3.0;
    let mut nonfinite_bounds = variable_meta_candidate();
    nonfinite_bounds.max = f64::INFINITY;
    let mut nonfinite_position = variable_meta_candidate();
    nonfinite_position.position.x = f64::NAN;

    for candidate in [reversed, nonfinite_bounds, nonfinite_position] {
        assert!(document
            .try_replace_variable_meta_with_previous("t", candidate)
            .is_err());
        assert_eq!(document.version, version_before);
        assert!(!document.spatial_dirty);
        assert_eq!(document.spatial.len(), spatial_len_before);
        assert_eq!(serde_json::to_value(&document).unwrap(), before);
    }
}

#[test]
fn variable_meta_replacement_requires_an_existing_variable_without_mutation() {
    let mut document = Document::new();
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();

    assert!(document
        .try_replace_variable_meta_with_previous("missing", variable_meta_candidate())
        .expect("a missing variable is a no-op")
        .is_none());
    assert!(document.variable_meta("missing").is_none());
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn persistent_locus_tracks_a_constructed_target_across_save_load_and_cascade_delete() {
    let mut document = Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let mut translation = std::collections::HashMap::new();
    translation.insert("dx".to_string(), 1.0);
    translation.insert("dy".to_string(), -2.0);
    let (target, _) = document
        .try_add_constructed_object_with_params(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("B")),
            "Translate",
            &[driver],
            translation,
        )
        .expect("translated target should be constructible");
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("two distinct points should define a locus");

    assert!(matches!(
        document.get_object(locus),
        Some(GeoObject::Pencil(pencil))
            if pencil.is_dynamic_locus()
                && pencil.locus_binding().is_some_and(|binding| binding.driver == driver && binding.target == target)
                && pencil.points == vec![Point2::new(1.0, -2.0)]
    ));

    assert!(document
        .try_move_point_and_re_evaluate(driver, Point2::new(3.0, 4.0))
        .expect("moving the driver should remain valid"));
    assert!(matches!(
        document.get_object(target),
        Some(GeoObject::Point(point)) if point.position == Point2::new(4.0, 2.0)
    ));
    assert!(matches!(
        document.get_object(locus),
        Some(GeoObject::Pencil(pencil))
            if pencil.points == vec![Point2::new(1.0, -2.0), Point2::new(4.0, 2.0)]
    ));

    let saved = serialize_document(&document).expect("locus document should serialize");
    let mut loaded = deserialize_document(&saved).expect("locus document should deserialize");
    assert!(loaded
        .try_move_point_and_re_evaluate(driver, Point2::new(5.0, 6.0))
        .expect("loaded locus should continue tracing"));
    assert!(matches!(
        loaded.get_object(locus),
        Some(GeoObject::Pencil(pencil))
            if pencil.points.last() == Some(&Point2::new(6.0, 4.0))
    ));

    loaded.remove_object(driver);
    assert!(loaded.get_object(target).is_none());
    assert!(loaded.get_object(locus).is_none());
}

#[test]
fn variable_animation_captures_one_post_propagation_locus_sample_without_pointer_data() {
    let mut document = Document::new();
    document.set_variable("phase".to_string(), 0.0);
    let mut bound_driver = PointObj::new(Point2::new(0.0, 0.0)).with_label("A");
    bound_driver.x_expr = Some("phase".to_string());
    bound_driver.y_expr = Some("0".to_string());
    let driver = document.add_object(GeoObject::Point(bound_driver));
    let mut translation = std::collections::HashMap::new();
    translation.insert("dx".to_string(), 2.0);
    translation.insert("dy".to_string(), 0.0);
    let (target, _) = document
        .try_add_constructed_object_with_params(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("B")),
            "Translate",
            &[driver],
            translation,
        )
        .expect("translated target should be constructible");
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("locus should be constructible");
    document
        .configure_variable_animation("phase", 0.0, 1.0, 1.0, AnimationMode::Loop)
        .expect("animation should be configurable");

    assert!(document.advance_variable_animations(0.25));
    assert!(matches!(
        document.get_object(locus),
        Some(GeoObject::Pencil(pencil))
            if pencil.points == vec![Point2::new(2.0, 0.0), Point2::new(2.25, 0.0)]
                && pencil.locus_binding().is_some()
    ));
    let serialized = serialize_document(&document).expect("locus state should serialize");
    assert!(!serialized.contains("pointer"));
    assert!(!serialized.contains("timestamp"));
}

#[test]
fn variable_animation_recomputes_spreadsheet_dependencies_before_bound_geometry() {
    let mut document = Document::new();
    document.set_variable("phase".to_string(), 0.0);
    let mut document = document
        .stage_spreadsheet_cell_edits(&[(0, 0, "phase * 2".to_string())])
        .expect("spreadsheet formula should stage");
    let mut point = PointObj::new(Point2::new(0.0, 0.0)).with_label("P");
    point.x_expr = Some("A1".to_string());
    let point = document.add_object(GeoObject::Point(point));
    document
        .configure_variable_animation("phase", 0.0, 1.0, 1.0, AnimationMode::Loop)
        .expect("source variable should be animatable");

    assert!(document.advance_variable_animations(0.25));
    assert_eq!(document.get_variable("phase"), Some(0.25));
    assert_eq!(document.get_variable("A1"), Some(0.5));
    assert!(matches!(
        document.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(0.5, 0.0)
    ));

    let reopened = deserialize_document(
        &serialize_document(&document).expect("animated spreadsheet document serializes"),
    )
    .expect("animated spreadsheet document reopens");
    assert_eq!(reopened.get_variable("A1"), Some(0.5));
    assert!(matches!(
        reopened.get_object(point),
        Some(GeoObject::Point(point)) if point.position == Point2::new(0.5, 0.0)
    ));
}

#[test]
fn spreadsheet_coordinate_points_reject_direct_moves_and_reconcile_on_load() {
    let mut document = Document::new()
        .stage_spreadsheet_cell_edits(&[(0, 0, "(1, 2)".to_string())])
        .expect("coordinate cell should stage");
    let point = document
        .spreadsheet_coordinate_point("A1")
        .expect("coordinate cell owns its point");
    let before = serde_json::to_value(&document).expect("document serializes");

    let error = document
        .try_move_point_and_re_evaluate(point, Point2::new(3.0, 4.0))
        .expect_err("coordinate-owned points must reject direct moves");
    assert!(error.contains("spreadsheet"), "{error}");
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        before
    );

    let Some(GeoObject::Point(point_object)) = document.get_object_mut(point) else {
        panic!("coordinate point remains available");
    };
    point_object.position = Point2::new(3.0, 4.0);
    let reopened = deserialize_document(
        &serialize_document(&document).expect("legacy-mismatched coordinate point serializes"),
    )
    .expect("coordinate source reconciles during load");
    assert!(matches!(
        reopened.get_object(point),
        Some(GeoObject::Point(point_object)) if point_object.position == Point2::new(1.0, 2.0)
    ));
}

#[test]
fn manual_variable_update_propagates_and_captures_a_locus_atomically() {
    let mut document = Document::new();
    document.set_variable("phase".to_string(), 0.0);
    let mut driver = PointObj::new(Point2::new(0.0, 0.0)).with_label("A");
    driver.x_expr = Some("phase".to_string());
    let driver = document.add_object(GeoObject::Point(driver));
    let mut translation = std::collections::HashMap::new();
    translation.insert("dx".to_string(), 1.0);
    translation.insert("dy".to_string(), 0.0);
    let (target, _) = document
        .try_add_constructed_object_with_params(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("B")),
            "Translate",
            &[driver],
            translation,
        )
        .expect("translated target should be constructible");
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("locus should be constructible");

    document.set_variable("phase".to_string(), 3.0);

    assert!(matches!(
        document.get_object(target),
        Some(GeoObject::Point(point)) if point.position == Point2::new(4.0, 0.0)
    ));
    assert!(matches!(
        document.get_object(locus),
        Some(GeoObject::Pencil(pencil))
            if pencil.points == vec![Point2::new(1.0, 0.0), Point2::new(4.0, 0.0)]
    ));
}

#[test]
fn initial_locus_sample_is_pickable_before_the_target_moves() {
    let mut document = Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let target = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 1.0)).with_label("B"),
    ));
    let (locus, _) = document
        .try_add_locus(driver, target)
        .expect("locus should be constructible");
    document
        .get_object_mut(target)
        .expect("target exists")
        .set_visible(false);

    assert_eq!(
        document.pick_object(Point2::new(2.0, 1.0), 0.1),
        Some(locus),
        "the initial locus marker must be selectable before it has a segment"
    );
}

#[test]
fn try_replace_object_commits_a_valid_candidate_as_one_revision() {
    let mut document = Document::new();
    let id = document.add_point(Point2::new(1.0, 2.0));
    document.rebuild_spatial_index();
    let version_before = document.version;
    let mut candidate = document
        .get_object(id)
        .cloned()
        .expect("inserted point remains available");
    let GeoObject::Point(point) = &mut candidate else {
        panic!("fixture remains a point");
    };
    point.position = Point2::new(3.0, 4.0);

    assert!(document
        .try_replace_object(id, candidate)
        .expect("a valid replacement commits"));
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert!(document.spatial_dirty);
    assert!(matches!(
        document.get_object(id),
        Some(GeoObject::Point(point)) if point.position == Point2::new(3.0, 4.0)
    ));
}

#[test]
fn try_replace_object_no_op_preserves_runtime_state() {
    let mut document = Document::new();
    let id = document.add_point(Point2::new(1.0, 2.0));
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(1.0, 2.0, 0.1);
    let constraint_count_before = document.constraints.constraint_count();
    let candidate = document
        .get_object(id)
        .cloned()
        .expect("inserted point remains available");

    assert!(!document
        .try_replace_object(id, candidate)
        .expect("an unchanged candidate is a no-op"));
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(1.0, 2.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(
        document.constraints.constraint_count(),
        constraint_count_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn try_replace_object_rejects_a_mismatched_candidate_without_mutation() {
    let mut document = Document::new();
    let id = document.add_point(Point2::new(1.0, 2.0));
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(1.0, 2.0, 0.1);
    let constraint_count_before = document.constraints.constraint_count();
    let candidate = GeoObject::Point(PointObj::new(Point2::new(3.0, 4.0)));

    assert!(document.try_replace_object(id, candidate).is_err());
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(1.0, 2.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(
        document.constraints.constraint_count(),
        constraint_count_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn try_replace_object_rejects_a_document_invalid_candidate_atomically() {
    let mut document = Document::new();
    let first = document.add_point(Point2::new(0.0, 0.0));
    let second = document.add_point(Point2::new(2.0, 0.0));
    let (midpoint, _) = document
        .try_add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0)).with_label("M")),
            "Midpoint",
            &[first, second],
        )
        .expect("midpoint fixture is valid");
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(1.0, 0.0, 0.1);
    let constraint_count_before = document.constraints.constraint_count();
    let mut replacement = CircleObj::new(Point2::new(1.0, 0.0), 1.0).with_label("M");
    replacement.id = midpoint;

    assert!(document
        .try_replace_object(midpoint, GeoObject::Circle(replacement))
        .is_err());
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(1.0, 0.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(
        document.constraints.constraint_count(),
        constraint_count_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert!(matches!(
        document.get_object(midpoint),
        Some(GeoObject::Point(_))
    ));
}

#[test]
fn try_replace_object_rejects_a_duplicate_candidate_label_atomically() {
    let mut document = Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let id = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    document.rebuild_spatial_index();
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let spatial_len_before = document.spatial.len();
    let spatial_candidates_before = document.spatial.candidates(1.0, 0.0, 0.1);
    let constraint_count_before = document.constraints.constraint_count();
    let mut candidate = document
        .get_object(id)
        .cloned()
        .expect("second labeled point remains available");
    candidate.set_label("A".to_string());

    let error = document
        .try_replace_object(id, candidate)
        .expect_err("a replacement cannot introduce a duplicate label");

    assert!(error.contains("already in use"), "{error}");
    assert_eq!(document.version, version_before);
    assert!(!document.spatial_dirty);
    assert_eq!(document.spatial.len(), spatial_len_before);
    assert_eq!(
        document.spatial.candidates(1.0, 0.0, 0.1),
        spatial_candidates_before
    );
    assert_eq!(
        document.constraints.constraint_count(),
        constraint_count_before
    );
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert_eq!(document.object_ids_by_label("A").len(), 1);
}

#[test]
fn try_replace_object_preserves_persisted_legacy_label_ambiguity_and_rejects_new_collision() {
    let mut document = Document::new();
    let first = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let id = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
    ));
    let third = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("C"),
    ));
    document
        .get_object_mut(id)
        .expect("second point remains available")
        .set_label("A".to_string());
    let serialized = serialize_document(&document).expect("legacy document persists");
    let mut document =
        deserialize_document(&serialized).expect("legacy duplicate labels load unchanged");
    let mut candidate = document
        .get_object(id)
        .cloned()
        .expect("legacy duplicate remains editable");
    let GeoObject::Point(point) = &mut candidate else {
        panic!("fixture remains a point");
    };
    point.position = Point2::new(2.0, 0.0);

    assert!(document
        .try_replace_object(id, candidate)
        .expect("an unchanged legacy label remains compatible"));
    let mut expected = vec![first, id];
    expected.sort_unstable();
    assert_eq!(document.object_ids_by_label("A"), expected);
    assert!(document.try_find_object_by_label("A").is_err());
    assert!(matches!(
        document.get_object(id),
        Some(GeoObject::Point(point)) if point.position == Point2::new(2.0, 0.0)
    ));

    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let mut colliding_candidate = document
        .get_object(third)
        .cloned()
        .expect("third point remains available");
    colliding_candidate.set_label("A".to_string());

    let error = document
        .try_replace_object(third, colliding_candidate)
        .expect_err("a changed candidate cannot introduce another legacy-label collision");

    assert!(error.contains("already in use"), "{error}");
    assert_eq!(document.version, version_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert_eq!(document.object_ids_by_label("A"), expected);
    assert!(document.try_find_object_by_label("A").is_err());
    assert!(matches!(
        document.get_object(third),
        Some(GeoObject::Point(point)) if point.label == "C"
    ));
}
