//! Tests de performance y correctitud para el caché del fill de curvas
//! implícitas.
//!
//! Verifica que la lógica de cache key de `compute_fill_cache_key` produce
//! hits consecutivos para los mismos parámetros y misses cuando los
//! `view_bounds` cambian (invalidación del caché).

use std::collections::HashMap;

use grafito_app::render_2d::{compute_fill_cache_key, fill_cache_texture_size};
use grafito_core::implicit_curve::padded_snapped_bounds;
use grafito_core::{ImplicitCurveObj, RelationOperator};
use grafito_geometry::Color;

/// Factor de padding y snap de celdas usados en producción.
const PAD_FACTOR: f64 = 2.0;
const SNAP_CELLS: usize = 64;

#[test]
fn fill_cache_key_stable_for_same_view_bounds() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);

    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let key1 = compute_fill_cache_key(&ic, padded, canvas_size, &vars, fill_color);
    let key2 = compute_fill_cache_key(&ic, padded, canvas_size, &vars, fill_color);

    assert_eq!(
        key1, key2,
        "La cache key debe ser determinista para los mismos parámetros"
    );
}

#[test]
fn fill_cache_reuse_within_padded_region() {
    // Simula dos frames consecutivos con un pan pequeño: los view_bounds
    // cambian levemente pero caen dentro de la región padded/snapped, así
    // que la cache key (que usa padded_bounds) debe coincidir.
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);

    let view_bounds_1 = (-5.0, 5.0, -4.0, 4.0);
    let padded_1 = padded_snapped_bounds(view_bounds_1, PAD_FACTOR, SNAP_CELLS);
    let key1 = compute_fill_cache_key(&ic, padded_1, canvas_size, &vars, fill_color);

    // Pequeño pan (dentro del padding): los padded_bounds pueden coincidir.
    let view_bounds_2 = (-4.9, 5.1, -3.9, 4.1);
    let padded_2 = padded_snapped_bounds(view_bounds_2, PAD_FACTOR, SNAP_CELLS);
    let key2 = compute_fill_cache_key(&ic, padded_2, canvas_size, &vars, fill_color);

    // Si los padded_bounds coinciden, la key debe coincidir (reuso de caché).
    if padded_1 == padded_2 {
        assert_eq!(
            key1, key2,
            "Pequeños pans dentro del snap no deben invalidar el caché"
        );
    }
    // Verificar que view_bounds_2 sigue dentro de la región cacheada.
    let (rx_min, rx_max, ry_min, ry_max) = padded_1;
    let (vx_min, vx_max, vy_min, vy_max) = view_bounds_2;
    assert!(
        vx_min >= rx_min && vx_max <= rx_max && vy_min >= ry_min && vy_max <= ry_max,
        "Los nuevos view_bounds deben caer dentro de la región cacheada para reusar"
    );
}

#[test]
fn fill_cache_invalidates_on_large_view_change() {
    // Un pan/zoom grande que mueve los view_bounds fuera de la región
    // padded debe invalidar el caché (producir una key diferente o fallar
    // el check de contención de región).
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);

    let view_bounds_1 = (-5.0, 5.0, -4.0, 4.0);
    let padded_1 = padded_snapped_bounds(view_bounds_1, PAD_FACTOR, SNAP_CELLS);
    let key1 = compute_fill_cache_key(&ic, padded_1, canvas_size, &vars, fill_color);

    // Pan grande: los view_bounds salen de la región cacheada.
    let view_bounds_2 = (-50.0, 50.0, -40.0, 40.0);
    let padded_2 = padded_snapped_bounds(view_bounds_2, PAD_FACTOR, SNAP_CELLS);
    let key2 = compute_fill_cache_key(&ic, padded_2, canvas_size, &vars, fill_color);

    assert_ne!(
        key1, key2,
        "Un cambio grande de vista debe invalidar el caché (key diferente)"
    );
}

#[test]
fn fill_cache_invalidates_on_expression_change() {
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let ic1 = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let ic2 = ImplicitCurveObj::new("x^2 + y^2", "4", RelationOperator::Less);

    let key1 = compute_fill_cache_key(&ic1, padded, canvas_size, &vars, fill_color);
    let key2 = compute_fill_cache_key(&ic2, padded, canvas_size, &vars, fill_color);

    assert_ne!(key1, key2, "Cambiar la expresión debe invalidar el caché");
}

