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
    doc.set_variable("a".into(), 42.0);

    let tmp = std::env::temp_dir().join("grafito_test_roundtrip.json");
    crate::export::save_document(&doc, &tmp.to_string_lossy()).expect("save failed");
    let loaded = crate::export::load_document(&tmp.to_string_lossy()).expect("load failed");
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(loaded.object_count(), 2);
    assert_eq!(loaded.get_variable("a"), Some(42.0));
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

// ── Tests del sistema de Perspectivas ────────────────────────────────────

#[test]
fn test_perspective_all_has_ten_variants() {
    assert_eq!(crate::Perspective::ALL.len(), 10);
}

#[test]
fn test_perspective_view_mode_derivation() {
    use crate::Perspective;
    use crate::ViewMode;
    // Perspectivas 3D → D3, el resto → D2.
    assert_eq!(Perspective::Geometry3D.view_mode(), ViewMode::D3);
    assert_eq!(Perspective::Dynamics.view_mode(), ViewMode::D3);
    assert_eq!(Perspective::Geometry2D.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::AlgebraCas.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::Calculus.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::Probability.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::Statistics.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::Complex.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::DataAnalysis.view_mode(), ViewMode::D2);
    assert_eq!(Perspective::Exam.view_mode(), ViewMode::D2);
}

#[test]
fn test_perspective_shortcut_numbers_unique() {
    use crate::Perspective;
    let mut nums: Vec<u8> = Perspective::ALL
        .iter()
        .map(|p| p.shortcut_number())
        .collect();
    nums.sort_unstable();
    // Cada atajo es único y cubre 0..=9 (1..9 para las nueve primeras, 0 para Exam).
    assert_eq!(nums, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn test_perspective_layout_canvas_modes() {
    use crate::CanvasMode;
    use crate::Perspective;
    assert_eq!(Perspective::Geometry2D.layout().canvas_mode, CanvasMode::D2);
    assert_eq!(Perspective::Geometry3D.layout().canvas_mode, CanvasMode::D3);
    assert_eq!(
        Perspective::AlgebraCas.layout().canvas_mode,
        CanvasMode::SmallD2
    );
    assert_eq!(
        Perspective::Probability.layout().canvas_mode,
        CanvasMode::SmallD2
    );
    assert_eq!(Perspective::Dynamics.layout().canvas_mode, CanvasMode::D3);
}

#[test]
fn test_perspective_layout_tool_groups_nonempty() {
    use crate::Perspective;
    for p in Perspective::ALL {
        let layout = p.layout();
        assert!(
            !layout.visible_tool_groups.is_empty(),
            "perspectiva {:?} no define grupos de herramientas",
            p
        );
    }
}

#[test]
fn test_perspective_layout_exam_restricted() {
    use crate::Perspective;
    let layout = Perspective::Exam.layout();
    assert!(layout.right_panel.is_none());
    assert!(!layout.show_math_keyboard);
    // Modo examen: sólo herramientas básicas (Move, Point, Line, Circle, Polygon).
    assert_eq!(layout.visible_tool_groups.len(), 5);
}

#[test]
fn test_left_panel_default_sidebar_tab() {
    use crate::LeftPanelContent;
    assert_eq!(LeftPanelContent::Algebra.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::AlgebraAndCas.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Tools.default_sidebar_tab(), 1);
    assert_eq!(LeftPanelContent::Cas.default_sidebar_tab(), 2);
    assert_eq!(LeftPanelContent::Stats.default_sidebar_tab(), 3);
    assert_eq!(LeftPanelContent::Spreadsheet.default_sidebar_tab(), 4);
}

#[test]
fn test_tool_group_id_def_nonempty() {
    for &gid in grafito_ui::toolbar::ALL_GROUPS {
        let (_icon, tools) = gid.def();
        assert!(!tools.is_empty(), "grupo {:?} vacío", gid);
    }
}
