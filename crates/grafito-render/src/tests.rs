#[cfg(test)]
#[allow(clippy::module_inception, clippy::approx_constant)]
mod tests {
    use grafito_core::{
        CircleObj, Document, Fractal2DObj, GeoObject, ImplicitCurveObj, LineObj, PointObj,
        PolygonObj, Quadric3DObj, RelationOperator, TransformedObj,
    };
    use grafito_geometry::{Camera3D, Color, Point2, ViewTransform};

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
    fn document_layer_beats_type_layer_in_paint_order() {
        // Q2: un punto (Marker) en capa 0 se pinta antes que una curva en
        // capa 1 aunque el orden por tipo diga lo contrario; dentro de la
        // misma capa vale el orden histórico (tipo, id).
        let mut document = Document::new();
        let curve = document.add_object(GeoObject::Line(LineObj::new(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
        )));
        let marker = document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        document.set_layer(curve, 1).expect("capa válida");
        let ordered: Vec<_> = crate::ordered_visible_2d_objects(&document)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ordered, vec![marker, curve]);
        // Misma capa → orden histórico por tipo (curva antes que marcador).
        document.set_layer(curve, 0).expect("capa 0");
        let ordered: Vec<_> = crate::ordered_visible_2d_objects(&document)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ordered, vec![curve, marker]);
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
            GeoObject::BarChart(BarChartObj::new(vec![1.0, 2.0, 3.0])),
            GeoObject::PieChart(PieChartObj::new(vec![1.0, 1.0, 2.0])),
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
    #[test]
    fn offscreen_circle_is_culled_from_geometry() {
        // Viewport por defecto (scale 50, 800×600) ≈ [-8, 8] × [-6, 6] mundo.
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(1000.0, 1000.0),
            1.0,
        )));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            vertices.is_empty(),
            "off-screen circle must not tessellate (got {} vertices)",
            vertices.len()
        );
    }

    #[test]
    fn onscreen_circle_still_tessellates() {
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(0.0, 0.0),
            1.0,
        )));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            !vertices.is_empty(),
            "on-screen circle must tessellate (got {} vertices)",
            vertices.len()
        );
    }

    #[test]
    fn huge_circle_overlapping_viewport_is_not_culled() {
        // Centro fuera del viewport pero radio enorme: el AABB intersecta y
        // el círculo SÍ debe teselarse (borde visible dentro del canvas).
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(1000.0, 0.0),
            995.0,
        )));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            !vertices.is_empty(),
            "circle overlapping the viewport must not be culled"
        );
    }

    #[test]
    fn offscreen_fractal_is_culled_before_compute() {
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        let mut fractal = Fractal2DObj::mandelbrot();
        fractal.x_min += 1000.0;
        fractal.x_max += 1000.0;
        fractal.y_min += 1000.0;
        fractal.y_max += 1000.0;
        doc.add_object(GeoObject::Fractal2D(fractal));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            vertices.is_empty(),
            "off-screen fractal must not compute 160k pixels"
        );
    }

    #[test]
    fn offscreen_complex_grid_is_culled_before_compute() {
        use grafito_core::ComplexGridObj;
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        let mut grid = ComplexGridObj::new("z", -1.0, 1.0, -1.0, 1.0);
        grid.render_mode = 1; // domain coloring (250k celdas en resolución alta)
        grid.x_min += 500.0;
        grid.x_max += 500.0;
        grid.y_min += 500.0;
        grid.y_max += 500.0;
        doc.add_object(GeoObject::ComplexGrid(grid));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            vertices.is_empty(),
            "off-screen complex grid must not compute 250k cells"
        );
    }

    #[test]
    fn offscreen_parametric_curve_is_culled_from_projection() {
        use grafito_core::ParametricCurve2DObj;
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        // Círculo centrado lejos del viewport: las 4000 muestras no se proyectan.
        doc.add_object(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
            "1000 + cos(t)",
            "1000 + sin(t)",
            0.0,
            6.28,
        )));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            vertices.is_empty(),
            "off-screen parametric curve must not project 4000 samples"
        );
    }

    #[test]
    fn viewport_culling_margin_covers_stroke_width() {
        // Un círculo pegado al borde del viewport (a menos de un trazo de
        // distancia) NO se culla: el margen mundial cubre el ancho del trazo.
        let mut doc = Document::new();
        doc.set_view(ViewTransform::new(800.0, 600.0));
        // Borde derecho del viewport ≈ x = 8. Círculo con borde en x = 8.05,
        // dentro del margen de trazo (4px / 50 = 0.08 mundo).
        doc.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(9.05, 0.0),
            1.0,
        )));
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            !vertices.is_empty(),
            "circle within stroke margin of the viewport must not be culled"
        );
    }

    #[test]
    fn object_world_aabb_is_conservative_for_rotated_ellipse() {
        use grafito_core::EllipseObj;
        let view = ViewTransform::new(800.0, 600.0);
        let mut ellipse = EllipseObj::new(Point2::new(0.0, 0.0), 2.0, 1.0);
        ellipse.angle = std::f64::consts::FRAC_PI_4;
        let doc = Document::new();
        let aabb = crate::object_world_aabb(&view, &doc, &GeoObject::Ellipse(ellipse))
            .expect("ellipse has a bounded AABB");
        // La elipse rotada cabe dentro de la caja ±(rx, ry) sin importar el ángulo.
        assert!(aabb.min.x <= -2.0 && aabb.max.x >= 2.0);
        assert!(aabb.min.y <= -1.0 && aabb.max.y >= 1.0);
    }

    #[test]
    fn unbounded_and_mapped_objects_never_cull() {
        use grafito_core::{ComplexMappingObj, LineObj, PointObj};
        let view = ViewTransform::new(800.0, 600.0);
        let mut doc = Document::new();
        // Línea infinita: extensión no acotada → nunca se culla.
        assert!(crate::object_world_aabb(
            &view,
            &doc,
            &GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0),))
        )
        .is_none());
        // ComplexMapping: el mapa puede traer puntos de fuera hacia dentro.
        let target = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        assert!(crate::object_world_aabb(
            &view,
            &doc,
            &GeoObject::ComplexMapping(ComplexMappingObj::new("1/z", target)),
        )
        .is_none());
    }
    #[test]
    fn scatter_plot_aabb_covers_data_beyond_declared_bounds() {
        use grafito_core::ScatterPlotObj;
        let view = ViewTransform::new(800.0, 600.0);
        let doc = Document::new();
        // Los bounds declarados (x_min/x_max = ±5) NO cubren el dato en 1000:
        // el AABB debe derivarse de los datos reales para no sobre-cullar.
        let mut scatter = ScatterPlotObj::new(vec![0.0, 1000.0], vec![0.0, 1000.0]);
        scatter.x_min = -5.0;
        scatter.x_max = 5.0;
        scatter.y_min = -5.0;
        scatter.y_max = 5.0;
        let aabb = crate::object_world_aabb(&view, &doc, &GeoObject::ScatterPlot(scatter))
            .expect("scatter plot has a bounded AABB");
        assert!(aabb.max.x >= 1000.0 && aabb.max.y >= 1000.0);
        assert!(aabb.min.x <= 0.0 && aabb.min.y <= 0.0);
    }
    #[test]
    fn quadric_no_elipsoide_usa_placeholder() {
        // Elipsoide real: parámetros derivables, sin placeholder.
        let elipsoide =
            Quadric3DObj::from_coeffs([0.25, 1.0, 1.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.75]);
        assert!(crate::quadric_ellipsoid_params(&elipsoide).is_some());
        assert!(!crate::quadric_uses_placeholder(&elipsoide));
        // Hiperboloide: no es elipsoide, pero tiene malla exacta → sin placeholder.
        let hiperboloide =
            Quadric3DObj::from_coeffs([1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
        assert!(crate::quadric_ellipsoid_params(&hiperboloide).is_none());
        assert!(!crate::quadric_uses_placeholder(&hiperboloide));
        // Vacío `x² + y² + z² = -1`: sin superficie → badge honesto.
        let vacia = Quadric3DObj::from_coeffs([1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(crate::quadric_ellipsoid_params(&vacia).is_none());
        assert!(crate::quadric_uses_placeholder(&vacia));
    }

    #[test]
    fn r2_v6_density_u32_max_da_none_sin_panic() {
        assert!(crate::phase_portrait_capacity_for_density(u32::MAX).is_none());
        assert_eq!(
            crate::phase_portrait_capacity_for_density(40),
            Some(41 * 41)
        );
        assert_eq!(crate::phase_portrait_capacity_for_density(5), Some(36));
    }

    #[test]
    fn r2_v7_perm_corrupta_da_none_sin_panic() {
        assert!(crate::apply_quadric_axis_permutation([5, 0, 0], [1.0, 2.0, 3.0]).is_none());
        assert_eq!(
            crate::apply_quadric_axis_permutation([0, 1, 2], [1.0, 2.0, 3.0]),
            Some([1.0, 2.0, 3.0])
        );
        assert_eq!(
            crate::apply_quadric_axis_permutation([2, 0, 1], [1.0, 2.0, 3.0]),
            Some([2.0, 3.0, 1.0])
        );
    }

    #[test]
    fn presupuestos_render_pineados() {
        // Si alguien cambia estos topes, el test falla honesto en vez de
        // regresión silenciosa (GIF 64/8M/5MB + nativo 48/64MiB viven en
        // app/anim, fuera de scope render; se verifican por lectura).
        assert_eq!(crate::TRANSFORMED_CACHE_CAP, 64);
        assert_eq!(crate::MAX_GEOMETRY_VERTICES, 1_000_000);
        assert_eq!(crate::MAX_GEOMETRY_INDICES, 3_000_000);
        assert_eq!(crate::MAX_PRISM_BASE_VERTICES, 64);
        assert!(crate::domain_coloring_compute::domain_cells_within_budget(
            250_000
        ));
        assert!(!crate::domain_coloring_compute::domain_cells_within_budget(
            250_001
        ));
    }

    #[test]
    fn light_default_centraliza_coeficientes_legacy() {
        let light = crate::Light::DEFAULT;
        assert_eq!(light.dir, crate::Light::DEFAULT_DIR);
        assert_eq!(light.dir, glam::Vec3::new(0.5, 1.0, 0.3));
        assert_eq!((light.ambient, light.diffuse), (0.45, 0.65));
        assert_eq!((light.specular, light.shininess), (0.30, 32.0));
        // `calculate_lighting` legacy intacto: el dueño 3D (app) sigue viendo
        // el mismo color hasta migrar a `Light::shade`.
        let legacy = crate::calculate_lighting(
            Color::RED,
            glam::Vec3::new(0.0, 0.0, 1.0),
            crate::Light::DEFAULT_DIR,
        );
        let dir = crate::Light::DEFAULT_DIR.normalize();
        let expected_intensity = 0.45 + 0.65 * dir.z.max(0.0);
        for (got, base) in [(legacy.r, 0.9), (legacy.g, 0.2), (legacy.b, 0.2)] {
            assert!(
                (got - base * expected_intensity).abs() < 1e-6,
                "legacy cambió sin aviso: {got} vs {}",
                base * expected_intensity
            );
        }
        assert_eq!(legacy.a, 1.0);
    }

    #[test]
    fn lighting_golden_pinea_canon_con_specular() {
        // Canon NUEVO (justificación: el especular Blinn-Phong añade brillo
        // acotado +0.30/canal solo cuando la normal bisecea luz/vista; en
        // frontal +Z el delta es ~+0.002, imperceptible, y en grazing
        // atenúa el apagado total del legacy. Pineado aquí, no regresión).
        let light = crate::Light::DEFAULT;
        let base = Color::new(0.9, 0.2, 0.2, 1.0);
        let normal = glam::Vec3::new(0.0, 0.0, 1.0);
        let legacy = crate::calculate_lighting(base, normal, crate::Light::DEFAULT_DIR);
        let nuevo = light.shade(base, normal);
        println!("DORADO legacy={legacy:?} nuevo={nuevo:?}");
        // Canon pineado 2026-09-10 (f32, frontal +Z, base 0.9/0.2/0.2):
        // legacy (0.5566089, 0.123690866) → nuevo (+0.0001645, +0.0000366).
        // Delta imperceptible pero NO cero: si cambia, es cambio de look y
        // debe justificarse aquí, no pasar como regresión silenciosa.
        for (got, canon) in [
            (legacy.r, 0.5566089),
            (legacy.g, 0.123690866),
            (nuevo.r, 0.5567734),
            (nuevo.g, 0.12372743),
        ] {
            assert!(
                (got - canon).abs() < 1e-6,
                "canon de iluminación cambió: {got} vs {canon}"
            );
        }
        // El especular nunca oscurece respecto al legacy.
        assert!(nuevo.r >= legacy.r && nuevo.g >= legacy.g && nuevo.b >= legacy.b);
        // ...y está acotado por `specular` (aquí base.r=0.9 → techo +0.27).
        assert!((nuevo.r - legacy.r) <= 0.9 * light.specular + 1e-6);
        assert_eq!(nuevo.a, base.a);
        // should_apply_shading: transparente o no-finito no se sombrea.
        assert!(light.should_apply_shading(base));
        assert!(!light.should_apply_shading(Color::new(0.9, 0.2, 0.2, 0.0)));
        assert!(!light.should_apply_shading(Color::new(f32::NAN, 0.2, 0.2, 1.0)));
        assert_eq!(light.shade(Color::new(0.9, 0.2, 0.2, 0.0), normal).r, 0.9);
    }

    #[test]
    fn transformed_shared_hit_cero_copia_y_equivale_al_clon() {
        let view = ViewTransform::new(800.0, 600.0);
        let transformed = TransformedObj::new(
            GeoObject::Line(LineObj::new(Point2::new(-1.0, 0.0), Point2::new(1.0, 0.0))),
            "z + 2",
        );
        let mut document = Document::new();
        document.add_object(GeoObject::Transformed(transformed.clone()));

        // Miss compartido: contenido útil y guardado en el LRU.
        let (miss_v, miss_i) = crate::Renderer::build_transformed_geometry_static_shared(
            &document,
            &transformed,
            &view,
            false,
        );
        assert!(!miss_v.is_empty() && !miss_i.is_empty());

        // Hit compartido: MISMO `Arc` (puntero idéntico, cero copia de `Vec`).
        let (hit_v, hit_i) = crate::Renderer::build_transformed_geometry_static_shared(
            &document,
            &transformed,
            &view,
            false,
        );
        assert!(std::sync::Arc::ptr_eq(&miss_v, &hit_v));
        assert!(std::sync::Arc::ptr_eq(&miss_i, &hit_i));

        // El path `Vec` legacy sigue devolviendo el mismo contenido.
        let (clon_v, clon_i) = crate::Renderer::build_transformed_geometry_static(
            &document,
            &transformed,
            &view,
            false,
        );
        assert_eq!(clon_v.len(), miss_v.len());
        assert_eq!(clon_i.as_slice(), miss_i.as_slice());
        for (a, b) in clon_v.iter().zip(miss_v.iter()) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.color, b.color);
        }
    }
}
