use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_core::*;
use grafito_geometry::{
    fractals::FractalType, rotate_nd_in_plane, Camera3D, Color, NdPerspectiveProjection, Point2,
    Point3D, Point4D, RegularPolychoron, RegularPolytopeFamily, ViewTransform,
};
use grafito_render::{
    depth_3d::{project_regular_polychoron, project_regular_polytope_nd},
    sample_phase_portrait, sample_vector_field_3d, transform_complex_mapping_segments, Renderer,
};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn view_800x600() -> ViewTransform {
    ViewTransform::new(800.0, 600.0)
}

fn gpu_test_guard() -> MutexGuard<'static, ()> {
    static GPU_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    GPU_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn assert_points_close(actual: Point3D, expected: Point3D) {
    for (actual, expected) in [
        (actual.x, expected.x),
        (actual.y, expected.y),
        (actual.z, expected.z),
    ] {
        assert!(
            (actual - expected).abs() <= 1.0e-12,
            "expected {expected}, got {actual}"
        );
    }
}

fn geometry_planned_polychoron_vertices(
    object: &RegularPolychoron4DObj,
    angles: [f64; 6],
) -> Vec<Point3D> {
    let plan = object
        .kind
        .projection_plan(object.scale)
        .expect("valid typed polychoron has a geometry projection plan");
    object
        .kind
        .topology()
        .expect("valid typed polychoron has canonical topology")
        .vertices
        .into_iter()
        .map(|vertex| {
            let vertex = vertex
                .rotate_all_planes(angles)
                .expect("finite rotations remain finite");
            Point4D::new(
                vertex.x * object.scale,
                vertex.y * object.scale,
                vertex.z * object.scale,
                vertex.w * object.scale,
            )
            .perspective_project(plan.distance())
            .expect("the geometry plan projects every canonical vertex")
        })
        .collect()
}

fn geometry_planned_generic_polytope_vertices(
    object: &RegularPolytopeNDObj,
    angles: &[f64],
) -> Vec<Point3D> {
    let plan = object
        .family
        .projection_plan(object.dimension, object.scale)
        .expect("valid typed polytope has a geometry projection plan");
    let projection = NdPerspectiveProjection::new(plan.distance())
        .expect("a geometry plan always supplies a valid perspective distance");
    object
        .family
        .topology(object.dimension)
        .expect("valid typed polytope has canonical topology")
        .vertices
        .into_iter()
        .map(|mut coordinates| {
            for ((first_axis, second_axis), angle) in object.rotation_plane_pairs().zip(angles) {
                rotate_nd_in_plane(&mut coordinates, first_axis, second_axis, *angle)
                    .expect("finite rotations remain finite");
            }
            for coordinate in &mut coordinates {
                *coordinate *= object.scale;
            }
            projection
                .project(&coordinates)
                .expect("the geometry plan projects every canonical vertex")
        })
        .collect()
}

#[test]
fn renderer_typed_polytope_projection_uses_the_geometry_plan_for_static_and_rotated_inputs() {
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.scale = 1.75;
    let mut generic = RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5);
    generic.scale = 0.75;

    for angles in [[0.0; 6], [0.13, -0.29, 0.41, -0.53, 0.67, -0.79]] {
        let projected = project_regular_polychoron(&polychoron, angles)
            .expect("the renderer bridge projects valid polychoron inputs");
        let expected = geometry_planned_polychoron_vertices(&polychoron, angles);

        assert_eq!(projected.vertices().len(), expected.len());
        for (&actual, expected) in projected.vertices().iter().zip(expected) {
            assert_points_close(actual, expected);
        }
    }

    for angles in [
        vec![0.0; generic.rotation_angles.len()],
        vec![
            0.11, -0.23, 0.37, -0.41, 0.53, -0.67, 0.71, -0.83, 0.97, -1.09,
        ],
    ] {
        let projected = project_regular_polytope_nd(&generic, &angles)
            .expect("the renderer bridge projects valid generic inputs");
        let expected = geometry_planned_generic_polytope_vertices(&generic, &angles);

        assert_eq!(projected.vertices().len(), expected.len());
        for (&actual, expected) in projected.vertices().iter().zip(expected) {
            assert_points_close(actual, expected);
        }
        assert!(projected.faces().is_empty(), "generic N-D stays wire-only");
    }
}

#[test]
fn projected_polytope_cache_ignores_presentation_style() {
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.scale = 1.234_567;
    polychoron.rotation_angles = [0.11, -0.23, 0.37, -0.41, 0.53, -0.67];
    let mut restyled_polychoron = polychoron.clone();
    restyled_polychoron.color = Color::new(0.1, 0.8, 0.3, 0.4);
    restyled_polychoron.width = 7.0;
    restyled_polychoron.fill_color = None;

    let first = project_regular_polychoron(&polychoron, polychoron.rotation_angles)
        .expect("first typed polychoron projection");
    let restyled =
        project_regular_polychoron(&restyled_polychoron, restyled_polychoron.rotation_angles)
            .expect("restyling cannot invalidate geometry projection");
    assert!(std::ptr::eq(
        first.vertices().as_ptr(),
        restyled.vertices().as_ptr()
    ));

    let mut generic = RegularPolytopeNDObj::new(RegularPolytopeFamily::CrossPolytope, 5);
    generic.scale = 0.987_654;
    generic.rotation_angles = vec![0.07; generic.rotation_angles.len()];
    let mut restyled_generic = generic.clone();
    restyled_generic.color = Color::new(0.9, 0.2, 0.6, 0.7);
    restyled_generic.width = 4.0;
    restyled_generic.fill_color = Some(Color::new(0.2, 0.3, 0.4, 0.5));

    let first = project_regular_polytope_nd(&generic, &generic.rotation_angles)
        .expect("first generic projection");
    let restyled =
        project_regular_polytope_nd(&restyled_generic, &restyled_generic.rotation_angles)
            .expect("restyling cannot invalidate generic geometry projection");
    assert!(std::ptr::eq(
        first.vertices().as_ptr(),
        restyled.vertices().as_ptr()
    ));
}

#[test]
fn test_complex_mapping_transform_does_not_bridge_segments() {
    let segments = vec![
        (Point2::new(1.0, 0.0), Point2::new(2.0, 0.0)),
        (Point2::new(-1.0, 0.0), Point2::new(-2.0, 0.0)),
    ];

    let strokes = transform_complex_mapping_segments(ConformalMap::Inversion, &segments, 1, 1.0);

    assert_eq!(strokes.len(), 2);
    assert!(strokes.iter().all(|(a, b)| a.x.signum() == b.x.signum()));
}

#[test]
fn recognized_complex_mapping_of_circle_and_pencil_adds_gpu_geometry() {
    let view = view_800x600();
    let mut circle_document = Document::new();
    let circle = circle_document.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(2.0, 0.0),
        0.5,
    )));
    let (circle_base_vertices, circle_base_indices) =
        Renderer::build_geometry_static(&circle_document, &view, false, false);
    circle_document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "1/z", circle,
    )));
    let (circle_mapped_vertices, circle_mapped_indices) =
        Renderer::build_geometry_static(&circle_document, &view, false, false);
    assert!(circle_mapped_vertices.len() > circle_base_vertices.len());
    assert!(circle_mapped_indices.len() > circle_base_indices.len());

    let mut pencil_document = Document::new();
    let pencil = pencil_document.add_object(GeoObject::Pencil(PencilObj::new(vec![
        Point2::new(1.0, 0.0),
        Point2::new(2.0, 1.0),
        Point2::new(3.0, 0.0),
    ])));
    let (pencil_base_vertices, pencil_base_indices) =
        Renderer::build_geometry_static(&pencil_document, &view, false, false);
    pencil_document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "1/z", pencil,
    )));
    let (pencil_mapped_vertices, pencil_mapped_indices) =
        Renderer::build_geometry_static(&pencil_document, &view, false, false);
    assert!(pencil_mapped_vertices.len() > pencil_base_vertices.len());
    assert!(pencil_mapped_indices.len() > pencil_base_indices.len());
}

