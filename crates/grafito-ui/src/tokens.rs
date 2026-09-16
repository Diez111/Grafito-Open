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
// Piso mínimo ya aplicado (11.0).
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
/// Título de card overlay (círculo unitario, animación compleja) — Inter 13.
/// Ola 2: faltaba en la escala; evita el literal `13.0` en draws.
pub const TYPE_CARD_TITLE: f32 = 13.0;
/// Anotación flotante (hover del canvas, ejes 3D, teclas Del/Enter) — Inter 14.
/// Ola 2: faltaba en la escala; evita el literal `14.0` en draws.
pub const TYPE_ANNOTATION: f32 = 14.0;

// ═══════════════════════════════════════════════════════════
// Spacing scale — Scandinavian (16 / 24 / 40)
// base 4 — todo múltiplo de 4
// ═══════════════════════════════════════════════════════════

/// Espacio extra-pequeño: entre items muy cercanos.
pub const SPACE_XS: f32 = 4.0;
/// Espacio mínimo: separadores densos (toolbar, locale) — 2 px.
/// Ola 2: evita el literal `2.0` en `item_spacing` densos.
pub const SPACE_XXS: f32 = 2.0;
/// Espacio pequeño: entre items de un grupo.
pub const SPACE_SM: f32 = 8.0;
/// Intermedio 10 px: offsets de texto en cards overlay del canvas.
/// Ola 2: evita el literal `10.0` en `render_2d`.
pub const SPACE_SM_PLUS: f32 = 10.0;
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

/// Radio extra-pequeño: teclas del teclado en pantalla — 4 px.
/// Ola 2: completa la escala (4 / 8 / 12 / 16); evita `rect(r, 4.0, …)`.
pub const RADIUS_XS: f32 = 4.0;
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

/// Ancho rail lateral izquierdo (icon bar 68 px) — Scandinavian single rail.
/// 68 = múltiplo de base 4; deja 4 px de respiro por lado para que el borde
/// de la burbuja activa nunca quede clipado por el panel.
pub const RAIL_WIDTH: f32 = 68.0;
/// Alto de cada tab del rail — 60 px (icono 20 + etiqueta 11 + aire).
pub const RAIL_ITEM_HEIGHT: f32 = 60.0;
/// Respiro horizontal del item dentro del rail — 4 px por lado.
pub const RAIL_ITEM_PAD_X: f32 = SPACE_XS;

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
// Ola 2 — literales de draws centralizados (sin magia en Piel)
// ═══════════════════════════════════════════════════════════

/// Ancho del breakpoint angosto del preview de adjuntos del asistente.
/// `assistant.rs`: preview arriba si `available_width < 720`.
pub const ASSISTANT_PREVIEW_NARROW_BREAKPOINT: f32 = 720.0;

/// Botón del selector de idioma del toolbar — 36×24 mínimo táctil.
pub const TOOLBAR_LOCALE_MIN_W: f32 = 36.0;
/// Alto mínimo del botón de idioma — 24 px (piso táctil reducido, texto 11).
pub const TOOLBAR_LOCALE_MIN_H: f32 = 24.0;
/// Padding horizontal interno del frame del toolbar — 4 px = SPACE_XS.
pub const TOOLBAR_INNER_PAD_X: f32 = SPACE_XS;

/// Alto de tecla del teclado en pantalla — 32 px (piso táctil).
pub const KEYBOARD_KEY_H: f32 = 32.0;
/// Ancho máximo de chip del teclado — 40 px (clamp superior).
pub const KEYBOARD_CHIP_W_MAX: f32 = 40.0;

/// Ancho de la ventana de onboarding — 420 px Scandinavian.
pub const ONBOARDING_WINDOW_WIDTH: f32 = 420.0;
/// Botón de onboarding — 120×32.
pub const ONBOARDING_BUTTON_W: f32 = 120.0;
pub const ONBOARDING_BUTTON_H: f32 = KEYBOARD_KEY_H;
/// Separación entre botones de onboarding — 8 px = SPACE_SM.
pub const ONBOARDING_BUTTON_GAP: f32 = SPACE_SM;
/// Padding horizontal interno de la ventana de onboarding — 20 px.
pub const ONBOARDING_INNER_PAD_X: f32 = 20.0;

