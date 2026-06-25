#[cfg(test)]
#[allow(clippy::module_inception, clippy::approx_constant)]
mod tests {
    use grafito_core::{CircleObj, Document, GeoObject, LineObj, PointObj, PolygonObj};
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
    fn test_vertical_line_infinite_does_not_overflow_viewport() {
        // Regresión: un LineObj con kind=Line y dos puntos verticales
        // (mismo x) no debe generar vértices fuera del AABB del viewport.
        // Esto reproduce el bug visual donde una línea vertical
        // "desbordaba" el canvas en la imagen del usuario.
        use grafito_core::LineKind;
        use grafito_core::PencilObj;
        let mut doc = Document::new();
        // Línea infinita vertical en x=-3 (debe recortarse a la altura
        // del viewport visible).
        let mut line = LineObj::new_with_kind(
            Point2::new(-3.0, -1.0),
            Point2::new(-3.0, 1.0),
            LineKind::Line,
        );
        line = line.with_label("l");
        doc.add_object(GeoObject::Line(line));

        // PencilObj con un trazo vertical extremo.
        let pencil = PencilObj::new(vec![Point2::new(-3.0, -1000.0), Point2::new(-3.0, 1000.0)]);
        doc.add_object(GeoObject::Pencil(pencil));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _indices) =
            crate::Renderer::build_geometry_static(&doc, &view, false, false);

        // El viewport visible es x ∈ [-8, 8], y ∈ [-6, 6] (800/50/2, 600/50/2).
        // Cualquier vértice de línea/Pencil debe tener coordenadas en
        // píxeles dentro de la pantalla, ±100 px de margen para el ancho
        // del trazo.
        for v in &vertices {
            let x = v.position[0];
            let y = v.position[1];
            // Permitimos un margen generoso de 200 px para los anchos
            // de trazo, pero no permitimos valores absurdos.
            assert!(
                x > -200.0 && x < 1000.0,
                "Vértice con x fuera del viewport: {}",
                x
            );
            assert!(
                y > -200.0 && y < 800.0,
                "Vértice con y fuera del viewport: {}",
                y
            );
        }
    }
    #[test]
    fn test_vertical_line_infinite_clipped_to_viewport() {
        // Regresión: una LineObj con kind=Line vertical NO debe
        // generar vértices (defensa nuclear contra "líneas negras
        // feas" que cruzan el canvas de borde a borde). La limpieza
        // automática y el filtro del render se encargan de ignorarla.
        // Usamos x = -3.25 (que no es múltiplo de 50) para no
        // confundirnos con el grid.
        use grafito_core::LineKind;
        let mut doc = Document::new();
        let line = LineObj::new_with_kind(
            Point2::new(-3.25, -1.0),
            Point2::new(-3.25, 1.0),
            LineKind::Line,
        )
        .with_label("l");
        doc.add_object(GeoObject::Line(line));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _indices) =
            crate::Renderer::build_geometry_static(&doc, &view, false, false);

        // La línea vertical debe dibujarse (la defensa nuclear fue removida)
        // y recortarse al viewport. Filtramos los vértices con x en (236, 239).
        let line_verts: Vec<_> = vertices
            .iter()
            .filter(|v| v.position[0] > 236.0 && v.position[0] < 239.0)
            .collect();
        assert_eq!(
            line_verts.len(),
            4,
            "La línea vertical debería haberse dibujado con 4 vértices, pero hay {}",
            line_verts.len()
        );
        for v in &line_verts {
            let y = v.position[1];
            assert!(
                y >= -50.0 && y <= 650.0,
                "Vértice vertical desborda y: {}",
                y
            );
        }
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
    fn test_huge_line_width_is_clamped_in_gpu_geometry() {
        use grafito_core::LineKind;

        let mut doc = Document::new();
        let mut line = LineObj::new_with_kind(
            Point2::new(-3.0, -1.0),
            Point2::new(-2.0, 1.0),
            LineKind::Segment,
        );
        line.width = 10_000.0;
        doc.add_object(GeoObject::Line(line));

        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, _indices) =
            crate::Renderer::build_geometry_static(&doc, &view, false, false);
        assert!(!vertices.is_empty());

        for v in &vertices {
            let x = v.position[0];
            let y = v.position[1];
            assert!((-20.0..=820.0).contains(&x), "x out of bounds: {x}");
            assert!((-20.0..=620.0).contains(&y), "y out of bounds: {y}");
        }
    }
}
