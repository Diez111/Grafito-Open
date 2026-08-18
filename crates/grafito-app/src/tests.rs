use glam::Vec3;

const _: [(); 1] = [(); crate::app::DEFAULT_KEYBOARD_VISIBLE as usize];
const _: [(); 0] = [(); crate::app::DEFAULT_CONSTRUCTION_PROTOCOL_VISIBLE as usize];

#[test]
fn fresh_workspace_starts_with_an_empty_document_and_visible_keyboard() {
    let document = crate::app::initial_document();

    assert_eq!(document.object_count(), 0);
    assert!(document.variables.is_empty());
}

#[test]
fn three_dimensional_gpu_warmup_waits_for_view_changes_to_settle() {
    assert!(!crate::app::should_use_gpu_3d(true, true, true));
    assert!(!crate::app::should_use_gpu_3d(true, false, false));
    assert!(crate::app::should_use_gpu_3d(true, true, false));
}

#[test]
fn construction_protocol_avoids_controls_that_do_not_change_construction() {
    let source = include_str!("panels.rs");

    assert!(!source.contains("move_step_button("));
    assert!(!source.contains("button(if disabled"));
}

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
fn cpu_point_label_projection_rejects_a_point_beyond_the_far_plane() {
    let mut camera = axis_aligned_test_camera();
    camera.far = 5.0;

    assert_eq!(
        crate::render_3d::projected_point_position(
            &camera,
            grafito_geometry::Point3D::new(0.0, 0.0, 0.0),
            egui::vec2(800.0, 600.0),
        ),
        None,
        "CPU point labels must not survive far-plane clipping"
    );
}

fn axis_aligned_test_camera() -> grafito_geometry::Camera3D {
    let mut camera = grafito_geometry::Camera3D::new(4.0 / 3.0);
    camera.theta = 0.0;
    camera.phi = 0.0;
    camera.distance = 10.0;
    camera.target = Vec3::ZERO;
    camera
}

#[test]
fn construction_points_follow_canvas_local_pointer_on_target_plane() {
    let camera = axis_aligned_test_camera();
    let canvas = egui::Rect::from_min_size(egui::pos2(125.0, 75.0), egui::vec2(800.0, 600.0));
    let center_pointer = crate::input::canvas_local_pointer(canvas, egui::pos2(525.0, 375.0))
        .expect("global center pointer becomes canvas-local");
    let off_center_pointer = crate::input::canvas_local_pointer(canvas, egui::pos2(725.0, 275.0))
        .expect("global off-center pointer becomes canvas-local");
    assert_eq!(center_pointer, egui::vec2(400.0, 300.0));
    assert_eq!(off_center_pointer, egui::vec2(600.0, 200.0));
    assert!(crate::input::canvas_local_pointer(canvas, egui::pos2(124.0, 75.0)).is_none());

    let center =
        crate::render_3d::construction_point_from_canvas(&camera, center_pointer, canvas.size())
            .expect("center placement");
    let off_center = crate::render_3d::construction_point_from_canvas(
        &camera,
        off_center_pointer,
        canvas.size(),
    )
    .expect("off-center placement");

    assert!(center.distance(&grafito_geometry::Point3D::new(0.0, 0.0, 0.0)) < 1.0e-4);
    assert!(center.distance(&off_center) > 0.1);
    assert!(center.x.abs() < 1.0e-4);
    assert!(off_center.x.abs() < 1.0e-4);
}

#[test]
fn cpu_tetrahedron_fallback_projects_four_depth_sorted_faces() {
    let camera = axis_aligned_test_camera();
    let tetrahedron =
        grafito_geometry::Tetrahedron3D::new(grafito_geometry::Point3D::new(0.0, 0.0, 0.0), 2.0);

    let faces = crate::render_3d::projected_tetrahedron_faces(&camera, &tetrahedron, 800.0, 600.0);

    assert_eq!(faces.len(), 4);
    assert!(faces.windows(2).all(|faces| faces[0].0 >= faces[1].0));
    assert!(faces
        .iter()
        .all(|(_, face)| { face.iter().all(|(x, y)| x.is_finite() && y.is_finite()) }));
}

#[test]
fn picker_hits_required_3d_primitive_types() {
    use grafito_core::{
        Cube3DObj, GeoObject, Line3DObj, Plane3DObj, Point3DObj, Segment3DObj, Sphere3DObj,
        Tetrahedron3DObj,
    };
    use grafito_geometry::Point3D;

    let camera = axis_aligned_test_camera();
    let pointer = egui::vec2(400.0, 300.0);
    let canvas_size = egui::vec2(800.0, 600.0);
    let objects = [
        GeoObject::Point3D(Point3DObj::new(Point3D::new(0.0, 0.0, 0.0))),
        GeoObject::Segment3D(Segment3DObj::new(
            Point3D::new(0.0, -1.0, 0.0),
            Point3D::new(0.0, 1.0, 0.0),
        )),
        GeoObject::Line3D(Line3DObj::from_point_and_direction(
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(0.0, 1.0, 0.0),
        )),
        GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0)),
        GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0)),
        GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0)),
        GeoObject::Plane3D(Plane3DObj::from_equation(1.0, 0.0, 0.0, 0.0)),
    ];

    for object in objects {
        let expected = object.id();
        let mut document = grafito_core::Document::new();
        document
            .try_add_object(object)
            .expect("valid primitive fixture");
        assert_eq!(
            crate::render_3d::pick_3d_object(&document, &camera, pointer, canvas_size),
            Some(expected),
            "primitive {expected} should be selectable"
        );
    }
}

#[test]
fn picker_uses_nearest_visible_hit_and_deterministic_ties() {
    use grafito_core::{GeoObject, Sphere3DObj};
    use grafito_geometry::Point3D;

    let camera = axis_aligned_test_camera();
    let pointer = egui::vec2(400.0, 300.0);
    let canvas_size = egui::vec2(800.0, 600.0);
    let mut document = grafito_core::Document::new();
    let far = document
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(
            Point3D::new(-2.0, 0.0, 0.0),
            1.0,
        )))
        .expect("far sphere");
    let near = document
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(
            Point3D::new(3.0, 0.0, 0.0),
            1.0,
        )))
        .expect("near sphere");

    assert_eq!(
        crate::render_3d::pick_3d_object(&document, &camera, pointer, canvas_size),
        Some(near)
    );
    document
        .get_object_mut(near)
        .expect("near sphere remains")
        .set_visible(false);
    assert_eq!(
        crate::render_3d::pick_3d_object(&document, &camera, pointer, canvas_size),
        Some(far)
    );

    let mut tied = grafito_core::Document::new();
    let first = tied
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(
            Point3D::new(0.0, 0.0, 0.0),
            1.0,
        )))
        .expect("first tied sphere");
    let second = tied
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(
            Point3D::new(0.0, 0.0, 0.0),
            1.0,
        )))
        .expect("second tied sphere");
    assert_eq!(
        crate::render_3d::pick_3d_object(&tied, &camera, pointer, canvas_size),
        Some(first.min(second))
    );
}

#[test]
fn picker_exact_hit_outranks_nearer_torus_fallback_through_its_hole() {
    use grafito_core::{GeoObject, Sphere3DObj, Torus3DObj};
    use grafito_geometry::Point3D;

    let mut camera = axis_aligned_test_camera();
    camera.phi = std::f32::consts::FRAC_PI_2 - 0.01;
    let pointer = egui::vec2(400.0, 300.0);
    let canvas_size = egui::vec2(800.0, 600.0);
    let ray = camera
        .screen_ray(pointer.x, pointer.y, canvas_size.x, canvas_size.y)
        .expect("top-view center ray");
    let sphere_center = ray.point_at(12.0).expect("sphere center inside frustum");
    let mut document = grafito_core::Document::new();
    let torus = document
        .try_add_object(GeoObject::Torus3D(Torus3DObj::new(
            Point3D::new(0.0, 0.0, 0.0),
            2.0,
            0.5,
        )))
        .expect("torus fixture");
    let sphere = document
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(sphere_center, 0.5)))
        .expect("sphere visible through the torus hole");

    assert_ne!(torus, sphere);
    assert_eq!(
        crate::render_3d::pick_3d_object(&document, &camera, pointer, canvas_size),
        Some(sphere),
        "a coarse torus AABB must not occlude an exact sphere hit"
    );
}

#[test]
fn picker_respects_frustum_clipping_and_surface_fallback_bounds() {
    use grafito_core::{GeoObject, Point3DObj, Surface3DObj, VectorField3DObj};
    use grafito_geometry::Point3D;

    let pointer = egui::vec2(400.0, 300.0);
    let canvas_size = egui::vec2(800.0, 600.0);
    let mut document = grafito_core::Document::new();
    document
        .try_add_object(GeoObject::Point3D(Point3DObj::new(Point3D::new(
            12.0, 0.0, 0.0,
        ))))
        .expect("point behind camera");
    let mut short_camera = axis_aligned_test_camera();
    short_camera.far = 5.0;
    document
        .try_add_object(GeoObject::Point3D(Point3DObj::new(Point3D::new(
            0.0, 0.0, 0.0,
        ))))
        .expect("point beyond far plane");
    assert_eq!(
        crate::render_3d::pick_3d_object(&document, &short_camera, pointer, canvas_size),
        None
    );

    let mut just_beyond_far = grafito_core::Document::new();
    just_beyond_far
        .try_add_object(GeoObject::Point3D(Point3DObj::new(Point3D::new(
            -991.0, 0.0, 0.0,
        ))))
        .expect("point just beyond the default far plane");
    assert_eq!(
        crate::render_3d::pick_3d_object(
            &just_beyond_far,
            &axis_aligned_test_camera(),
            pointer,
            canvas_size,
        ),
        None,
        "screen-space hit tolerance must not bypass far clipping"
    );

    let mut clipped_fallback = grafito_core::Document::new();
    clipped_fallback
        .try_add_object(GeoObject::VectorField3D(
            VectorField3DObj::new("1", "0", "0").with_bounds(
                (-992.0, -991.0),
                (-1.0, 1.0),
                (-1.0, 1.0),
            ),
        ))
        .expect("fallback bounds beyond far plane");
    assert_eq!(
        crate::render_3d::pick_3d_object(
            &clipped_fallback,
            &axis_aligned_test_camera(),
            pointer,
            canvas_size,
        ),
        None,
        "fallback padding must not bypass far clipping"
    );

    let mut surface_document = grafito_core::Document::new();
    let surface = surface_document
        .try_add_object(GeoObject::Surface3D(Surface3DObj::new(
            "0",
            (-1.0, 1.0),
            (-1.0, 1.0),
        )))
        .expect("flat surface");
    assert_eq!(
        crate::render_3d::pick_3d_object(
            &surface_document,
            &axis_aligned_test_camera(),
            pointer,
            canvas_size,
        ),
        Some(surface)
    );
}

#[test]
fn select_3d_click_updates_and_empty_click_clears_selection() {
    use grafito_core::{GeoObject, Sphere3DObj};
    use grafito_geometry::Point3D;

    let mut document = grafito_core::Document::new();
    let sphere = document
        .try_add_object(GeoObject::Sphere3D(Sphere3DObj::new(
            Point3D::new(0.0, 0.0, 0.0),
            1.0,
        )))
        .expect("sphere fixture");
    let mut selected_object = None;
    let camera = axis_aligned_test_camera();
    let canvas_size = egui::vec2(800.0, 600.0);

    assert_eq!(
        crate::render_3d::select_3d_object_at_pointer(
            &mut document,
            &mut selected_object,
            &camera,
            egui::vec2(400.0, 300.0),
            canvas_size,
        ),
        Some(sphere)
    );
    assert_eq!(selected_object, Some(sphere));
    assert_eq!(document.selection(), &[sphere]);

    assert_eq!(
        crate::render_3d::select_3d_object_at_pointer(
            &mut document,
            &mut selected_object,
            &camera,
            egui::vec2(0.0, 0.0),
            canvas_size,
        ),
        None
    );
    assert_eq!(selected_object, None);
    assert!(document.selection().is_empty());
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

    doc.view_mut().screen_size = glam::Vec2::new(800.0, 600.0);
    let path = std::env::temp_dir().join(format!(
        "grafito_app_export_test_{}.svg",
        std::process::id()
    ));
    crate::export::export_svg(&doc, &path).expect("SVG export should succeed");
    let svg = std::fs::read_to_string(&path).expect("SVG should be readable");
    let _ = std::fs::remove_file(path);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("data-grafito-type=\"Point\""));
    assert!(svg.contains("data-grafito-type=\"Line\""));
}

#[test]
fn export_ui_never_uses_a_fixed_working_directory_path_or_ignored_write_result() {
    let panels = include_str!("panels.rs");
    let app = include_str!("app.rs");

    assert!(!panels.contains("grafito_export.svg"));
    assert!(!panels.contains("let _ = std::fs::write"));
    assert!(!app.contains("std::fs::write(&path, svg)"));
    assert!(!app.contains("std::fs::write(&path, tex)"));
}