#[test]
fn recognized_complex_mapping_of_analytic_curves_adds_gpu_geometry() {
    let view = view_800x600();
    let targets = [
        GeoObject::Ellipse(EllipseObj::new(Point2::new(2.0, 0.0), 1.0, 0.5)),
        GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 0.0), 1.0)),
        GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 0.5)),
        GeoObject::RegressionLine(RegressionLineObj::linear(
            vec![-1.0, 0.0, 1.0],
            vec![-1.0, 0.0, 1.0],
            1.0,
            0.0,
            1.0,
        )),
    ];

    for target in targets {
        let mut document = Document::new();
        let target_id = document.add_object(target);
        document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
            "1/z", target_id,
        )));

        let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);
        assert!(
            !vertices.is_empty() && !indices.is_empty(),
            "a recognized mapping must render every analytic 2D target"
        );
    }
}

#[test]
fn static_geometry_covers_every_assistant_enabled_2d_curve_and_data_route() {
    let view = view_800x600();
    let objects = vec![
        GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
            "cos(t)",
            "sin(t)",
            0.0,
            std::f64::consts::TAU,
        )),
        GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, std::f64::consts::TAU)),
        GeoObject::ImplicitCurve(ImplicitCurveObj::new("x^2+y^2", "1", RelationOperator::Eq)),
        GeoObject::Histogram(HistogramObj::new(vec![-1.0, 0.0, 0.5, 1.0], 4)),
        GeoObject::ScatterPlot(ScatterPlotObj::new(
            vec![-1.0, 0.0, 1.0],
            vec![1.0, 0.0, 1.0],
        )),
        GeoObject::BoxPlot(BoxPlotObj::new(vec![-1.0, 0.0, 0.5, 1.0, 2.0])),
        GeoObject::RegressionLine(RegressionLineObj::linear(
            vec![-1.0, 0.0, 1.0],
            vec![-1.0, 0.0, 1.0],
            1.0,
            0.0,
            1.0,
        )),
        GeoObject::PhasePortrait(PhasePortraitObj::new("y", "-x", -2.0, 2.0, -2.0, 2.0)),
    ];

    for object in objects {
        let mut document = Document::new();
        let name = object.name().to_string();
        document.add_object(object);

        let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);

        assert!(
            !vertices.is_empty() && !indices.is_empty(),
            "{name} must have a standalone static render proof"
        );
    }
}

#[test]
fn transparent_sphere_fill_uses_the_non_depth_writing_stream() {
    let mut document = Document::new();
    document.add_object(GeoObject::Sphere3D(Sphere3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        1.0,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.opaque_indices.is_empty());
    assert!(!mesh.wire_indices.is_empty());
}

#[test]
fn opaque_tetrahedron_emits_four_faces_and_six_wire_edges() {
    let mut document = Document::new();
    document.add_object(GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert_eq!(mesh.opaque_indices.len(), 12, "four solid triangles");
    assert_eq!(mesh.wire_indices.len(), 36, "six wire quads");
    mesh.validate().expect("tetrahedron world mesh stays valid");
}

#[test]
fn out_of_range_tetrahedron_marks_the_world_mesh_incomplete() {
    let mut document = Document::new();
    let id = document.add_object(GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));
    let Some(GeoObject::Tetrahedron3D(tetrahedron)) = document.get_object_mut(id) else {
        panic!("valid tetrahedron fixture");
    };
    tetrahedron.center = Point3D::new(1_000_000_000_000.0, 0.0, 0.0);

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(!mesh.is_complete());
    assert!(mesh.opaque_indices.is_empty());
    assert!(mesh.wire_indices.is_empty());
}

#[test]
fn out_of_range_tetrahedron_cannot_be_inserted_normally() {
    let mut document = Document::new();
    let error = document
        .try_add_object(GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(
            Point3D::new(1_000_000_000_000.0, 0.0, 0.0),
            2.0,
        )))
        .expect_err("out-of-range tetrahedron must not enter the document");

    assert!(error.contains("maximum renderable coordinate"));
}

#[test]
fn regular_polychora_emit_their_exact_face_and_edge_mesh_counts() {
    let cases = [
        (RegularPolychoron::Pentachoron, 10, 10),
        (RegularPolychoron::Tesseract, 48, 32),
        (RegularPolychoron::SixteenCell, 32, 24),
        (RegularPolychoron::TwentyFourCell, 96, 96),
        (RegularPolychoron::OneTwentyCell, 2_160, 1_200),
        (RegularPolychoron::SixHundredCell, 1_200, 720),
    ];

    for (kind, face_triangles, edges) in cases {
        let mut document = Document::new();
        document.add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            kind,
        )));

        let mesh =
            Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

        assert!(
            mesh.is_complete(),
            "{kind:?} must fit the world mesh budget"
        );
        assert_eq!(
            mesh.opaque_vertices.len(),
            face_triangles * 3,
            "{kind:?} must fan-triangulate every canonical face"
        );
        assert_eq!(mesh.opaque_indices.len(), face_triangles * 3);
        assert_eq!(mesh.wire_vertices.len(), edges * 4);
        assert_eq!(mesh.wire_indices.len(), edges * 6);
        mesh.validate()
            .expect("regular polychoron mesh must remain finite and indexed");
    }
}

#[test]
fn generic_regular_polytopes_emit_only_canonical_wire_edges() {
    let mut document = Document::new();
    document.add_object(GeoObject::RegularPolytopeND(RegularPolytopeNDObj::new(
        RegularPolytopeFamily::Hypercube,
        5,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.is_complete());
    assert!(mesh.opaque_vertices.is_empty());
    assert!(mesh.opaque_indices.is_empty());
    assert_eq!(mesh.wire_vertices.len(), 80 * 4, "the 5-cube has 80 edges");
    assert_eq!(mesh.wire_indices.len(), 80 * 6);
    mesh.validate()
        .expect("generic regular-polytope wire mesh must be valid");
}

#[test]
fn regular_polychoron_preview_quality_emits_edges_without_faces() {
    let mut document = Document::new();
    document.render_quality = RenderQuality::Preview;
    document.add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
        RegularPolychoron::Tesseract,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.is_complete());
    assert!(mesh.opaque_indices.is_empty());
    assert_eq!(mesh.wire_vertices.len(), 32 * 4);
    assert_eq!(mesh.wire_indices.len(), 32 * 6);
}

#[test]
fn regular_polychoron_preview_output_estimate_does_not_charge_omitted_faces() {
    let object = GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
        RegularPolychoron::OneTwentyCell,
    ));
    let normal = grafito_render::depth_3d::world_mesh_output_usage_for_quality(
        &object,
        RenderQuality::Normal,
    )
    .expect("a 120-cell has bounded normal output");
    let preview = grafito_render::depth_3d::world_mesh_output_usage_for_quality(
        &object,
        RenderQuality::Preview,
    )
    .expect("a 120-cell has bounded preview output");
    let mut normal_budget = grafito_render::depth_3d::WorldMeshOutputBudget::default();
    let mut preview_budget = grafito_render::depth_3d::WorldMeshOutputBudget::default();

    for _ in 0..80 {
        assert!(normal_budget.fits(normal));
        assert!(preview_budget.fits(preview));
        normal_budget.consume(normal);
        preview_budget.consume(preview);
    }

    assert!(!normal_budget.fits(normal));
    assert!(preview_budget.fits(preview));
}

#[test]
fn translucent_regular_polychoron_faces_use_the_non_depth_writing_stream() {
    let mut document = Document::new();
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Pentachoron);
    polychoron.fill_color = Some(Color::new(0.2, 0.5, 0.9, 0.5));
    document.add_object(GeoObject::RegularPolychoron4D(polychoron));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.is_complete());
    assert!(mesh.opaque_indices.is_empty());
    assert_eq!(mesh.wire_vertices.len(), 10 * 3 + 10 * 4);
    assert_eq!(mesh.wire_indices.len(), 10 * 3 + 10 * 6);
    mesh.validate()
        .expect("translucent regular-polychoron mesh must be valid");
}

#[test]
fn rotated_regular_polychoron_emits_only_finite_world_vertices() {
    let mut document = Document::new();
    let mut polychoron = RegularPolychoron4DObj::new(RegularPolychoron::Tesseract);
    polychoron.rotation_angles = [0.13, -0.29, 0.41, -0.53, 0.67, -0.79];
    document.add_object(GeoObject::RegularPolychoron4D(polychoron));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.is_complete());
    assert!(!mesh.opaque_vertices.is_empty());
    assert!(mesh
        .opaque_vertices
        .iter()
        .chain(&mesh.wire_vertices)
        .all(|vertex| vertex
            .position
            .iter()
            .all(|coordinate| coordinate.is_finite())));
    mesh.validate()
        .expect("all six 4D rotations must leave renderable finite output");
}

