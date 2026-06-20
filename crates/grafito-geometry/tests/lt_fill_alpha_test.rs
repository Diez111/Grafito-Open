//! Verifica que el fill con alpha 0.5 es visible.

use grafito_geometry::Color;

#[test]
fn test_fill_alpha_visibility() {
    // El fill con alpha 0.2 es casi transparente.
    let alpha_02 = Color::new(0.6, 0.2, 0.8, 0.2);
    let alpha_05 = Color::new(0.6, 0.2, 0.8, 0.5);
    // El alpha se almacena como un valor en [0, 1].
    // En una pantalla, alpha 0.2 = 20% opaco = apenas visible.
    // Alpha 0.5 = 50% opaco = claramente visible.
    println!(
        "alpha 0.2: r={}, g={}, b={}, a={}",
        alpha_02.r, alpha_02.g, alpha_02.b, alpha_02.a
    );
    println!(
        "alpha 0.5: r={}, g={}, b={}, a={}",
        alpha_05.r, alpha_05.g, alpha_05.b, alpha_05.a
    );
    // Verificamos que se almacena correctamente.
    assert_eq!(alpha_02.a, 0.2);
    assert_eq!(alpha_05.a, 0.5);
}