#[test]
fn export_outcomes_keep_full_persistent_feedback_and_show_a_toast() {
    use crate::export::{ExportError, ExportFormat, ExportItem, ExportReport};

    let mut cas_result = String::new();
    let mut toasts = grafito_ui::toast::ToastManager::default();
    let empty_toasts = toast_shape_count(&mut grafito_ui::toast::ToastManager::default());
    let report = ExportReport {
        format: ExportFormat::Svg,
        path: std::path::PathBuf::from("/tmp/scene.svg"),
        exported_objects: 2,
        hidden_objects: 1,
        primitive_count: 4,
        object_types: std::collections::BTreeMap::from([
            ("Circle".to_string(), 1),
            ("Point".to_string(), 1),
        ]),
    };

    crate::app::apply_export_outcome(Ok(report), &mut cas_result, &mut toasts, 0.0);
    assert!(cas_result.contains("2 objetos"));
    assert!(cas_result.contains("Circle x1"));
    assert!(cas_result.contains("1 ocultos"));
    assert!(toast_shape_count(&mut toasts) > empty_toasts);

    let omitted = ExportItem {
        object_type: "Surface3D".to_string(),
        label: "S".to_string(),
        object_id: "fixture".to_string(),
    };
    crate::app::apply_export_outcome(
        Err(ExportError::UnsupportedObjects {
            format: ExportFormat::Png,
            objects: vec![omitted],
        }),
        &mut cas_result,
        &mut toasts,
        1.0,
    );
    assert!(cas_result.contains("Surface3D 'S'"));
    assert!(cas_result.contains("no reemplazo el destino"));
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
fn stable_perspectives_do_not_expose_unavailable_placeholder_tools() {
    use crate::Perspective;
    use grafito_ui::Tool;

    for perspective in Perspective::ALL {
        for group in perspective.layout().visible_tool_groups {
            let (_, tools) = group.def();
            for unavailable in [Tool::Button, Tool::Image] {
                assert!(
                    tools.iter().all(|(tool, _, _)| *tool != unavailable),
                    "{perspective:?} exposes unavailable tool {unavailable:?} through {group:?}"
                );
            }
        }
    }
}

#[test]
fn stable_tools_panel_and_status_help_do_not_claim_unavailable_features() {
    let tools_panel = include_str!("tools_panel.rs");
    let status_help = include_str!("ui.rs");

    for registration in ["(Tool::Button,", "(Tool::Image,"] {
        assert!(
            !tools_panel.contains(registration),
            "stable tools panel still registers {registration}"
        );
    }
    assert!(status_help.contains("Locus: clic punto driver, clic punto objetivo"));
}

#[test]
fn test_3d_perspectives_do_not_expose_2d_curve_group() {
    use crate::Perspective;
    use grafito_ui::toolbar::ToolGroupId;

    let geometry_3d = Perspective::Geometry3D.layout();
    assert!(geometry_3d
        .visible_tool_groups
        .contains(&ToolGroupId::ThreeD));
    assert!(!geometry_3d
        .visible_tool_groups
        .contains(&ToolGroupId::Curve));

    let dynamics = Perspective::Dynamics.layout();
    assert!(dynamics
        .visible_tool_groups
        .contains(&ToolGroupId::Dynamics));
    assert!(!dynamics
        .visible_tool_groups
        .contains(&ToolGroupId::Advanced));
}

#[test]
fn four_d_tools_are_available_only_from_the_geometry_3d_perspective() {
    use crate::Perspective;
    use grafito_ui::toolbar::ToolGroupId;

    assert!(Perspective::Geometry3D
        .layout()
        .visible_tool_groups
        .contains(&ToolGroupId::FourD));
    for perspective in Perspective::ALL {
        if perspective != Perspective::Geometry3D {
            assert!(
                !perspective
                    .layout()
                    .visible_tool_groups
                    .contains(&ToolGroupId::FourD),
                "{perspective:?} must not expose the centered projected 4D tools"
            );
        }
    }
}

#[test]
fn four_d_tools_create_select_typed_centered_defaults_and_reset_to_select() {
    use grafito_core::GeoObject;
    use grafito_geometry::{RegularPolychoron, RegularPolytopeFamily};
    use grafito_ui::Tool;

    for tool in [Tool::Tesseract4D, Tool::Hypercube5D] {
        let mut document = grafito_core::Document::new();
        let mut selected = None;
        let mut undo_stack = Vec::new();
        let mut redo_stack = Vec::new();
        let mut current_tool = tool;

        let (id, action) = crate::render_3d::create_centered_four_d_tool_object(
            &mut current_tool,
            &mut document,
            &mut selected,
            &mut undo_stack,
            &mut redo_stack,
        )
        .expect("the default centered projected object is valid")
        .expect("the selected tool creates an object");

        assert_eq!(current_tool, Tool::Select);
        assert_eq!(selected, Some(id));
        assert!(document.is_selected(id));
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
        match (tool, document.get_object(id)) {
            (Tool::Tesseract4D, Some(GeoObject::RegularPolychoron4D(polychoron))) => {
                assert_eq!(action, "Tesseract4D");
                assert_eq!(polychoron.kind, RegularPolychoron::Tesseract);
                assert_eq!(polychoron.scale, 1.0);
                assert_eq!(polychoron.rotation_angles, [0.0; 6]);
            }
            (Tool::Hypercube5D, Some(GeoObject::RegularPolytopeND(polytope))) => {
                assert_eq!(action, "Hypercube5D");
                assert_eq!(polytope.family, RegularPolytopeFamily::Hypercube);
                assert_eq!(polytope.dimension, 5);
                assert_eq!(polytope.scale, 1.0);
                assert_eq!(polytope.rotation_angles, vec![0.0; 10]);
            }
            (_, object) => panic!("unexpected centered projected object: {object:?}"),
        }
    }
}

#[test]
fn rejected_four_d_tool_insertion_preserves_the_document_history_and_tool_state() {
    use grafito_core::ChangeSet;
    use grafito_ui::Tool;

    let mut document = document_at_object_capacity();
    let selected = document
        .objects_iter()
        .next()
        .map(|(id, _)| *id)
        .expect("capacity fixture contains objects");
    let before = serde_json::to_value(&document).expect("serialize full document");
    let mut selected_object = Some(selected);
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];
    let mut current_tool = Tool::Tesseract4D;

    let error = crate::render_3d::create_centered_four_d_tool_object(
        &mut current_tool,
        &mut document,
        &mut selected_object,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect_err("the at-capacity insertion must fail atomically");

    assert!(error.contains("maximum"));
    assert_eq!(current_tool, Tool::Tesseract4D);
    assert_eq!(selected_object, Some(selected));
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
    assert_eq!(
        serde_json::to_value(&document).expect("serialize rejected full document"),
        before
    );
}

#[test]
fn four_d_tools_do_not_use_pointer_position_ghosts_and_explain_centered_projection() {
    use grafito_ui::Tool;

    assert!(!crate::input::uses_3d_position_ghost(Tool::Tesseract4D));
    assert!(!crate::input::uses_3d_position_ghost(Tool::Hypercube5D));
    assert!(crate::ui::status_hint_for_3d_tool(Tool::Tesseract4D).contains("centrado"));
    assert!(crate::ui::status_hint_for_3d_tool(Tool::Tesseract4D).contains("proyectado"));
    assert!(crate::ui::status_hint_for_3d_tool(Tool::Hypercube5D).contains("centrado"));
    assert!(crate::ui::status_hint_for_3d_tool(Tool::Hypercube5D).contains("proyectado"));
}

#[test]
fn four_d_tools_are_discoverable_from_the_3d_tools_panel_and_3d_click_route() {
    let tools_panel = include_str!("tools_panel.rs");
    let render_3d = include_str!("render_3d.rs");

    assert!(tools_panel.contains("4D proyectado"));
    assert!(tools_panel.contains("Crea un teseracto 4D centrado y proyectado"));
    assert!(tools_panel.contains("Crea un hipercubo 5D centrado y proyectado"));
    assert!(tools_panel.contains("WidgetInfo::labeled"));

    let handler_start = render_3d
        .find("pub fn handle_3d_click")
        .expect("3D click handler");
    let construction_point = render_3d
        .find("let Some(c) = construction_point_from_canvas")
        .expect("position-based 3D construction route");
    let centered_handler_route = &render_3d[handler_start..construction_point];
    assert!(
        centered_handler_route.contains("create_centered_four_d_tool_object("),
        "the centered projected object must be created before a pointer position is consumed"
    );
    assert!(
        !centered_handler_route.contains("tool_dispatcher"),
        "the centered projected object must not route through the 2D dispatcher"
    );
}

#[test]
fn algebra_summaries_identify_typed_centered_projected_polytopes() {
    use grafito_core::{GeoObject, RegularPolychoron4DObj, RegularPolytopeNDObj};
    use grafito_geometry::{RegularPolychoron, RegularPolytopeFamily};

    let tesseract = crate::algebra::object_expression_summary(&GeoObject::RegularPolychoron4D(
        RegularPolychoron4DObj::new(RegularPolychoron::Tesseract),
    ));
    let hypercube = crate::algebra::object_expression_summary(&GeoObject::RegularPolytopeND(
        RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5),
    ));

    assert!(tesseract.contains("Teseracto 4D"));
    assert!(tesseract.contains("centrado"));
    assert!(hypercube.contains("Hipercubo 5D"));
    assert!(hypercube.contains("centrado"));
}

#[test]
fn test_perspective_layout_exam_restricted() {
    use crate::Perspective;
    let layout = Perspective::Exam.layout();
    assert!(layout.right_panel.is_none());
    assert!(layout.show_math_keyboard);
    // Modo examen: sólo herramientas básicas (Move, Point, Line, Circle, Polygon).
    assert_eq!(layout.visible_tool_groups.len(), 5);
}

#[test]
fn every_perspective_keeps_the_math_keyboard_available() {
    use crate::Perspective;

    assert!(Perspective::ALL
        .into_iter()
        .all(|perspective| perspective.layout().show_math_keyboard));
}

#[test]
fn complex_perspective_uses_panel_input_instead_of_bottom_bar() {
    use crate::Perspective;

    assert!(!Perspective::Complex.layout().show_input_bar);
    assert!(Perspective::Geometry2D.layout().show_input_bar);
    assert!(Perspective::Calculus.layout().show_input_bar);
}

#[test]
fn complex_panel_recommends_executable_domain_coloring() {
    let source = include_str!("panels.rs");

    assert!(source.contains("DomainColoring[1/z, -2, 2, -2, 2, 160]"));
    assert!(!source.contains("ComplexGrid[1/z]\nColoración por fase"));
}

fn document_at_object_capacity() -> grafito_core::Document {
    static DOCUMENT: std::sync::OnceLock<grafito_core::Document> = std::sync::OnceLock::new();

    DOCUMENT
        .get_or_init(|| {
            let mut document = grafito_core::Document::new();
            for index in 0..grafito_core::validation::MAX_OBJECT_COUNT {
                document
                    .try_add_point(grafito_geometry::Point2::new(index as f64, 0.0))
                    .expect("capacity fixture point must be valid");
            }
            document
        })
        .clone()
}

#[test]
fn zero_radius_circle_tool_is_rejected_without_history_or_document_mutation() {
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    let mut document = grafito_core::Document::new();
    let mut state = crate::tool_dispatcher::ToolState::default();
    let point = Point2::new(2.0, 3.0);
    let first =
        crate::tool_dispatcher::dispatch_tool(Tool::Circle, &mut state, &mut document, point);
    assert!(first.objects.is_empty());
    let second =
        crate::tool_dispatcher::dispatch_tool(Tool::Circle, &mut state, &mut document, point);
    assert_eq!(second.objects.len(), 1);

    let before = serde_json::to_value(&document).expect("serialize document before rejection");
    let mut undo_stack = Vec::new();
    let mut redo_stack = Vec::new();
    let error = crate::app::commit_object_insertions(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        second.objects,
    )
    .expect_err("a circle whose two tool clicks coincide must be rejected");

    assert!(error.contains("Circle.radius"));
    assert!(state.pending.is_empty());
    assert!(undo_stack.is_empty());
    assert!(redo_stack.is_empty());
    assert_eq!(
        serde_json::to_value(&document).expect("serialize document after rejection"),
        before
    );
}

#[test]
fn legacy_placeholder_tools_error_without_document_mutation() {
    use grafito_command::commands::CommandOutcome;
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    for tool in [Tool::Button, Tool::Image] {
        let mut document = grafito_core::Document::new();
        document.set_variable("baseline".into(), 7.0);
        let before = serde_json::to_value(&document).expect("serialize document before tool");
        let mut state = crate::tool_dispatcher::ToolState::default();

        let result = crate::tool_dispatcher::dispatch_tool(
            tool,
            &mut state,
            &mut document,
            Point2::new(2.0, 3.0),
        );

        assert!(
            result.objects.is_empty(),
            "{tool:?} returned substitute objects"
        );
        assert!(result.reset_tool, "{tool:?} must return to Select");
        assert!(
            matches!(
                state.last_outcome,
                Some(CommandOutcome::Error(ref message)) if message.contains("no está disponible")
            ),
            "{tool:?} did not report an unavailable-feature error: {:?}",
            state.last_outcome
        );
        assert_eq!(
            serde_json::to_value(&document).expect("serialize document after tool"),
            before,
            "{tool:?} mutated the document"
        );
    }
}

#[test]
fn slider_tool_creates_validated_variable_metadata() {
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    let mut document = grafito_core::Document::new();
    let mut state = crate::tool_dispatcher::ToolState::default();

    let result = crate::tool_dispatcher::dispatch_tool(
        Tool::Slider,
        &mut state,
        &mut document,
        Point2::new(2.0, 3.0),
    );

    assert!(result.objects.is_empty());
    assert!(result.reset_tool);
    assert_eq!(document.get_variable("v0"), Some(0.0));
    assert!(matches!(
        document.variable_meta("v0"),
        Some(metadata)
            if metadata.position == Point2::new(2.0, 3.0)
                && metadata.min == -5.0
                && metadata.max == 5.0
                && metadata.step == 0.1
    ));
    grafito_core::validation::validate_document(&document)
        .expect("slider metadata is committed through document validation");
}

#[test]
fn slider_tool_rejects_nonfinite_metadata_without_creating_a_variable() {
    use grafito_command::commands::CommandOutcome;
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    let mut document = grafito_core::Document::new();
    let mut state = crate::tool_dispatcher::ToolState::default();
    let before = serde_json::to_value(&document).expect("document serializes");

    let result = crate::tool_dispatcher::dispatch_tool(
        Tool::Slider,
        &mut state,
        &mut document,
        Point2::new(f64::NAN, 3.0),
    );

    assert!(result.objects.is_empty());
    assert!(result.reset_tool);
    assert!(matches!(state.last_outcome, Some(CommandOutcome::Error(_))));
    assert!(document.variables().is_empty());
    assert!(document.variable_meta("v0").is_none());
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
}

#[test]
fn locus_tool_creates_a_persistent_trace_after_two_point_clicks() {
    use grafito_command::commands::CommandOutcome;
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    let mut document = grafito_core::Document::new();
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("B"),
    ));
    let mut state = crate::tool_dispatcher::ToolState::default();

    let first = crate::tool_dispatcher::dispatch_tool(
        Tool::Locus,
        &mut state,
        &mut document,
        Point2::new(0.0, 0.0),
    );
    assert!(!first.reset_tool);
    assert!(state.driver.is_some());

    let second = crate::tool_dispatcher::dispatch_tool(
        Tool::Locus,
        &mut state,
        &mut document,
        Point2::new(2.0, 0.0),
    );
    assert!(second.reset_tool);
    assert!(matches!(
        state.last_outcome,
        Some(CommandOutcome::Message(_))
    ));
    assert!(document.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::Pencil(pencil) if pencil.is_dynamic_locus())
    }));
}

#[test]
fn cancelling_locus_selection_clears_the_driver_before_the_next_click() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;
    use grafito_ui::Tool;

    let mut document = grafito_core::Document::new();
    let driver = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
    ));
    let target = document.add_object(GeoObject::Point(
        PointObj::new(Point2::new(2.0, 0.0)).with_label("B"),
    ));
    let mut state = crate::tool_dispatcher::ToolState {
        driver: Some(driver),
        ..Default::default()
    };

    crate::input::cancel_locus_selection(&mut state);
    let result = crate::tool_dispatcher::dispatch_tool(
        Tool::Locus,
        &mut state,
        &mut document,
        Point2::new(2.0, 0.0),
    );

    assert!(!result.reset_tool);
    assert_eq!(state.driver, Some(target));
    assert!(document.objects_iter().all(|(_, object)| {
        !matches!(object, GeoObject::Pencil(pencil) if pencil.is_dynamic_locus())
    }));
}

#[test]
fn at_capacity_single_tool_insertion_returns_error_without_clearing_history() {
    use grafito_core::{ChangeSet, GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = document_at_object_capacity();
    let before = serde_json::to_value(&document).expect("serialize full document");
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    let error = crate::app::commit_object_insertions(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        vec![GeoObject::Point(PointObj::new(Point2::new(-1.0, -1.0)))],
    )
    .expect_err("an at-capacity insertion must be a normal error");

    assert!(error.contains("maximum"));
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
    assert_eq!(
        document.object_count(),
        grafito_core::validation::MAX_OBJECT_COUNT
    );
    assert_eq!(
        serde_json::to_value(&document).expect("serialize rejected full document"),
        before
    );
}

#[test]
fn eraser_stroke_saves_exactly_one_snapshot_and_noop_keeps_redo() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let first = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
    let second = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: document.clone(),
        after: document.clone(),
    }];
    let mut stroke_has_mutated = false;

    assert!(crate::app::erase_object_for_stroke(
        &mut document,
        first,
        &mut stroke_has_mutated,
        &mut undo_stack,
        &mut redo_stack,
    ));
    assert!(crate::app::erase_object_for_stroke(
        &mut document,
        second,
        &mut stroke_has_mutated,
        &mut undo_stack,
        &mut redo_stack,
    ));
    assert_eq!(document.object_count(), 0);
    assert_eq!(undo_stack.len(), 1);
    assert_eq!(undo_stack[0].object_count(), 2);
    assert!(redo_stack.is_empty());

    let mut noop_document = grafito_core::Document::new();
    let mut noop_undo_stack = Vec::new();
    let mut noop_redo_stack = vec![grafito_core::ChangeSet {
        before: noop_document.clone(),
        after: noop_document.clone(),
    }];
    let mut noop_stroke_has_mutated = false;
    assert!(!crate::app::erase_object_for_stroke(
        &mut noop_document,
        grafito_core::ObjectId::new(),
        &mut noop_stroke_has_mutated,
        &mut noop_undo_stack,
        &mut noop_redo_stack,
    ));
    assert!(noop_undo_stack.is_empty());
    assert_eq!(noop_redo_stack.len(), 1);
}

#[test]
fn failed_conic_tool_commit_preserves_document_and_history() {
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let points: Vec<grafito_core::ObjectId> = (0..5)
        .map(|x| document.add_point(Point2::new(x as f64, 0.0)))
        .collect();
    let before = serde_json::to_value(&document).expect("document should serialize");
    let version_before = document.version;
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: document.clone(),
        after: document.clone(),
    }];

    let error = crate::app::commit_conic_by_five_points(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        &points,
    )
    .expect_err("five collinear points must reject the conic tool operation");

    assert!(error.contains("ConicByFivePoints"));
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document should serialize"),
        before
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn interactive_conic_tools_commit_outputs_and_history_only_after_propagation() {
    use grafito_core::{GeoObject, LineObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let first_focus = document.add_point(Point2::new(-1.0, 0.0));
    let second_focus = document.add_point(Point2::new(1.0, 0.0));
    let point_on_conic = document.add_point(Point2::new(3.0, 1.0));
    let directrix = document.add_object(GeoObject::Line(LineObj::new(
        Point2::new(-4.0, -3.0),
        Point2::new(4.0, -3.0),
    )));
    let object_count_before = document.object_count();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: document.clone(),
        after: document.clone(),
    }];

    crate::app::commit_ellipse_by_foci(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        first_focus,
        second_focus,
        point_on_conic,
    )
    .expect("valid ellipse construction should propagate");
    crate::app::commit_parabola_by_focus_directrix(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        first_focus,
        directrix,
    )
    .expect("valid parabola construction should propagate");
    crate::app::commit_hyperbola_by_foci(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
        first_focus,
        second_focus,
        point_on_conic,
    )
    .expect("valid hyperbola construction should propagate");

    assert_eq!(document.object_count(), object_count_before + 3);
    assert!(document
        .objects_iter()
        .any(|(_, object)| matches!(object, GeoObject::Ellipse(_))));
    assert!(document
        .objects_iter()
        .any(|(_, object)| matches!(object, GeoObject::Parabola(_))));
    assert!(document
        .objects_iter()
        .any(|(_, object)| matches!(object, GeoObject::Hyperbola(_))));
    assert_eq!(undo_stack.len(), 3);
    assert!(redo_stack.is_empty());
}

