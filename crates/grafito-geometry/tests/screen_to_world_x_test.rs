//! Verifica que el world x no depende del screen y (transformación ortogonal).

use grafito_geometry::ViewTransform;

#[test]
fn test_screen_to_world_x_independent_of_y() {
    let view = ViewTransform::new(1000.0, 500.0);
    // screen(500, 0): centro x, top.
    let p_x_top = view.screen_to_world(glam::Vec2::new(500.0, 0.0));
    // screen(500, 250): centro x, centro y.
    let p_x_mid = view.screen_to_world(glam::Vec2::new(500.0, 250.0));
    // screen(500, 500): centro x, bottom.
    let p_x_bot = view.screen_to_world(glam::Vec2::new(500.0, 500.0));
    println!("(500, 0)   -> ({}, {})", p_x_top.x, p_x_top.y);
    println!("(500, 250) -> ({}, {})", p_x_mid.x, p_x_mid.y);
    println!("(500, 500) -> ({}, {})", p_x_bot.x, p_x_bot.y);
    // El x debe ser el mismo en los 3 casos.
    assert!(
        (p_x_top.x - p_x_mid.x).abs() < 1e-9,
        "x de top y mid deben ser iguales"
    );
    assert!(
        (p_x_mid.x - p_x_bot.x).abs() < 1e-9,
        "x de mid y bot deben ser iguales"
    );
    // Los y deben ser diferentes.
    assert!(
        (p_x_top.y - p_x_mid.y).abs() > 1.0,
        "y de top y mid deben ser diferentes"
    );
    assert!(
        (p_x_mid.y - p_x_bot.y).abs() > 1.0,
        "y de mid y bot deben ser diferentes"
    );
}