#[test]
fn invalid_mutated_regular_polychoron_leaves_no_partial_geometry_and_requests_fallback() {
    let mut document = Document::new();
    let id = document.add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
        RegularPolychoron::Tesseract,
    )));
    let Some(GeoObject::RegularPolychoron4D(polychoron)) = document.get_object_mut(id) else {
        panic!("valid regular-polychoron fixture");
    };
    polychoron.scale = f64::INFINITY;

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(!mesh.is_complete());
    assert!(mesh.opaque_vertices.is_empty());
    assert!(mesh.opaque_indices.is_empty());
    assert!(mesh.wire_vertices.is_empty());
    assert!(mesh.wire_indices.is_empty());
    mesh.validate()
        .expect("an incomplete fallback mesh must still be structurally valid");
}

#[test]
fn direct_mutation_rejects_a_finite_polychoron_outside_the_geometry_projection_plan() {
    let mut document = Document::new();
    let id = document.add_object(GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
        RegularPolychoron::Tesseract,
    )));
    let Some(GeoObject::RegularPolychoron4D(polychoron)) = document.get_object_mut(id) else {
        panic!("valid regular-polychoron fixture");
    };
    let threshold = grafito_geometry::MAX_WORLD_COORDINATE * 5.0
        / (6.0 * polychoron.kind.canonical_radius_bound());
    polychoron.scale = threshold * 1.001;
    assert!(polychoron
        .kind
        .projection_plan(polychoron.scale)
        .expect("the finite mutation still has a scalar plan")
        .ensure_within_coordinate_limit(grafito_geometry::MAX_WORLD_COORDINATE)
        .is_err());

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(!mesh.is_complete());
    assert!(mesh.opaque_vertices.is_empty());
    assert!(mesh.opaque_indices.is_empty());
    assert!(mesh.wire_vertices.is_empty());
    assert!(mesh.wire_indices.is_empty());
}

#[test]
fn translucent_solid_fills_use_the_non_depth_writing_stream() {
    let translucent = Color::new(0.2, 0.5, 0.9, 0.5);
    let mut document = Document::new();

    let mut plane = Plane3DObj::from_equation(0.0, 1.0, 0.0, 0.0);
    plane.opacity = translucent.a;
    plane.color = translucent;
    document.add_object(GeoObject::Plane3D(plane));

    let mut sphere = Sphere3DObj::new(Point3D::new(-8.0, 0.0, 0.0), 1.0);
    sphere.fill_color = Some(translucent);
    document.add_object(GeoObject::Sphere3D(sphere));

    let mut cube = Cube3DObj::new(Point3D::new(-5.0, 0.0, 0.0), 1.0);
    cube.fill_color = Some(translucent);
    document.add_object(GeoObject::Cube3D(cube));

    let mut tetrahedron = Tetrahedron3DObj::new(Point3D::new(-3.5, 0.0, 0.0), 1.0);
    tetrahedron.fill_color = Some(translucent);
    document.add_object(GeoObject::Tetrahedron3D(tetrahedron));

    let mut pyramid = Pyramid3DObj::new(
        Point3D::new(-2.0, -0.5, 0.0),
        Point3D::new(-2.0, 1.0, 0.0),
        1.0,
    );
    pyramid.fill_color = Some(translucent);
    document.add_object(GeoObject::Pyramid3D(pyramid));

    let mut cone = Cone3DObj::new(
        Point3D::new(1.0, -0.5, 0.0),
        Point3D::new(1.0, 1.0, 0.0),
        0.5,
    );
    cone.fill_color = Some(translucent);
    document.add_object(GeoObject::Cone3D(cone));

    let mut cylinder = Cylinder3DObj::new(
        Point3D::new(4.0, -0.5, 0.0),
        Point3D::new(4.0, 1.0, 0.0),
        0.5,
    );
    cylinder.fill_color = Some(translucent);
    document.add_object(GeoObject::Cylinder3D(cylinder));

    let mut surface = Surface3DObj::new("0", (-1.0, 1.0), (-1.0, 1.0));
    surface.solid = true;
    surface.mesh_res = 2;
    surface.color = translucent;
    document.add_object(GeoObject::Surface3D(surface));

    let mut torus = Torus3DObj::new(Point3D::new(7.0, 0.0, 0.0), 1.0, 0.25);
    torus.color = translucent;
    document.add_object(GeoObject::Torus3D(torus));

    let mut moebius = MoebiusStripObj::new(Point3D::new(10.0, 0.0, 0.0), 1.0, 0.25);
    moebius.color = translucent;
    document.add_object(GeoObject::MoebiusStrip(moebius));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.opaque_indices.is_empty());
    assert!(!mesh.wire_indices.is_empty());
    mesh.validate().expect("transparent world mesh stays valid");
}

#[test]
fn non_depth_writing_triangles_are_sorted_back_to_front() {
    let camera = depth_test_camera();
    let mut document = Document::new();
    // The near cube is much farther radially because it is off-axis. A radial
    // sort puts it before the centered far cube even though its view depth is smaller.
    let mut near = Cube3DObj::new(Point3D::new(20.0, 0.0, 2.0), 2.0);
    near.fill_color = Some(Color::new(1.0, 0.0, 0.0, 0.5));
    document.add_object(GeoObject::Cube3D(near));
    let mut far = Cube3DObj::new(Point3D::new(0.0, 0.0, -2.0), 2.0);
    far.fill_color = Some(Color::new(0.0, 0.0, 1.0, 0.5));
    document.add_object(GeoObject::Cube3D(far));

    let mesh = Renderer::build_3d_world_mesh(&document, &camera, 800.0, 600.0);
    let view = camera.view_matrix();
    let depths: Vec<_> = mesh
        .wire_indices
        .chunks_exact(3)
        .filter(|triangle| {
            triangle
                .iter()
                .all(|&index| mesh.wire_vertices[index as usize].color[3] < 0.999)
        })
        .map(|triangle| {
            let centroid = triangle.iter().fold(glam::Vec3::ZERO, |sum, &index| {
                sum + glam::Vec3::from(mesh.wire_vertices[index as usize].position)
            }) / 3.0;
            -(view * centroid.extend(1.0)).z
        })
        .collect();

    assert!(mesh.opaque_indices.is_empty());
    assert!(!depths.is_empty());
    assert!(depths.windows(2).all(|depths| depths[0] >= depths[1]));
}

#[test]
fn point_3d_is_emitted_as_depth_tested_world_geometry_at_document_xyz() {
    let position = Point3D::new(1.0, 2.0, 3.0);
    let mut document = Document::new();
    document.add_object(GeoObject::Point3D(Point3DObj::new(position)));

    let mesh = Renderer::build_3d_world_mesh(&document, &depth_test_camera(), 800.0, 600.0);

    mesh.validate()
        .expect("point billboard must be a valid mesh");
    assert_eq!(mesh.opaque_indices.len(), 6);
    assert_eq!(mesh.opaque_vertices.len(), 6);
    let center = mesh
        .opaque_vertices
        .iter()
        .fold(glam::Vec3::ZERO, |sum, vertex| {
            sum + glam::Vec3::from(vertex.position)
        })
        / mesh.opaque_vertices.len() as f32;
    assert!((center.x - position.x as f32).abs() < 1e-5);
    assert!((center.y - position.y as f32).abs() < 1e-5);
    assert!((center.z - position.z as f32).abs() < 1e-5);
}

#[test]
fn parametric_curve_world_mesh_keeps_document_y_and_z_axes() {
    let mut document = Document::new();
    document.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
        "t", "2", "3", 0.0, 1.0,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &depth_test_camera(), 800.0, 600.0);

    assert!(!mesh.wire_vertices.is_empty());
    for quad in mesh.wire_vertices.chunks_exact(4) {
        let center = quad.iter().fold(glam::Vec3::ZERO, |sum, vertex| {
            sum + glam::Vec3::from(vertex.position)
        }) / 4.0;
        assert!((center.y - 2.0).abs() < 1e-4);
        assert!((center.z - 3.0).abs() < 1e-4);
    }
}