#[test]
fn failed_interactive_conic_tools_preserve_document_and_history() {
    use grafito_core::{GeoObject, LineObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let first_focus = document.add_point(Point2::new(-1.0, 0.0));
    let second_focus = document.add_point(Point2::new(1.0, 0.0));
    let point_on_conic = document.add_point(Point2::new(3.0, 1.0));
    let directrix = document.add_object(GeoObject::Line(LineObj::new(
        Point2::new(-4.0, -3.0),
        Point2::new(4.0, -3.0),
    )));
    document
        .try_add_distance_constraint(first_focus, second_focus, 1.0)
        .expect("first distance constraint should register");
    document
        .try_add_distance_constraint(first_focus, second_focus, 2.0)
        .expect("conflicting distance constraint should stage for regression");
    let before = serde_json::to_value(&document).expect("document should serialize");
    let version_before = document.version;
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: document.clone(),
        after: document.clone(),
    }];

    for error in [
        crate::app::commit_ellipse_by_foci(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            first_focus,
            second_focus,
            point_on_conic,
        ),
        crate::app::commit_parabola_by_focus_directrix(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            first_focus,
            directrix,
        ),
        crate::app::commit_hyperbola_by_foci(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            first_focus,
            second_focus,
            point_on_conic,
        ),
    ] {
        assert!(
            error.is_err(),
            "unsatisfied propagation must reject the tool commit"
        );
        assert_eq!(document.version, version_before);
        assert_eq!(
            serde_json::to_value(&document).expect("document should serialize"),
            before
        );
        assert!(undo_stack.is_empty());
        assert_eq!(redo_stack.len(), 1);
    }
}

#[test]
fn select_drag_captures_the_object_under_the_pointer_not_an_old_selection() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let under_pointer = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))));
    let previously_selected =
        document.add_object(GeoObject::Point(PointObj::new(Point2::new(5.0, 5.0))));
    document.select(previously_selected);

    let captured =
        crate::app::captured_select_drag_object(&mut document, Point2::new(1.0, 1.0), 0.1);
    assert_eq!(captured, Some(under_pointer));
    assert!(crate::app::is_free_point(
        &document,
        captured.expect("point capture")
    ));
    assert_eq!(
        crate::app::captured_select_drag_object(&mut document, Point2::new(9.0, 9.0), 0.1),
        None
    );
}

#[test]
fn autocomplete_inserts_canonical_command_for_visual_palette_names() {
    let mut input = "Tho".to_string();
    let item = crate::app::AutocompleteItem {
        text: "Thomas (Butterfly)".to_string(),
        detail: "Atractores".to_string(),
        bracket: true,
    };

    crate::ui::apply_autocomplete_item(&mut input, &item);
    assert_eq!(input, "Thomas[");
}

#[test]
fn autocomplete_accepts_tab_but_not_enter_as_completion_key() {
    assert!(crate::ui::is_autocomplete_completion_key(egui::Key::Tab));
    assert!(!crate::ui::is_autocomplete_completion_key(egui::Key::Enter));
    assert!(!crate::ui::is_autocomplete_completion_key(
        egui::Key::Escape
    ));
}

#[test]
fn autocomplete_selection_inserts_structure_and_closes_popup() {
    let mut input = "Comp".to_string();
    let mut autocomplete = crate::app::InputAutocomplete {
        open: true,
        selected: 0,
    };
    let suggestions = vec![crate::app::AutocompleteItem {
        text: "ComplexGrid".to_string(),
        detail: "Complejos".to_string(),
        bracket: true,
    }];

    assert!(crate::ui::complete_autocomplete_selection(
        &mut input,
        &suggestions,
        &mut autocomplete,
    ));
    assert_eq!(input, "ComplexGrid[");
    assert!(!autocomplete.open);
    assert_eq!(autocomplete.selected, 0);
}

#[test]
fn autocomplete_rejects_oversized_tokens_before_fuzzy_scoring() {
    let oversized = "a".repeat(4_096);

    assert_eq!(crate::ui::similarity_score(&oversized, "a"), 0.0);
    assert!(crate::ui::compute_autocomplete_suggestions(
        &oversized,
        &grafito_core::Document::new(),
    )
    .is_empty());
}

#[test]
fn autocomplete_keeps_ordinary_fuzzy_matches() {
    let suggestions =
        crate::ui::compute_autocomplete_suggestions("si", &grafito_core::Document::new());

    assert!(suggestions.iter().any(|item| item.text == "sin"));
}

#[test]
fn autocomplete_hides_only_unavailable_features_and_offers_dynamic_locus() {
    let document = grafito_core::Document::new();

    for unavailable in ["Button", "Image"] {
        let suggestions = crate::ui::compute_autocomplete_suggestions(unavailable, &document);
        assert!(
            suggestions.iter().all(|item| item.text != unavailable),
            "autocomplete exposed unavailable feature {unavailable}"
        );
    }
    let suggestions = crate::ui::compute_autocomplete_suggestions("SampledGraph", &document);
    assert!(suggestions.iter().any(|item| item.text == "SampledGraph"));
    let suggestions = crate::ui::compute_autocomplete_suggestions("Locus", &document);
    assert!(suggestions.iter().any(|item| item.text == "Locus"));
}

#[test]
fn autocomplete_keeps_only_eight_candidates_while_collecting() {
    let retained = (0..8)
        .map(|index| {
            (
                crate::app::AutocompleteItem {
                    text: format!("candidate_{index}"),
                    detail: "test".to_string(),
                    bracket: false,
                },
                1.0,
            )
        })
        .collect::<Vec<_>>();

    assert!(crate::ui::autocomplete_candidate_slot(&retained, "lower_priority", 0.5,).is_none());
    assert!(crate::ui::autocomplete_candidate_slot(&retained, "exact", 2.0).is_some());
}

#[test]
fn autocomplete_keeps_eight_results_for_many_document_matches() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    for index in 0..32 {
        document.add_object(GeoObject::Point(
            PointObj::new(Point2::new(index as f64, 0.0)).with_label(format!("needle_{index}")),
        ));
    }

    let suggestions = crate::ui::compute_autocomplete_suggestions("needle", &document);

    assert_eq!(suggestions.len(), 8);
    assert!(suggestions
        .iter()
        .all(|item| item.text.starts_with("needle_")));
}

#[test]
fn cleared_document_restarts_an_unchanged_constraint_tool() {
    use grafito_ui::Tool;

    assert!(crate::app::pending_action_needs_reinitialization(
        Tool::DistanceConstraint,
        Tool::DistanceConstraint,
        &crate::app::PendingAction::None,
    ));
    assert!(!crate::app::pending_action_needs_reinitialization(
        Tool::Select,
        Tool::Select,
        &crate::app::PendingAction::None,
    ));
}

#[test]
fn moving_a_free_point_updates_render_state_only_when_needed() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let point = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let unchanged = document.clone();

    assert!(!document
        .try_move_point_and_re_evaluate(point, Point2::new(1.0, 2.0))
        .expect("unchanged point move should be accepted as a no-op"));
    assert_eq!(
        serde_json::to_value(&document).expect("document should serialize"),
        serde_json::to_value(&unchanged).expect("document should serialize"),
    );

    let before_move = document.clone();
    assert!(document
        .try_move_point_and_re_evaluate(point, Point2::new(3.0, 4.0))
        .expect("free point move should succeed"));
    assert!(crate::app::refresh_unversioned_document_change(
        &before_move,
        &mut document,
    ));
    assert_eq!(document.version, before_move.version.wrapping_add(1));
    let Some(GeoObject::Point(point_object)) = document.get_object(point) else {
        panic!("moved object should remain a point");
    };
    assert_eq!(point_object.position, Point2::new(3.0, 4.0));
}

#[test]
fn numeric_constraint_tool_staging_rejects_unsatisfiable_additions_without_committing() {
    use grafito_core::{GeoObject, PointObj};
    use grafito_geometry::Point2;

    let mut document = grafito_core::Document::new();
    let first = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
    let second = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));

    crate::app::try_stage_numeric_constraint(&mut document, |staged| {
        staged
            .try_add_distance_constraint(first, second, 1.0)
            .map(|_| ())
    })
    .expect("the initial distance constraint is satisfiable");
    let before = serde_json::to_value(&document).expect("document should serialize");
    let version_before = document.version;
    let constraint_count = document.constraints.constraint_count();

    let error = crate::app::try_stage_numeric_constraint(&mut document, |staged| {
        staged
            .try_add_distance_constraint(first, second, 2.0)
            .map(|_| ())
    })
    .expect_err("a conflicting distance constraint must be rejected");

    assert!(error.contains("Numeric constraint"));
    assert_eq!(document.version, version_before);
    assert_eq!(document.constraints.constraint_count(), constraint_count);
    assert_eq!(
        serde_json::to_value(&document).expect("document should serialize"),
        before,
    );
}

#[test]
fn test_left_panel_default_sidebar_tab() {
    use crate::LeftPanelContent;
    assert_eq!(LeftPanelContent::Algebra.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::AlgebraAndCas.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Tools.default_sidebar_tab(), 1);
    assert_eq!(LeftPanelContent::Cas.default_sidebar_tab(), 2);
    // Stats ya no tiene tab propio (se quitó la pestaña «Datos»);
    // Complejos a "Álgebra" (0); Atractores a "Herram." (1).
    assert_eq!(LeftPanelContent::Stats.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Complex.default_sidebar_tab(), 0);
    assert_eq!(LeftPanelContent::Attractor.default_sidebar_tab(), 1);
}

#[test]
fn test_tool_group_id_def_nonempty() {
    for &gid in grafito_ui::toolbar::ALL_GROUPS {
        let (_icon, tools) = gid.def();
        assert!(!tools.is_empty(), "grupo {:?} vacío", gid);
    }
}

#[test]
fn trig_animation_supports_six_internal_function_types() {
    use crate::app::{GrafitoApp, TRIG_FUNCTIONS};

    assert_eq!(TRIG_FUNCTIONS.len(), 6);
    let names: Vec<&str> = TRIG_FUNCTIONS.iter().map(|spec| spec.name).collect();
    assert_eq!(names, vec!["sin", "cos", "tan", "cot", "sec", "csc"]);

    let t = std::f64::consts::FRAC_PI_4;
    assert!((GrafitoApp::trig_value(0, t) - t.sin()).abs() < 1e-12);
    assert!((GrafitoApp::trig_value(1, t) - t.cos()).abs() < 1e-12);
    assert!((GrafitoApp::trig_value(2, t) - 1.0).abs() < 1e-12);
    assert!((GrafitoApp::trig_value(3, t) - 1.0).abs() < 1e-12);
    assert!((GrafitoApp::trig_value(4, t) - 2.0_f64.sqrt()).abs() < 1e-12);
    assert!((GrafitoApp::trig_value(5, t) - 2.0_f64.sqrt()).abs() < 1e-12);
}

#[test]
fn trig_graph_sampling_is_safe_at_extreme_zoom() {
    use grafito_core::RenderQuality;

    assert_eq!(
        crate::render_2d::trig_sample_count(800, 80.0, RenderQuality::High),
        320
    );
    assert_eq!(
        crate::render_2d::trig_sample_count(800, 80.000_001, RenderQuality::High),
        200
    );
}

#[test]
fn trig_animation_explains_identities_for_teaching() {
    use crate::app::GrafitoApp;

    assert!(GrafitoApp::trig_identity(0).contains("altura"));
    assert!(GrafitoApp::trig_identity(2).contains("sin θ / cos θ"));
    assert!(GrafitoApp::trig_identity(4).contains("1 / cos θ"));
    assert!(GrafitoApp::trig_identity(5).contains("1 / sin θ"));
}

#[test]
fn invalid_command_submission_does_not_request_an_undo_snapshot() {
    let before = grafito_core::Document::new();
    let mut after = before.clone();
    let mut input = "FooBar[]".to_string();
    let outcome = crate::commands::process_input(&mut after, &mut input);
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: before.clone(),
        after: before.clone(),
    }];

    assert!(matches!(
        outcome,
        grafito_command::commands::CommandOutcome::Error(_)
    ));
    crate::app::save_command_snapshot_if_mutated(
        &outcome,
        before,
        &after,
        &mut undo_stack,
        &mut redo_stack,
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn informational_command_submission_does_not_request_an_undo_snapshot() {
    let before = grafito_core::Document::new();
    let mut after = before.clone();
    let mut input = "Simplify[x + 0]".to_string();
    let outcome = crate::commands::process_input(&mut after, &mut input);
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: before.clone(),
        after: before.clone(),
    }];

    assert!(matches!(
        outcome,
        grafito_command::commands::CommandOutcome::Message(_)
    ));
    crate::app::save_command_snapshot_if_mutated(
        &outcome,
        before,
        &after,
        &mut undo_stack,
        &mut redo_stack,
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn ordinary_message_submission_records_history_shows_info_and_stays_out_of_undo() {
    let before = grafito_core::Document::new();
    let mut after = before.clone();
    let mut input = "Simplify[x + 0]".to_string();
    let outcome = crate::commands::process_input(&mut after, &mut input);
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: before.clone(),
        after: before.clone(),
    }];
    let mut cas_result = String::new();
    let mut cas_history = Vec::new();
    let mut toasts = grafito_ui::toast::ToastManager::default();

    crate::app::save_command_snapshot_if_mutated(
        &outcome,
        before,
        &after,
        &mut undo_stack,
        &mut redo_stack,
    );
    crate::app::apply_command_outcome(
        &outcome,
        &mut cas_result,
        &mut cas_history,
        &mut toasts,
        0.0,
        "Simplify[x + 0]",
    );

    assert!(matches!(
        outcome,
        grafito_command::commands::CommandOutcome::Message(_)
    ));
    assert!(!cas_result.is_empty());
    assert_eq!(cas_history.len(), 1);
    assert!(cas_history[0].contains("Simplify[x + 0]"));
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);

    let empty_toast_shapes = toast_shape_count(&mut grafito_ui::toast::ToastManager::default());
    assert!(toast_shape_count(&mut toasts) > empty_toast_shapes);
}

#[test]
fn no_op_command_outcome_stays_quiet() {
    let mut cas_result = String::new();
    let mut cas_history = Vec::new();
    let mut toasts = grafito_ui::toast::ToastManager::default();

    crate::app::apply_command_outcome(
        &grafito_command::commands::CommandOutcome::Ok,
        &mut cas_result,
        &mut cas_history,
        &mut toasts,
        0.0,
        "",
    );

    assert!(cas_result.is_empty());
    assert!(cas_history.is_empty());
    assert_eq!(
        toast_shape_count(&mut toasts),
        toast_shape_count(&mut grafito_ui::toast::ToastManager::default())
    );
}

#[test]
fn error_command_outcome_keeps_persistent_feedback_and_history() {
    let mut cas_result = String::new();
    let mut cas_history = Vec::new();
    let mut toasts = grafito_ui::toast::ToastManager::default();

    crate::app::apply_command_outcome(
        &grafito_command::commands::CommandOutcome::Error("entrada inválida".to_string()),
        &mut cas_result,
        &mut cas_history,
        &mut toasts,
        0.0,
        "FooBar[]",
    );

    assert_eq!(cas_result, "entrada inválida");
    assert_eq!(cas_history.len(), 1);
    assert!(cas_history[0].contains("FooBar[]"));
    assert!(cas_history[0].contains("entrada inválida"));
    assert!(
        toast_shape_count(&mut toasts)
            > toast_shape_count(&mut grafito_ui::toast::ToastManager::default())
    );
}

#[test]
fn persisted_cas_error_cells_request_undo_even_when_the_command_failed() {
    let before = grafito_core::Document::new();
    let mut after = before.clone();
    after
        .try_append_cas_worksheet_cell(
            "FooBar[]".to_string(),
            "comando desconocido".to_string(),
            grafito_core::CasWorksheetStatus::Error,
        )
        .expect("fixture worksheet cell is valid");
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: before.clone(),
        after: before.clone(),
    }];

    crate::app::save_cas_worksheet_snapshot_if_mutated(
        before,
        &after,
        &mut undo_stack,
        &mut redo_stack,
    );

    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
}

#[test]
fn clearing_persisted_cas_worksheet_is_undoable_and_redoable() {
    let mut before = grafito_core::Document::new();
    before
        .try_append_cas_worksheet_cell(
            "Simplify[x + 0]".to_string(),
            "x".to_string(),
            grafito_core::CasWorksheetStatus::Success,
        )
        .expect("fixture worksheet cell is valid");
    let mut after = before.clone();
    assert!(after.clear_cas_worksheet());
    let changes = grafito_core::ChangeSet {
        before: before.clone(),
        after: after.clone(),
    };
    let mut current = after;

    changes.undo(&mut current).expect("clear can be undone");
    assert_eq!(current.cas_worksheet(), before.cas_worksheet());
    changes.redo(&mut current).expect("clear can be redone");
    assert!(current.cas_worksheet().is_empty());
}

