use glam::Vec3;

#[test]
fn test_camera_project() {
    let aspect = 1.6;
    let mut camera = grafito_geometry::types3d::Camera3D::new(aspect);
    camera.distance = 60.0;
    camera.target = Vec3::new(0.0, 0.0, 20.0);

    let p = grafito_geometry::types3d::Point3D::new(10.0, 20.0, 25.0);
    let proj = camera.project(&p, 1000.0, 800.0);
    println!("Projection of (10, 20, 25): {:?}", proj);
}

#[test]
fn test_save_load_roundtrip() {
    use grafito_core::*;
    use grafito_geometry::*;
    let mut doc = Document::new();
    doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    doc.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(0.0, 0.0),
        5.0,
    )));
    doc.set_variable("a".into(), 3.14);

    let tmp = std::env::temp_dir().join("grafito_test_roundtrip.json");
    crate::export::save_document(&doc, &tmp.to_string_lossy()).expect("save failed");
    let loaded = crate::export::load_document(&tmp.to_string_lossy()).expect("load failed");
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(loaded.object_count(), 2);
    assert_eq!(loaded.get_variable("a"), Some(3.14));
}

#[test]
fn test_save_load_constraint_params_roundtrip() {
    use grafito_core::*;
    use grafito_geometry::*;
    use std::collections::HashMap;

    let mut doc = Document::new();
    let a = doc.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let mut params = HashMap::new();
    params.insert("dx".to_string(), 2.0);
    params.insert("dy".to_string(), 3.0);
    let (_p, cons_id) = doc.add_constructed_object_with_params(
        GeoObject::Point(PointObj::new(Point2::new(2.0, 3.0)).with_label("A'")),
        "Translate",
        &[a],
        params,
    );

    let tmp = std::env::temp_dir().join("grafito_test_constraint_params.json");
    crate::export::save_document(&doc, &tmp.to_string_lossy()).expect("save failed");
    let loaded = crate::export::load_document(&tmp.to_string_lossy()).expect("load failed");
    let _ = std::fs::remove_file(&tmp);

    let cons = loaded
        .constraints
        .get_constraint(cons_id)
        .expect("constraint should survive roundtrip");
    assert_eq!(cons.params.get("dx"), Some(&2.0));
    assert_eq!(cons.params.get("dy"), Some(&3.0));
}

#[test]
fn test_export_svg() {
    use grafito_core::*;
    use grafito_geometry::*;
    let mut doc = Document::new();
    doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(3.0, 4.0),
    )));

    let svg = crate::export::export_svg(&doc, 800.0, 600.0);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("<circle"));
    assert!(svg.contains("<line"));
}
