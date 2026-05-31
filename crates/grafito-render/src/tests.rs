#[cfg(test)]
mod tests {
    use grafito_core::{Document, GeoObject, PointObj, LineObj, CircleObj, PolygonObj};
    use grafito_geometry::{Point2, ViewTransform, Camera3D};

    #[test]
    fn test_build_geometry_empty_document() {
        let doc = Document::new();
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false);
        
        assert!(!vertices.is_empty(), "Grid and axes should produce vertices");
        assert!(!indices.is_empty(), "Grid and axes should produce indices");
    }

    #[test]
    fn test_build_geometry_with_point() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))));
        
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false);
        
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_geometry_with_line() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))));
        
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false);
        
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_geometry_with_circle() {
        let mut doc = Document::new();
        doc.add_object(GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0)));
        
        let view = ViewTransform::new(800.0, 600.0);
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false);
        
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
        let (vertices, indices) = crate::Renderer::build_geometry_static(&doc, &view, false);
        
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn test_build_3d_geometry_empty_document() {
        let doc = Document::new();
        let camera = Camera3D::new(1.6);
        let (vertices, indices) = crate::Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
        
        assert!(!vertices.is_empty(), "3D grid and axes should produce vertices");
        assert!(!indices.is_empty(), "3D grid and axes should produce indices");
    }

    #[test]
    fn test_vertex_size() {
        assert_eq!(std::mem::size_of::<crate::Vertex>(), 28, "Vertex should be 28 bytes (3 floats position + 4 floats color)");
    }
}
