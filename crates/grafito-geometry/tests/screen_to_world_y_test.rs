//! Verifica que el view transform maneja correctamente screen_to_world
//! para coordenadas (x, 0) y (0, y) en view no cuadrados.

use grafito_geometry::ViewTransform;

#[test]
fn test_screen_to_world_consistency() {
    let view = ViewTransform::new(1000.0, 500.0); // No cuadrático.
                                                  // screen_to_world en el centro X debe dar world x = 0 sin importar Y.
    let p_top = view.screen_to_world(glam::Vec2::new(500.0, 0.0));
    let p_bot = view.screen_to_world(glam::Vec2::new(500.0, 500.0));
    println!("screen(500, 0) -> world = ({}, {})", p_top.x, p_top.y);
    println!("screen(500, 500) -> world = ({}, {})", p_bot.x, p_bot.y);
    // La X world debe ser la misma para ambos (independiente de Y).
    assert!(
        (p_top.x - p_bot.x).abs() < 1e-9,
        "x debe ser independiente de y"
    );
    // Pero Y debe differir.
    assert!(
        (p_top.y - p_bot.y).abs() > 1.0,
        "y debe cambiar con screen y"
    );
}
