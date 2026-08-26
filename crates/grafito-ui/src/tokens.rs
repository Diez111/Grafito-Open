//! Design tokens — Scandinavian (Grafito).
//!
//! Estética: calm, restraint, luz natural. Tipografía Inter, paleta
//! cálida neutra (canvas #FAFAF9), acento sage #6B7A6F. Sin sombras
//! duras ni translucidez. Todo opaco (`from_rgb`).
//!
//! F5 quiet 2026-08-21: ink secondary 64% y tertiary 44%, border 10%,
//! hover 5% — definidos en `theme.rs` vía gamma_multiply (ver `Theme`);
//! tokens aquí fijan spacing/radios, no colores.
//!
//! Estos tokens son la **única fuente de verdad** para tamaños y
//! espacios. Ningún `.size(N)` o `vec2(N,M)` hardcodeado fuera de aquí.

// ═══════════════════════════════════════════════════════════
// Familias tipográficas — Scandinavian
// ═══════════════════════════════════════════════════════════

/// Inter — familia principal Scandinavian (sans geométrica humana).
pub const FONT_SF_TEXT: &str = "Inter";
/// Inter — alias display, misma familia (sin SF Pro Display).
pub const FONT_SF_DISPLAY: &str = "Inter";
/// SF Mono — código, expresiones y valores.
pub const FONT_SF_MONO: &str = "SF Mono";
/// Fallback sans del sistema cuando Inter no está disponible.
pub const FONT_FALLBACK_SANS: &str = "Inter";

// ═══════════════════════════════════════════════════════════
// Type scale — Scandinavian (Inter 12 / 15 / 19)
// ═══════════════════════════════════════════════════════════

/// Texto doble-extra-pequeño: micro hints, eye-tracking status.
pub const TYPE_2XS: f32 = 9.0;
/// Texto extra-pequeño: notas, metadatos, hints.
pub const TYPE_XS: f32 = 11.0;
/// Texto pequeño: labels secundarios, captions — Inter 12.
pub const TYPE_SM: f32 = 12.0;
/// Texto base: cuerpo, inputs — Inter 15 (Scandinavian body 15).
pub const TYPE_BASE: f32 = 15.0;
/// Texto mediano: labels destacados, sub-headers — Inter 16.
pub const TYPE_MD: f32 = 16.0;
/// Texto grande: headers de panel — Inter 19.
pub const TYPE_LG: f32 = 19.0;
/// Texto extra-grande: titles — Scandinavian 24.
pub const TYPE_XL: f32 = 24.0;
/// Texto doble-extra-grande: splash, branding.
pub const TYPE_XXL: f32 = 28.0;

// ═══════════════════════════════════════════════════════════
// Spacing scale — Scandinavian (16 / 24 / 40)
// ═══════════════════════════════════════════════════════════

/// Espacio extra-pequeño: entre items muy cercanos.
pub const SPACE_XS: f32 = 4.0;
/// Espacio pequeño: entre items de un grupo.
pub const SPACE_SM: f32 = 8.0;
/// Espacio mediano: padding interno de chips.
pub const SPACE_MD: f32 = 12.0;
/// Espacio grande: padding interno de paneles — Scandinavian 16.
pub const SPACE_LG: f32 = 16.0;
/// Espacio extra-grande: separación entre secciones — Scandinavian 24.
pub const SPACE_XL: f32 = 24.0;
/// Espacio doble-extra-grande: separación entre paneles — Scandinavian 40.
pub const SPACE_XXL: f32 = 40.0;

/// Ritmo Scandinavian horizontal (item_spacing.x).
pub const SPACING_MINIMAL_X: f32 = 16.0;
/// Ritmo Scandinavian vertical (item_spacing.y).
pub const SPACING_MINIMAL_Y: f32 = 16.0;

/// Button padding Scandinavian — 16 x 8.
pub const SPACING_BUTTON_X: f32 = 16.0;
pub const SPACING_BUTTON_Y: f32 = 8.0;

// ═══════════════════════════════════════════════════════════
// Radii — Scandinavian (8 / 12 / 16)
// ═══════════════════════════════════════════════════════════

pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
pub const RADIUS_LG: f32 = 16.0;
pub const RADIUS_XL: f32 = 16.0;
pub const RADIUS_2XL: f32 = 16.0;
pub const RADIUS_PILL: f32 = 999.0;

// ═══════════════════════════════════════════════════════════
// Tamaños de íconos
// ═══════════════════════════════════════════════════════════

pub const ICON_SM: f32 = 16.0;
pub const ICON_MD: f32 = 20.0;
pub const ICON_LG: f32 = 24.0;
pub const ICON_XL: f32 = 32.0;

// ═══════════════════════════════════════════════════════════
// Sombras — Scandinavian (0,2,8 alpha 8) calm, sin drama
// ═══════════════════════════════════════════════════════════

