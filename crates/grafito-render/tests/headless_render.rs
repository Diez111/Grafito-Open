use grafito_core::*;
use grafito_geometry::{Camera3D, Point2, ViewTransform};
use grafito_render::Renderer;
use std::collections::HashMap;

fn view_800x600() -> ViewTransform {
    ViewTransform::new(800.0, 600.0)
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
    let (vertices, indices) =
        Renderer::build_3d_geometry_static(&doc, &camera, false, 800.0, 600.0);
    assert!(!vertices.is_empty(), "surface should produce vertices");
    assert!(!indices.is_empty(), "surface should produce indices");
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
fn test_gpu_function_no_stale_bytecode() {
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
