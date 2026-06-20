//! Verifica que el ViewTransform está correctamente implementado
//! (mismas pruebas que las de grafito-geometry/src/types.rs).

use grafito_geometry::{Point2, ViewTransform};

#[test]
fn test_screen_to_world_basic() {
    // View por defecto: screen_size=800x600, scale=50, offset=(0,0).
    let view = ViewTransform::new(800.0, 600.0);
    // El centro (400, 300) -> (0, 0) en mundo.
    let p = view.screen_to_world(glam::Vec2::new(400.0, 300.0));
    assert!((p.x).abs() < 1e-6);
    assert!((p.y).abs() < 1e-6);

    // (800, 300) -> (8, 0) (4 unidades a la derecha del centro).
    let p = view.screen_to_world(glam::Vec2::new(800.0, 300.0));
    assert!((p.x - 8.0).abs() < 1e-6);
    assert!((p.y).abs() < 1e-6);

    // (400, 0) -> (0, 6) (6 unidades arriba del centro).
    let p = view.screen_to_world(glam::Vec2::new(400.0, 0.0));
    assert!((p.x).abs() < 1e-6);
    assert!((p.y - 6.0).abs() < 1e-6);
}

#[test]
fn test_world_to_screen_basic() {
    let view = ViewTransform::new(800.0, 600.0);
    let s = view.world_to_screen(Point2::new(0.0, 0.0));
    assert!((s.x - 400.0).abs() < 1e-6);
    assert!((s.y - 300.0).abs() < 1e-6);

    let s = view.world_to_screen(Point2::new(8.0, 0.0));
    assert!((s.x - 800.0).abs() < 1e-6);
    assert!((s.y - 300.0).abs() < 1e-6);
}