/// Campos del editor de rango del deslizador — 64×20: columna alineada,
/// compacta, múltiplos de base 4.
pub const RANGE_FIELD_W: f32 = 64.0;
/// Alto del campo de rango — 20 px.
pub const RANGE_FIELD_H: f32 = 20.0;
/// Alto de la acción al pie del popup (Borrar) — 28 px: táctil, base 4.
pub const POPUP_ACTION_H: f32 = 28.0;
/// Ancho mínimo del contenido del popup de rango — 140 px: dos campos de
/// ~66 + gap, múltiplo de base 4 (la tarjeta se ciñe, no flota vacía).
pub const POPUP_MIN_W: f32 = 140.0;

/// Margen de las cards overlay del canvas 2D (círculo unitario, etc.) — 14 px.
pub const OVERLAY_CARD_MARGIN: f32 = 14.0;
/// Radio del marcador de hover con snap — 6 px.
pub const HOVER_MARKER_R_SNAP: f32 = 6.0;
/// Radio del marcador de hover libre — 4 px = SPACE_XS.
pub const HOVER_MARKER_R: f32 = SPACE_XS;
/// Anillo extra del marcador de hover — 1 px.
pub const HOVER_MARKER_RING: f32 = 1.0;

// ═══════════════════════════════════════════════════════════
// Paleta de comandos — Scandinavian quiet overlay
// ═══════════════════════════════════════════════════════════

/// Ancho máximo de la paleta — 640 px (lectura cómoda, no tapa el lienzo).
pub const PALETTE_MAX_WIDTH: f32 = 640.0;
/// Margen lateral que la paleta deja al viewport — 16 px = SPACE_LG.
pub const PALETTE_VIEWPORT_MARGIN: f32 = SPACE_LG;
/// Ancho mínimo de ventana (evita colapso en viewports angostos) — 1 px.
pub const PALETTE_MIN_WIDTH: f32 = 1.0;
/// Posición por defecto de la paleta: 8 px desde la izquierda…
pub const PALETTE_POS_X: f32 = SPACE_SM;
/// …y 48 px desde arriba (= TOP_BAR_HEIGHT, respira bajo la barra).
pub const PALETTE_POS_Y: f32 = TOP_BAR_HEIGHT;
/// Caja del icono de búsqueda — 20 px = ICON_MD (piso táctil visual).
pub const PALETTE_SEARCH_ICON: f32 = ICON_MD;
/// Altura reservada a cromo (búsqueda + estado + pie): la lista usa el
/// resto del viewport. 168 px = 42 × base 4 (antes 170, fuera de escala).
pub const PALETTE_LIST_RESERVED: f32 = 168.0;
/// Altura mínima de la lista — 120 px (muestra ~4 filas + aire).
pub const PALETTE_LIST_MIN_HEIGHT: f32 = 120.0;
/// Sangría del detalle de la fila seleccionada — 16 px = SPACE_LG.
pub const PALETTE_DETAIL_INDENT: f32 = SPACE_LG;
/// Paso de página en la paleta (PageUp/PageDown) — 10 filas.
pub const PALETTE_PAGE_STEP: usize = 10;

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

// ═══════════════════════════════════════════════════════════
// Accesibilidad aula (A11Y) — WCAG 2.2
// Única fuente para foco visible, hit-targets, tipo aula y toasts.
// ═══════════════════════════════════════════════════════════

/// Anillo de foco visible — ancho 2 px (WCAG 2.4.13: perímetro ≥ 2 px).
/// Ver `crate::theme::Theme::focus_ring_stroke`.
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// Anillo de foco — desplazamiento respecto al borde del widget, 2 px.
pub const FOCUS_RING_OFFSET: f32 = 2.0;
/// Anillo de foco — contraste mínimo contra el fondo adyacente, 3:1 (WCAG 1.4.11).
pub const FOCUS_RING_MIN_CONTRAST: f32 = 3.0;

