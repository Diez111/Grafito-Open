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
//! — Sistema base 4 para spacing, ratio tipográfico 1.25.

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
// ratio 1.25 (Major Third) — calm progression
// ═══════════════════════════════════════════════════════════

/// Texto doble-extra-pequeño: micro hints, eye-tracking status.
/// Piso mínimo 11.0 (legibilidad: 9.0 viola el mínimo accesible).
// MIGRATION: 9.0→11.0, update pou.rs/ui.rs in next phase.
pub const TYPE_2XS: f32 = 11.0;
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
// base 4 — todo múltiplo de 4
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
// Alphas — Scandinavian quiet overlays
// ═══════════════════════════════════════════════════════════

/// Alpha sombra — canonical 8 (~3 %) — alias de SHADOW_ALPHA.
/// Usar `Color32::from_black_alpha(ALPHA_SHADOW)` para sombras sutiles.
pub const ALPHA_SHADOW: u8 = 8;
/// Alpha overlay — 25 (~10 %) para hover sutil / scrim ligero.
pub const ALPHA_OVERLAY: u8 = 25;
/// Alpha separator referencia — 18 (~7 %) para strokes from_black_alpha.
/// Nota: el separator canónico en `theme.rs` usa `separator.gamma_multiply(0.10)`
/// (10 % hairline sobre #E8E8E6) — preferir gamma_multiply sobre from_black_alpha
/// para bordes; ALPHA_SEPARATOR queda como referencia histórica y para strokes
/// donde gamma no aplica (p.ej. `Color32::from_black_alpha(ALPHA_SEPARATOR)`).
pub const ALPHA_SEPARATOR: u8 = 18;

// ═══════════════════════════════════════════════════════════
// Layout — breakpoints (Scandinavian responsive)
// ═══════════════════════════════════════════════════════════

/// Breakpoint compact — 1360 px.
/// Reemplaza 3× hardcodeados 1360:
/// - `COMPACT_TOOLBAR_MAX_WIDTH` (toolbar.rs)
/// - `COMPACT_TOP_CHROME_MAX_WIDTH` (ui.rs)
/// - `ShellLayout::CANVAS_FOCUS_MAX_WIDTH` (lib.rs)
///
/// Por debajo → overflow compact (solo Move + grupo activo + "Más");
/// por encima → toolbar completa y drawers simultáneos.
pub const BREAKPOINT_COMPACT: f32 = 1360.0;

// ═══════════════════════════════════════════════════════════
// Layout — paneles laterales y drawers
// ═══════════════════════════════════════════════════════════

/// Ancho por defecto panel izquierdo (Álgebra/CAS/Vista) — 260 px.
pub const PANEL_LEFT_DEFAULT: f32 = 260.0;
/// Ancho mínimo panel izquierdo — 180 px.
pub const PANEL_LEFT_MIN: f32 = 180.0;
/// Fracción máxima del viewport para panel izquierdo — 0.45 (45 %).
/// Uso: `max_width = (available_width * PANEL_LEFT_MAX_FRACTION).max(200.0)`
pub const PANEL_LEFT_MAX_FRACTION: f32 = 0.45;

/// Drawer derecho (Inspector/Utilidad Geometry 3D) — ancho por defecto 344 px.
pub const DRAWER_RIGHT_DEFAULT: f32 = 344.0;
/// Drawer derecho — ancho mínimo 292 px.
pub const DRAWER_RIGHT_MIN: f32 = 292.0;
/// Drawer derecho — ancho máximo 440 px.
pub const DRAWER_RIGHT_MAX: f32 = 440.0;

/// Ancho rail lateral izquierdo (icon bar 60 px) — Scandinavian single rail.
pub const RAIL_WIDTH: f32 = 60.0;

// ═══════════════════════════════════════════════════════════
// Cards — Scandinavian quiet surfaces
// ═══════════════════════════════════════════════════════════

/// Espacio entre cards — 12.0 = SPACE_MD (base 4).
pub const CARD_SPACING: f32 = SPACE_MD;
/// Radio card de objeto — 8.0 = RADIUS_SM.
pub const OBJECT_CARD_RADIUS: f32 = RADIUS_SM;
/// Radio card inspector/sección — 12.0 = RADIUS_MD.
pub const INSPECTOR_CARD_RADIUS: f32 = RADIUS_MD;

// ═══════════════════════════════════════════════════════════
// Splash
// ═══════════════════════════════════════════════════════════

/// Tamaño logo splash — 128 px cuadrado.
pub const SPLASH_LOGO_SIZE: f32 = 128.0;

// ═══════════════════════════════════════════════════════════
// Helpers — layout functions (Scandinavian, sin hardcodes)
// ═══════════════════════════════════════════════════════════

/// Indica si el viewport exige layout compacto (≤ BREAKPOINT_COMPACT).
#[inline]
pub fn is_compact_viewport(width: f32) -> bool {
    width <= BREAKPOINT_COMPACT
}

/// Ancho máximo permitido para panel izquierdo dado el ancho disponible.
/// Clamp inferior 200 px evita drawer inutilizable en viewports estrechos.
#[inline]
pub fn panel_left_max_width(available_width: f32) -> f32 {
    (available_width * PANEL_LEFT_MAX_FRACTION).max(200.0)
}

/// Ancho clamped para panel izquierdo (min .. max dinámico).
#[inline]
pub fn clamp_panel_left_width(requested: f32, available_width: f32) -> f32 {
    requested.clamp(PANEL_LEFT_MIN, panel_left_max_width(available_width))
}

