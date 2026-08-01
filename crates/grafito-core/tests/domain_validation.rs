use grafito_core::{
    deserialize_document, serialize_document, CircleObj, ComplexGridObj, Cube3DObj, Document,
    EllipseObj, GeoObject, LineObj, ParametricCurve2DObj, ParametricCurve3DObj, PointObj,
    PolarCurveObj, RegularPolychoron4DObj, RegularPolytopeNDObj, Sphere3DObj, Surface3DObj,
    Tetrahedron3DObj,
};
use grafito_geometry::{
    Point2, Point3D, RegularPolychoron, RegularPolytopeFamily, MAX_REGULAR_POLYTOPE_DIMENSION,
    MAX_WORLD_COORDINATE, MIN_REGULAR_POLYTOPE_DIMENSION,
};
use std::collections::HashMap;

fn assert_persistence_rejects(object: GeoObject, expected_message: &str) {
    let id = object.id();
    let mut raw = serde_json::to_value(Document::new()).expect("serialize empty document");
    raw["objects"]
        .as_object_mut()
        .expect("objects are represented as a map")
        .insert(
            id.0.to_string(),
            serde_json::to_value(object).expect("serialize unchecked object"),
        );
    let document: Document =
        serde_json::from_value(raw).expect("deserialize unchecked test document");

    let save_error = serialize_document(&document).expect_err("invalid domain must not serialize");
    assert!(
        save_error.to_string().contains(expected_message),
        "{save_error}"
    );

    let raw_document = serde_json::to_string(&document).expect("serialize raw document");
    let load_error =
        deserialize_document(&raw_document).expect_err("invalid domain must not deserialize");
    assert!(
        load_error.to_string().contains(expected_message),
        "{load_error}"
    );
}

#[test]
fn persistence_rejects_unordered_graph_domains() {
    for (object, expected_message) in [
        (
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new("t", "t", 1.0, 1.0)),
            "ParametricCurve2D.t_min must be less than ParametricCurve2D.t_max",
        ),
        (
            GeoObject::PolarCurve(PolarCurveObj::new("1", 1.0, 0.0)),
            "PolarCurve.t_min must be less than PolarCurve.t_max",
        ),
        (
            GeoObject::ComplexGrid(ComplexGridObj::new("z", 1.0, -1.0, -1.0, 1.0)),
            "ComplexGrid.x_min must be less than ComplexGrid.x_max",
        ),
        (
            GeoObject::Surface3D(Surface3DObj::new("x + y", (1.0, -1.0), (-1.0, 1.0))),
            "Surface3D.x_min must be less than Surface3D.x_max",
        ),
        (
            GeoObject::Surface3D(Surface3DObj::new_parametric(
                "u",
                "v",
                "u + v",
                (0.0, 1.0),
                (1.0, 0.0),
            )),
            "Surface3D.v_min must be less than Surface3D.v_max",
        ),
    ] {
        assert_persistence_rejects(object, expected_message);
    }
}

#[test]
fn persistence_rejects_unordered_parametric_curve_3d_domains() {
    for (object, expected_message) in [
        (
            GeoObject::ParametricCurve3D(ParametricCurve3DObj::new("t", "t", "t", 1.0, 1.0)),
            "ParametricCurve3D.t_min must be less than ParametricCurve3D.t_max",
        ),
        (
            GeoObject::ParametricCurve3D(ParametricCurve3DObj::new("t", "t", "t", 1.0, 0.0)),
            "ParametricCurve3D.t_min must be less than ParametricCurve3D.t_max",
        ),
    ] {
        assert_persistence_rejects(object, expected_message);
    }
}

#[test]
fn persistence_preserves_ordered_parametric_curve_3d_domains() {
    let mut document = Document::new();
    document.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
        "cos(t)",
        "sin(t)",
        "t",
        0.0,
        std::f64::consts::TAU,
    )));

    let serialized = serialize_document(&document).expect("valid curve must serialize");
    deserialize_document(&serialized).expect("valid curve must deserialize");
}

#[test]
fn persistence_rejects_numeric_constraints_with_incompatible_inputs() {
    let mut document = Document::new();
    let point = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
    let other_point = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
    let line = document.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let circle = document.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(0.0, 0.0),
        1.0,
    )));

    let mut distance = HashMap::new();
    distance.insert("distance".to_string(), 1.0);
    let mut angle = HashMap::new();
    angle.insert("angle".to_string(), 90.0);

    for (name, inputs, params) in [
        ("Distance", vec![point, line], distance),
        ("Angle", vec![point, line], angle),
        ("Tangent", vec![point, line], HashMap::new()),
        ("Coincident", vec![point, line], HashMap::new()),
        ("Horizontal", vec![point], HashMap::new()),
        ("Vertical", vec![point], HashMap::new()),
        ("EqualLength", vec![line, circle], HashMap::new()),
        ("Symmetry", vec![point, line, other_point], HashMap::new()),
        ("Distance", vec![point], HashMap::new()),
    ] {
        let mut persisted = document.clone();
        persisted
            .constraints
            .add_constraint(name, inputs, Vec::new(), params);

        let raw = serde_json::to_string(&persisted).expect("serialize persisted document");
        let error = deserialize_document(&raw)
            .expect_err("incompatible persisted numeric constraint must be rejected");
        assert!(
            error.to_string().contains(name),
            "expected {name} validation error, got {error}"
        );
    }
}