/// Piso de atenuación gamma para texto — 0.85.
/// Ningún texto se atenúa por debajo del 85 %: `Theme::dimmed_text` hace
/// clamp de todo `gamma_multiply` a `[TEXT_GAMMA_FLOOR, 1.0]` (WCAG 1.4.3 AA).
pub const TEXT_GAMMA_FLOOR: f32 = 0.85;

/// Hit-target mínimo general — 24 px (WCAG 2.5.8 Target Size Minimum).
pub const HIT_TARGET_MIN: f32 = 24.0;
/// Hit-target modo aula/proyector y táctil — 44 px (WCAG 2.5.5 Enhanced).
pub const HIT_TARGET_AULA: f32 = 44.0;

/// Tamaño tipográfico mínimo en modo aula — 12 px (legible a distancia).
/// Ver `aula_font_size`.
pub const TYPE_MIN_AULA: f32 = 12.0;
/// Escala tipográfica modo aula — 1.25 (ratio Major Third del sistema).
pub const AULA_FONT_SCALE: f32 = 1.25;

/// Duración por defecto de toasts — 7 s (WCAG 2.2.1: tiempo de lectura).
pub const TOAST_DURATION_DEFAULT: f64 = 7.0;
/// Duración de toasts de error — persistente hasta dismiss con clic (sin auto-dismiss).
pub const TOAST_DURATION_ERROR: f64 = f64::INFINITY;
/// Fade-in de toasts — 0.2 s.
pub const TOAST_FADE_IN: f32 = 0.2;
/// Fade-out de toasts — 0.5 s.
pub const TOAST_FADE_OUT: f32 = 0.5;
/// Altura mínima de toast — 30 px (≥ HIT_TARGET_MIN: clicable para dismiss).
pub const TOAST_MIN_HEIGHT: f32 = 30.0;
/// Offset superior de la pila de toasts — 56 px = TOP_BAR_HEIGHT (48) + SPACE_SM (8).
pub const TOAST_TOP_OFFSET: f32 = TOP_BAR_HEIGHT + SPACE_SM;
/// Fracción máxima de pantalla para la pila — 0.25 (deja 3/4 libres al composer).
pub const TOAST_MAX_SCREEN_FRACTION: f32 = 0.25;

/// Clamp de tamaño interactivo al mínimo WCAG 2.5.8 (24 px).
#[inline]
pub fn hit_target_size(requested: f32) -> f32 {
    requested.max(HIT_TARGET_MIN)
}

/// Clamp de tamaño interactivo al mínimo aula/táctil (44 px).
#[inline]
pub fn aula_hit_target_size(requested: f32) -> f32 {
    requested.max(HIT_TARGET_AULA)
}

/// Tamaño tipográfico modo aula: escala 1.25 con piso 12 px.
#[inline]
pub fn aula_font_size(base: f32) -> f32 {
    (base * AULA_FONT_SCALE).max(TYPE_MIN_AULA)
}