/// Ancho clamped para drawer derecho (292 .. 440).
#[inline]
pub fn clamp_drawer_right_width(requested: f32) -> f32 {
    requested.clamp(DRAWER_RIGHT_MIN, DRAWER_RIGHT_MAX)
}

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
        // TYPE_2XS == TYPE_XS == 11.0: piso mínimo, ya no estrictamente menor.
        assert!(TYPE_2XS <= TYPE_XS);
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

    // ── New tokens — breakpoint & panel relationships ──

    #[test]
    fn breakpoint_compact_is_canonical_1360() {
        assert_eq!(BREAKPOINT_COMPACT, 1360.0);
        // Helper debe coincidir con el breakpoint
        assert!(is_compact_viewport(1360.0));
        assert!(is_compact_viewport(960.0));
        assert!(!is_compact_viewport(1361.0));
        assert!(!is_compact_viewport(1680.0));
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn panel_left_relationships_hold() {
        assert!(PANEL_LEFT_MIN < PANEL_LEFT_DEFAULT);
        assert_eq!(PANEL_LEFT_DEFAULT, 260.0);
        assert_eq!(PANEL_LEFT_MIN, 180.0);
        assert!((PANEL_LEFT_MAX_FRACTION - 0.45).abs() < f32::EPSILON);
        assert!(PANEL_LEFT_MAX_FRACTION > 0.0 && PANEL_LEFT_MAX_FRACTION < 1.0);
        // Default cabe dentro del max para viewport típico 1280
        let max_1280 = panel_left_max_width(1280.0);
        assert!(PANEL_LEFT_DEFAULT <= max_1280);
        assert!(max_1280 >= 200.0);
        // Clamp respeta min/max
        assert_eq!(clamp_panel_left_width(100.0, 1280.0), PANEL_LEFT_MIN);
        assert_eq!(clamp_panel_left_width(1000.0, 1280.0), max_1280);
        assert_eq!(clamp_panel_left_width(260.0, 1280.0), 260.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn drawer_right_relationships_hold() {
        assert!(DRAWER_RIGHT_MIN < DRAWER_RIGHT_DEFAULT);
        assert!(DRAWER_RIGHT_DEFAULT < DRAWER_RIGHT_MAX);
        assert_eq!(DRAWER_RIGHT_DEFAULT, 344.0);
        assert_eq!(DRAWER_RIGHT_MIN, 292.0);
        assert_eq!(DRAWER_RIGHT_MAX, 440.0);
        assert_eq!(clamp_drawer_right_width(200.0), DRAWER_RIGHT_MIN);
        assert_eq!(clamp_drawer_right_width(500.0), DRAWER_RIGHT_MAX);
        assert_eq!(clamp_drawer_right_width(344.0), 344.0);
    }

    #[test]
    fn rail_and_splash_use_scandinavian_tokens() {
        assert_eq!(RAIL_WIDTH, 60.0);
        assert_eq!(SPLASH_LOGO_SIZE, 128.0);
        // Splash es múltiplo de base 4 y cuadrado
        assert_eq!(SPLASH_LOGO_SIZE % 4.0, 0.0);
        assert_eq!(RAIL_WIDTH % 4.0, 0.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn alphas_match_scandinavian_quiet() {
        assert_eq!(ALPHA_SHADOW, 8);
        assert_eq!(SHADOW_ALPHA, ALPHA_SHADOW);
        assert_eq!(ALPHA_OVERLAY, 25);
        assert_eq!(ALPHA_SEPARATOR, 18);
        // Shadow < separator < overlay (sutil → visible)
        assert!(ALPHA_SHADOW < ALPHA_SEPARATOR);
        assert!(ALPHA_SEPARATOR < ALPHA_OVERLAY);
    }

    #[test]
    fn card_tokens_are_canonical_aliases() {
        assert_eq!(CARD_SPACING, SPACE_MD);
        assert_eq!(CARD_SPACING, 12.0);
        assert_eq!(OBJECT_CARD_RADIUS, RADIUS_SM);
        assert_eq!(OBJECT_CARD_RADIUS, 8.0);
        assert_eq!(INSPECTOR_CARD_RADIUS, RADIUS_MD);
        assert_eq!(INSPECTOR_CARD_RADIUS, 12.0);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn spacing_uses_base_4() {
        for v in [
            SPACE_XS,
            SPACE_SM,
            SPACE_MD,
            SPACE_LG,
            SPACE_XL,
            SPACE_XXL,
            SPACING_MINIMAL_X,
            SPACING_MINIMAL_Y,
            SPACING_BUTTON_X,
            SPACING_BUTTON_Y,
            CARD_SPACING,
            RAIL_WIDTH,
            SPLASH_LOGO_SIZE,
        ] {
            assert_eq!(v % 4.0, 0.0, "spacing value {v} must be multiple of base 4");
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_scale_ratio_stays_near_1_25() {
        // Ratio 1.25 Major Third — verificar que la progresión no se rompa.
        // No exacto por redondeo Scandinavian (12/15/19), pero cercano.
        let ratio_sm_base = TYPE_BASE / TYPE_SM; // 15/12 = 1.25 exact
        let ratio_base_lg = TYPE_LG / TYPE_BASE; // 19/15 ≈ 1.266
        assert!((ratio_sm_base - 1.25).abs() < 0.01);
        assert!((ratio_base_lg - 1.25).abs() < 0.05);
        // Monotonic ya verificado, aquí sólo ratio
        assert!(TYPE_SM < TYPE_BASE && TYPE_BASE < TYPE_LG);
    }
}