#[test]
fn shared_vector_field_3d_sampling_uses_document_variables_and_xyz() {
    let mut field = VectorField3DObj::new("0", "a", "0");
    field.density = 3;
    field = field.with_bounds((1.0, 4.0), (10.0, 13.0), (100.0, 103.0));
    let variables = HashMap::from([("a".to_string(), 2.0)]);

    let segments = sample_vector_field_3d(&field, &variables);

    assert_eq!(segments.len(), 64);
    assert_eq!(segments[0].0, Point3D::new(1.0, 10.0, 100.0));
    assert_eq!(segments[0].1, Point3D::new(1.0, 10.4, 100.0));
}

#[test]
fn world_mesh_vector_field_3d_receives_document_variables() {
    let mut document = Document::new();
    document.variables.insert("a".to_string(), 1.0);
    let mut field = VectorField3DObj::new("a", "0", "0");
    field.density = 3;
    document.add_object(GeoObject::VectorField3D(field));

    let mesh = Renderer::build_3d_world_mesh(&document, &depth_test_camera(), 800.0, 600.0);

    assert!(!mesh.wire_vertices.is_empty());
    mesh.validate()
        .expect("variable-aware field mesh must be valid");
}

#[test]
fn shared_phase_portrait_sampling_receives_document_variables() {
    let mut portrait = PhasePortraitObj::new("a", "0", 0.0, 5.0, 0.0, 5.0);
    portrait.density = 5;
    let variables = HashMap::from([("a".to_string(), 2.0)]);

    let segments = sample_phase_portrait(&portrait, &variables);

    assert_eq!(segments.len(), 36);
    assert_eq!(segments[0].0, Point2::new(0.0, 0.0));
    assert_eq!(segments[0].1, Point2::new(0.5, 0.0));
}

#[test]
fn static_phase_portrait_geometry_receives_document_variables() {
    let mut document = Document::new();
    document.set_view(view_800x600());
    document.variables.insert("a".to_string(), 1.0);
    document.add_object(GeoObject::PhasePortrait(PhasePortraitObj::new(
        "a", "0", -2.0, 2.0, -2.0, 2.0,
    )));

    let (vertices, indices) =
        Renderer::build_geometry_static(&document, &view_800x600(), false, false);

    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn world_mesh_keeps_finite_geometry_when_the_camera_targets_far_coordinates() {
    let mut document = Document::new();
    document.add_object(GeoObject::Cube3D(Cube3DObj::new(
        Point3D::new(2_000.0, 0.0, 0.0),
        2.0,
    )));
    let mut camera = Camera3D::new(4.0 / 3.0);
    camera.target = glam::Vec3::new(2_000.0, 0.0, 0.0);

    let mesh = Renderer::build_3d_world_mesh(&document, &camera, 800.0, 600.0);

    assert!(mesh.validate().is_ok());
    assert!(!mesh.wire_indices.is_empty());
}

#[test]
fn every_recognized_complex_mapping_target_emits_gpu_geometry() {
    let view = view_800x600();
    let targets = vec![
        (
            "point",
            GeoObject::Point(PointObj::new(Point2::new(2.0, 0.0))),
        ),
        (
            "line",
            GeoObject::Line(LineObj::new(Point2::new(2.0, 0.0), Point2::new(3.0, 0.0))),
        ),
        (
            "circle",
            GeoObject::Circle(CircleObj::new(Point2::new(3.0, 0.0), 0.5)),
        ),
        (
            "polygon",
            GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(2.0, 0.5),
                Point2::new(3.0, 0.5),
                Point2::new(2.5, 1.5),
            ])),
        ),
        (
            "pencil",
            GeoObject::Pencil(PencilObj::new(vec![
                Point2::new(2.0, 0.0),
                Point2::new(2.5, 1.0),
                Point2::new(3.0, 0.0),
            ])),
        ),
        ("function", GeoObject::Function(FunctionObj::new("2"))),
        (
            "implicit curve",
            GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                "x^2 + y^2",
                "4",
                RelationOperator::Eq,
            )),
        ),
        (
            "parametric curve",
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                "2 + cos(t)",
                "sin(t)",
                0.0,
                std::f64::consts::TAU,
            )),
        ),
        (
            "polar curve",
            GeoObject::PolarCurve(PolarCurveObj::new("2", 0.0, std::f64::consts::TAU)),
        ),
        (
            "ellipse",
            GeoObject::Ellipse(EllipseObj::new(Point2::new(3.0, 0.0), 1.0, 0.5)),
        ),
        (
            "parabola",
            GeoObject::Parabola(ParabolaObj::new(Point2::new(2.0, 0.0), 1.0)),
        ),
        (
            "hyperbola",
            GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(3.0, 0.0), 1.0, 0.5)),
        ),
        (
            "regression line",
            GeoObject::RegressionLine(RegressionLineObj::linear(
                vec![2.0, 3.0],
                vec![1.0, 1.0],
                0.0,
                1.0,
                1.0,
            )),
        ),
        (
            "vector field",
            GeoObject::VectorField2D(VectorField2DObj::new("1", "0")),
        ),
    ];

    for (name, target) in targets {
        let mut document = Document::new();
        let target_id = document.add_object(target);
        document.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
            "1/z", target_id,
        )));

        let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);
        assert!(
            !vertices.is_empty() && !indices.is_empty(),
            "recognized ComplexMapping target '{name}' must emit geometry"
        );
    }
}

#[test]
fn test_renderer_builds_geometry_for_function() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::Function(FunctionObj::new("sin(x)")));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(!vertices.is_empty(), "function should produce vertices");
    assert!(!indices.is_empty(), "function should produce indices");
}

#[test]
fn domain_coloring_emits_geometry_without_grid_or_axis_overlays() {
    let view = view_800x600();
    let mut document = Document::new();
    document.set_view(view);
    document.add_object(GeoObject::ComplexGrid(
        ComplexGridObj::new("1/z", -2.0, 2.0, -2.0, 2.0).as_domain_coloring(),
    ));

    let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);

    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn transformed_complex_grid_emits_geometry_without_grid_or_axis_overlays() {
    let view = view_800x600();
    let mut document = Document::new();
    document.set_view(view);
    document.add_object(GeoObject::ComplexGrid(ComplexGridObj::new(
        "1/z", -2.0, 2.0, -2.0, 2.0,
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);

    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn transformed_object_cpu_fallback_applies_the_persisted_complex_map() {
    let view = view_800x600();
    let line = LineObj::new(Point2::new(-1.0, 0.0), Point2::new(1.0, 0.0));
    let mut source = Document::new();
    source.add_object(GeoObject::Line(line.clone()));
    let (source_vertices, source_indices) =
        Renderer::build_geometry_static(&source, &view, false, false);

    let transformed = TransformedObj::new(GeoObject::Line(line), "z + 2");
    let mut document = Document::new();
    document.add_object(GeoObject::Transformed(transformed.clone()));
    let (transformed_vertices, transformed_indices) =
        Renderer::build_transformed_geometry_static(&document, &transformed, &view, false);

    assert_eq!(transformed_indices, source_indices);
    assert_eq!(transformed_vertices.len(), source_vertices.len());
    for (source, transformed) in source_vertices.iter().zip(&transformed_vertices) {
        assert!(
            (transformed.position[0] - source.position[0] - 2.0 * view.scale as f32).abs() < 1e-3
        );
        assert!((transformed.position[1] - source.position[1]).abs() < 1e-3);
    }
}

#[test]
fn transformed_object_cpu_fallback_supports_cpu_only_complex_opcodes() {
    let view = view_800x600();
    let point = GeoObject::Point(PointObj::new(Point2::new(3.0, 0.0)));
    let transformed = TransformedObj::new(point.clone(), "gamma(z)");
    let mut document = Document::new();
    document.add_object(GeoObject::Transformed(transformed.clone()));

    let (vertices, indices) =
        Renderer::build_transformed_geometry_static(&document, &transformed, &view, false);
    let identity = TransformedObj::new(point, "z");
    let (source_vertices, _) =
        Renderer::build_transformed_geometry_static(&document, &identity, &view, false);
    let gamma = grafito_complex::math::complex_expr::parse("gamma(z)").unwrap();

    assert!(!indices.is_empty());
    assert_eq!(vertices.len(), source_vertices.len());
    for (source, transformed) in source_vertices.iter().zip(&vertices) {
        let source_world =
            view.screen_to_world(glam::Vec2::new(source.position[0], source.position[1]));
        let expected = gamma
            .eval(&HashMap::from([(
                "z".to_string(),
                num_complex::Complex64::new(source_world.x, source_world.y),
            )]))
            .unwrap();
        let expected_screen = view.world_to_screen(Point2::new(expected.re, expected.im));
        assert!((transformed.position[0] - expected_screen.x).abs() < 1e-3);
        assert!((transformed.position[1] - expected_screen.y).abs() < 1e-3);
    }
}

#[test]
fn static_geometry_renders_a_million_constant_function_when_it_is_in_view() {
    let mut doc = Document::new();
    let mut view = view_800x600();
    view.offset.y = 1_000_000.0 * view.scale;
    doc.set_view(view);
    doc.add_object(GeoObject::Function(FunctionObj::new("1000000")));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view, false, false);

    assert!(
        !vertices.is_empty(),
        "a finite constant in the visible view should produce geometry"
    );
    assert!(!indices.is_empty());
    assert!(vertices.iter().all(|vertex| vertex
        .position
        .iter()
        .all(|coordinate| coordinate.is_finite())));
}

#[test]
fn extreme_finite_2d_primitives_do_not_emit_nonfinite_gpu_vertices() {
    let mut document = Document::new();
    document.set_view(view_800x600());
    document.add_object(GeoObject::Circle(CircleObj::new(
        Point2::new(f64::MAX, f64::MAX),
        f64::MAX,
    )));

    let (vertices, _) = Renderer::build_geometry_static(&document, &view_800x600(), false, false);

    assert!(vertices.iter().all(|vertex| {
        vertex
            .position
            .iter()
            .all(|coordinate| coordinate.is_finite())
    }));
}

#[test]
fn test_renderer_builds_geometry_for_parametric_curve() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
        "cos(t)",
        "sin(t)",
        0.0,
        std::f64::consts::TAU,
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "parametric curve should produce vertices"
    );
    assert!(
        !indices.is_empty(),
        "parametric curve should produce indices"
    );
}