#[test]
fn failed_command_submission_retains_input_for_correction() {
    let mut failed_input = "FooBar[]".to_string();
    let failed = grafito_command::commands::CommandOutcome::Error("entrada inválida".to_string());

    assert!(!crate::app::clear_submitted_input_on_success(
        &mut failed_input,
        &failed,
    ));
    assert_eq!(failed_input, "FooBar[]");

    let mut successful_input = "A = (1, 2)".to_string();
    assert!(crate::app::clear_submitted_input_on_success(
        &mut successful_input,
        &grafito_command::commands::CommandOutcome::Ok,
    ));
    assert!(successful_input.is_empty());

    let source = include_str!("app.rs");
    let start = source
        .find("pub(crate) fn submit_input_text")
        .expect("shared command submitter");
    let end = source[start..]
        .find("pub(crate) fn label_of")
        .map(|offset| start + offset)
        .expect("next app method");
    let submitter = &source[start..end];
    assert!(submitter.contains("clear_submitted_input_on_success"));
    assert!(!submitter.contains("self.input_text.clear()"));
}

#[test]
fn cas_sidebar_routes_shared_input_to_the_persisted_worksheet() {
    assert!(crate::app::sidebar_uses_cas_worksheet(2));
    assert!(!crate::app::sidebar_uses_cas_worksheet(0));
    assert!(!crate::app::sidebar_uses_cas_worksheet(3));

    let source = include_str!("app.rs");
    let start = source
        .find("pub(crate) fn submit_input_text")
        .expect("shared command submitter");
    let end = source[start..]
        .find("/// Etiqueta de un objeto")
        .map(|offset| start + offset)
        .expect("next app method");
    let submitter = &source[start..end];
    assert!(submitter.contains("sidebar_uses_cas_worksheet(self.sidebar_tab)"));
    assert!(submitter.contains("self.submit_cas_worksheet_cell(time)"));
}

