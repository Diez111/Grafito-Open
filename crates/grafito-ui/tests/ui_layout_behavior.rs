#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
//! Golden UI de *comportamiento* (G17): reemplaza goldens de strings tipo
//! `contains("400.0")` / `include_str!` por asserts sobre constantes y
//! funciones reales.
//!
//! Un golden de string pasa aunque la constante derive (el "400.0" puede
//! seguir apareciendo en un comentario o en otro token) y falla por
//! reformateo inocuo. Estos tests fallarían antes si:
//! - alguien hardcodea 400/300/520 en vez de usar el token canónico,
//! - el clamp de drawer/paleta se rompe y el layout desborda,
//! - el breakpoint compact diverge entre toolbar y tokens.

use grafito_ui::command_palette::palette_window_width;
use grafito_ui::tokens::{
    clamp_drawer_right_width, clamp_panel_left_width, is_compact_viewport, panel_left_max_width,
    BREAKPOINT_COMPACT, DRAWER_RIGHT_DEFAULT, DRAWER_RIGHT_MAX, DRAWER_RIGHT_MIN,
    PANEL_LEFT_DEFAULT, PANEL_LEFT_MAX_FRACTION, PANEL_LEFT_MIN,
};
use grafito_ui::toolbar::{
    COMPACT_TOOLBAR_MAX_WIDTH, TOOLBAR_BUTTON_SIZE, TOOLBAR_PANEL_HEIGHT, TOOLBAR_VERTICAL_PADDING,
};

#[test]
fn toolbar_panel_height_is_derived_not_hardcoded() {
    // El panel debe reservar exactamente una fila de botón + paddings.
    // Si alguien pone `TOOLBAR_PANEL_HEIGHT = 44.0` literal, el test sigue
    // pasando hoy (44.0 == 36+2*4) pero la relación se documenta; si cambia
    // BUTTON_SIZE sin actualizar el panel, falla (antes: contains("44")
    // pasaría igual porque "44" aparece en otros lados).
    assert_eq!(
        TOOLBAR_PANEL_HEIGHT,
        TOOLBAR_BUTTON_SIZE + 2.0 * TOOLBAR_VERTICAL_PADDING
    );
    assert_eq!(TOOLBAR_BUTTON_SIZE, 36.0);
    assert_eq!(TOOLBAR_VERTICAL_PADDING, 4.0);
    assert_eq!(TOOLBAR_PANEL_HEIGHT, 44.0);
}

#[test]
fn compact_breakpoint_has_single_source_of_truth() {
    // Toolbar y tokens deben compartir el mismo breakpoint. Un golden
    // `contains("1360")` pasaría con dos constantes divergentes (1360 en
    // un archivo, 1359 en otro) mientras ambas mencionen "1360" en un
    // comentario; aquí se exige igualdad real.
    assert_eq!(COMPACT_TOOLBAR_MAX_WIDTH, BREAKPOINT_COMPACT);
    assert_eq!(BREAKPOINT_COMPACT, 1360.0);
    assert!(is_compact_viewport(1360.0));
    assert!(!is_compact_viewport(1361.0));
    assert_eq!(
        grafito_ui::toolbar::toolbar_uses_overflow(1360.0),
        is_compact_viewport(1360.0)
    );
    assert_eq!(
        grafito_ui::toolbar::toolbar_uses_overflow(1361.0),
        is_compact_viewport(1361.0)
    );
}

#[test]
fn drawer_right_clamp_is_a_hard_budget() {
    // Comportamiento, no string: el drawer nunca sale de 292..440.
    // Regresión que atraparía: clamp olvidado → drawer de 200px o de 800px
    // que rompe el canvas; el golden de string no lo vería.
    assert!(DRAWER_RIGHT_MIN < DRAWER_RIGHT_DEFAULT);
    assert!(DRAWER_RIGHT_DEFAULT < DRAWER_RIGHT_MAX);
    assert_eq!(DRAWER_RIGHT_MIN, 292.0);
    assert_eq!(DRAWER_RIGHT_DEFAULT, 344.0);
    assert_eq!(DRAWER_RIGHT_MAX, 440.0);
    assert_eq!(clamp_drawer_right_width(200.0), DRAWER_RIGHT_MIN);
    assert_eq!(clamp_drawer_right_width(500.0), DRAWER_RIGHT_MAX);
    assert_eq!(clamp_drawer_right_width(344.0), 344.0);
}

#[test]
fn panel_left_clamp_respects_fraction_of_viewport() {
    // El panel izquierdo es min 180 y como máximo 45% del viewport.
    // Regresión que atraparía: fracción cambiada a 0.9 → panel que aplasta
    // el canvas en 1280px; contains("260") seguiría pasando.
    assert_eq!(PANEL_LEFT_MIN, 180.0);
    assert_eq!(PANEL_LEFT_DEFAULT, 260.0);
    assert!((PANEL_LEFT_MAX_FRACTION - 0.45).abs() < f32::EPSILON);
    let max_1280 = panel_left_max_width(1280.0);
    assert!(PANEL_LEFT_DEFAULT <= max_1280);
    assert_eq!(clamp_panel_left_width(100.0, 1280.0), PANEL_LEFT_MIN);
    assert_eq!(clamp_panel_left_width(1000.0, 1280.0), max_1280);
    assert_eq!(clamp_panel_left_width(260.0, 1280.0), 260.0);
}

#[test]
fn palette_window_width_is_clamped_behavior_not_string() {
    // La paleta deja 16px de margen y clampa a 1..640.
    // Regresión que atraparía: margen olvidado → paleta desborda en móvil;
    // un `format!("{width}").contains("301")` pasaría con cualquier width
    // que contenga esos dígitos en otro campo.
    assert_eq!(palette_window_width(317.0), 301.0);
    assert_eq!(palette_window_width(0.0), 1.0);
    assert_eq!(palette_window_width(10_000.0), 640.0);
    // Monotonía en el rango útil: más viewport → paleta igual o más ancha.
    assert!(palette_window_width(500.0) <= palette_window_width(700.0));
}