#[test]
fn persistence_prunes_invalid_spreadsheet_coordinate_ownership() {
    let mut document = Document::new();
    let owned = document.add_object(GeoObject::Point(
        grafito_core::PointObj::new(grafito_geometry::Point2::new(1.0, 2.0)).with_label("A1"),
    ));
    let manual = document.add_object(GeoObject::Point(
        grafito_core::PointObj::new(grafito_geometry::Point2::new(9.0, 9.0)).with_label("B1"),
    ));
    document.set_spreadsheet_coordinate_point("A1".to_string(), owned);

    let encoded = serialize_document(&document).expect("valid document serializes");
    let mut value: serde_json::Value = serde_json::from_str(&encoded).expect("parse document");
    let owners = value["document"]["spreadsheet_coordinate_points"]
        .as_object_mut()
        .expect("serialized ownership map");
    owners.clear();
    owners.insert(
        "invalid".to_string(),
        serde_json::to_value(owned).expect("encode object id"),
    );
    owners.insert(
        "A1".to_string(),
        serde_json::to_value(manual).expect("encode object id"),
    );

    let mut loaded = deserialize_document(&serde_json::to_string(&value).expect("encode document"))
        .expect("invalid ownership mappings are safely pruned");
    assert_eq!(loaded.spreadsheet_coordinate_point("A1"), None);
    assert_eq!(loaded.spreadsheet_coordinate_point("invalid"), None);
    assert!(
        matches!(loaded.get_object(manual), Some(GeoObject::Point(point)) if point.label == "B1")
    );
}

#[test]
fn persistence_rejects_invalid_primitive_dimensions_and_render_bounds() {
    for (object, expected_message) in [
        (
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), -1.0)),
            "Circle.radius must be positive",
        ),
        (
            GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 0.0, 1.0)),
            "Ellipse.rx must be positive",
        ),
        (
            GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), -1.0)),
            "Sphere3D.radius must be positive",
        ),
        (
            GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 0.0)),
            "Cube3D.size must be positive",
        ),
        (
            GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(Point3D::new(0.0, 0.0, 0.0), 0.0)),
            "Tetrahedron3D.edge_length must be positive",
        ),
        (
            GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(
                Point3D::new(1_000_000_000_000.0, 0.0, 0.0),
                2.0,
            )),
            "Tetrahedron3D vertices exceed the maximum renderable coordinate",
        ),
    ] {
        assert_persistence_rejects(object, expected_message);
    }
}

#[test]
fn persistence_rejects_invalid_regular_polytope_parameters() {
    let mut zero_scale = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    zero_scale.scale = 0.0;

    let mut zero_width = RegularPolychoron4DObj::new(RegularPolychoron::Pentachoron);
    zero_width.width = 0.0;

    let mut wrong_rotation_count = RegularPolytopeNDObj::new(RegularPolytopeFamily::Simplex, 5);
    wrong_rotation_count.rotation_angles.pop();

    let mut below_minimum = RegularPolytopeNDObj::new(
        RegularPolytopeFamily::CrossPolytope,
        MIN_REGULAR_POLYTOPE_DIMENSION,
    );
    below_minimum.dimension = MIN_REGULAR_POLYTOPE_DIMENSION - 1;

    let mut above_maximum = RegularPolytopeNDObj::new(
        RegularPolytopeFamily::Hypercube,
        MAX_REGULAR_POLYTOPE_DIMENSION,
    );
    above_maximum.dimension = MAX_REGULAR_POLYTOPE_DIMENSION + 1;

    for (object, expected_message) in [
        (
            GeoObject::RegularPolychoron4D(zero_scale),
            "RegularPolychoron4D.scale must be positive",
        ),
        (
            GeoObject::RegularPolychoron4D(zero_width),
            "RegularPolychoron4D.width must be positive",
        ),
        (
            GeoObject::RegularPolytopeND(wrong_rotation_count),
            "RegularPolytopeND.rotation_angles must contain 10 angles for dimension 5",
        ),
        (
            GeoObject::RegularPolytopeND(below_minimum),
            "RegularPolytopeND.dimension 2 must be between 3 and 10",
        ),
        (
            GeoObject::RegularPolytopeND(above_maximum),
            "RegularPolytopeND.dimension 11 must be between 3 and 10",
        ),
    ] {
        assert_persistence_rejects(object, expected_message);
    }
}

#[test]
fn persistence_rejects_regular_polytope_projection_bounds() {
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.scale = 1.0e13;

    let mut polytope = RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5);
    polytope.scale = 1.0e13;

    for object in [
        GeoObject::RegularPolychoron4D(polychoron),
        GeoObject::RegularPolytopeND(polytope),
    ] {
        assert_persistence_rejects(
            object,
            "projection bound exceeds maximum renderable coordinate",
        );
    }
}

#[test]
fn persistence_rejects_finite_oversized_one_twenty_cell_projection_bound() {
    let radius = RegularPolychoron::OneTwentyCell.canonical_radius_bound();
    let threshold_scale = MAX_WORLD_COORDINATE * 5.0 / (6.0 * radius);
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::OneTwentyCell);
    polychoron.scale = threshold_scale * 1.000_001;

    assert!(polychoron.scale.is_finite());
    assert_persistence_rejects(
        GeoObject::RegularPolychoron4D(polychoron),
        "projection bound exceeds maximum renderable coordinate",
    );
}