#[test]
fn statistics_input_is_strict_and_finite() {
    assert_eq!(
        crate::panels::parse_statistics_input("1, 2\n3"),
        Ok(vec![1.0, 2.0, 3.0])
    );

    for invalid in ["1, nope, 3", "1,,3", "NaN", "inf", "-inf"] {
        assert!(
            crate::panels::parse_statistics_input(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }
}

#[test]
fn statistics_summary_stays_finite_for_equal_extreme_values() {
    let summary = crate::panels::statistics_summary(&[f64::MAX, f64::MAX])
        .expect("equal finite values have representable central statistics");

    assert_eq!(summary.mean, f64::MAX);
    assert_eq!(summary.median, f64::MAX);
    assert_eq!(summary.variance, 0.0);
    assert_eq!(summary.standard_deviation, 0.0);
    assert!(
        summary.sum.is_none(),
        "the overflowing sum must be explicit"
    );
}

#[test]
fn statistics_summary_preserves_equal_minimum_subnormal_quantiles() {
    let minimum_subnormal = f64::from_bits(1);
    let summary = crate::panels::statistics_summary(&[minimum_subnormal, minimum_subnormal])
        .expect("equal subnormal values have representable central statistics");

    assert_eq!(summary.mean, minimum_subnormal);
    assert_eq!(summary.median, minimum_subnormal);
    assert_eq!(summary.q1, minimum_subnormal);
    assert_eq!(summary.q3, minimum_subnormal);
    assert_eq!(summary.variance, 0.0);
}

#[test]
fn statistics_summary_interpolates_opposite_sign_values() {
    let summary = crate::panels::statistics_summary(&[-2.0, 2.0])
        .expect("opposite-sign values have representable statistics");

    assert_eq!(summary.median, 0.0);
    assert_eq!(summary.q1, -1.0);
    assert_eq!(summary.q3, 1.0);
    assert_eq!(summary.iqr, 2.0);
}

#[test]
fn statistics_summary_preserves_ordinary_population_results() {
    let summary = crate::panels::statistics_summary(&[1.0, 2.0, 3.0, 4.0])
        .expect("ordinary finite data must remain supported");

    assert!((summary.mean - 2.5).abs() < 1.0e-12);
    assert!((summary.median - 2.5).abs() < 1.0e-12);
    assert!((summary.variance - 1.25).abs() < 1.0e-12);
    assert!((summary.standard_deviation - 1.25_f64.sqrt()).abs() < 1.0e-12);
}

#[test]
fn statistics_summary_rejects_unrepresentable_true_results() {
    let error = crate::panels::statistics_summary(&[-1.0e308, 1.0e308])
        .expect_err("unrepresentable dispersion must not become infinity");

    assert!(error.contains("no es representable"));
}

#[test]
fn statistics_panel_has_vertical_scroll_and_persistent_validation_feedback() {
    let source = include_str!("panels.rs");
    let start = source
        .find("pub(crate) fn draw_statistics_panel")
        .expect("statistics panel");
    let end = source[start..]
        .find("pub(crate) fn draw_complex_panel")
        .map(|offset| start + offset)
        .expect("next panel");
    let panel = &source[start..end];

    assert!(panel.contains("ScrollArea::vertical()"));
    assert!(panel.contains("statistics_input_error"));
    assert!(!panel.contains("filter_map"));
}

#[test]
fn idle_object_panel_edit_does_not_change_document_version() {
    use grafito_core::{Attractor3DObj, Cube3DObj, GeoObject};
    use grafito_geometry::Point3D;

    let mut document = grafito_core::Document::new();
    let cube = document.add_object(GeoObject::Cube3D(Cube3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));
    let attractor = document.add_object(GeoObject::Attractor3D(Attractor3DObj::new(
        "lorenz",
        vec![10.0, 28.0, 8.0 / 3.0],
    )));
    let version_before = document.version;

    assert!(
        !crate::panels::apply_object_panel_edit(&mut document, cube, false, |_| panic!(
            "idle edit must not acquire mutable object access"
        ),)
        .expect("idle panel edit is a no-op")
    );
    assert!(
        !crate::panels::apply_object_panel_edit(&mut document, attractor, false, |_| panic!(
            "idle edit must not acquire mutable object access"
        ),)
        .expect("idle panel edit is a no-op")
    );
    assert_eq!(document.version, version_before);
    assert!(
        !crate::panels::apply_object_panel_edit(&mut document, cube, true, |_| {},)
            .expect("an unchanged panel candidate is a no-op")
    );
    assert_eq!(document.version, version_before);

    let source = include_str!("panels.rs");
    for (start_marker, end_marker) in [
        (
            "pub(crate) fn draw_right_properties_panel",
            "pub(crate) fn draw_right_domain_coloring_panel",
        ),
        (
            "pub(crate) fn draw_right_parameters_panel",
            "pub(crate) fn draw_right_regression_panel",
        ),
    ] {
        let start = source.find(start_marker).expect("property panel");
        let end = source[start..]
            .find(end_marker)
            .map(|offset| start + offset)
            .expect("next panel");
        let panel = &source[start..end];
        assert!(panel.contains("apply_object_panel_edit"));
        assert!(panel.contains("capture_successful_replacement"));
        // get_object_mut puede usarse para ediciones con snapshot (no mutación
        // directa del documento); las regresiones relevantes son las otras.
        assert!(!panel.contains("document.bump_version"));
        assert!(!panel.contains("let before = app.document.clone()"));
        assert!(!panel.contains("snapshot.capture(&app.document)"));
        assert!(panel.contains("DeferredPanelSnapshot::new"));
    }
}

#[test]
fn deferred_panel_snapshot_is_idle_without_a_document_snapshot_and_records_one_edit() {
    use grafito_core::{ChangeSet, Cube3DObj, GeoObject};
    use grafito_geometry::Point3D;

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Cube3D(Cube3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    let mut snapshot = crate::app::DeferredPanelSnapshot::new(undo_stack.len());
    assert!(!snapshot.is_captured());
    assert!(!snapshot.save_if_semantically_changed(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
    ));
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);

    let mut snapshot = crate::app::DeferredPanelSnapshot::new(undo_stack.len());
    let before =
        crate::panels::apply_object_panel_edit_with_previous(&mut document, id, true, |object| {
            let GeoObject::Cube3D(cube) = object else {
                panic!("fixture remains a cube");
            };
            cube.size = 3.0;
        })
        .expect("the panel edit is valid")
        .expect("a valid panel edit returns its prior document");
    snapshot.capture_successful_replacement(before);
    assert!(snapshot.is_captured());
    assert!(!snapshot.requires_semantic_comparison());
    assert!(snapshot.save_if_semantically_changed(&mut document, &mut undo_stack, &mut redo_stack,));
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
    assert!(matches!(
        undo_stack[0].get_object(id),
        Some(GeoObject::Cube3D(cube)) if cube.size == 2.0
    ));
}

#[test]
fn deferred_panel_snapshot_ignores_no_op_and_rejected_replacements_without_comparison() {
    use grafito_core::{ChangeSet, GeoObject, RegularPolychoron4DObj};
    use grafito_geometry::RegularPolychoron;

    let mut document = grafito_core::Document::new();
    let id = document
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("fixture inserts");
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(undo_stack.len());

    assert!(
        crate::panels::apply_object_panel_edit_with_previous(&mut document, id, true, |_| {},)
            .expect("an unchanged candidate is a no-op")
            .is_none()
    );
    assert!(!snapshot.is_captured());
    assert!(!snapshot.requires_semantic_comparison());

    let error =
        crate::panels::apply_object_panel_edit_with_previous(&mut document, id, true, |object| {
            let GeoObject::RegularPolychoron4D(polychoron) = object else {
                panic!("fixture remains a regular polychoron");
            };
            polychoron.scale = 0.0;
        })
        .expect_err("invalid detached candidates are rejected before history capture");

    assert!(error.contains("scale"), "{error}");
    assert!(!snapshot.is_captured());
    assert!(!snapshot.requires_semantic_comparison());
    assert!(!snapshot.save_if_semantically_changed(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
    ));
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn color_picker_keeps_staged_changes_transient_until_an_explicit_apply() {
    let source = include_str!("ui.rs");
    let start = source
        .find("pub(crate) fn draw_color_picker")
        .expect("color picker renderer");
    let end = source[start..]
        .find("const MAX_AUTOCOMPLETE_TOKEN_CHARS")
        .map(|offset| start + offset)
        .expect("next ui section");
    let picker = &source[start..end];

    assert!(picker.contains("let Some(ActiveColorPicker {"));
    assert!(picker.contains("object_id,"));
    assert!(picker.contains("target,"));
    assert!(picker.contains("mut picker,"));
    assert!(picker.contains("return;"));
    assert!(!picker.contains("app.document.clone()"));
    assert!(!picker.contains("save_snapshot_if_semantically_changed"));
    assert!(!picker.contains("get_object_mut"));
    assert!(picker.contains("ColorPickerDialogAction::Apply"));
    assert!(picker.contains("ColorPickerDialogAction::Cancel"));
    assert!(picker.contains("ColorPickerDialogAction::Dismiss"));
    assert!(picker.contains("apply_color_picker_dialog_action"));
    assert!(picker.contains("outcome.any_changed()"));
    assert!(!picker.contains("outcome.object_color_changed"));
    assert!(!picker.contains("outcome.color_changed"));
}

#[test]
fn color_picker_targets_are_explicit_and_distinct() {
    use crate::app::{ActiveColorPicker, ColorPickerTarget};
    use grafito_core::ObjectId;
    use grafito_geometry::Color;

    let object = ActiveColorPicker {
        object_id: ObjectId::new(),
        target: ColorPickerTarget::ObjectColor,
        picker: grafito_ui::color_picker::HsvColorPicker::new(Color::RED),
    };
    let fill = ActiveColorPicker {
        object_id: ObjectId::new(),
        target: ColorPickerTarget::RegularPolychoronFill,
        picker: grafito_ui::color_picker::HsvColorPicker::new(Color::BLUE),
    };

    assert_eq!(object.target, ColorPickerTarget::ObjectColor);
    assert_eq!(fill.target, ColorPickerTarget::RegularPolychoronFill);
    assert_ne!(object.target, fill.target);
}

#[test]
fn color_picker_dialog_is_centered_foreground_and_constrained_to_a_safe_viewport() {
    let source = include_str!("ui.rs");
    let start = source
        .find("pub(crate) fn draw_color_picker")
        .expect("color picker renderer");
    let end = source[start..]
        .find("const MAX_AUTOCOMPLETE_TOKEN_CHARS")
        .map(|offset| start + offset)
        .expect("next ui section");
    let picker = &source[start..end];

    assert!(picker.contains(".order(egui::Order::Foreground)"));
    assert!(picker.contains(".anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)"));
    assert!(picker.contains(".fixed_size(COLOR_PICKER_DIALOG_SIZE)"));
    assert!(picker.contains(".resizable(false)"));
    assert!(picker.contains(".constrain_to(color_picker_safe_viewport(ctx.screen_rect()))"));
    assert!(picker.contains(".fill(theme.panel_bg)"));
    assert!(picker.contains("theme.separator"));

    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let safe_viewport = crate::ui::color_picker_safe_viewport(viewport);
    assert!(safe_viewport.min.x > viewport.min.x);
    assert!(safe_viewport.min.y > viewport.min.y);
    assert!(safe_viewport.max.x < viewport.max.x);
    assert!(safe_viewport.max.y < viewport.max.y);

    let narrow_viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(8.0, 6.0));
    let narrow_safe_viewport = crate::ui::color_picker_safe_viewport(narrow_viewport);
    assert!(narrow_safe_viewport.width() > 0.0);
    assert!(narrow_safe_viewport.height() > 0.0);
    assert!(narrow_viewport.contains_rect(narrow_safe_viewport));
}

#[test]
fn color_picker_real_color_change_replaces_once_and_records_one_undo() {
    use grafito_core::{ChangeSet, GeoObject, PointObj};
    use grafito_geometry::{Color, Point2};

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let previous_color = document.get_object(id).expect("point exists").color();
    let version_before = document.version;
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(crate::ui::apply_color_picker_object_color_change(
        &mut document,
        id,
        Color::GREEN,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("valid color replacement commits"));
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert_eq!(
        document.get_object(id).expect("point remains").color(),
        Color::GREEN
    );
    assert_eq!(undo_stack.len(), 1);
    assert_eq!(
        undo_stack[0]
            .get_object(id)
            .expect("undo point exists")
            .color(),
        previous_color
    );
    assert!(redo_stack.is_empty());
}

#[test]
fn color_picker_dialog_commits_only_on_apply_and_discards_cancel_or_dismiss() {
    use crate::app::ColorPickerTarget;
    use crate::ui::ColorPickerDialogAction;
    use grafito_core::{ChangeSet, GeoObject, PointObj};
    use grafito_geometry::{Color, Point2};

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let original = document.get_object(id).expect("point exists").color();
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    for action in [
        ColorPickerDialogAction::Cancel,
        ColorPickerDialogAction::Dismiss,
    ] {
        assert!(!crate::ui::apply_color_picker_dialog_action(
            action,
            &mut document,
            ColorPickerTarget::ObjectColor,
            id,
            Color::GREEN,
            &mut undo_stack,
            &mut redo_stack,
        )
        .expect("non-apply dialog actions discard staged colors"));
    }
    assert_eq!(document.version, version_before);
    assert_eq!(
        document.get_object(id).expect("point remains").color(),
        original
    );
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);

    assert!(crate::ui::apply_color_picker_dialog_action(
        ColorPickerDialogAction::Apply,
        &mut document,
        ColorPickerTarget::ObjectColor,
        id,
        Color::GREEN,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("apply commits the staged color once"));
    assert_eq!(
        document.get_object(id).expect("point remains").color(),
        Color::GREEN
    );
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());

    assert!(!crate::ui::apply_color_picker_dialog_action(
        ColorPickerDialogAction::Apply,
        &mut document,
        ColorPickerTarget::ObjectColor,
        id,
        Color::GREEN,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("an equal applied color remains a no-op"));
    assert_eq!(undo_stack.len(), 1);
}

#[test]
fn color_picker_dialog_apply_of_untouched_hsv_roundtrip_preserves_document_and_history() {
    use crate::app::ColorPickerTarget;
    use crate::ui::ColorPickerDialogAction;
    use grafito_core::{ChangeSet, GeoObject, PointObj};
    use grafito_geometry::{Color, Point2};

    let original = Color::new(0.123_456_7, 0.456_789_1, 0.987_654_3, 0.35);
    let staged = grafito_ui::color_picker::HsvColorPicker::new(original).to_color();
    assert_ne!(staged, original, "fixture must exercise a ULP round trip");

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    document
        .get_object_mut(id)
        .expect("point exists")
        .set_color(original);
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(!crate::ui::apply_color_picker_dialog_action(
        ColorPickerDialogAction::Apply,
        &mut document,
        ColorPickerTarget::ObjectColor,
        id,
        staged,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("untouched picker Apply is a no-op"));
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn color_picker_dialog_apply_of_untouched_hsv_roundtrip_fill_preserves_document_and_history() {
    use crate::app::ColorPickerTarget;
    use crate::ui::ColorPickerDialogAction;
    use grafito_core::{ChangeSet, GeoObject, RegularPolychoron4DObj};
    use grafito_geometry::{Color, RegularPolychoron};

    let original = Color::new(0.123_456_7, 0.456_789_1, 0.987_654_3, 0.35);
    let staged = grafito_ui::color_picker::HsvColorPicker::new(original).to_color();
    assert_ne!(staged, original, "fixture must exercise a ULP round trip");

    let mut document = grafito_core::Document::new();
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.fill_color = Some(original);
    let id = document
        .try_add_object(GeoObject::RegularPolychoron4D(polychoron))
        .expect("fixture inserts");
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(!crate::ui::apply_color_picker_dialog_action(
        ColorPickerDialogAction::Apply,
        &mut document,
        ColorPickerTarget::RegularPolychoronFill,
        id,
        staged,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("untouched fill picker Apply is a no-op"));
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn color_picker_dialog_apply_routes_a_polychoron_fill_target_once() {
    use crate::app::ColorPickerTarget;
    use crate::ui::ColorPickerDialogAction;
    use grafito_core::{ChangeSet, GeoObject, RegularPolychoron4DObj};
    use grafito_geometry::{Color, RegularPolychoron};

    let mut document = grafito_core::Document::new();
    let id = document
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("fixture inserts");
    let edge_color = document.get_object(id).expect("polychoron exists").color();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(crate::ui::apply_color_picker_dialog_action(
        ColorPickerDialogAction::Apply,
        &mut document,
        ColorPickerTarget::RegularPolychoronFill,
        id,
        Color::GREEN,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("apply commits the staged fill once"));
    assert!(matches!(
        document.get_object(id),
        Some(GeoObject::RegularPolychoron4D(polychoron))
            if polychoron.color == edge_color && polychoron.fill_color == Some(Color::GREEN)
    ));
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
}

#[test]
fn color_picker_equal_or_missing_color_change_preserves_document_and_history() {
    use grafito_core::{ChangeSet, GeoObject, ObjectId, PointObj};
    use grafito_geometry::{Color, Point2};

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let current_color = document.get_object(id).expect("point exists").color();
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(!crate::ui::apply_color_picker_object_color_change(
        &mut document,
        id,
        current_color,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("equal color is a no-op"));
    assert!(!crate::ui::apply_color_picker_object_color_change(
        &mut document,
        ObjectId::new(),
        Color::GREEN,
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect("missing object is a no-op"));
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn color_picker_equal_color_short_circuits_before_candidate_or_core_replacement() {
    let source = include_str!("ui.rs");
    let start = source
        .find("pub(crate) fn apply_color_picker_object_color_change")
        .expect("color picker object edit helper");
    let end = source[start..]
        .find("pub(crate) fn draw_color_picker")
        .map(|offset| start + offset)
        .expect("color picker renderer");
    let helper = &source[start..end];

    let equal_color_return = helper
        .find("if grafito_ui::color_picker::colors_match(object.color(), color)")
        .expect("equal colors must return before candidate work");
    let candidate_clone = helper.find("object.clone()").expect("candidate clone");
    let replacement = helper
        .find("try_replace_object_with_previous")
        .expect("core replacement");
    let snapshot = helper
        .find("DeferredPanelSnapshot::new")
        .expect("undo snapshot");

    assert!(equal_color_return < candidate_clone);
    assert!(equal_color_return < replacement);
    assert!(equal_color_return < snapshot);
    assert!(helper[equal_color_return..candidate_clone].contains("return Ok(false);"));
}

#[test]
fn color_picker_rejected_color_preserves_document_and_history() {
    use grafito_core::{ChangeSet, GeoObject, PointObj};
    use grafito_geometry::{Color, Point2};

    let mut document = grafito_core::Document::new();
    let id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    let error = crate::ui::apply_color_picker_object_color_change(
        &mut document,
        id,
        Color::new(f32::NAN, 0.2, 0.3, 1.0),
        &mut undo_stack,
        &mut redo_stack,
    )
    .expect_err("invalid detached candidate is rejected");

    assert!(error.contains("color.r"), "{error}");
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn color_picker_polychoron_fill_change_replaces_once_and_records_one_undo() {
    use grafito_core::{ChangeSet, GeoObject, RegularPolychoron4DObj};
    use grafito_geometry::{Color, RegularPolychoron};

    let mut document = grafito_core::Document::new();
    let id = document
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("fixture inserts");
    let edge_color = document.get_object(id).expect("polychoron exists").color();
    let previous_fill = match document.get_object(id) {
        Some(GeoObject::RegularPolychoron4D(polychoron)) => {
            polychoron.fill_color.expect("fixture has a fill color")
        }
        _ => panic!("fixture remains a regular polychoron"),
    };
    let version_before = document.version;
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    assert!(
        crate::ui::apply_color_picker_regular_polychoron_fill_color_change(
            &mut document,
            id,
            Color::GREEN,
            &mut undo_stack,
            &mut redo_stack,
        )
        .expect("valid polychoron fill replacement commits")
    );
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert!(matches!(
        document.get_object(id),
        Some(GeoObject::RegularPolychoron4D(polychoron))
            if polychoron.color == edge_color && polychoron.fill_color == Some(Color::GREEN)
    ));
    assert_eq!(undo_stack.len(), 1);
    assert!(matches!(
        undo_stack[0].get_object(id),
        Some(GeoObject::RegularPolychoron4D(polychoron))
            if polychoron.fill_color == Some(previous_fill)
    ));
    assert!(redo_stack.is_empty());
}

#[test]
fn color_picker_polychoron_fill_no_ops_preserve_document_and_history() {
    use grafito_core::{ChangeSet, GeoObject, ObjectId, PointObj, RegularPolychoron4DObj};
    use grafito_geometry::{Color, Point2, RegularPolychoron};

    let mut document = grafito_core::Document::new();
    let polychoron_id = document
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("fixture inserts");
    let no_fill_id = document
        .try_add_object(GeoObject::RegularPolychoron4D({
            let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Pentachoron);
            polychoron.fill_color = None;
            polychoron
        }))
        .expect("no-fill fixture inserts");
    let point_id = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))));
    let existing_fill = match document.get_object(polychoron_id) {
        Some(GeoObject::RegularPolychoron4D(polychoron)) => {
            polychoron.fill_color.expect("fixture has a fill color")
        }
        _ => panic!("fixture remains a regular polychoron"),
    };
    let version_before = document.version;
    let before = document.clone();
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    for id in [polychoron_id, no_fill_id, point_id, ObjectId::new()] {
        let color = if id == polychoron_id {
            existing_fill
        } else {
            Color::GREEN
        };
        assert!(
            !crate::ui::apply_color_picker_regular_polychoron_fill_color_change(
                &mut document,
                id,
                color,
                &mut undo_stack,
                &mut redo_stack,
            )
            .expect("equal, missing, and wrong-type fill targets are no-ops")
        );
    }

    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        serde_json::to_value(&before).expect("baseline serializes")
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn missing_algebra_variable_metadata_is_display_only_until_changed() {
    let mut document = grafito_core::Document::new();
    document
        .try_set_variable("t".to_string(), 100.0)
        .expect("finite variable inserts");
    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;

    let metadata = crate::algebra::variable_meta_for_display(&document, "t");

    assert_eq!(metadata.min, -5.0);
    assert_eq!(metadata.max, 5.0);
    assert_eq!(metadata.step, 0.1);
    assert!(!metadata.animating);
    assert_eq!(metadata.animation_speed, 1.0);
    assert!(document.variable_meta("t").is_none());
    assert_eq!(document.version, version_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);

    let mut displayed_value = document
        .get_variable("t")
        .expect("variable remains available");
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(
                egui::Slider::new(&mut displayed_value, metadata.min..=metadata.max)
                    .clamping(egui::SliderClamping::Edits),
            );
        });
    });
    assert_eq!(displayed_value, 100.0);

    let source = include_str!("algebra.rs");
    assert!(source.contains("variable_meta_for_display(&app.document, name)"));
    assert!(source.contains(".clamping(egui::SliderClamping::Edits)"));
    assert!(source.contains("try_replace_variable_meta_with_previous"));
    assert!(!source.contains("variable_meta.insert"));
    assert!(!source.contains("app.document.variable_meta"));
}

#[test]
fn algebra_variable_metadata_edits_capture_one_valid_undo_and_reject_invalid_ranges() {
    use grafito_core::ChangeSet;

    let mut document = grafito_core::Document::new();
    document
        .try_set_variable("t".to_string(), 0.0)
        .expect("fixture variable inserts");
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(undo_stack.len());
    let mut candidate = crate::algebra::variable_meta_for_display(&document, "t");
    candidate.min = -2.0;
    candidate.max = 8.0;

    assert!(crate::algebra::apply_variable_meta_panel_edit(
        &mut document,
        "t",
        candidate.clone(),
        &mut snapshot,
    )
    .expect("valid metadata edit commits"));
    assert!(snapshot.is_captured());
    assert!(!snapshot.requires_semantic_comparison());
    assert!(snapshot.save_if_semantically_changed(&mut document, &mut undo_stack, &mut redo_stack,));
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
    assert!(undo_stack[0].variable_meta("t").is_none());
    assert_eq!(document.variable_meta("t"), Some(&candidate));

    let before = serde_json::to_value(&document).expect("document serializes");
    let version_before = document.version;
    let mut rejected_snapshot = crate::app::DeferredPanelSnapshot::new(undo_stack.len());
    let mut rejected = candidate;
    rejected.min = rejected.max;

    let error = crate::algebra::apply_variable_meta_panel_edit(
        &mut document,
        "t",
        rejected,
        &mut rejected_snapshot,
    )
    .expect_err("reversed metadata is rejected before history capture");

    assert!(error.contains("smaller"), "{error}");
    assert_eq!(document.version, version_before);
    assert_eq!(serde_json::to_value(&document).unwrap(), before);
    assert!(!rejected_snapshot.is_captured());
    assert!(!rejected_snapshot.requires_semantic_comparison());
    assert!(!rejected_snapshot.save_if_semantically_changed(
        &mut document,
        &mut undo_stack,
        &mut redo_stack,
    ));
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
}

#[test]
fn idle_selected_algebra_properties_do_not_acquire_mutable_document_access() {
    let source = include_str!("algebra.rs");
    let start = source
        .find("// Properties Panel (Inline)")
        .expect("selected property panel");
    let end = source[start..]
        .find("if row_clicked")
        .map(|offset| start + offset)
        .expect("selected property panel end");
    let properties = &source[start..end];

    assert!(properties.contains("get_object(oid).cloned()"));
    assert!(properties.contains("apply_object_panel_edit"));
    assert!(properties.contains("capture_successful_replacement"));
    assert!(!properties.contains("get_object_mut"));
    assert!(!properties.contains("snapshot.capture(&app.document)"));
    assert!(!source.contains("let before = app.document.clone()"));
    assert!(source.contains("DeferredPanelSnapshot::new"));
}

#[test]
fn changed_object_panel_edit_bumps_version_once() {
    use grafito_core::{Cube3DObj, GeoObject};
    use grafito_geometry::Point3D;

    let mut document = grafito_core::Document::new();
    let cube = document.add_object(GeoObject::Cube3D(Cube3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));
    let version_before = document.version;

    assert!(
        crate::panels::apply_object_panel_edit(&mut document, cube, true, |object| {
            let GeoObject::Cube3D(cube) = object else {
                panic!("expected cube");
            };
            cube.size = 3.0;
        },)
        .expect("a valid panel edit commits")
    );
    assert_eq!(document.version, version_before.wrapping_add(1));
}

#[test]
fn object_panel_edit_uses_staged_replacement_and_rejection_preserves_history() {
    use grafito_core::{ChangeSet, GeoObject, RegularPolychoron4DObj};
    use grafito_geometry::RegularPolychoron;

    let mut document = grafito_core::Document::new();
    let id = document
        .try_add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            RegularPolychoron::Tesseract,
        )))
        .expect("fixture inserts");
    let before = document.clone();
    let version_before = document.version;
    let undo_stack: Vec<grafito_core::Document> = Vec::new();
    let redo_stack = [ChangeSet {
        before: grafito_core::Document::new(),
        after: grafito_core::Document::new(),
    }];

    let error = crate::panels::apply_object_panel_edit(&mut document, id, true, |object| {
        let GeoObject::RegularPolychoron4D(polychoron) = object else {
            panic!("fixture remains a regular polychoron");
        };
        polychoron.scale = 0.0;
    })
    .expect_err("invalid detached candidates must be rejected");

    assert!(error.contains("scale"));
    assert_eq!(document.version, version_before);
    assert_eq!(
        serde_json::to_value(&document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    assert!(undo_stack.is_empty());
    assert_eq!(redo_stack.len(), 1);
}

#[test]
fn object_panel_edit_commits_compound_4d_and_nd_edits_with_one_snapshot() {
    use grafito_core::{GeoObject, RegularPolychoron4DObj, RegularPolytopeNDObj};
    use grafito_geometry::{Color, RegularPolychoron, RegularPolytopeFamily};

    let mut document = grafito_core::Document::new();
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.label = "P4".to_string();
    polychoron.visible = false;
    let polychoron_id = document
        .try_add_object(GeoObject::RegularPolychoron4D(polychoron))
        .expect("4D fixture inserts");
    let version_before = document.version;

    assert!(
        crate::panels::apply_object_panel_edit(&mut document, polychoron_id, true, |object| {
            let GeoObject::RegularPolychoron4D(polychoron) = object else {
                panic!("fixture remains a regular polychoron");
            };
            polychoron.kind = RegularPolychoron::TwentyFourCell;
            polychoron.scale = 2.5;
            polychoron.width = 3.0;
            polychoron.color = Color::new(0.1, 0.2, 0.3, 0.9);
            polychoron.fill_color = Some(Color::new(0.7, 0.5, 0.2, 0.4));
            polychoron.rotation_angles = [0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
        },)
        .expect("compound 4D edit commits")
    );
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert!(matches!(
        document.get_object(polychoron_id),
        Some(GeoObject::RegularPolychoron4D(polychoron))
            if polychoron.label == "P4"
                && !polychoron.visible
                && polychoron.kind == RegularPolychoron::TwentyFourCell
                && polychoron.scale == 2.5
                && polychoron.width == 3.0
                && polychoron.rotation_angles == [0.1, -0.2, 0.3, -0.4, 0.5, -0.6]
                && polychoron.fill_color == Some(Color::new(0.7, 0.5, 0.2, 0.4))
    ));

    let mut polytope = RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5);
    polytope.label = "PN".to_string();
    polytope.visible = false;
    polytope.fill_color = Some(Color::new(0.3, 0.4, 0.5, 0.6));
    polytope.rotation_angles.fill(0.75);
    let polytope_id = document
        .try_add_object(GeoObject::RegularPolytopeND(polytope))
        .expect("N-D fixture inserts");
    let version_before = document.version;

    assert!(
        crate::panels::apply_object_panel_edit(&mut document, polytope_id, true, |object| {
            let GeoObject::RegularPolytopeND(polytope) = object else {
                panic!("fixture remains a regular N-D polytope");
            };
            polytope.family = RegularPolytopeFamily::CrossPolytope;
            polytope.dimension = 10;
            polytope.scale = 1.75;
            polytope.width = 2.5;
            polytope.color = Color::new(0.6, 0.2, 0.4, 0.8);
            let rotation_count =
                RegularPolytopeNDObj::expected_rotation_angle_count(polytope.dimension)
                    .expect("the selected dimension is supported");
            polytope.rotation_angles = vec![0.0; rotation_count];
        },)
        .expect("compound N-D edit commits")
    );
    assert_eq!(document.version, version_before.wrapping_add(1));
    assert!(matches!(
        document.get_object(polytope_id),
        Some(GeoObject::RegularPolytopeND(polytope))
            if polytope.label == "PN"
                && !polytope.visible
                && polytope.family == RegularPolytopeFamily::CrossPolytope
                && polytope.dimension == 10
                && polytope.scale == 1.75
                && polytope.width == 2.5
                && polytope.rotation_angles.len() == 45
                && polytope.rotation_angles.iter().all(|angle| *angle == 0.0)
                && polytope.fill_color == Some(Color::new(0.3, 0.4, 0.5, 0.6))
    ));
}

#[test]
fn geometry_3d_polytope_inspectors_expose_labeled_scrollable_controls() {
    let source = include_str!("panels.rs");
    let inspector_start = source
        .find("pub(crate) fn draw_right_properties_panel")
        .expect("Geometry3D properties panel");
    let inspector_end = source[inspector_start..]
        .find("pub(crate) fn draw_right_domain_coloring_panel")
        .map(|offset| inspector_start + offset)
        .expect("next panel");
    let inspector = &source[inspector_start..inspector_end];
    let polychoron_start = inspector
        .find("GeoObject::RegularPolychoron4D")
        .expect("4D polychoron inspector");
    let polytope_start = inspector
        .find("GeoObject::RegularPolytopeND")
        .expect("N-D polytope inspector");
    let polychoron = &inspector[polychoron_start..polytope_start];
    let polytope = &inspector[polytope_start..];

    for plane in [
        "xy (rad)", "xz (rad)", "xw (rad)", "yz (rad)", "yw (rad)", "zw (rad)",
    ] {
        assert!(polychoron.contains(plane), "missing {plane} control");
    }
    assert!(polychoron.contains("Relleno habilitado"));
    assert!(polychoron.contains("app.open_object_color_picker(id)"));
    assert!(polychoron.contains("app.open_regular_polychoron_fill_color_picker(id)"));
    assert!(!polychoron.contains("color_edit_button_srgba_unmultiplied"));
    assert!(!polychoron.contains("polychoron.color = color_from_srgba_unmultiplied"));
    assert!(polychoron.contains("Vista previa"));
    assert!(polychoron.contains("movimiento"));
    assert!(polychoron.contains("Restablecer rotaciones"));
    assert!(polychoron.contains("CollapsingHeader::new(\"Rotación manual\")"));
    assert!(polychoron.contains(".default_open(false)"));
    assert!(polychoron.contains("Animación de proyección"));
    assert!(polychoron.contains("draw_multidimensional_motion_card"));
    assert!(polychoron.contains("ui.push_id"));

    assert!(polytope.contains("ComboBox"));
    assert!(polytope.contains("3..=10"));
    assert!(polytope.contains("expected_rotation_angle_count"));
    assert!(polytope.contains("rotation_angles = vec![0.0; rotation_count]"));
    assert!(polytope.contains("ScrollArea::vertical"));
    assert!(polytope.contains("regular_polytope_nd_rotation_planes"));
    assert!(polytope.contains("CollapsingHeader::new(\"Rotación manual\")"));
    assert!(polytope.contains(".default_open(false)"));
    assert!(polytope.contains("x{}/x{} (rad)"));
    assert!(polytope.contains("solo como aristas"));
    assert!(!polytope.contains("fill_color"));
    assert!(polytope.contains("app.open_object_color_picker(id)"));
    assert!(!polytope.contains("color_edit_button_srgba_unmultiplied"));
    assert!(polytope.contains("polytope.dimension == 4"));
    assert!(polytope.contains("draw_multidimensional_motion_card"));
    assert!(polytope.contains("ui.push_id"));
    assert!(inspector.contains("right_properties_scroll"));
    assert!(inspector.contains(".auto_shrink([false, true])"));
    assert!(inspector.contains("draw_inspector_identity"));
    assert!(source.contains("fn draw_inspector_empty_state"));
    assert!(source.contains("Inspector listo"));
    assert!(source.contains("Identidad del objeto"));
    assert!(inspector.contains("Proyección"));
    assert!(inspector.contains("Geometría"));
    assert!(inspector.contains("Apariencia"));
    for control in [
        "Iniciar animación",
        "Pausar animación",
        "Velocidad",
        "Restablecer velocidad",
        "Mostrá el objeto en la vista 3D",
    ] {
        assert!(source.contains(control), "missing {control}");
    }
    assert!(source.contains(".text(\"Velocidad de animación\")"));
}

#[test]
fn workspace_utility_dock_reserves_one_right_column_for_3d_properties() {
    use crate::{Perspective, RightPanelContent, ShellWidthClass};

    assert!(crate::geometry_utility_dock_available(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Medium,
    ));
    assert!(crate::geometry_utility_dock_available(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Wide,
    ));
    assert!(!crate::uses_geometry_utility_dock(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Compact,
        true,
    ));
    assert!(crate::uses_geometry_utility_dock(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Medium,
        true,
    ));
    assert!(crate::uses_geometry_utility_dock(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Wide,
        true,
    ));
    assert!(!crate::uses_geometry_utility_dock(
        Perspective::Geometry2D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Wide,
        true,
    ));
    assert!(!crate::uses_geometry_utility_dock(
        Perspective::Geometry3D,
        Some(RightPanelContent::Properties),
        ShellWidthClass::Wide,
        false,
    ));
    assert!(crate::uses_compact_geometry_utility_dock(
        Perspective::Geometry3D,
        ShellWidthClass::Compact,
        true,
    ));
    assert!(!crate::uses_compact_geometry_utility_dock(
        Perspective::Geometry3D,
        ShellWidthClass::Medium,
        true,
    ));
    assert!(!crate::uses_compact_geometry_utility_dock(
        Perspective::Geometry3D,
        ShellWidthClass::Compact,
        false,
    ));

    let app_source = include_str!("app.rs");
    assert!(app_source.contains("if geometry_utility_dock_available {"));
    assert!(app_source.contains("if compact_geometry_utility_dock {"));
}

#[test]
fn reopening_the_assistant_restores_its_geometry_utility_host() {
    let app_source = include_str!("app.rs");
    let ui_source = include_str!("ui.rs");

    assert!(ui_source.contains("app.open_assistant_workspace();"));
    assert!(app_source.contains("pub(crate) fn open_assistant_workspace"));
    assert!(app_source.contains("self.workspace_dock_tab = crate::WorkspaceDockTab::Assistant;"));
    assert!(app_source.contains("self.right_drawer_open = true;"));
    assert!(app_source.contains("self.compact_geometry_utility_open = true;"));
}

#[test]
fn trig_animation_never_claims_the_geometry_3d_utility_column() {
    assert!(crate::app::trig_animation_supported(crate::ViewMode::D2));
    assert!(!crate::app::trig_animation_supported(crate::ViewMode::D3));

    let app_source = include_str!("app.rs");
    assert!(app_source.contains("visible && trig_animation_supported(self.current_view)"));
}

#[test]
fn three_d_algebra_rows_leave_full_editing_to_the_inspector() {
    let source = include_str!("algebra.rs");

    assert!(source.contains("app.current_view != ViewMode::D3"));
    assert!(source.contains("Abrí el Inspector para editar este objeto 3D."));
}

#[test]
fn object_color_control_has_a_practical_pointer_target() {
    let size = crate::algebra::OBJECT_COLOR_TARGET_SIZE;
    assert!(size.x >= 24.0);
    assert!(size.y >= 24.0);
}

#[test]
fn algebra_color_swatches_open_the_shared_picker_helpers() {
    let source = include_str!("algebra.rs");

    assert!(source.contains("app.open_object_color_picker(oid)"));
    assert!(!source.contains("active_color_picker = Some"));
}

#[test]
fn renderer_readiness_rejects_missing_or_locked_renderer() {
    use std::sync::{Arc, RwLock};

    let missing = Arc::new(RwLock::new(None::<()>));
    assert!(!crate::app::renderer_is_ready(Some(&missing)));

    let ready = Arc::new(RwLock::new(Some(())));
    assert!(crate::app::renderer_is_ready(Some(&ready)));
    let read_guard = ready.read().expect("renderer lock should be available");
    assert!(!crate::app::renderer_is_ready(Some(&ready)));
    drop(read_guard);
    let write_guard = ready.write().expect("renderer lock should be available");
    assert!(!crate::app::renderer_is_ready(Some(&ready)));
    drop(write_guard);
}

#[test]
fn two_dimensional_gpu_path_requires_ready_renderer_and_nonempty_canvas() {
    assert!(!crate::app::should_use_gpu_2d(
        true,
        false,
        egui::vec2(800.0, 600.0)
    ));
    assert!(!crate::app::should_use_gpu_2d(
        true,
        true,
        egui::vec2(0.0, 600.0)
    ));
    assert!(!crate::app::should_use_gpu_2d(
        false,
        true,
        egui::vec2(800.0, 600.0)
    ));
    assert!(crate::app::should_use_gpu_2d(
        true,
        true,
        egui::vec2(800.0, 600.0)
    ));
}

#[test]
fn unversioned_legacy_visibility_refreshes_all_gpu_graphable_objects_and_caches() {
    use grafito_core::{
        function_sampling, ComplexGridObj, ComplexMappingObj, Fractal2DObj, FunctionObj, GeoObject,
        ParametricCurve2DObj, PointObj, PolarCurveObj, VectorField2DObj,
    };
    use grafito_geometry::Point2;
    use std::collections::HashMap;

    let mut document = grafito_core::Document::new();
    let target = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
    let function = document.add_object(GeoObject::Function(FunctionObj::new("x")));
    let graphable_ids = [
        function,
        document.add_object(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
            "t", "t", 0.0, 1.0,
        ))),
        document.add_object(GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, 1.0))),
        document.add_object(GeoObject::VectorField2D(VectorField2DObj::new("x", "y"))),
        document.add_object(GeoObject::Fractal2D(Fractal2DObj::mandelbrot())),
        document.add_object(GeoObject::ComplexGrid(ComplexGridObj::new(
            "z", -1.0, 1.0, -1.0, 1.0,
        ))),
        document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
            "z", target,
        ))),
    ];

    let GeoObject::Function(function_obj) = document
        .get_object(function)
        .expect("function should exist")
    else {
        panic!("expected function");
    };
    drop(function_sampling::samples_or_compute(
        function_obj,
        (-1.0, 1.0),
        32,
        &HashMap::new(),
    ));
    assert!(function_obj
        .cached_key
        .read()
        .expect("cache lock")
        .is_some());

    let before = document.clone();
    for id in graphable_ids {
        document
            .get_object_mut(id)
            .expect("graphable object should exist")
            .set_visible(false);
    }

    assert_eq!(
        document.version,
        before.version.wrapping_add(graphable_ids.len() as u64)
    );
    assert_ne!(
        serde_json::to_value(&before).expect("before document should serialize"),
        serde_json::to_value(&document).expect("updated document should serialize")
    );
    assert!(crate::app::refresh_unversioned_document_change(
        &before,
        &mut document
    ));
    assert_eq!(
        document.version,
        before.version.wrapping_add(graphable_ids.len() as u64)
    );
    assert!(graphable_ids.iter().all(|id| {
        !document
            .get_object(*id)
            .expect("graphable object should exist")
            .is_visible()
    }));
    let GeoObject::Function(function_obj) = document
        .get_object(function)
        .expect("function should exist")
    else {
        panic!("expected function");
    };
    assert!(function_obj
        .cached_key
        .read()
        .expect("cache lock")
        .is_none());
}

