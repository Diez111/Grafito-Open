//! Verifica que el view transform maneja correctamente screen_to_world
//! para coordenadas (x, 0) y (0, y) en view no cuadrados.

use grafito_geometry::ViewTransform;

#[test]
fn test_screen_to_world_consistency() {
    let view = ViewTransform::new(1000.0, 500.0); // No cuadrado.
                                                  // Sample x en y=0 (top del canvas).
    let p_x = view.screen_to_world(glam::Vec2::new(500.0, 0.0));
    println!("screen(500, 0) -> world = ({}, {})", p_x.x, p_x.y);
    // Sample y en x=0 (left del canvas).
    let p_y = view.screen_to_world(glam::Vec2::new(0.0, 250.0));
    println!("screen(0, 250) -> world = ({}, {})", p_y.x, p_y.y);
    // Para evaluar f(x, y) correctamente, necesitamos que ambos
    // devuelvan coordenadas world consistentes.
    assert!((p_x.x - p_y.x).abs() < 1e-9, "x de ambos debe ser igual");
}
