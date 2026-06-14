use grafito_core::*;
use grafito_geometry::{Camera3D, Point2, ViewTransform};
use grafito_render::Renderer;

fn view_800x600() -> ViewTransform {
    ViewTransform::new(800.0, 600.0)
}

#[test]
fn test_renderer_builds_geometry_for_function() {
    let mut doc = Document::new();
    doc.set_view(view_800x600());
    doc.add_object(GeoObject::Function(FunctionObj::new("sin(x)")));

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false);
    assert!(!vertices.is_empty(), "function should produce vertices");
    assert!(!indices.is_empty(), "function should produce indices");
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

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false);
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
    let (vertices, indices) =
        Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
    assert!(!vertices.is_empty(), "surface should produce vertices");
    assert!(!indices.is_empty(), "surface should produce indices");
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

    let (vertices, indices) = Renderer::build_geometry_static(&doc, &view_800x600(), false);
    assert!(
        !vertices.is_empty(),
        "boolean polygon should produce vertices"
    );
    assert!(
        !indices.is_empty(),
        "boolean polygon should produce indices"
    );
}
