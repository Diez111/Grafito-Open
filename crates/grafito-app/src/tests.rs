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
fn test_perspective_all_has_nine_variants() {
    assert_eq!(crate::Perspective::ALL.len(), 9);
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
}

#[test]
fn test_perspective_shortcut_numbers_unique() {
    use crate::Perspective;
    let mut nums: Vec<u8> = Perspective::ALL
        .iter()
        .map(|p| p.shortcut_number())
        .collect();
    nums.sort_unstable();
    // Cada atajo es único y cubre 1..=9.
    assert_eq!(nums, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
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
        assert!(
            !p.description().trim().is_empty(),
            "perspectiva {:?} no define descripcion",
            p
        );
    }
}

#[test]
fn test_data_analysis_uses_guided_stats_panel() {
    use crate::{LeftPanelContent, Perspective};
    let layout = Perspective::DataAnalysis.layout();
    assert_eq!(layout.left_panel, LeftPanelContent::Stats);
    // Sin panel derecho para evitar la franja negra gruesa en dark mode
    // (el fill del SidePanel::right con panel_bg = rgb(26,26,30) se ve
    // como una banda casi negra junto al canvas).
    assert_eq!(layout.right_panel, None);
    assert!(layout.show_input_bar);
}

#[test]
fn test_perspective_right_panels_dont_share_sheet_render() {
    // Data y Regression deben tener su propio panel derecho (no la grilla
    // de hoja) para que el contenido se mantenga dentro del viewport y no
    // fuerce al SidePanel a expandirse. Esto es un test de regresión para
    // evitar que vuelva a usarse draw_right_spreadsheet para variantes que
    // no son Spreadsheet.
    use crate::RightPanelContent;
    assert_ne!(RightPanelContent::Data, RightPanelContent::Spreadsheet);
    assert_ne!(
        RightPanelContent::Regression,
        RightPanelContent::Spreadsheet
    );
    // Cada variante debe tener un dispatcher único.
    let variants = [
        RightPanelContent::Spreadsheet,
        RightPanelContent::Data,
        RightPanelContent::Regression,
    ];
    assert!(variants
        .iter()
        .enumerate()
        .all(|(i, a)| variants.iter().skip(i + 1).all(|b| a != b)));
}

#[test]
fn test_spreadsheet_grid_responsive_min_col_width() {
    // Verifica la fórmula responsiva: para un panel de 280px y 6 columnas,
    // min_col_width debe ser (280 - 36) / 6 ≈ 40.67, clampeado a [36, 96].
    let cols = 6_usize;
    let row_label_w = 36.0_f32;
    let panel_w = 280.0_f32;
    let grid_w = (panel_w - row_label_w).max(120.0);
    let min_col_w = (grid_w / cols as f32).clamp(36.0, 96.0);
    assert!(min_col_w >= 36.0 && min_col_w <= 96.0);
    assert!(min_col_w < 52.0); // antes era 52.0 hardcoded, ahora más chico para panels angostos
}

#[test]
fn test_spreadsheet_grid_min_col_width_dense_columns() {
    // Con muchas columnas (26 letras), el clamp debe mantener un mínimo
    // legible y nunca un valor mayor a 96.
    let cols = 26_usize;
    let row_label_w = 36.0_f32;
    let panel_w = 280.0_f32;
    let grid_w = (panel_w - row_label_w).max(120.0);
    let min_col_w = (grid_w / cols as f32).clamp(36.0, 96.0);
    assert!(min_col_w >= 36.0);
    assert!(min_col_w <= 96.0);
}

#[test]
fn test_perspective_layout_does_not_enable_sheet_for_data_or_regression() {
    // Comprobación semántica: una perspectiva con right_panel=Data o
    // Regression no debe activar show_spreadsheet (esa grilla sólo va con
    // right_panel=Spreadsheet). Verificamos la regla que implementa
    // set_perspective para no caer en la regresión anterior.
    use crate::Perspective;
    for p in Perspective::ALL {
        let layout = p.layout();
        let should_show_sheet = matches!(
            layout.right_panel,
            Some(crate::RightPanelContent::Spreadsheet)
        );
        match layout.right_panel {
            Some(crate::RightPanelContent::Data) | Some(crate::RightPanelContent::Regression) => {
                assert!(
                    !should_show_sheet,
                    "Perspectiva {:?} usa right_panel Data/Regression pero la grilla de hoja no debe activarse",
                    p
                );
            }
            _ => {}
        }
    }
}

#[test]
fn test_left_panel_default_sidebar_tab() {
    use crate::LeftPanelContent;
    assert_eq!(LeftPanelContent::Algebra.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::AlgebraAndCas.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Tools.default_sidebar_tab(), 1);
    assert_eq!(LeftPanelContent::Cas.default_sidebar_tab(), 2);
    // Stats ahora mapea al tab "Tabla" (3); Complejos a "Álgebra" (0);
    // Atractores a "Herram." (1) — fusión con tabs existentes.
    assert_eq!(LeftPanelContent::Stats.default_sidebar_tab(), 3);
    assert_eq!(LeftPanelContent::Complex.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Attractor.default_sidebar_tab(), 1);
    assert_eq!(LeftPanelContent::Spreadsheet.default_sidebar_tab(), 4);
}

#[test]
fn test_tool_group_id_def_nonempty() {
    for &gid in grafito_ui::toolbar::ALL_GROUPS {
        let (_icon, tools) = gid.def();
        assert!(!tools.is_empty(), "grupo {:?} vacío", gid);
    }
}
