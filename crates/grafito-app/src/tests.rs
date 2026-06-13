use glam::{Vec3, Mat4};

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
    doc.add_object(GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 5.0)));
    doc.set_variable("a".into(), 3.14);

    let tmp = std::env::temp_dir().join("grafito_test_roundtrip.json");
    crate::export::save_document(&doc, &tmp.to_string_lossy()).expect("save failed");
    let loaded = crate::export::load_document(&tmp.to_string_lossy()).expect("load failed");
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(loaded.object_count(), 2);
    assert_eq!(loaded.get_variable("a"), Some(3.14));
}

#[test]
fn test_export_svg() {
    use grafito_core::*;
    use grafito_geometry::*;
    let mut doc = Document::new();
    doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    doc.add_object(GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(3.0, 4.0))));

    let svg = crate::export::export_svg(&doc, 800.0, 600.0);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("<circle"));
    assert!(svg.contains("<line"));
}