#[test]
fn test_renderer_builds_geometry_for_surface() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Surface3D(Surface3DObj::new(
        "x^2 + y^2",
        (-1.0, 1.0),
        (-1.0, 1.0),
    )));

    let camera = Camera3D::new(1.6);
    let (empty_vertices, empty_indices) =
        Renderer::build_3d_geometry_static(&Document::new(), &camera, false, 800.0, 600.0);
    let (vertices, indices) =
        Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
    assert!(
        vertices.len() > empty_vertices.len(),
        "surface should produce object vertices"
    );
    assert!(
        indices.len() > empty_indices.len(),
        "surface should produce object indices"
    );
}

#[test]
fn test_renderer_builds_geometry_for_parametric_surface_3d() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Surface3D(Surface3DObj::new_parametric(
        "u*cos(v)",
        "u*sin(v)",
        "v",
        (0.0, 1.0),
        (0.0, std::f64::consts::TAU),
    )));

    let camera = Camera3D::new(1.6);
    let (empty_vertices, _) =
        Renderer::build_3d_geometry_static(&Document::new(), &camera, false, 800.0, 600.0);
    let (vertices, indices) =
        Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
    assert!(vertices.len() > empty_vertices.len());
    assert!(!indices.is_empty());
}

#[test]
fn parametric_heart_surface_emits_a_valid_world_mesh() {
    let mut document = Document::new();
    document.add_object(GeoObject::Surface3D(Surface3DObj::new_parametric(
        "(1 - sin(u)) * cos(u) * v",
        "(1 - sin(u)) * sin(u) * v",
        "(1 - cos(u)) * (1 - v / 2) - 0.5",
        (0.0, std::f64::consts::TAU),
        (0.0, 1.0),
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    assert!(mesh.validate().is_ok());
    assert!(!mesh.wire_indices.is_empty());
}

fn depth_test_camera() -> Camera3D {
    Camera3D {
        theta: std::f32::consts::FRAC_PI_2,
        phi: 0.0,
        distance: 10.0,
        target: glam::Vec3::ZERO,
        fov: 60.0,
        near: 0.1,
        far: 100.0,
        aspect: 4.0 / 3.0,
    }
}

fn frontmost_center_color(mesh: &grafito_render::WorldMesh, camera: &Camera3D) -> [f32; 4] {
    let mvp = camera.mvp();
    mesh.opaque_indices
        .chunks_exact(3)
        .filter_map(|triangle| {
            let vertices = [
                mesh.opaque_vertices[triangle[0] as usize],
                mesh.opaque_vertices[triangle[1] as usize],
                mesh.opaque_vertices[triangle[2] as usize],
            ];
            let clip = vertices.map(|vertex| mvp * glam::Vec3::from(vertex.position).extend(1.0));
            let ndc = clip.map(|point| point.truncate() / point.w);
            let area = (ndc[1].x - ndc[0].x) * (ndc[2].y - ndc[0].y)
                - (ndc[1].y - ndc[0].y) * (ndc[2].x - ndc[0].x);
            if area.abs() < 1e-6 {
                return None;
            }
            let weights = [
                (ndc[1].x * ndc[2].y - ndc[1].y * ndc[2].x) / area,
                (ndc[2].x * ndc[0].y - ndc[2].y * ndc[0].x) / area,
                (ndc[0].x * ndc[1].y - ndc[0].y * ndc[1].x) / area,
            ];
            weights.iter().all(|weight| *weight >= -1e-5).then_some((
                weights
                    .iter()
                    .zip(ndc)
                    .map(|(weight, point)| weight * point.z)
                    .sum::<f32>(),
                vertices[0].color,
            ))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("overlapping cubes should cover the camera target")
        .1
}

#[test]
fn world_mesh_depth_result_is_independent_of_opaque_object_order() {
    let camera = depth_test_camera();
    let mut near = Cube3DObj::new(Point3D::new(0.0, 0.0, 2.0), 3.0);
    near.fill_color = Some(Color::RED);
    let mut far = Cube3DObj::new(Point3D::new(0.0, 0.0, -1.0), 3.0);
    far.fill_color = Some(Color::BLUE);

    let mut front_then_back = Document::new();
    front_then_back.add_object(GeoObject::Cube3D(near.clone()));
    front_then_back.add_object(GeoObject::Cube3D(far.clone()));

    let mut back_then_front = Document::new();
    back_then_front.add_object(GeoObject::Cube3D(far));
    back_then_front.add_object(GeoObject::Cube3D(near));

    let first = Renderer::build_3d_world_mesh(&front_then_back, &camera, 800.0, 600.0);
    let second = Renderer::build_3d_world_mesh(&back_then_front, &camera, 800.0, 600.0);

    first.validate().expect("first world mesh must be valid");
    second.validate().expect("second world mesh must be valid");
    let first_color = frontmost_center_color(&first, &camera);
    let second_color = frontmost_center_color(&second, &camera);
    assert!(first_color[0] > first_color[2]);
    assert!(second_color[0] > second_color[2]);
}

#[test]
fn solid_surface_emits_world_space_triangles() {
    let mut doc = Document::new();
    let mut surface = Surface3DObj::new("x^2 + y^2", (-1.0, 1.0), (-1.0, 1.0));
    surface.solid = true;
    surface.mesh_res = 2;
    doc.add_object(GeoObject::Surface3D(surface));

    let mesh = Renderer::build_3d_world_mesh(&doc, &depth_test_camera(), 800.0, 600.0);
    mesh.validate().expect("solid surface mesh must be valid");
    assert!(mesh.opaque_indices.len() >= 6);
    assert!(mesh
        .opaque_vertices
        .iter()
        .any(|vertex| vertex.position[2].abs() > 1e-4));
}

#[test]
fn world_mesh_validation_rejects_invalid_triangle_indices() {
    let mut mesh = grafito_render::WorldMesh::default();
    mesh.opaque_vertices = vec![grafito_render::Vertex3D {
        position: [0.0, 0.0, 0.0],
        color: Color::WHITE.to_array(),
    }];
    mesh.opaque_indices = vec![0, 1, 2];

    assert_eq!(
        mesh.validate(),
        Err("3D mesh index is outside its vertex stream")
    );
}

#[test]
fn world_mesh_leaves_near_plane_clipping_to_the_gpu() {
    let camera = depth_test_camera();
    let mut doc = Document::new();
    doc.add_object(GeoObject::Segment3D(Segment3DObj::new(
        Point3D::new(0.0, 0.0, 9.95),
        Point3D::new(0.0, 0.0, 0.0),
    )));

    let mesh = Renderer::build_3d_world_mesh(&doc, &camera, 800.0, 600.0);
    mesh.validate()
        .expect("near-plane-crossing mesh must be valid");
    let clip_w: Vec<_> = mesh
        .wire_vertices
        .iter()
        .map(|vertex| (camera.mvp() * glam::Vec3::from(vertex.position).extend(1.0)).w)
        .collect();
    assert!(clip_w.iter().any(|w| *w < camera.near));
    assert!(clip_w.iter().any(|w| *w > camera.near));
}

#[test]
fn world_mesh_enforces_aggregate_opaque_and_wire_budgets() {
    let mut document = Document::new();
    for _ in 0..6 {
        let mut surface = Surface3DObj::new("0", (-1.0, 1.0), (-1.0, 1.0));
        surface.solid = true;
        surface.mesh_res = 128;
        document.add_object(GeoObject::Surface3D(surface));
    }

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("a budget-truncated world mesh stays valid");
    assert!(
        !mesh.is_complete(),
        "GPU rendering must fall back when aggregate output omits visible objects"
    );
    assert!(mesh.opaque_vertices.len() <= 524_288);
    assert!(mesh.opaque_indices.len() <= 524_288);
    assert!(mesh.wire_vertices.len() <= 524_288);
    assert!(mesh.wire_indices.len() <= 786_432);
}

#[test]
fn world_mesh_skips_later_surfaces_once_the_work_budget_is_spent() {
    let mut document = Document::new();
    for _ in 0..5 {
        let mut surface = Surface3DObj::new("0", (-1.0, 1.0), (-1.0, 1.0));
        surface.solid = true;
        surface.mesh_res = 128;
        document.add_object(GeoObject::Surface3D(surface));
    }

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("work-budget truncation must keep a valid world mesh");
    assert!(
        !mesh.is_complete(),
        "GPU rendering must fall back when work limits omit visible surfaces"
    );
    let sampled_surfaces = document
        .objects_iter()
        .filter_map(|(_, object)| match object {
            GeoObject::Surface3D(surface) => Some(!surface.cached_grid.read().unwrap().is_empty()),
            _ => None,
        })
        .filter(|sampled| *sampled)
        .count();
    assert!(
        sampled_surfaces < 5,
        "later expensive surfaces must not be evaluated and cached after the world-mesh work budget is spent"
    );
}

#[test]
fn world_mesh_limits_attractor_count_and_steps() {
    let mut document = Document::new();
    for _ in 0..9 {
        let mut attractor = Attractor3DObj::new("lorenz", vec![10.0, 28.0, 8.0 / 3.0]);
        attractor.steps = 2;
        attractor.skip = 0;
        document.add_object(GeoObject::Attractor3D(attractor));
    }

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("attractor count truncation must keep indices valid");
    assert!(mesh.wire_vertices.len() <= 8 * 4);
    assert!(
        !mesh.is_complete(),
        "the GPU mesh must report omitted visible attractors so the app keeps its CPU fallback"
    );

    let mut one_attractor = Document::new();
    let mut attractor = Attractor3DObj::new("lorenz", vec![10.0, 28.0, 8.0 / 3.0]);
    attractor.steps = 20_000;
    attractor.skip = 0;
    one_attractor.add_object(GeoObject::Attractor3D(attractor));
    let mesh =
        Renderer::build_3d_world_mesh(&one_attractor, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("attractor step truncation must keep indices valid");
    assert!(mesh.wire_vertices.len() <= (16_000 - 1) * 4);
}

#[test]
fn world_mesh_breaks_parametric_curve_at_a_nonfinite_sample() {
    let mut document = Document::new();
    document.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
        "1 / (t - 0.5)",
        "0",
        "0",
        0.0,
        1.0,
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("curve mesh must remain valid around a pole");
    for quad in mesh.wire_vertices.chunks_exact(4) {
        let start_x = (quad[0].position[0] + quad[1].position[0]) * 0.5;
        let end_x = (quad[2].position[0] + quad[3].position[0]) * 0.5;
        assert!(
            start_x.signum() == end_x.signum() || start_x == 0.0 || end_x == 0.0,
            "a wire quad bridged the discontinuity from {start_x} to {end_x}"
        );
    }
}

#[test]
fn world_mesh_omits_extreme_finite_segments_without_dropping_normal_geometry() {
    let mut document = Document::new();
    document.add_object(GeoObject::Cube3D(Cube3DObj::new(
        Point3D::new(0.0, 0.0, 0.0),
        2.0,
    )));
    document.add_object(GeoObject::Segment3D(Segment3DObj::new(
        Point3D::new(1.0e30, 0.0, 0.0),
        Point3D::new(1.0e30, 1.0, 0.0),
    )));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(4.0 / 3.0), 800.0, 600.0);

    mesh.validate()
        .expect("one extreme segment must not poison the mesh");
    assert!(
        !mesh.wire_vertices.is_empty(),
        "the ordinary cube must remain visible"
    );
    assert!(mesh
        .wire_vertices
        .iter()
        .all(|vertex| vertex.position.iter().all(|value| value.abs() <= 1.0e12)));
}

#[test]
fn test_renderer_builds_geometry_for_attractor_3d() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Attractor3D(
        Attractor3DObj::new("lorenz", vec![10.0, 28.0, 8.0 / 3.0]).with_steps(1000, 10),
    ));

    let camera = Camera3D::new(1.6);
    let (empty_vertices, _) =
        Renderer::build_3d_geometry_static(&Document::new(), &camera, false, 800.0, 600.0);
    let (vertices, indices) =
        Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
    assert!(vertices.len() > empty_vertices.len());
    assert!(!indices.is_empty());
}

#[test]
fn test_renderer_builds_geometry_for_vector_field() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::VectorField2D(VectorField2DObj::new("x", "y")));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(!vertices.is_empty(), "vector field should produce vertices");
    assert!(!indices.is_empty(), "vector field should produce indices");
}

#[test]
fn weak_nonzero_vector_field_stays_out_of_gpu_base_geometry() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::VectorField2D(VectorField2DObj::new(
        "0.0001", "0",
    )));

    let (static_vertices, static_indices) =
        Renderer::build_geometry_static(&doc, &view_800x600(), false, false);
    assert!(
        !static_vertices.is_empty(),
        "finite vectors above the CPU epsilon should emit static geometry"
    );
    assert!(!static_indices.is_empty());

    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        Some(Renderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            1,
        ))
    });
    let Some(renderer) = result else {
        return;
    };

    let (gpu_vertices, gpu_indices) = renderer.build_geometry(&doc, false, false, None, None);
    assert!(
        gpu_vertices.is_empty(),
        "GPU geometry must leave vector-field shafts to the CPU overlay"
    );
    assert!(
        gpu_indices.is_empty(),
        "GPU geometry must not duplicate CPU vector-field arrows"
    );
}