#[test]
fn unversioned_legacy_style_change_refreshes_gpu_geometry() {
    use grafito_core::{FunctionObj, GeoObject};

    let mut document = grafito_core::Document::new();
    let function = document.add_object(GeoObject::Function(FunctionObj::new("x")));
    let before = document.clone();
    let GeoObject::Function(function_obj) = document
        .get_object_mut(function)
        .expect("function should exist")
    else {
        panic!("expected function");
    };
    function_obj.width = 6.0;

    assert_eq!(document.version, before.version.wrapping_add(1));
    assert_ne!(
        serde_json::to_value(&before).expect("before document should serialize"),
        serde_json::to_value(&document).expect("updated document should serialize")
    );
    assert!(crate::app::refresh_unversioned_document_change(
        &before,
        &mut document
    ));
    assert_eq!(document.version, before.version.wrapping_add(1));
}

fn toast_shape_count(toasts: &mut grafito_ui::toast::ToastManager) -> usize {
    let context = egui::Context::default();
    let output = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| toasts.draw(ui, 0.1));
    });
    output.shapes.len()
}

#[test]
fn mutating_command_submission_saves_undo_and_replaces_redo() {
    let before = grafito_core::Document::new();
    let mut after = before.clone();
    let mut input = "A = (8, 8)".to_string();
    let outcome = crate::commands::process_input(&mut after, &mut input);
    let mut undo_stack = Vec::new();
    let mut redo_stack = vec![grafito_core::ChangeSet {
        before: before.clone(),
        after: before.clone(),
    }];

    assert!(!matches!(
        outcome,
        grafito_command::commands::CommandOutcome::Error(_)
    ));
    crate::app::save_command_snapshot_if_mutated(
        &outcome,
        before,
        &after,
        &mut undo_stack,
        &mut redo_stack,
    );
    assert_eq!(undo_stack.len(), 1);
    assert!(redo_stack.is_empty());
}

#[test]
fn compact_shell_never_reserves_both_drawers() {
    let shell = crate::ShellLayout::for_viewport(
        317.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        true,
    );

    assert_eq!(shell.width_class, crate::ShellWidthClass::Compact);
    assert!(!shell.show_left_drawer);
    assert!(!shell.show_right_drawer);
    assert!(shell.show_bottom_input);
}

#[test]
fn compact_panel_menu_can_open_one_left_drawer() {
    let shell = crate::ShellLayout::for_viewport(
        960.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        true,
    )
    .with_compact_left_drawer(true, 0);

    assert_eq!(shell.width_class, crate::ShellWidthClass::Compact);
    assert!(shell.show_left_drawer);
    assert!(!shell.show_right_drawer);
    assert!(!shell.show_sidebar);
    assert!(!shell.show_bottom_input);
}

#[test]
fn canvas_focus_shell_keeps_the_assistant_and_restores_the_bottom_input() {
    let algebra = crate::ShellLayout::for_viewport(
        960.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        true,
    );
    assert_eq!(algebra.width_class, crate::ShellWidthClass::Compact);
    assert!(!algebra.show_sidebar);
    assert!(!algebra.show_left_drawer);
    assert!(!algebra.show_right_drawer);
    assert!(algebra.show_bottom_input);

    let tools = crate::ShellLayout::for_viewport(
        960.0,
        crate::Perspective::Geometry2D,
        1,
        true,
        true,
        true,
    );
    assert!(tools.show_bottom_input);
}

#[test]
fn medium_shell_restores_a_single_left_drawer_after_the_canvas_focus_band() {
    let shell = crate::ShellLayout::for_viewport(
        1_360.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        true,
    );

    assert_eq!(shell.width_class, crate::ShellWidthClass::Medium);
    assert!(shell.show_sidebar);
    assert!(shell.show_left_drawer);
    assert!(!shell.show_right_drawer);
}

#[test]
fn canvas_focus_extends_until_the_default_drawers_fit_a_useful_canvas() {
    let shell = crate::ShellLayout::for_viewport(
        1_280.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        true,
    );

    assert_eq!(shell.width_class, crate::ShellWidthClass::Compact);
    assert!(!shell.show_left_drawer);
    assert!(!shell.show_sidebar);
}

#[test]
fn wide_shell_keeps_the_panel_rail_available_when_its_drawer_is_closed() {
    let shell = crate::ShellLayout::for_viewport(
        1_920.0,
        crate::Perspective::Geometry2D,
        0,
        true,
        true,
        false,
    );

    assert!(shell.show_sidebar);
    assert!(!shell.show_left_drawer);
    assert!(shell.show_bottom_input);
}

#[test]
fn shell_and_keyboard_gates_use_window_dimensions_before_panels_reserve_space() {
    let app_source = include_str!("app.rs");
    let keyboard_source = include_str!("keyboard.rs");

    assert!(app_source.contains("let viewport_width = ctx.screen_rect().width();"));
    assert!(
        app_source.contains("crate::ShellLayout::for_viewport(\n                viewport_width,")
    );
    assert!(app_source
        .contains(".with_compact_left_drawer(self.compact_drawer_open, self.sidebar_tab)"));
    assert!(keyboard_source.contains("math_keyboard_layout"));
    assert!(app_source.contains("uses_geometry_utility_dock"));
    assert!(app_source.contains("draw_geometry_utility_dock"));
}

#[test]
fn top_chrome_uses_a_dedicated_narrow_width_policy() {
    for width in [960.0, 1_026.0, 1_120.0] {
        assert!(crate::ui::top_chrome_uses_overflow(width));
    }
    assert!(!crate::ui::top_chrome_uses_overflow(1_121.0));
}

#[test]
fn assistant_reopen_control_only_appears_when_panel_is_hidden() {
    assert!(crate::ui::assistant_reopen_control_visible(false));
    assert!(!crate::ui::assistant_reopen_control_visible(true));
}

fn top_chrome_input(width: f32, time: f64, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 720.0),
        )),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn render_assistant_reopen_control(
    ctx: &egui::Context,
    width: f32,
    time: f64,
    events: Vec<egui::Event>,
    assistant_visible: &mut bool,
) -> Option<egui::Rect> {
    let mut rect = None;
    let _ = ctx.run(top_chrome_input(width, time, events), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            rect = crate::ui::draw_assistant_reopen_control(
                ui,
                assistant_visible,
                egui::Color32::WHITE,
            )
            .map(|response| response.rect);
        });
    });
    rect
}