#[test]
fn fill_cache_invalidates_on_operator_change() {
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let ic_less = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let ic_greater = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Greater);

    let key_less = compute_fill_cache_key(&ic_less, padded, canvas_size, &vars, fill_color);
    let key_greater = compute_fill_cache_key(&ic_greater, padded, canvas_size, &vars, fill_color);

    assert_ne!(
        key_less, key_greater,
        "Cambiar el operador de relación debe invalidar el caché"
    );
}

#[test]
fn fill_cache_invalidates_on_canvas_size_change() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::<String, f64>::new();
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let key_800x600 = compute_fill_cache_key(&ic, padded, (800, 600), &vars, fill_color);
    let key_1920x1080 = compute_fill_cache_key(&ic, padded, (1920, 1080), &vars, fill_color);

    assert_ne!(
        key_800x600, key_1920x1080,
        "Cambiar el tamaño del canvas debe invalidar el caché"
    );
}

#[test]
fn fill_cache_invalidates_on_fill_color_change() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let vars = HashMap::<String, f64>::new();
    let canvas_size = (800_u32, 600_u32);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let color_a = Color::new(0.4, 0.3, 0.8, 0.5);
    let color_b = Color::new(0.1, 0.9, 0.2, 0.5);

    let key_a = compute_fill_cache_key(&ic, padded, canvas_size, &vars, color_a);
    let key_b = compute_fill_cache_key(&ic, padded, canvas_size, &vars, color_b);

    assert_ne!(
        key_a, key_b,
        "Cambiar el color de relleno debe invalidar el caché"
    );
}

#[test]
fn fill_cache_invalidates_on_variables_change() {
    let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
    let fill_color = Color::new(0.4, 0.3, 0.8, 0.5);
    let canvas_size = (800_u32, 600_u32);
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let padded = padded_snapped_bounds(view_bounds, PAD_FACTOR, SNAP_CELLS);

    let mut vars_empty = HashMap::new();
    let key_empty = compute_fill_cache_key(&ic, padded, canvas_size, &vars_empty, fill_color);

    vars_empty.insert("r".to_string(), 2.0);
    let key_with_var = compute_fill_cache_key(&ic, padded, canvas_size, &vars_empty, fill_color);

    assert_ne!(
        key_empty, key_with_var,
        "Cambiar las variables del documento debe invalidar el caché"
    );
}

#[test]
fn fill_cache_region_containment_check() {
    // Simula la lógica de contención usada en draw_implicit_curve_fill:
    // si los view_bounds actuales caen dentro de cached_region, se reusa.
    let view_bounds_initial = (-5.0, 5.0, -4.0, 4.0);
    let cached_region = padded_snapped_bounds(view_bounds_initial, PAD_FACTOR, SNAP_CELLS);
    let (rx_min, rx_max, ry_min, ry_max) = cached_region;

    // Mismos view_bounds: debe estar dentro.
    let (vx, vx2, vy, vy2) = view_bounds_initial;
    assert!(
        vx >= rx_min && vx2 <= rx_max && vy >= ry_min && vy2 <= ry_max,
        "Los view_bounds originales deben estar dentro de la región cacheada"
    );

    // View_bounds fuera de la región (pan grande).
    let (vx, _, vy, _) = (-100.0, 100.0, -80.0, 80.0);
    assert!(
        !(vx >= rx_min && vy >= ry_min),
        "Un pan grande debe fallar el check de contención de región"
    );
}

#[test]
fn fill_cache_texture_size_covers_padded_region_at_view_density() {
    let view_bounds = (-5.0, 5.0, -4.0, 4.0);
    let cached_region = (-10.0, 10.0, -8.0, 8.0);

    let size = fill_cache_texture_size(view_bounds, cached_region, (800, 600));

    assert_eq!(size, (1600, 1200));
}

#[test]
fn fill_cache_texture_size_is_capped_for_extreme_padding() {
    let view_bounds = (-1.0, 1.0, -1.0, 1.0);
    let cached_region = (-1000.0, 1000.0, -1000.0, 1000.0);

    let size = fill_cache_texture_size(view_bounds, cached_region, (800, 600));

    assert_eq!(size, (4096, 4096));
}