#[cfg(test)]
// Onda 2: un solo allow a nivel módulo en vez de 11 por test. Estos tests
// pinnean relaciones entre constantes a propósito (p. ej. `TYPE_XS < TYPE_SM`),
// así que `clippy::assertions_on_constants` es un falso positivo aquí.
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
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
        assert_eq!(RAIL_WIDTH, 68.0);
        assert_eq!(RAIL_ITEM_HEIGHT, 60.0);
        assert_eq!(RAIL_ITEM_PAD_X, SPACE_XS);
        // El item respira 4 px por lado: el borde nunca toca el panel.
        assert_eq!(RAIL_WIDTH - 2.0 * RAIL_ITEM_PAD_X, 60.0);
        assert_eq!(SPLASH_LOGO_SIZE, 128.0);
        // Splash es múltiplo de base 4 y cuadrado
        assert_eq!(SPLASH_LOGO_SIZE % 4.0, 0.0);
        assert_eq!(RAIL_WIDTH % 4.0, 0.0);
    }

    #[test]
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
            RANGE_FIELD_W,
            RANGE_FIELD_H,
            POPUP_ACTION_H,
            POPUP_MIN_W,
        ] {
            assert_eq!(v % 4.0, 0.0, "spacing value {v} must be multiple of base 4");
        }
    }

    #[test]
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

    #[test]
    fn focus_ring_tokens_meet_wcag_minimums() {
        assert_eq!(FOCUS_RING_WIDTH, 2.0);
        assert_eq!(FOCUS_RING_OFFSET, 2.0);
        assert_eq!(FOCUS_RING_MIN_CONTRAST, 3.0);
    }

    #[test]
    fn text_gamma_floor_preserves_contrast() {
        assert_eq!(TEXT_GAMMA_FLOOR, 0.85);
        assert!(TEXT_GAMMA_FLOOR > 0.0 && TEXT_GAMMA_FLOOR <= 1.0);
    }

    #[test]
    fn hit_targets_meet_wcag_sizes() {
        assert_eq!(HIT_TARGET_MIN, 24.0);
        assert_eq!(HIT_TARGET_AULA, 44.0);
        assert!(HIT_TARGET_MIN < HIT_TARGET_AULA);
        assert_eq!(hit_target_size(16.0), HIT_TARGET_MIN);
        assert_eq!(hit_target_size(32.0), 32.0);
        assert_eq!(aula_hit_target_size(24.0), HIT_TARGET_AULA);
        assert_eq!(aula_hit_target_size(48.0), 48.0);
    }

    #[test]
    fn aula_type_has_floor_and_scale() {
        assert_eq!(TYPE_MIN_AULA, 12.0);
        assert_eq!(AULA_FONT_SCALE, 1.25);
        assert_eq!(aula_font_size(TYPE_BASE), TYPE_BASE * AULA_FONT_SCALE);
        // Piso: bases pequeñas no bajan de 12 px.
        assert_eq!(aula_font_size(9.0), TYPE_MIN_AULA);
        assert_eq!(aula_font_size(TYPE_XS), TYPE_XS * AULA_FONT_SCALE);
    }

    #[test]
    fn toast_durations_give_reading_time_and_persistent_errors() {
        assert_eq!(TOAST_DURATION_DEFAULT, 7.0);
        assert!(TOAST_DURATION_ERROR.is_infinite());
        assert!(TOAST_DURATION_DEFAULT < TOAST_DURATION_ERROR);
        assert!(TOAST_MIN_HEIGHT >= HIT_TARGET_MIN);
        assert_eq!(TOAST_TOP_OFFSET, TOP_BAR_HEIGHT + SPACE_SM);
    }

    #[test]
    fn palette_tokens_use_base_4_and_fit_viewport() {
        assert_eq!(PALETTE_MAX_WIDTH, 640.0);
        assert_eq!(PALETTE_VIEWPORT_MARGIN, SPACE_LG);
        assert_eq!(PALETTE_POS_X, SPACE_SM);
        assert_eq!(PALETTE_POS_Y, TOP_BAR_HEIGHT);
        assert_eq!(PALETTE_SEARCH_ICON, ICON_MD);
        assert_eq!(PALETTE_DETAIL_INDENT, SPACE_LG);
        assert_eq!(PALETTE_PAGE_STEP, 10);
        for v in [
            PALETTE_MAX_WIDTH,
            PALETTE_VIEWPORT_MARGIN,
            PALETTE_POS_X,
            PALETTE_POS_Y,
            PALETTE_SEARCH_ICON,
            PALETTE_LIST_RESERVED,
            PALETTE_LIST_MIN_HEIGHT,
            PALETTE_DETAIL_INDENT,
        ] {
            assert_eq!(v % 4.0, 0.0, "palette value {v} must be multiple of base 4");
        }
        // La lista siempre deja aire: reservado > mínimo.
        assert!(PALETTE_LIST_RESERVED > PALETTE_LIST_MIN_HEIGHT);
    }
}