#[test]
fn assistant_reopen_control_renders_only_when_hidden_and_restores_the_panel() {
    let ctx = egui::Context::default();
    let mut assistant_visible = false;

    let rendered =
        render_assistant_reopen_control(&ctx, 960.0, 0.0, Vec::new(), &mut assistant_visible);

    assert!(rendered.is_some());
    assert!(!assistant_visible);
    assert!(crate::ui::restore_assistant_visibility(
        &mut assistant_visible,
        true
    ));
    assert!(assistant_visible);

    let rendered =
        render_assistant_reopen_control(&ctx, 960.0, 0.1, Vec::new(), &mut assistant_visible);

    assert!(rendered.is_none());
    assert!(!crate::ui::restore_assistant_visibility(
        &mut assistant_visible,
        true
    ));
}

#[test]
fn assistant_reopen_control_accepts_a_click_at_compact_and_wide_widths() {
    for (width, compact) in [(960.0, true), (1680.0, false)] {
        assert_eq!(crate::ui::top_chrome_uses_overflow(width), compact);

        let ctx = egui::Context::default();
        let mut assistant_visible = false;
        let rect =
            render_assistant_reopen_control(&ctx, width, 0.0, Vec::new(), &mut assistant_visible)
                .expect("hidden assistant must render a reopen control");
        let pos = rect.center();

        let _ = render_assistant_reopen_control(
            &ctx,
            width,
            0.1,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            &mut assistant_visible,
        );
        let _ = render_assistant_reopen_control(
            &ctx,
            width,
            0.2,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            &mut assistant_visible,
        );

        assert!(assistant_visible);
    }
}

#[test]
fn side_drawers_reserve_height_before_the_math_keyboard() {
    let app_source = include_str!("app.rs");
    let assistant = app_source
        .find("self.draw_assistant(ctx, keyboard_height);")
        .expect("assistant draw call");
    let right_drawer = app_source
        .find("if shell.show_right_drawer && !geometry_utility_dock_available {")
        .expect("right drawer block");
    let utility_dock = app_source
        .find("crate::ui::draw_geometry_utility_dock(self, ctx);")
        .expect("Geometry 3D utility dock");
    let keyboard = app_source
        .rfind("crate::keyboard::draw_math_keyboard(self, ctx, keyboard_layout);")
        .expect("keyboard draw call");

    assert!(assistant < keyboard);
    assert!(right_drawer < keyboard);
    assert!(utility_dock < keyboard);
}

#[test]
fn gpu_3d_overlay_keeps_cpu_only_4d_projections_visible() {
    let hypercube =
        grafito_core::GeoObject::HyperSurface4D(grafito_core::HyperSurface4DObj::hypercube());
    let polychoron = grafito_core::GeoObject::RegularPolychoron4D(
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Tesseract),
    );
    let polytope =
        grafito_core::GeoObject::RegularPolytopeND(grafito_core::RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            4,
        ));
    let cube = grafito_core::GeoObject::Cube3D(grafito_core::Cube3DObj::new(
        grafito_geometry::Point3D::new(0.0, 0.0, 0.0),
        2.0,
    ));

    assert!(crate::render_3d::requires_cpu_3d_overlay(&hypercube));
    assert!(!crate::render_3d::requires_cpu_3d_overlay(&polychoron));
    assert!(!crate::render_3d::requires_cpu_3d_overlay(&polytope));
    assert!(!crate::render_3d::requires_cpu_3d_overlay(&cube));
    assert!(!crate::render_3d::should_draw_cpu_3d_geometry(
        &polychoron,
        true
    ));
    assert!(!crate::render_3d::should_draw_cpu_3d_geometry(
        &polytope, true
    ));
}

#[test]
fn typed_cpu_projection_reuses_the_renderer_bridge_and_skips_unlabeled_gpu_overlays() {
    let mut polychoron =
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Tesseract);
    polychoron.rotation_angles = [0.13, -0.29, 0.41, -0.53, 0.67, -0.79];

    let cpu = crate::render_3d::project_regular_polychoron_cpu(&polychoron, None)
        .expect("CPU fallback projects valid typed geometry");
    let renderer = grafito_render::depth_3d::project_regular_polychoron(
        &polychoron,
        polychoron.rotation_angles,
    )
    .expect("renderer bridge projects valid typed geometry");
    assert!(std::ptr::eq(
        cpu.vertices().as_ptr(),
        renderer.vertices().as_ptr()
    ));

    let mut generic = grafito_core::RegularPolytopeNDObj::new(
        grafito_geometry::RegularPolytopeFamily::Hypercube,
        4,
    );
    generic.rotation_angles = vec![0.11, -0.23, 0.37, -0.41, 0.53, -0.67];
    let cpu = crate::render_3d::project_regular_polytope_nd_cpu(&generic, Some(0.25))
        .expect("CPU fallback projects valid generic geometry");
    let effective = crate::render_3d::effective_typed_four_d_angles(
        generic
            .rotation_angles
            .as_slice()
            .try_into()
            .expect("a four-dimensional generic polytope has six angles"),
        Some(0.25),
    );
    let renderer = grafito_render::depth_3d::project_regular_polytope_nd(&generic, &effective)
        .expect("renderer bridge projects valid generic geometry");
    assert!(std::ptr::eq(
        cpu.vertices().as_ptr(),
        renderer.vertices().as_ptr()
    ));

    let object = grafito_core::GeoObject::RegularPolychoron4D(polychoron.clone());
    assert!(!crate::render_3d::typed_cpu_projection_is_needed(
        &object, true
    ));

    polychoron.label = "P".to_string();
    let labeled = grafito_core::GeoObject::RegularPolychoron4D(polychoron);
    assert!(crate::render_3d::typed_cpu_projection_is_needed(
        &labeled, true
    ));
}

#[test]
fn regular_polytope_cpu_typed_four_d_motion_recognizes_only_four_dimensional_variants() {
    let polychoron = grafito_core::GeoObject::RegularPolychoron4D(
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Tesseract),
    );
    let polytope_4d =
        grafito_core::GeoObject::RegularPolytopeND(grafito_core::RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            4,
        ));
    let polytope_5d =
        grafito_core::GeoObject::RegularPolytopeND(grafito_core::RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            5,
        ));
    let legacy =
        grafito_core::GeoObject::HyperSurface4D(grafito_core::HyperSurface4DObj::hypercube());

    assert!(crate::app::is_typed_four_d_projection(&polychoron));
    assert!(crate::app::is_typed_four_d_projection(&polytope_4d));
    assert!(!crate::app::is_typed_four_d_projection(&polytope_5d));
    assert!(!crate::app::is_typed_four_d_projection(&legacy));
    assert_eq!(crate::app::typed_four_d_motion_phase(0.75), Some(0.75));
    assert_eq!(crate::app::typed_four_d_motion_phase(f64::NAN), None);
}

#[test]
fn regular_polytope_cpu_phase_is_continuous_across_all_six_fixed_planes() {
    let base = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let animated = crate::render_3d::effective_typed_four_d_angles(base, Some(0.25));

    assert_eq!(
        crate::render_3d::effective_typed_four_d_angles(base, None),
        base
    );
    for (index, (base_angle, animated_angle)) in base.into_iter().zip(animated).enumerate() {
        assert_eq!(animated_angle, base_angle + (index + 1) as f64 * 0.25);
    }

    let before =
        crate::render_3d::effective_typed_four_d_angles(base, Some(std::f64::consts::TAU - 1.0e-6));
    let after = crate::render_3d::effective_typed_four_d_angles(base, Some(0.0));
    for (left, right) in before.into_iter().zip(after) {
        assert!((left.sin() - right.sin()).abs() < 1.0e-5);
        assert!((left.cos() - right.cos()).abs() < 1.0e-5);
    }
}

#[test]
fn regular_polytope_cpu_uses_each_fixed_and_lexicographic_four_d_rotation_plane() {
    let fixed =
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Tesseract);
    let fixed_baseline = crate::render_3d::project_regular_polychoron_cpu(&fixed, None)
        .expect("unrotated typed polychoron projects");
    for plane in 0..fixed.rotation_angles.len() {
        let mut rotated = fixed.clone();
        rotated.rotation_angles[plane] = 0.37;
        let projection = crate::render_3d::project_regular_polychoron_cpu(&rotated, None)
            .expect("each fixed rotation plane projects");
        assert_ne!(
            projection.vertices(),
            fixed_baseline.vertices(),
            "plane {plane}"
        );
    }

    let generic = grafito_core::RegularPolytopeNDObj::new(
        grafito_geometry::RegularPolytopeFamily::Hypercube,
        4,
    );
    let generic_baseline = crate::render_3d::project_regular_polytope_nd_cpu(&generic, None)
        .expect("unrotated four-dimensional generic polytope projects");
    for plane in 0..generic.rotation_angles.len() {
        let mut rotated = generic.clone();
        rotated.rotation_angles[plane] = 0.37;
        let projection = crate::render_3d::project_regular_polytope_nd_cpu(&rotated, None)
            .expect("each lexicographic rotation plane projects");
        assert_ne!(
            projection.vertices(),
            generic_baseline.vertices(),
            "plane {plane}"
        );
    }
}

#[test]
fn regular_polytope_cpu_projects_every_polychoron_finitely_with_all_six_rotations() {
    let kinds = [
        grafito_geometry::RegularPolychoron::Pentachoron,
        grafito_geometry::RegularPolychoron::Tesseract,
        grafito_geometry::RegularPolychoron::SixteenCell,
        grafito_geometry::RegularPolychoron::TwentyFourCell,
        grafito_geometry::RegularPolychoron::OneTwentyCell,
        grafito_geometry::RegularPolychoron::SixHundredCell,
    ];

    for kind in kinds {
        let mut object = grafito_core::RegularPolychoron4DObj::new(kind);
        object.rotation_angles = [0.13, -0.27, 0.41, -0.59, 0.73, -0.89];
        let geometry = crate::render_3d::project_regular_polychoron_cpu(&object, None)
            .expect("valid canonical polychoron projects through the CPU fallback");

        assert_eq!(geometry.vertices().len(), kind.expected_counts().vertices);
        assert_eq!(geometry.edges().len(), kind.expected_counts().edges);
        assert_eq!(geometry.faces().len(), kind.expected_counts().faces);
        assert!(geometry
            .vertices()
            .iter()
            .all(|point| { grafito_render::depth_3d::point_is_renderable(*point) }));
    }
}

#[test]
fn regular_polytope_cpu_keeps_generic_nd_wireframe_and_phases_only_four_dimensions() {
    let mut four_d = grafito_core::RegularPolytopeNDObj::new(
        grafito_geometry::RegularPolytopeFamily::Hypercube,
        4,
    );
    four_d.rotation_angles = vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
    let static_four_d = crate::render_3d::project_regular_polytope_nd_cpu(&four_d, None)
        .expect("static four-dimensional generic polytope projects");
    let animated_four_d = crate::render_3d::project_regular_polytope_nd_cpu(&four_d, Some(0.25))
        .expect("animated four-dimensional generic polytope projects");

    assert!(static_four_d.faces().is_empty());
    assert_ne!(static_four_d.vertices(), animated_four_d.vertices());

    let mut five_d = grafito_core::RegularPolytopeNDObj::new(
        grafito_geometry::RegularPolytopeFamily::Hypercube,
        5,
    );
    five_d.rotation_angles = vec![0.1; 10];
    let static_five_d = crate::render_3d::project_regular_polytope_nd_cpu(&five_d, None)
        .expect("static five-dimensional generic polytope projects");
    let phased_five_d = crate::render_3d::project_regular_polytope_nd_cpu(&five_d, Some(0.25))
        .expect("five-dimensional generic polytope ignores the four-dimensional phase");

    assert!(static_five_d.faces().is_empty());
    assert_eq!(static_five_d.vertices(), phased_five_d.vertices());
}

#[test]
fn typed_four_d_phase_snapshot_matches_drawing_bounds_without_mutating_the_document() {
    let mut document = grafito_core::Document::new();
    let mut polychoron =
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Pentachoron);
    polychoron.rotation_angles = [0.13, -0.29, 0.41, -0.53, 0.67, -0.79];
    let id = document
        .try_add_object(grafito_core::GeoObject::RegularPolychoron4D(polychoron))
        .expect("valid typed polychoron inserts");
    let version_before = document.version;
    let object_before = document.get_object(id).cloned();
    let phase_snapshot = crate::app::typed_four_d_motion_phase(0.73);

    let object = document
        .get_object(id)
        .expect("inserted polychoron remains");
    let grafito_core::GeoObject::RegularPolychoron4D(polychoron) = object else {
        panic!("fixture remains a typed polychoron");
    };
    let drawing = crate::render_3d::project_regular_polychoron_cpu(polychoron, phase_snapshot)
        .expect("drawing projection uses the phase snapshot");
    let drawing_bounds = grafito_geometry::Aabb3D::from_points(drawing.vertices().iter().copied())
        .expect("drawing projection has finite bounds");
    let static_bounds = crate::render_3d::fallback_object_bounds_with_typed_four_d_phase(
        object,
        &document.variables,
        None,
    )
    .expect("static picking bounds");
    let picking_bounds = crate::render_3d::fallback_object_bounds_with_typed_four_d_phase(
        object,
        &document.variables,
        phase_snapshot,
    )
    .expect("phased picking bounds");

    assert!(
        (
            static_bounds.min.x,
            static_bounds.min.y,
            static_bounds.min.z
        ) != (
            picking_bounds.min.x,
            picking_bounds.min.y,
            picking_bounds.min.z
        ) || (
            static_bounds.max.x,
            static_bounds.max.y,
            static_bounds.max.z
        ) != (
            picking_bounds.max.x,
            picking_bounds.max.y,
            picking_bounds.max.z
        ),
        "the typed 4D phase must change projected bounds"
    );
    assert_eq!(
        (
            picking_bounds.min.x,
            picking_bounds.min.y,
            picking_bounds.min.z
        ),
        (
            drawing_bounds.min.x,
            drawing_bounds.min.y,
            drawing_bounds.min.z
        )
    );
    assert_eq!(
        (
            picking_bounds.max.x,
            picking_bounds.max.y,
            picking_bounds.max.z
        ),
        (
            drawing_bounds.max.x,
            drawing_bounds.max.y,
            drawing_bounds.max.z
        )
    );
    assert_eq!(document.version, version_before);
    assert_eq!(document.get_object(id), object_before.as_ref());
}

#[test]
fn typed_four_d_picker_uses_the_phase_snapshot_instead_of_static_bounds() {
    let camera = axis_aligned_test_camera();
    let canvas_size = egui::vec2(800.0, 600.0);
    let mut document = grafito_core::Document::new();
    let mut polychoron =
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Pentachoron);
    polychoron.rotation_angles = [0.13, -0.29, 0.41, -0.53, 0.67, -0.79];
    let id = document
        .try_add_object(grafito_core::GeoObject::RegularPolychoron4D(polychoron))
        .expect("valid typed polychoron inserts");

    let phase_snapshot = crate::app::typed_four_d_motion_phase(0.73);
    let object = document
        .get_object(id)
        .expect("inserted polychoron remains");
    let grafito_core::GeoObject::RegularPolychoron4D(polychoron) = object else {
        panic!("fixture remains a typed polychoron");
    };
    let drawing = crate::render_3d::project_regular_polychoron_cpu(polychoron, phase_snapshot)
        .expect("drawing projection uses the phase snapshot");
    let phase_only_pointer = drawing
        .vertices()
        .iter()
        .filter_map(|point| camera.project(point, canvas_size.x, canvas_size.y))
        .map(|(x, y)| egui::vec2(x, y))
        .find(|pointer| {
            crate::render_3d::pick_3d_object_with_typed_four_d_phase(
                &document,
                &camera,
                *pointer,
                canvas_size,
                phase_snapshot,
            ) == Some(id)
                && crate::render_3d::pick_3d_object(&document, &camera, *pointer, canvas_size)
                    .is_none()
        })
        .expect("a phased vertex lies outside the static fallback bounds");

    assert_eq!(
        crate::render_3d::pick_3d_object_with_typed_four_d_phase(
            &document,
            &camera,
            phase_only_pointer,
            canvas_size,
            phase_snapshot,
        ),
        Some(id),
        "selection must use the same phase as the drawn typed polytope"
    );
    assert_eq!(
        crate::render_3d::pick_3d_object(&document, &camera, phase_only_pointer, canvas_size),
        None,
        "the legacy no-phase picker remains static"
    );
}

#[test]
fn typed_four_d_phase_leaves_legacy_and_general_five_d_bounds_static() {
    let phase_snapshot = crate::app::typed_four_d_motion_phase(0.73);
    let legacy =
        grafito_core::GeoObject::HyperSurface4D(grafito_core::HyperSurface4DObj::hypercube());
    let five_d =
        grafito_core::GeoObject::RegularPolytopeND(grafito_core::RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            5,
        ));
    let four_d =
        grafito_core::GeoObject::RegularPolytopeND(grafito_core::RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            4,
        ));
    let variables = std::collections::HashMap::new();

    assert_eq!(
        crate::render_3d::typed_four_d_phase_for_object(&legacy, phase_snapshot),
        None
    );
    assert_eq!(
        crate::render_3d::typed_four_d_phase_for_object(&five_d, phase_snapshot),
        None
    );
    assert_eq!(
        crate::render_3d::typed_four_d_phase_for_object(&four_d, phase_snapshot),
        phase_snapshot
    );

    for object in [&legacy, &five_d] {
        let static_bounds = crate::render_3d::fallback_object_bounds_with_typed_four_d_phase(
            object, &variables, None,
        );
        let phased_bounds = crate::render_3d::fallback_object_bounds_with_typed_four_d_phase(
            object,
            &variables,
            phase_snapshot,
        );
        assert_eq!(
            static_bounds, phased_bounds,
            "only typed regular four-dimensional projections receive the motion phase"
        );
    }
}

