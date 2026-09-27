#[cfg(test)]
#[allow(clippy::module_inception, clippy::approx_constant)]
mod tests {
    use grafito_core::{
        CircleObj, ComplexIntegralObj, Document, Fractal2DObj, GeoObject, ImplicitCurveObj,
        LineObj, PencilObj, PointObj, Quadric3DObj, RelationOperator, TransformedObj,
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
    fn test_build_geometry_with_drawn_contour_emits_integral_label() {
        // Contorno dibujado a mano (cuadrado) + ComplexIntegral de 1/z: el arm
        // del render recorre el Pencil y agrega la etiqueta `∮ = …` como
        // geometría de texto (antes del feature el trazo era no-op silencioso).
        let mut doc = Document::new();
        let pencil = PencilObj::new(vec![
            Point2::new(-1.0, -1.0),
            Point2::new(1.0, -1.0),
            Point2::new(1.0, 1.0),
            Point2::new(-1.0, 1.0),
            Point2::new(-1.0, -1.0),
        ])
        .with_label("trazo");
        let pencil_id = pencil.id;
        doc.try_add_object(GeoObject::Pencil(pencil))
            .expect("trazo");
        let view = ViewTransform::new(800.0, 600.0);
        let (without_label, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);

        doc.try_add_object(GeoObject::ComplexIntegral(ComplexIntegralObj::new(
            "1/z", pencil_id, false,
        )))
        .expect("integral");
        let (with_label, _) = crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(
            with_label.len() > without_label.len(),
            "la etiqueta del contorno agrega vértices: {} vs {}",
            with_label.len(),
            without_label.len()
        );
    }

    /// Firma de geometría comparable: posiciones redondeadas a medio píxel,
    /// ordenadas (independiente del orden de emisión).
    fn geometry_signature(vertices: &[crate::Vertex]) -> Vec<(i32, i32)> {
        let mut signature: Vec<(i32, i32)> = vertices
            .iter()
            .map(|vertex| {
                (
                    (vertex.position[0] * 2.0).round() as i32,
                    (vertex.position[1] * 2.0).round() as i32,
                )
            })
            .collect();
        signature.sort_unstable();
        signature.dedup();
        signature
    }

    fn mapping_document(expr: &str) -> Document {
        let mut document = Document::new();
        let target = document
            .try_add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                "x^2 + y^2",
                "1",
                RelationOperator::Less,
            )))
            .expect("disco unidad");
        document
            .try_add_object(GeoObject::ComplexMapping(
                grafito_core::ComplexMappingObj::new_with_symbol(expr, target, "z"),
            ))
            .expect("mapeo");
        document
    }

    #[test]
    fn complex_mapping_identity_and_affine_maps_draw_geometry() {
        // Regresión: `z`, `2*z` o `z^2+1` no estaban en la lista corta de
        // `ConformalMap` y el mapeo entero era un no-op silencioso.
        let view = ViewTransform::new(800.0, 600.0);
        let mut baseline = Document::new();
        baseline
            .try_add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                "x^2 + y^2",
                "1",
                RelationOperator::Less,
            )))
            .expect("disco");
        let (baseline_vertices, _) =
            crate::Renderer::build_geometry_static(&baseline, &view, false, false);
        for expr in ["z", "2*z", "z + 1", "z^2 + 1", "i*z"] {
            let document = mapping_document(expr);
            let (vertices, _) =
                crate::Renderer::build_geometry_static(&document, &view, false, false);
            assert!(
                vertices.len() > baseline_vertices.len(),
                "ComplexMapping[{expr}] debe dibujar geometría (no-op silencioso)"
            );
        }
    }

    #[test]
    fn complex_mapping_reference_lattice_shows_power_deformation() {
        // La frontera del disco se preserva bajo z y z^5 (mismo círculo), pero
        // la retícula de referencia debe verse claramente distinta.
        let view = ViewTransform::new(800.0, 600.0);
        let identity = mapping_document("z");
        let power = mapping_document("z^5");
        let (identity_vertices, _) =
            crate::Renderer::build_geometry_static(&identity, &view, false, false);
        let (power_vertices, _) =
            crate::Renderer::build_geometry_static(&power, &view, false, false);
        let identity_signature = geometry_signature(&identity_vertices);
        let power_signature = geometry_signature(&power_vertices);
        assert!(!power_signature.is_empty(), "z^5 dibuja retícula");
        let common = power_signature
            .iter()
            .filter(|point| identity_signature.binary_search(point).is_ok())
            .count();
        let union = identity_signature.len() + power_signature.len() - common;
        let distinct = union - common;
        // La frontera (mismo círculo) y los radios sobre los ejes coinciden;
        // el resto de la retícula debe deformarse visiblemente (≥25%).
        assert!(
            distinct * 100 >= union * 25,
            "z y z^5 deben diferir en la retícula (distintos {distinct} de {union})"
        );
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
        assert_eq!(crate::TRANSFORMED_CACHE_SIZE.get(), 64);
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

    #[test]
    fn transformed_cache_key_colisiona_solo_si_contenido_igual() {
        // Key O(1) sin allocs: exprs de la cadena + discriminante/id hoja +
        // version/nonce/view. Mismo contenido + misma versión => misma key;
        // cualquier diferencia => distinta key (jamás stale).
        let view = ViewTransform::new(800.0, 600.0);
        let base = TransformedObj::new(
            GeoObject::Line(LineObj::new(Point2::new(-1.0, 0.0), Point2::new(1.0, 0.0))),
            "z + 2",
        );
        let mut document = Document::new();
        document.add_object(GeoObject::Transformed(base.clone()));
        let k1 = crate::transformed_cache_key(&document, &base, &view, false, 0);
        let k1b = crate::transformed_cache_key(&document, &base, &view, false, 0);
        assert_eq!(k1, k1b, "mismo contenido + misma versión => misma key");

        let mut other_expr = base.clone();
        other_expr.complex_expr = "z + 3".to_string();
        let k_expr = crate::transformed_cache_key(&document, &other_expr, &view, false, 0);
        assert_ne!(k1, k_expr, "distinto expr => distinta key");

        let other_id = TransformedObj::new(
            GeoObject::Line(LineObj::new(Point2::new(-1.0, 0.0), Point2::new(1.0, 0.0))),
            "z + 2",
        );
        assert_ne!(base.inner.id(), other_id.inner.id());
        let k_id = crate::transformed_cache_key(&document, &other_id, &view, false, 0);
        assert_ne!(k1, k_id, "distinto inner id => distinta key");

        let circle = TransformedObj::new(
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0)),
            "z + 2",
        );
        let k_var = crate::transformed_cache_key(&document, &circle, &view, false, 0);
        assert_ne!(k1, k_var, "distinta variante hoja => distinta key");

        let plano = TransformedObj::new(
            GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
            "z + 2",
        );
        let nested_inner = TransformedObj::new(
            GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
            "z + 1",
        );
        let nested = TransformedObj::new(GeoObject::Transformed(nested_inner), "z + 2");
        let k_flat = crate::transformed_cache_key(&document, &plano, &view, false, 0);
        let k_nested = crate::transformed_cache_key(&document, &nested, &view, false, 0);
        assert_ne!(k_flat, k_nested, "anidado vs plano => distinta key");

        let mut view2 = ViewTransform::new(800.0, 600.0);
        view2.scale = 100.0;
        let k_view = crate::transformed_cache_key(&document, &base, &view2, false, 0);
        assert_ne!(k1, k_view, "distinto view.scale => distinta key");
        let k_dark = crate::transformed_cache_key(&document, &base, &view, true, 0);
        assert_ne!(k1, k_dark, "distinto dark_mode => distinta key");
        let k_depth = crate::transformed_cache_key(&document, &base, &view, false, 1);
        assert_ne!(k1, k_depth, "distinto depth => distinta key");

        let mut document2 = document.clone();
        document2.bump_version();
        let k_ver = crate::transformed_cache_key(&document2, &base, &view, false, 0);
        assert_ne!(k1, k_ver, "distinta document.version => distinta key");

        let mut with_compiled = base.clone();
        with_compiled.compiled_expr = Some("compiled".to_string());
        let k_comp = crate::transformed_cache_key(&document, &with_compiled, &view, false, 0);
        assert_ne!(k1, k_comp, "distinto compiled_expr => distinta key");
    }

    #[test]
    fn curve_aabb_cache_igual_al_muestreo_completo() {
        use grafito_core::{FunctionObj, ParametricCurve2DObj, PolarCurveObj};
        let view = ViewTransform::new(800.0, 600.0);
        let document = Document::new();

        let fun = GeoObject::Function(FunctionObj::new("sin(x)"));
        assert_eq!(crate::object_world_aabb(&view, &document, &fun), None);
        assert_eq!(crate::object_world_aabb(&view, &document, &fun), None);

        let parametrics = [
            ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, 6.283185307179586),
            ParametricCurve2DObj::new("t", "t*t", -2.0, 2.0),
            ParametricCurve2DObj::new("sin(2*t)", "sin(3*t)", 0.0, 6.283185307179586),
        ];
        for pc in &parametrics {
            let obj = GeoObject::ParametricCurve2D(pc.clone());
            let cached =
                crate::object_world_aabb(&view, &document, &obj).expect("paramétrica acotada");
            let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                pc,
                4000,
                &document.variables,
            );
            let mut expected: Option<grafito_geometry::AABB> = None;
            for &(x, y) in samples.iter() {
                if x.is_finite() && y.is_finite() {
                    let p = Point2::new(x, y);
                    match &mut expected {
                        Some(a) => a.expand(&p),
                        None => expected = Some(grafito_geometry::AABB::new(p, p)),
                    }
                }
            }
            assert_eq!(
                Some(cached),
                expected,
                "AABB paramétrica idéntico al muestreo completo"
            );
            let cached2 = crate::object_world_aabb(&view, &document, &obj).expect("hit");
            assert_eq!(cached.min.x, cached2.min.x);
            assert_eq!(cached.min.y, cached2.min.y);
            assert_eq!(cached.max.x, cached2.max.x);
            assert_eq!(cached.max.y, cached2.max.y);
        }

        let polars = [
            PolarCurveObj::new("1+cos(t)", 0.0, 6.283185307179586),
            PolarCurveObj::new("sin(3*t)", 0.0, 6.283185307179586),
            PolarCurveObj::new("t", 0.0, 6.283185307179586),
        ];
        for pol in &polars {
            let obj = GeoObject::PolarCurve(pol.clone());
            let cached = crate::object_world_aabb(&view, &document, &obj).expect("polar acotada");
            let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
                pol,
                4000,
                &document.variables,
            );
            let mut expected: Option<grafito_geometry::AABB> = None;
            for &(x, y) in samples.iter() {
                if x.is_finite() && y.is_finite() {
                    let p = Point2::new(x, y);
                    match &mut expected {
                        Some(a) => a.expand(&p),
                        None => expected = Some(grafito_geometry::AABB::new(p, p)),
                    }
                }
            }
            assert_eq!(
                Some(cached),
                expected,
                "AABB polar idéntico al muestreo completo"
            );
            let cached2 = crate::object_world_aabb(&view, &document, &obj).expect("hit");
            assert_eq!(cached.min.x, cached2.min.x);
            assert_eq!(cached.max.x, cached2.max.x);
        }
    }

    #[test]
    fn curve_aabb_cache_invalida_con_version() {
        use grafito_core::ParametricCurve2DObj;
        let view = ViewTransform::new(800.0, 600.0);
        let mut document = Document::new();
        let pc = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, 6.283185307179586);
        let id = pc.id;
        document.add_object(GeoObject::ParametricCurve2D(pc));
        let obj1 = document.get_object(id).expect("existe").clone();
        let aabb1 = crate::object_world_aabb(&view, &document, &obj1).expect("aabb1");

        document.set_variable("k".to_string(), 1.0);
        let obj2 = document.get_object(id).expect("sigue").clone();
        let aabb2 = crate::object_world_aabb(&view, &document, &obj2).expect("aabb2");
        assert_eq!(aabb1.min.x, aabb2.min.x);
        assert_eq!(aabb1.max.x, aabb2.max.x);

        let document3 = Document::new();
        let pc_big = ParametricCurve2DObj::new("10*cos(t)", "10*sin(t)", 0.0, 6.283185307179586);
        let obj_big = GeoObject::ParametricCurve2D(pc_big);
        let aabb_big = crate::object_world_aabb(&view, &document3, &obj_big).expect("grande");
        assert!(
            aabb_big.max.x > aabb1.max.x + 5.0,
            "radio 10 vs 1: {} vs {}",
            aabb_big.max.x,
            aabb1.max.x
        );
    }
}
#[cfg(test)]
mod coverage_sweep_pure {
    use crate::{
        aabb_intersects, calculate_lighting, complex_mapping_homotopy_factor,
        interpolate_complex_mapping_point, object_cull_margin_world, object_world_aabb,
        ordered_visible_2d_objects, prism_solid_triangle_count, prism_wire_segment_count,
        prism_work_units, quadric_ellipsoid_params, quadric_uses_placeholder, scene_layer_2d,
        viewport_world_bounds, SceneLayer2D,
    };
    use grafito_core::{
        CircleObj, Document, GeoObject, LineObj, PointObj, PolygonObj, Quadric3DObj,
    };
    use grafito_geometry::{Color, Point2, ViewTransform};
    #[test]
    fn barrido_viewport_aabb_y_culling() {
        let view = ViewTransform::new(800.0, 600.0);
        let bounds = viewport_world_bounds(&view);
        assert!(bounds.max.x > bounds.min.x && bounds.max.y > bounds.min.y);
        let a = bounds;
        assert!(aabb_intersects(&a, &a, 0.0));
        let lejos = crate::AABB::new(Point2::new(1e9, 1e9), Point2::new(1e9 + 1.0, 1e9 + 1.0));
        assert!(!aabb_intersects(&a, &lejos, 0.0));
        let pt = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        assert!(object_cull_margin_world(&pt, 100.0) > 0.0);
        assert!(object_cull_margin_world(&pt, 0.0).is_finite());
        let doc = Document::new();
        assert!(object_world_aabb(&view, &doc, &pt).is_some());
        let circ = GeoObject::Circle(CircleObj::new(Point2::new(1.0, 1.0), 2.0));
        let ab = object_world_aabb(&view, &doc, &circ).expect("aabb círculo");
        assert!((ab.max.x - ab.min.x - 4.0).abs() < 1e-9);
    }
    #[test]
    fn barrido_capas_luz_y_prismas() {
        let pt = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        assert!(matches!(scene_layer_2d(&pt), SceneLayer2D::Marker));
        let line = GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)));
        assert!(matches!(scene_layer_2d(&line), SceneLayer2D::Curve));
        let lit = calculate_lighting(Color::new(1.0, 0.0, 0.0, 1.0), glam::Vec3::Z, glam::Vec3::Z);
        assert!((lit.r - 1.0).abs() < 1e-6 && lit.g.abs() < 1e-6);
        let dark = calculate_lighting(
            Color::new(1.0, 1.0, 1.0, 1.0),
            glam::Vec3::Z,
            -glam::Vec3::Z,
        );
        assert!(dark.r < 1.0 && dark.r > 0.3);
        assert_eq!(prism_solid_triangle_count(4), 12);
        assert_eq!(prism_wire_segment_count(4), 12);
        assert_eq!(prism_work_units(4), 24);
        assert_eq!(prism_solid_triangle_count(0), 0);
        let q = Quadric3DObj::from_coeffs([1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
        assert!(
            !quadric_uses_placeholder(&q),
            "la esfera unidad es elipsoide real, sin placeholder"
        );
        assert!(
            quadric_ellipsoid_params(&q).is_some(),
            "la esfera unidad deriva parámetros de elipsoide"
        );
    }
    #[test]
    fn barrido_mapeo_complejo_e_interpolacion() {
        let p =
            interpolate_complex_mapping_point(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0), 0.5);
        assert!((p.x - 1.0).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9);
        let f0 = complex_mapping_homotopy_factor(false, 1.0, 10.0);
        assert!((f0 - 1.0).abs() < 1e-9, "sin animar factor 1, fue {f0}");
        let f1 = complex_mapping_homotopy_factor(true, 1.0, 0.0);
        assert!((0.0..=1.0).contains(&f1), "animado en rango, fue {f1}");
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        doc.add_object(GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
        ])));
        let vis = ordered_visible_2d_objects(&doc);
        assert_eq!(vis.len(), 2, "dos visibles ordenados");
        let view = ViewTransform::new(800.0, 600.0);
        let (v, i) = crate::Renderer::build_geometry_static(&doc, &view, false, true);
        assert!(!v.is_empty() && !i.is_empty());
    }
}

#[test]
fn complex_grid_geometry_budget_caps_rect_emission() {
    // FIX 2: 1 rect = 4 vértices + 6 índices por celda; res 300 en High
    // eran 90k rects = 360k vértices por rebuild. El helper es el origen
    // único del chequeo en los paths CPU y GPU.
    assert!(crate::complex_grid_geometry_within_budget(200));
    assert!(crate::complex_grid_geometry_within_budget(256));
    assert!(!crate::complex_grid_geometry_within_budget(257));
    assert!(!crate::complex_grid_geometry_within_budget(300));
    assert!(!crate::complex_grid_geometry_within_budget(usize::MAX));
}
