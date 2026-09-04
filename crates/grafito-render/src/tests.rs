#[cfg(test)]
#[allow(clippy::module_inception, clippy::approx_constant)]
mod tests {
    use grafito_core::{
        CircleObj, Document, Fractal2DObj, GeoObject, ImplicitCurveObj, LineObj, PointObj,
        PolygonObj, RelationOperator,
    };
    use grafito_geometry::{Camera3D, Point2, ViewTransform};

    #[test]
    fn test_build_geometry_empty_document() {
        let doc = Document::new();
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false, true);

        assert!(
            !vertices.is_empty(),
            "Grid and axes should produce vertices"
        );
        assert!(!indices.is_empty(), "Grid and axes should produce indices");
    }

    #[test]
    fn test_build_geometry_with_point() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false, true);

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_geometry_with_line() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false, true);

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_geometry_with_circle() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(0.0, 0.0),
            1.0,
        )));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false, true);

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_geometry_with_polygon() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
        ])));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false, true);

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_3d_geometry_empty_document() {
        let doc = Document::new();
        let camera = Camera3D::new(1.6);
        let (vertices, indices) =
            crate::Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);

        assert!(
            !vertices.is_empty(),
            "3D grid and axes should produce vertices"
        );
        assert!(
            !indices.is_empty(),
            "3D grid and axes should produce indices"
        );
    }

    #[test]
    fn test_vertex_size() {
        assert_eq!(
            std::mem::size_of::<crate::Vertex>(),
            28,
            "Vertex should be 28 bytes (3 floats position + 4 floats color)"
        );
    }

    #[test]
    fn geometry_growth_is_capped_before_indices_overflow() {
        assert!(crate::can_append_geometry(0, 0, 4, 6));
        assert!(!crate::can_append_geometry(
            crate::MAX_GEOMETRY_VERTICES,
            0,
            1,
            0
        ));
        assert!(!crate::can_append_geometry(u32::MAX as usize, 0, 1, 0));
    }

    #[test]
    fn visible_2d_objects_are_ordered_by_explicit_layer_then_object_id() {
        let mut document = Document::new();
        let first_curve = document.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
        )));
        let marker = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        let background = document.add_object(GeoObject::Fractal2D(Fractal2DObj::mandelbrot()));
        let second_curve = document.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        )));

        let ordered: Vec<_> = crate::ordered_visible_2d_objects(&document)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut curves = [first_curve, second_curve];
        curves.sort_unstable();

        assert_eq!(ordered[0], background);
        assert_eq!(&ordered[1..3], &curves);
        assert_eq!(ordered[3], marker);
    }

    #[test]
    fn second_fractal_is_a_partial_scene_when_geometry_capacity_is_exhausted() {
        let mut fractal = Fractal2DObj::mandelbrot();
        fractal.resolution = 400;
        let (vertices, indices) =
            crate::Renderer::fractal_geometry_requirements(&fractal).expect("valid fractal");

        assert!(crate::fractal_geometry_fits(0, 0, &fractal));
        assert!(!crate::fractal_geometry_fits(vertices, indices, &fractal));
    }

    #[test]
    fn homotopy_factor_advances_without_document_variables() {
        let document = Document::new();
        let start = crate::complex_mapping_homotopy_factor(true, 2.0, 0.0);
        let advanced =
            crate::complex_mapping_homotopy_factor(true, 2.0, std::f64::consts::FRAC_PI_2);

        assert_eq!(start, 1.0);
        assert_eq!(advanced, 0.0);
        assert!(!document.variables.contains_key("t_homotopy"));
    }

    #[test]
    fn polygon_geometry_has_a_per_object_vertex_limit() {
        assert!(crate::polygon_geometry_is_within_limit(3));
        assert!(crate::polygon_geometry_is_within_limit(
            crate::MAX_POLYGON_VERTICES
        ));
        assert!(!crate::polygon_geometry_is_within_limit(
            crate::MAX_POLYGON_VERTICES + 1
        ));
    }

    #[test]
    fn row_major_domain_cells_keep_x_as_the_outer_dimension() {
        assert_eq!(crate::row_major_cell_coordinates(1, 4), Some((0, 1)));
        assert_eq!(crate::row_major_cell_coordinates(4, 4), Some((1, 0)));
        assert_eq!(crate::row_major_cell_coordinates(16, 4), None);
    }

    #[test]
    fn world_mesh_keeps_world_coordinates_in_the_opaque_stream() {
        let vertex = crate::Vertex3D {
            position: [1.0, -2.0, 3.5],
            color: [0.1, 0.2, 0.3, 1.0],
        };
        let mut mesh = crate::WorldMesh::default();
        mesh.opaque_vertices = vec![vertex];
        mesh.opaque_indices = vec![0, 0, 0];
        assert_eq!(mesh.opaque_vertices[0].position, [1.0, -2.0, 3.5]);
        assert_eq!(mesh.opaque_indices, vec![0, 0, 0]);
        assert!(mesh.validate().is_ok());
    }

    #[test]
    fn fill_compute_is_only_needed_when_a_document_has_a_fillable_implicit_curve() {
        // El pipeline de fill reserva dos buffers 4096×4096 (~128 MiB), así que
        // `document_needs_fill_compute` es la puerta que decide si
        // `ensure_fill_compute_for_document` llega a crearlo. Sin implícitas
        // rellenables el campo `fill_compute` permanece `None` (128 MiB
        // ahorrados).
        let empty = Document::new();
        assert!(!crate::Renderer::document_needs_fill_compute(&empty));

        let mut eq_only = Document::new();
        eq_only.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x",
            "y",
            RelationOperator::Eq,
        )));
        assert!(
            !crate::Renderer::document_needs_fill_compute(&eq_only),
            "Eq es solo contorno — nunca necesita el pipeline de fill"
        );

        let mut fillable = Document::new();
        fillable.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x",
            "y",
            RelationOperator::Less,
        )));
        assert!(crate::Renderer::document_needs_fill_compute(&fillable));

        let mut greater_eq = Document::new();
        greater_eq.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x",
            "y",
            RelationOperator::GreaterEq,
        )));
        assert!(crate::Renderer::document_needs_fill_compute(&greater_eq));
    }

    #[test]
    fn test_all_geo_variants_render() {
        use grafito_core::*;
        use grafito_geometry::*;
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        let view = ViewTransform::new(800.0, 600.0);
        let camera = Camera3D::new(1.6);

        let all_objects = vec![
            GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0))),
            GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(3.0, 4.0))),
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 2.0)),
            GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(0.5, 1.0),
            ])),
            GeoObject::Function(FunctionObj::new("sin(x)")),
            GeoObject::Text(TextObj::new("Hello", Point2::new(1.0, 1.0))),
            GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 2.0, 1.0)),
            GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 0.0), 1.0)),
            GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, 6.28)),
            GeoObject::PolarCurve(PolarCurveObj::new("1+cos(t)", 0.0, 6.28)),
            GeoObject::ScatterPlot(ScatterPlotObj::new(vec![1.0, 2.0], vec![3.0, 4.0])),
            GeoObject::RegressionLine(RegressionLineObj::linear(
                vec![1.0, 2.0],
                vec![3.0, 4.0],
                1.0,
                2.0,
                0.9,
            )),
            GeoObject::Histogram(HistogramObj::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], 5)),
            GeoObject::VectorField2D(VectorField2DObj::new("x", "y")),
            GeoObject::PhasePortrait(PhasePortraitObj::new(
                "x+y", "x-y", -10.0, 10.0, -10.0, 10.0,
            )),
        ];

        for obj in &all_objects {
            let mut single_doc = Document::new();
            single_doc.set_view(ViewTransform::new(800.0, 600.0));
            single_doc.add_object(obj.clone());
            let (v, _i) = crate::Renderer::build_geometry_static(&single_doc, &view, false, true);
            assert!(
                !v.is_empty(),
                "{} should render: got empty vertices",
                obj.name()
            );
        }

        let all_3d = vec![
            GeoObject::Point3D(Point3DObj::new(Point3D::new(1.0, 2.0, 3.0))),
            GeoObject::Segment3D(Segment3DObj::new(
                Point3D::new(0.0, 0.0, 0.0),
                Point3D::new(1.0, 1.0, 1.0),
            )),
            GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0)),
            GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0)),
            GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0)),
            GeoObject::Cylinder3D(Cylinder3DObj::new(
                Point3D::new(0.0, 0.0, 0.0),
                Point3D::new(0.0, 3.0, 0.0),
                1.0,
            )),
            GeoObject::Pyramid3D(Pyramid3DObj::new(
                Point3D::new(0.0, 0.0, 0.0),
                Point3D::new(0.0, 2.0, 0.0),
                2.0,
            )),
        ];

        for obj in &all_3d {
            let mut single_doc = Document::new();
            single_doc.add_object(obj.clone());
            let (v, _i) = crate::Renderer::build_3d_geometry_static(
                &single_doc,
                &camera,
                false,
                800.0,
                600.0,
            );
            assert!(
                !v.is_empty(),
                "{} should render in 3D: got empty vertices",
                obj.name()
            );
        }
    }
}
