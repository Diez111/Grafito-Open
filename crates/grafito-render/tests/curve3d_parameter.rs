#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_core::{Document, GeoObject, ParametricCurve3DObj};
use grafito_geometry::Camera3D;
use grafito_render::Renderer;

#[test]
fn world_mesh_uses_the_declared_curve_parameter() {
    let mut document = Document::new();
    document.add_object(GeoObject::ParametricCurve3D(
        ParametricCurve3DObj::new("s", "s^2", "0", 0.0, 1.0).with_parameter("s"),
    ));

    let mesh = Renderer::build_3d_world_mesh(&document, &Camera3D::new(1.6), 800.0, 600.0);

    assert!(!mesh.wire_vertices.is_empty());
    assert!(mesh
        .wire_vertices
        .iter()
        .any(|vertex| vertex.position[0] > 0.9));
}