/// Offset Y sombra ventana — 2 px.
pub const SHADOW_WINDOW_OFFSET_Y: f32 = 2.0;
/// Blur sombra ventana — 8 px.
pub const SHADOW_WINDOW_BLUR: f32 = 8.0;
/// Offset Y sombra popup — 2 px.
pub const SHADOW_POPUP_OFFSET_Y: f32 = 2.0;
/// Blur sombra popup — 8 px.
pub const SHADOW_POPUP_BLUR: f32 = 8.0;
/// Alpha sombra — 8 (~3 %) sutil, Scandinavian restraint.
pub const SHADOW_ALPHA: u8 = 8;

// ═══════════════════════════════════════════════════════════
// Top bar — Scandinavian single bar
// ═══════════════════════════════════════════════════════════

/// Altura única top bar Scandinavian — 48 px single bar.
pub const TOP_BAR_HEIGHT: f32 = 48.0;

// ═══════════════════════════════════════════════════════════
// Zoom — rangos y pill (Scandinavian, opaco, sin translucidez)
// Geogebra-like infinito: 12 órdenes (1e-6..1e6) pizarra, 15 órdenes (1e-6..1e9) 3D
// ═══════════════════════════════════════════════════════════

pub const ZOOM_WB_MIN: f64 = 1e-6;
pub const ZOOM_WB_MAX: f64 = 1e6;
pub const ZOOM_WB_DEFAULT: f64 = 1.0;
pub const ZOOM_3D_MIN: f32 = 1e-6;
pub const ZOOM_3D_MAX: f32 = 1e9;
pub const ZOOM_3D_DEFAULT: f32 = 10.0;
pub const ZOOM_PILL_RADIUS: f32 = RADIUS_SM;
pub const ZOOM_PILL_PAD_X: f32 = SPACE_XS;
pub const ZOOM_PILL_PAD_Y: f32 = SPACE_XS;
pub const ZOOM_PILL_GAP: f32 = SPACE_XS;
pub const ZOOM_PCT_MIN_W: f32 = 52.0;
pub const ZOOM_ICON_HIT: f32 = 32.0;

// ═══════════════════════════════════════════════════════════
// Animation timings
// ═══════════════════════════════════════════════════════════

/// Duración estándar de transiciones (ms).
pub const ANIM_FAST: f32 = 100.0;
/// Duración de respuestas visuales a hover y selección (ms).
pub const ANIM_MICRO: f32 = 180.0;
/// Duración de animaciones de creación/feedback.
pub const ANIM_NORMAL: f32 = 200.0;
/// Duración de highlights (e.g. objeto recién creado).
pub const ANIM_HIGHLIGHT: f32 = 1000.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_scale_is_monotonic() {
        assert!(TYPE_2XS < TYPE_XS);
        assert!(TYPE_XS < TYPE_SM);
        assert!(TYPE_SM < TYPE_BASE);
        assert!(TYPE_BASE < TYPE_MD);
        assert!(TYPE_MD < TYPE_LG);
        assert!(TYPE_LG < TYPE_XL);
        assert!(TYPE_XL < TYPE_XXL);
    }

    #[test]
    fn type_scale_uses_scandinavian_sizes() {
        assert_eq!(TYPE_SM, 12.0);
        assert_eq!(TYPE_BASE, 15.0);
        assert_eq!(TYPE_LG, 19.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn spacing_scale_is_monotonic() {
        assert!(SPACE_XS < SPACE_SM);
        assert!(SPACE_SM < SPACE_MD);
        assert!(SPACE_MD < SPACE_LG);
        assert!(SPACE_LG < SPACE_XL);
        assert!(SPACE_XL < SPACE_XXL);
    }

    #[test]
    fn spacing_uses_scandinavian_scale() {
        assert_eq!(SPACE_LG, 16.0);
        assert_eq!(SPACE_XL, 24.0);
        assert_eq!(SPACE_XXL, 40.0);
        assert_eq!(SPACING_MINIMAL_X, 16.0);
        assert_eq!(SPACING_MINIMAL_Y, 16.0);
        assert_eq!(SPACING_BUTTON_X, 16.0);
        assert_eq!(SPACING_BUTTON_Y, 8.0);
    }

    #[test]
    fn radii_use_scandinavian_scale() {
        assert_eq!(RADIUS_SM, 8.0);
        assert_eq!(RADIUS_MD, 12.0);
        assert_eq!(RADIUS_LG, 16.0);
        assert_eq!(RADIUS_XL, 16.0);
        assert_eq!(RADIUS_2XL, 16.0);
    }

    #[test]
    fn shadows_use_scandinavian_values() {
        assert_eq!(SHADOW_WINDOW_OFFSET_Y, 2.0);
        assert_eq!(SHADOW_WINDOW_BLUR, 8.0);
        assert_eq!(SHADOW_POPUP_OFFSET_Y, 2.0);
        assert_eq!(SHADOW_POPUP_BLUR, 8.0);
        assert_eq!(SHADOW_ALPHA, 8);
        assert_eq!(TOP_BAR_HEIGHT, 48.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn micro_interaction_timing_stays_between_fast_and_normal_feedback() {
        assert!(ANIM_FAST < ANIM_MICRO);
        assert!(ANIM_MICRO < ANIM_NORMAL);
    }
}
