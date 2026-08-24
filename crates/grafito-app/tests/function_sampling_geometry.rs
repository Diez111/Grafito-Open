#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_core::{Document, FunctionObj, GeoObject};
use grafito_geometry::ViewTransform;
use grafito_render::Renderer;

#[test]
fn unrepresentable_or_non_finite_function_values_do_not_emit_gpu_geometry() {
    let view = ViewTransform::new(800.0, 600.0);

    for expression in ["10^100", "1/0"] {
        let mut document = Document::new();
        document.set_view(view);
        document.add_object(GeoObject::Function(FunctionObj::new(expression)));

        let (vertices, indices) = Renderer::build_geometry_static(&document, &view, false, false);
        assert!(vertices.is_empty(), "{expression} should not emit vertices");
        assert!(indices.is_empty(), "{expression} should not emit indices");
    }
}