#[test]
fn depth_render_target_uses_msaa_and_a_resolve_texture_when_supported() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8UnormSrgb);
        let sample_count = if features.flags.sample_count_supported(4)
            && features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        {
            4
        } else {
            1
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        let renderer = Renderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            sample_count,
        );
        let target = renderer.create_depth_render_target(&device, 320, 180);
        Some((target, sample_count))
    });
    let Some((target, sample_count)) = result else {
        return;
    };

    assert!(target.matches_size(320, 180));
    assert_eq!(target.sample_count(), sample_count);
    if sample_count > 1 {
        assert!(target.resolve_target().is_some());
    } else {
        assert!(target.resolve_target().is_none());
    }
}

#[test]
fn test_renderer_builds_geometry_for_boolean_polygon() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    // A non-convex polygon similar to a boolean-union result.
    let poly = PolygonObj::new(vec![
        Point2::new(-1.0, -1.0),
        Point2::new(2.0, -1.0),
        Point2::new(2.0, 1.0),
        Point2::new(0.5, 1.0),
        Point2::new(0.5, 0.0),
        Point2::new(-0.5, 0.0),
        Point2::new(-0.5, 1.0),
        Point2::new(-1.0, 1.0),
    ]);
    doc.add_object(GeoObject::Polygon(poly));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "boolean polygon should produce vertices"
    );
    assert!(
        !indices.is_empty(),
        "boolean polygon should produce indices"
    );
}