#[test]
fn regular_polytope_cpu_sorts_static_polychoron_face_triangles_by_depth() {
    let camera = axis_aligned_test_camera();
    let object =
        grafito_core::RegularPolychoron4DObj::new(grafito_geometry::RegularPolychoron::Tesseract);
    let geometry = crate::render_3d::project_regular_polychoron_cpu(&object, None)
        .expect("tesseract CPU geometry");
    let faces = crate::render_3d::projected_polychoron_faces(&camera, &geometry, 800.0, 600.0);
    let expected_triangles = geometry
        .faces()
        .iter()
        .map(|face| face.len() - 2)
        .sum::<usize>();

    assert_eq!(faces.len(), expected_triangles);
    assert!(faces.windows(2).all(|faces| faces[0].0 >= faces[1].0));
    assert!(faces
        .iter()
        .all(|(_, face)| { face.iter().all(|(x, y)| x.is_finite() && y.is_finite()) }));
    assert!(crate::render_3d::should_draw_polychoron_faces(true, false));
    assert!(!crate::render_3d::should_draw_polychoron_faces(true, true));
    assert!(!crate::render_3d::should_draw_polychoron_faces(
        false, false
    ));
}

#[test]
fn regular_polytope_cpu_preview_bounds_wire_work_and_never_mutates_the_document() {
    let mut document = grafito_core::Document::new();
    let object = grafito_core::RegularPolychoron4DObj::new(
        grafito_geometry::RegularPolychoron::OneTwentyCell,
    );
    let id = object.id;
    document
        .try_add_object(grafito_core::GeoObject::RegularPolychoron4D(object))
        .expect("valid polychoron inserts into the document");
    let version_before = document.version;
    let object_before = document.get_object(id).cloned();

    let object = document
        .get_object(id)
        .expect("inserted object remains available");
    let grafito_core::GeoObject::RegularPolychoron4D(object) = object else {
        panic!("inserted object remains a typed polychoron");
    };
    let geometry = crate::render_3d::project_regular_polychoron_cpu(object, Some(0.25))
        .expect("motion-preview CPU projection remains finite");
    let stride =
        crate::render_3d::motion_preview_polytope_edge_stride(geometry.edges().len(), true);

    assert!(
        geometry.edges().len().div_ceil(stride)
            <= crate::render_3d::MAX_MOTION_PREVIEW_POLYTOPE_EDGES
    );
    assert_eq!(document.version, version_before);
    assert_eq!(document.get_object(id), object_before.as_ref());
}

#[test]
fn wide_shell_allows_contextual_drawer_only_when_available() {
    let available =
        crate::ShellLayout::for_viewport(1720.0, crate::Perspective::Complex, 0, true, true, true);
    assert!(available.show_left_drawer);
    assert!(available.show_right_drawer);

    let unavailable =
        crate::ShellLayout::for_viewport(1720.0, crate::Perspective::Complex, 0, false, true, true);
    assert!(!unavailable.show_right_drawer);
}

#[test]
fn contextual_drawer_waits_for_space_beyond_the_permanent_assistant() {
    let shell =
        crate::ShellLayout::for_viewport(1360.0, crate::Perspective::Complex, 0, true, true, true);

    assert_eq!(shell.width_class, crate::ShellWidthClass::Medium);
    assert!(!shell.show_right_drawer);
}

#[test]
fn command_input_width_never_becomes_negative() {
    assert_eq!(crate::ui::command_input_width(20.0, 40.0), 0.0);
    assert_eq!(crate::ui::command_input_width(140.0, 40.0), 100.0);
}

#[test]
fn assistant_credentials_use_fixed_keyring_accounts() {
    use grafito_assistant_types::ProviderProfile;

    assert_eq!(
        crate::assistant_credentials::account_for(ProviderProfile::OpenCodeGo),
        Some("assistant-opencode-go")
    );
    assert_eq!(
        crate::assistant_credentials::account_for(ProviderProfile::OllamaLocal),
        None
    );
}

#[test]
fn assistant_text_editor_blocks_global_shortcuts() {
    assert!(!crate::app::global_shortcuts_allowed(true));
    assert!(crate::app::global_shortcuts_allowed(false));
}

#[test]
fn status_pointer_is_converted_to_canvas_local_coordinates() {
    let local = crate::ui::canvas_local_pointer(egui::pos2(260.0, 180.0), egui::pos2(200.0, 100.0));
    assert_eq!(local, egui::pos2(60.0, 80.0));
}

#[test]
fn clean_document_actions_proceed_without_an_unsaved_changes_dialog() {
    use crate::app::{DocumentAction, DocumentActionRequest, DocumentLifecycle};

    for action in [
        DocumentAction::New,
        DocumentAction::Open,
        DocumentAction::Exit,
    ] {
        let document = crate::app::initial_document();
        let mut lifecycle = DocumentLifecycle::new(&document);
        assert_eq!(
            lifecycle.request_action(action, &document, false),
            DocumentActionRequest::Proceed(action)
        );
        assert_eq!(lifecycle.pending_action(), None);
    }
}

#[test]
fn toast_messages_are_wrapped_before_drawing() {
    let wrapped = crate::app::wrap_toast_message("uno dos tres cuatro cinco", 8);
    assert_eq!(wrapped, "uno dos\ntres\ncuatro\ncinco");
}

#[test]
fn toast_wrapping_splits_a_single_long_token() {
    assert_eq!(
        crate::app::wrap_toast_message("abcdefghij", 4),
        "abcd\nefgh\nij"
    );
}

#[test]
fn ctrl_shift_y_is_reserved_for_y_intercept_not_redo() {
    assert_eq!(
        crate::app::ctrl_y_shortcut(false),
        crate::app::CtrlYShortcut::Redo
    );
    assert_eq!(
        crate::app::ctrl_y_shortcut(true),
        crate::app::CtrlYShortcut::YIntercept
    );
}

#[test]
fn document_lifecycle_fresh_workspace_is_pathless_and_clean() {
    let document = crate::app::initial_document();
    let lifecycle = crate::app::DocumentLifecycle::new(&document);

    assert_eq!(lifecycle.current_path(), None);
    assert!(!lifecycle.is_dirty(&document, false));
}

#[test]
fn document_lifecycle_dirty_baseline_ignores_transient_revision_and_quality() {
    let mut document = crate::app::initial_document();
    let lifecycle = crate::app::DocumentLifecycle::new(&document);

    document.bump_version();
    document.render_quality = grafito_core::RenderQuality::Preview;
    document.view_mut().screen_size = glam::Vec2::new(931.0, 577.0);
    document.invalidate_all_caches();
    assert!(!lifecycle.is_dirty(&document, false));

    document.add_object(grafito_core::GeoObject::Point(grafito_core::PointObj::new(
        grafito_geometry::Point2::new(1.0, 2.0),
    )));
    assert!(lifecycle.is_dirty(&document, false));
}

#[test]
fn document_lifecycle_successful_save_updates_path_and_semantic_baseline() {
    let mut document = crate::app::initial_document();
    let mut lifecycle = crate::app::DocumentLifecycle::new(&document);
    let path = std::path::PathBuf::from("/tmp/grafito-saved.json");

    document.set_variable("a".into(), 3.0);
    assert!(lifecycle.is_dirty(&document, false));
    assert_eq!(
        lifecycle.current_save_path(crate::app::SaveMode::Save),
        None
    );

    assert_eq!(lifecycle.record_save_success(path.clone(), &document), None);
    assert_eq!(lifecycle.current_path(), Some(path.as_path()));
    assert_eq!(
        lifecycle.current_save_path(crate::app::SaveMode::Save),
        Some(path.as_path())
    );
    assert_eq!(
        lifecycle.current_save_path(crate::app::SaveMode::SaveAs),
        None
    );
    assert!(!lifecycle.is_dirty(&document, false));
}

#[test]
fn document_lifecycle_dirty_new_open_and_exit_wait_for_a_decision() {
    use crate::app::{DocumentAction, DocumentActionRequest, DocumentLifecycle};

    for action in [
        DocumentAction::New,
        DocumentAction::Open,
        DocumentAction::Exit,
    ] {
        let mut document = crate::app::initial_document();
        let mut lifecycle = DocumentLifecycle::new(&document);
        document.set_variable("dirty".into(), 1.0);

        assert_eq!(
            lifecycle.request_action(action, &document, false),
            DocumentActionRequest::AwaitDecision(action)
        );
        assert_eq!(lifecycle.pending_action(), Some(action));
    }
}

#[test]
fn document_lifecycle_cancel_and_discard_have_explicit_state_transitions() {
    use crate::app::{DocumentAction, DocumentLifecycle, UnsavedDecision, UnsavedResolution};

    let mut document = crate::app::initial_document();
    let mut lifecycle = DocumentLifecycle::new(&document);
    let path = std::path::PathBuf::from("/tmp/grafito-cancel-preserves.json");
    lifecycle.record_save_success(path.clone(), &document);
    document.set_variable("dirty".into(), 1.0);
    lifecycle.request_action(DocumentAction::New, &document, false);

    assert_eq!(
        lifecycle.resolve_unsaved_decision(UnsavedDecision::Cancel),
        Some(UnsavedResolution::Cancelled)
    );
    assert_eq!(lifecycle.pending_action(), None);
    assert_eq!(lifecycle.current_path(), Some(path.as_path()));
    assert!(lifecycle.is_dirty(&document, false));

    lifecycle.request_action(DocumentAction::Open, &document, false);
    assert_eq!(
        lifecycle.resolve_unsaved_decision(UnsavedDecision::Discard),
        Some(UnsavedResolution::Proceed(DocumentAction::Open))
    );
    assert_eq!(lifecycle.pending_action(), None);
    assert_eq!(lifecycle.current_path(), Some(path.as_path()));
    assert!(lifecycle.is_dirty(&document, false));
}

#[test]
fn document_lifecycle_save_failure_retains_the_document_action_dialog() {
    use crate::app::{DocumentAction, DocumentLifecycle, UnsavedDecision, UnsavedResolution};

    let mut document = crate::app::initial_document();
    let mut lifecycle = DocumentLifecycle::new(&document);
    document.set_variable("dirty".into(), 1.0);
    lifecycle.request_action(DocumentAction::Exit, &document, false);

    assert_eq!(
        lifecycle.resolve_unsaved_decision(UnsavedDecision::Save),
        Some(UnsavedResolution::Save(DocumentAction::Exit))
    );
    lifecycle.record_save_failure("disk full");

    assert_eq!(lifecycle.pending_action(), Some(DocumentAction::Exit));
    assert_eq!(lifecycle.save_error(), Some("disk full"));
    assert!(lifecycle.is_dirty(&document, false));

    lifecycle.request_action(DocumentAction::Exit, &document, false);
    assert_eq!(lifecycle.save_error(), Some("disk full"));
}

#[test]
fn document_lifecycle_save_then_open_releases_action_and_open_establishes_identity() {
    use crate::app::{DocumentAction, DocumentLifecycle};

    let mut document = crate::app::initial_document();
    let mut lifecycle = DocumentLifecycle::new(&document);
    document.set_variable("dirty".into(), 1.0);
    lifecycle.request_action(DocumentAction::Open, &document, false);

    let saved_path = std::path::PathBuf::from("/tmp/grafito-before-open.json");
    assert_eq!(
        lifecycle.record_save_success(saved_path.clone(), &document),
        Some(DocumentAction::Open)
    );
    assert_eq!(lifecycle.current_path(), Some(saved_path.as_path()));
    assert!(!lifecycle.is_dirty(&document, false));

    let mut opened = crate::app::initial_document();
    opened.set_variable("opened".into(), 42.0);
    let opened_path = std::path::PathBuf::from("/tmp/grafito-opened.json");
    lifecycle.establish_opened_document(opened_path.clone(), &opened);

    assert_eq!(lifecycle.current_path(), Some(opened_path.as_path()));
    assert!(!lifecycle.is_dirty(&opened, false));

    let blank = crate::app::initial_document();
    lifecycle.establish_new_document(&blank);
    assert_eq!(lifecycle.current_path(), None);
    assert!(!lifecycle.is_dirty(&blank, false));
}

#[test]
fn document_lifecycle_invalid_open_candidate_does_not_touch_live_document() {
    let mut live = crate::app::initial_document();
    live.set_variable("keep".into(), 7.0);
    let before = serde_json::to_value(&live).expect("live document should serialize");
    let path = std::env::temp_dir().join(format!(
        "grafito-invalid-open-{}-{}.json",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, b"not valid document json").expect("write invalid fixture");

    let candidate = crate::app::load_document_candidate(&path);
    let _ = std::fs::remove_file(path);

    assert!(candidate.is_err());
    assert_eq!(
        serde_json::to_value(&live).expect("live document should serialize"),
        before
    );
}

#[cfg(unix)]
#[test]
fn document_lifecycle_non_utf8_document_paths_are_not_lossy() {
    use std::os::unix::ffi::OsStringExt;

    let mut document = crate::app::initial_document();
    document.set_variable("non_utf8".into(), 9.0);
    let mut file_name = format!("grafito-non-utf8-{}-", std::process::id()).into_bytes();
    file_name.extend_from_slice(&[0xff, b'.', b'j', b's', b'o', b'n']);
    let path = std::env::temp_dir().join(std::ffi::OsString::from_vec(file_name));
    grafito_core::write_document_atomic(&document, &path).expect("write non-UTF-8 fixture");

    let loaded = crate::app::load_document_candidate(&path);
    let _ = std::fs::remove_file(path);

    assert_eq!(
        loaded
            .expect("non-UTF-8 path should load")
            .get_variable("non_utf8"),
        Some(9.0)
    );
}

#[test]
fn document_lifecycle_file_shortcuts_share_the_file_command_policy() {
    use crate::app::{file_shortcut, FileCommand};
    use egui::Key;

    assert_eq!(file_shortcut(Key::N, true, false), Some(FileCommand::New));
    assert_eq!(file_shortcut(Key::O, true, false), Some(FileCommand::Open));
    assert_eq!(file_shortcut(Key::S, true, false), Some(FileCommand::Save));
    assert_eq!(file_shortcut(Key::S, true, true), Some(FileCommand::SaveAs));
    assert_eq!(file_shortcut(Key::N, false, false), None);
}

#[test]
fn document_lifecycle_ui_exposes_save_as_and_native_close_cancellation() {
    let ui = include_str!("ui.rs");
    assert!(ui.contains("Nuevo (Ctrl+N)"));
    assert!(ui.contains("Abrir... (Ctrl+O)"));
    assert!(ui.contains("Guardar (Ctrl+S)"));
    assert!(ui.contains("Guardar como... (Ctrl+Shift+S)"));

    let app = include_str!("app.rs");
    assert!(app.contains("i.viewport().close_requested()"));
    assert!(app.contains("egui::ViewportCommand::CancelClose"));
}

#[test]
fn document_lifecycle_replacement_commits_before_transient_reset() {
    let source = include_str!("app.rs");
    let start = source
        .find("fn replace_document")
        .expect("document replacement helper");
    let body = &source[start..];
    let commit = body
        .find("self.document = document;")
        .expect("replacement commit");
    let reset = body
        .find("self.clear_document_bound_transient_state();")
        .expect("transient reset");

    assert!(commit < reset);
}

#[test]
fn deferred_file_actions_are_single_slot_and_dialog_decisions_have_priority() {
    use crate::app::{DeferredFileActions, DeferredFileIntent, FileCommand, UnsavedDecision};

    let mut actions = DeferredFileActions::default();
    assert!(actions.queue_command(FileCommand::Open));
    assert!(!actions.queue_command(FileCommand::Save));
    assert_eq!(
        actions.pending(),
        Some(DeferredFileIntent::Command(FileCommand::Open))
    );

    assert!(actions.queue_decision(UnsavedDecision::Save));
    assert!(!actions.queue_command(FileCommand::Exit));
    assert_eq!(
        actions.take_after_editors(),
        Some(DeferredFileIntent::Decision(UnsavedDecision::Save))
    );
    assert_eq!(actions.take_after_editors(), None);

    let mut native = DeferredFileActions::default();
    assert!(native.intercept_native_close(false));
    assert!(!native.intercept_native_close(true));
    assert_eq!(
        native.take_after_editors(),
        Some(DeferredFileIntent::Command(FileCommand::Exit))
    );
}