#[test]
fn concave_polygon_fill_never_emits_triangles_outside_the_polygon() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let fill = Color::new(0.13, 0.47, 0.83, 0.91);
    let mut poly = PolygonObj::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(3.0, 0.0),
        Point2::new(3.0, 3.0),
        Point2::new(2.0, 3.0),
        Point2::new(2.0, 1.0),
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 3.0),
        Point2::new(0.0, 3.0),
    ]);
    poly.fill_color = Some(fill);
    poly.color = Color::BLACK;
    doc.add_object(GeoObject::Polygon(poly));

    let view = view_800x600();
    let polygon: Vec<_> = [
        (0.0, 0.0),
        (3.0, 0.0),
        (3.0, 3.0),
        (2.0, 3.0),
        (2.0, 1.0),
        (1.0, 1.0),
        (1.0, 3.0),
        (0.0, 3.0),
    ]
    .into_iter()
    .map(|(x, y)| view.world_to_screen(Point2::new(x, y)))
    .collect();
    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view, false, false);

    let fill_triangles: Vec<_> = indices
        .chunks_exact(3)
        .filter(|triangle| {
            triangle
                .iter()
                .all(|&index| vertices[index as usize].color == fill.to_array())
        })
        .collect();
    assert!(
        !fill_triangles.is_empty(),
        "concave polygon should retain its fill"
    );

    for triangle in fill_triangles {
        let centroid = triangle.iter().fold(glam::Vec2::ZERO, |sum, &index| {
            let position = vertices[index as usize].position;
            sum + glam::Vec2::new(position[0], position[1])
        }) / 3.0;
        assert!(
            point_is_inside_polygon(centroid, &polygon),
            "fill triangle centroid {:?} escaped the concave polygon",
            centroid
        );
    }
}

fn point_is_inside_polygon(point: glam::Vec2, polygon: &[glam::Vec2]) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let crosses = (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x;
        if crosses {
            inside = !inside;
        }
    }
    inside
}

#[test]
fn gpu_renderer_does_not_allocate_the_unused_fill_pipeline() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        Some(Renderer::new(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            1,
        ))
    });

    let Some(renderer) = result else {
        return;
    };
    assert!(renderer.fill_compute.is_none());
}

#[test]
fn gpu_domain_coloring_preserves_row_major_cell_coordinates() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        Some((device, queue))
    });
    let Some((device, queue)) = result else {
        return;
    };

    let renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let mut grid = ComplexGridObj::new("z", -2.0, 2.0, -2.0, 2.0).as_domain_coloring();
    grid.density = 50;
    let resolution = grid.density.clamp(50, 500);
    let dx = (grid.x_max - grid.x_min) / resolution as f64;
    let dy = (grid.y_max - grid.y_min) / resolution as f64;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    renderer.add_complex_grid_geometry_gpu(
        &mut vertices,
        &mut indices,
        &doc,
        &view_800x600(),
        &grid,
        Some(&device),
        Some(&queue),
    );

    let second_cell = &vertices[4..8];
    let center = second_cell.iter().fold(glam::Vec2::ZERO, |sum, vertex| {
        sum + glam::Vec2::new(vertex.position[0], vertex.position[1])
    }) / 4.0;
    let expected =
        view_800x600().world_to_screen(Point2::new(grid.x_min + dx * 0.5, grid.y_min + dy * 1.5));
    assert!(center.abs_diff_eq(expected, 1e-3));
}

#[test]
fn gpu_complex_transform_reads_the_second_constant_pair() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        let expr = grafito_complex::math::complex_expr::parse("z + 1 + 2").ok()?;
        let compute = grafito_render::complex_compute::ComplexComputePipeline::new(&device, &queue);
        let result = compute.evaluate(
            &device,
            &queue,
            &expr,
            &[Point2::new(1.0, 0.0)],
            &HashMap::new(),
        )?;
        Some(result)
    });

    let Some(points) = result else {
        return;
    };
    assert_eq!(points.len(), 1);
    assert!((points[0].x - 4.0).abs() <= 1e-5);
    assert!(points[0].y.abs() <= 1e-5);
}

#[test]
fn gpu_domain_coloring_falls_back_for_cpu_only_complex_functions() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        Some((device, queue))
    });
    let Some((device, queue)) = result else {
        return;
    };

    let renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let mut grid = ComplexGridObj::new("gamma(z)", -2.0, 2.0, -2.0, 2.0).as_domain_coloring();
    grid.density = 50;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    renderer.add_complex_grid_geometry_gpu(
        &mut vertices,
        &mut indices,
        &doc,
        &view_800x600(),
        &grid,
        Some(&device),
        Some(&queue),
    );

    assert!(!vertices.is_empty());
    assert!(vertices
        .iter()
        .any(|vertex| vertex.color != [0.0, 0.0, 0.0, 1.0]));
}

#[test]
fn gpu_complex_transform_falls_back_for_cpu_only_opcodes() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        let expr = grafito_complex::math::complex_expr::ComplexExpr::Gamma(Box::new(
            grafito_complex::math::complex_expr::ComplexExpr::Var("z".to_string()),
        ));
        let compute = grafito_render::complex_compute::ComplexComputePipeline::new(&device, &queue);
        Some(compute.evaluate(
            &device,
            &queue,
            &expr,
            &[Point2::new(1.0, 0.0)],
            &HashMap::new(),
        ))
    });

    let Some(output) = result else {
        return;
    };
    assert!(
        output.is_none(),
        "CPU-only complex functions must not dispatch GPU NaNs"
    );
}

#[test]
fn test_gpu_function_no_stale_bytecode() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        let compute =
            grafito_render::function_compute::FunctionComputePipeline::new(&device, &queue, 10000);
        let variables = HashMap::new();
        let domain = (-std::f64::consts::PI, std::f64::consts::PI);
        let grid_size = 100;

        // Evaluate x^2 first to leave a longer bytecode in the shared buffer.
        let _ = compute.evaluate_expr(&device, &queue, "x^2", domain, grid_size, &variables)?;

        // Immediately evaluate sin(x) on the same pipeline. With the stale-bytecode
        // bug, leftover opcodes from x^2 would corrupt the stack and produce NaN
        // or out-of-range values.
        let ys = compute.evaluate_expr(&device, &queue, "sin(x)", domain, grid_size, &variables)?;
        Some(ys)
    });

    let Some(ys) = result else {
        // No GPU adapter available; skip this test.
        return;
    };

    assert!(!ys.is_empty(), "sin(x) should produce samples");
    for y in ys {
        assert!(y.is_finite(), "sin(x) produced non-finite value {}", y);
        assert!(y.abs() <= 1.0 + 1e-6, "sin(x) = {} is outside [-1, 1]", y);
    }
}

#[test]
fn gpu_function_cache_keeps_a_finite_million_constant() {
    let _gpu = gpu_test_guard();
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;
        let compute =
            grafito_render::function_compute::FunctionComputePipeline::new(&device, &queue, 10000);
        let function = FunctionObj::new("1000000");
        let populated = grafito_render::function_compute::maybe_compute_function_on_gpu(
            &compute,
            &device,
            &queue,
            &function,
            (-1.0, 1.0),
            32,
            &HashMap::new(),
        );
        let samples = function.cached_samples.read().ok()?.clone();
        Some((populated, samples))
    });

    let Some((populated, samples)) = result else {
        return;
    };

    assert!(populated);
    assert!(samples.iter().all(|(_, y)| {
        y.is_some_and(|value| value.is_finite() && (value - 1_000_000.0).abs() <= 1e-3)
    }));
}

#[test]
fn test_renderer_builds_geometry_for_implicit_curve() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
        "x^3 + y^3",
        "3*x*y",
        RelationOperator::Eq,
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "implicit curve should produce vertices"
    );
    assert!(!indices.is_empty(), "implicit curve should produce indices");
}

#[test]
fn fractal_geometry_uses_custom_julia_params_and_max_iter() {
    let mut default_doc = Document::new();
    default_doc.set_view(view_800x600());
    default_doc.add_object(GeoObject::Fractal2D(Fractal2DObj::julia(-0.70176, -0.3842)));

    let cr = -0.4;
    let ci = 0.6;
    let max_iter = 17;
    let custom = Fractal2DObj::julia(cr, ci).with_max_iter(max_iter);
    let mut custom_doc = Document::new();
    custom_doc.set_view(view_800x600());
    custom_doc.add_object(GeoObject::Fractal2D(custom.clone()));

    let (default_vertices, _) =
        Renderer::build_geometry_static(&default_doc, &view_800x600(), false, false);
    let (custom_vertices, _) =
        Renderer::build_geometry_static(&custom_doc, &view_800x600(), false, false);

    let expected = grafito_geometry::fractals::compute_fractal(
        &FractalType::Julia { cr, ci, max_iter },
        custom.x_min,
        custom.x_max,
        custom.y_min,
        custom.y_max,
        200,
        200,
    );

    assert_eq!(custom_vertices.len(), expected.len() * 4);
    assert!(custom_vertices
        .chunks_exact(4)
        .zip(expected)
        .all(|(quad, pixel)| {
            let (r, g, b, a) = grafito_geometry::fractals::fractal_color_hsv(
                pixel.iter,
                pixel.max_iter,
                pixel.smooth_value,
            );
            quad.iter().all(|vertex| vertex.color == [r, g, b, a])
        }));
    assert!(default_vertices
        .iter()
        .zip(&custom_vertices)
        .any(|(default, custom)| default.color != custom.color));
}

#[test]
fn fractal_geometry_uses_the_persisted_resolution_under_the_shared_budget() {
    let resolution = 8;
    let fractal = Fractal2DObj::mandelbrot()
        .with_resolution(resolution)
        .with_max_iter(grafito_geometry::fractals::MAX_FRACTAL_ITER);
    let mut document = Document::new();
    document.set_view(view_800x600());
    document.add_object(GeoObject::Fractal2D(fractal));

    let (vertices, indices) =
        Renderer::build_geometry_static(&document, &view_800x600(), false, false);

    assert_eq!(vertices.len(), resolution * resolution * 4);
    assert_eq!(indices.len(), resolution * resolution * 6);
}

#[test]
fn fractal_geometry_stops_before_the_shared_vertex_budget_is_exhausted() {
    let mut document = Document::new();
    document.set_view(view_800x600());
    for _ in 0..2 {
        document.add_object(GeoObject::Fractal2D(
            Fractal2DObj::mandelbrot()
                .with_resolution(400)
                .with_max_iter(1),
        ));
    }

    let (vertices, indices) =
        Renderer::build_geometry_static(&document, &view_800x600(), false, false);

    assert_eq!(vertices.len(), 400 * 400 * 4);
    assert_eq!(indices.len(), 400 * 400 * 6);
}

#[test]
fn test_renderer_builds_geometry_for_attractor_in_2d() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::Attractor3D(Attractor3DObj::new(
        "lorenz",
        vec![10.0, 28.0, 8.0 / 3.0],
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "attractor should produce vertices in 2D view"
    );
    assert!(
        !indices.is_empty(),
        "attractor should produce indices in 2D view"
    );
}

#[test]
fn test_renderer_builds_geometry_for_integral_function() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    // ∫₀ˣ t² dt = x³/3
    let fun = FunctionObj::new("x^2").as_integral("x", 0.0);
    doc.add_object(GeoObject::Function(fun));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "integral function should produce vertices"
    );
    assert!(
        !indices.is_empty(),
        "integral function should produce indices"
    );
}

#[test]
fn test_renderer_builds_geometry_for_piecewise_function() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::Function(FunctionObj::new(
        "piecewise(x<0, x^2, x>=0, sqrt(x))",
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, true);
    assert!(
        !vertices.is_empty(),
        "piecewise function should produce vertices"
    );
    assert!(
        !indices.is_empty(),
        "piecewise function should produce indices"
    );
}

#[test]
fn complex_grid_line_mode_uses_grid_color_not_domain_coloring() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let mut grid = ComplexGridObj::new("z^2", -2.0, 2.0, -2.0, 2.0);
    grid.density = 4;
    grid.color = grafito_geometry::Color::BLUE;
    doc.add_object(GeoObject::ComplexGrid(grid));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);
    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
    assert!(vertices
        .iter()
        .all(|v| v.color == grafito_geometry::Color::BLUE.to_array()));
}

#[test]
fn complex_grid_static_geometry_uses_custom_complex_symbol() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.complex_base_symbol = "w".to_string();
    let grid = ComplexGridObj::new("w", -2.0, 2.0, -2.0, 2.0).as_domain_coloring();
    doc.add_object(GeoObject::ComplexGrid(grid));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);
    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn complex_grid_preview_quality_caps_domain_coloring_geometry() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.render_quality = RenderQuality::Preview;
    let mut grid = ComplexGridObj::new("z", -2.0, 2.0, -2.0, 2.0).as_domain_coloring();
    grid.density = 500;
    doc.add_object(GeoObject::ComplexGrid(grid));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);
    assert_eq!(vertices.len(), 64 * 64 * 4);
    assert_eq!(indices.len(), 64 * 64 * 6);
}

#[test]
fn complex_grid_preview_quality_caps_line_mode_geometry() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.render_quality = RenderQuality::Preview;
    let mut grid = ComplexGridObj::new("z", -4.0, 4.0, -4.0, 4.0);
    grid.density = 128;
    doc.add_object(GeoObject::ComplexGrid(grid));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);

    assert!(
        vertices.len() <= 33 * 2 * 32 * 4,
        "Preview line grid should stay small enough for pan/zoom (vertices={})",
        vertices.len()
    );
    assert!(
        indices.len() <= 33 * 2 * 32 * 6,
        "Preview line grid should stay small enough for pan/zoom (indices={})",
        indices.len()
    );
}

#[test]
fn complex_mapping_implicit_target_builds_geometry_without_preseeded_cache() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let target = doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less).with_label("I"),
    ));
    doc.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "1/z", target,
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);
    assert!(
        !vertices.is_empty(),
        "ComplexMapping[1/z, I] should produce contour geometry without a prior GPU cache pass"
    );
    assert!(!indices.is_empty());
}

#[test]
fn complex_mapping_preview_quality_reduces_transformed_geometry() {
    let mut high_doc = Document::new();
    high_doc.set_view(view_800x600());
    high_doc.render_quality = RenderQuality::High;
    let target = high_doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less).with_label("I"),
    ));
    high_doc.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "1/z", target,
    )));

    let mut preview_doc = Document::new();
    preview_doc.set_view(view_800x600());
    preview_doc.render_quality = RenderQuality::Preview;
    let target = preview_doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less).with_label("I"),
    ));
    preview_doc.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "1/z", target,
    )));

    let (high_vertices, _) =
        Renderer::build_geometry_static(&high_doc, &view_800x600(), false, false);
    let (preview_vertices, _) =
        Renderer::build_geometry_static(&preview_doc, &view_800x600(), false, false);

    assert!(!high_vertices.is_empty());
    assert!(!preview_vertices.is_empty());
    assert!(
        preview_vertices.len() < high_vertices.len(),
        "Preview should emit less geometry than High (preview={}, high={})",
        preview_vertices.len(),
        high_vertices.len()
    );
}

#[test]
fn complex_mapping_renders_after_serde_skipped_cache_is_empty() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let target = doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less).with_label("I"),
    ));
    let mut mapping = ComplexMappingObj::new("1/z", target);
    mapping.conformal_cache = None;
    doc.add_object(GeoObject::ComplexMapping(mapping));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);

    assert!(
        !vertices.is_empty(),
        "ComplexMapping should render even after serde-skipped conformal_cache is empty"
    );
    assert!(!indices.is_empty());
}

#[test]
fn complex_mapping_renders_empty_cache_with_custom_complex_symbol() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.complex_base_symbol = "w".to_string();
    let target = doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less).with_label("I"),
    ));
    let mut mapping = ComplexMappingObj::new_with_symbol("1/w", target, "w");
    mapping.conformal_cache = None;
    doc.add_object(GeoObject::ComplexMapping(mapping));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);

    assert!(
        !vertices.is_empty(),
        "ComplexMapping[1/w, I] should render after cache loss when ComplexSymbol[w] is active"
    );
    assert!(!indices.is_empty());
}

#[test]
fn complex_mapping_of_a_point_emits_a_mapped_marker() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    let target = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
    doc.add_object(GeoObject::ComplexMapping(ComplexMappingObj::new(
        "z^2", target,
    )));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false, false);

    assert!(
        !vertices.is_empty(),
        "a recognized ComplexMapping targeting a Point should draw a marker"
    );
    assert!(!indices.is_empty());
    assert!(vertices.iter().all(|vertex| vertex
        .position
        .iter()
        .all(|coordinate| coordinate.is_finite())));
}
