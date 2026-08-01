//! Grafito Theme System — Dark/Light themes with semantic color tokens.
//!
//! Este módulo es la **única fuente de verdad** para los colores de Grafito.
//! Cualquier otro archivo debe usar los tokens semánticos definidos aquí
//! o [`current_theme`] para resolver al esquema activo.
//!
//! # Cómo agregar un token nuevo
//!
//! 1. Definir el campo en la struct `Theme` con un nombre semántico
//!    (`object_x`, `panel_y`, etc.) — **nunca** el color literal.
//! 2. Asignar un valor en `DARK` y `LIGHT`.
//! 3. Usar `current_theme(ctx).mi_token` en el código que lo necesite.
//!
//! # Cómo migrar un color hardcodeado
//!
//! ```ignore
//! // Antes
//! let accent = Color32::from_rgb(53, 132, 228);
//!
//! // Después
//! let accent = current_theme(ctx).accent;
//! ```
//!
//! El test de coherencia en `tests.rs` falla si encuentra
//! `Color32::from_rgb(` en cualquier archivo que no sea este.

use crate::tokens::{ANIM_MICRO, RADIUS_LG, RADIUS_MD};
use egui::{Color32, Context};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // ── Canvas y superficie ──
    pub canvas_bg: Color32,
    pub canvas_grid_minor: Color32,
    pub grid_line: Color32,
    pub grid_minor: Color32,

    // ── Paneles y chrome ──
    pub panel_bg: Color32,
    pub toolbar_bg: Color32,
    pub input_bar_bg: Color32,
    pub sidebar_bg: Color32,
    pub sidebar_tab_active_bg: Color32,
    pub sidebar_tab_inactive: Color32,
    pub sidebar_tab_active: Color32,
    pub status_bar_bg: Color32,
    pub separator: Color32,
    pub input_bg: Color32,
    pub input_text: Color32,
    pub button_bg: Color32,
    pub button_hover: Color32,

    // ── Teclado matemático ──
    pub keyboard_tab_active_bg: Color32,
    pub keyboard_tab_active_text: Color32,
    pub keyboard_tab_inactive: Color32,
    pub keyboard_key_bg: Color32,
    pub keyboard_key_hover: Color32,
    pub keyboard_key_border: Color32,
    pub keyboard_key_text: Color32,
    pub keyboard_enter_bg: Color32,
    pub keyboard_enter_hover: Color32,
    pub keyboard_enter_text: Color32,
    pub keyboard_delete_hover: Color32,
    pub keyboard_delete_hover_text: Color32,

    // ── Texto ──
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub text_label: Color32,

    // ── Acentos y estados ──
    pub accent: Color32,
    pub accent_muted: Color32,
    pub accent_strong: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub selection_bg: Color32,

    // ── Toast notifications ──
    pub toast_bg: Color32,
    pub toast_border: Color32,
    pub toast_text: Color32,
    pub toast_info: Color32,
    pub toast_success: Color32,
    pub toast_error: Color32,
    pub toast_cas: Color32,

    // ── Geometría 2D ──
    pub axis_2d: Color32,
    pub axis_label: Color32,
    pub grid_axis: Color32,
    pub snap_indicator: Color32,

    // ── Objetos geométricos (para leyenda del panel de álgebra) ──
    pub object_point: Color32,
    pub object_line: Color32,
    pub object_function: Color32,
    pub object_conic: Color32,
    pub object_polygon: Color32,
    pub object_label: Color32,

    // ── Highlights y overlays ──
    pub highlight: Color32,
    pub ghost_preview: Color32,
    pub newly_created_glow: Color32,
    pub selection_outline: Color32,
    pub hover_overlay: Color32,
}

/// Rol visual de una tecla del teclado matemático.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardKeyRole {
    /// Tecla matemática ordinaria.
    Standard,
    /// Tecla que borra el último carácter.
    Delete,
    /// Tecla que ejecuta la entrada.
    Enter,
}

/// Colores efectivos que el teclado pinta para un estado interactivo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardKeyVisuals {
    /// Fondo de la tecla.
    pub background: Color32,
    /// Texto de la tecla.
    pub text: Color32,
    /// Borde visible; `Stroke::NONE` para Enter.
    pub border: egui::Stroke,
}

impl Theme {
    /// Resuelve los mismos colores que debe usar el render del teclado.
    pub fn keyboard_key_visuals(&self, role: KeyboardKeyRole, hovered: bool) -> KeyboardKeyVisuals {
        match (role, hovered) {
            (KeyboardKeyRole::Enter, false) => KeyboardKeyVisuals {
                background: self.keyboard_enter_bg,
                text: self.keyboard_enter_text,
                border: egui::Stroke::NONE,
            },
            (KeyboardKeyRole::Enter, true) => KeyboardKeyVisuals {
                background: self.keyboard_enter_hover,
                text: self.keyboard_enter_text,
                border: egui::Stroke::NONE,
            },
            (KeyboardKeyRole::Delete, true) => KeyboardKeyVisuals {
                background: self.keyboard_delete_hover,
                text: self.keyboard_delete_hover_text,
                border: egui::Stroke::new(1.0, self.keyboard_key_border),
            },
            (KeyboardKeyRole::Standard | KeyboardKeyRole::Delete, hovered) => KeyboardKeyVisuals {
                background: if hovered {
                    self.keyboard_key_hover
                } else {
                    self.keyboard_key_bg
                },
                text: self.keyboard_key_text,
                border: egui::Stroke::new(1.0, self.keyboard_key_border),
            },
        }
    }

    pub fn apply(&self, ctx: &Context) {
        let is_dark = self.canvas_bg.r() < 100;
        let mut visuals = if is_dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        visuals.panel_fill = self.panel_bg;
        visuals.window_fill = self.panel_bg;
        visuals.faint_bg_color = self.canvas_grid_minor;
        visuals.extreme_bg_color = self.input_bg;
        visuals.hyperlink_color = self.accent;
        visuals.selection.bg_fill = self.selection_bg;
        visuals.selection.stroke = egui::Stroke::new(1.5, self.accent);
        visuals.window_rounding = egui::Rounding::same(RADIUS_LG);
        visuals.menu_rounding = egui::Rounding::same(RADIUS_MD);

        // Elevation separates floating controls without obscuring the canvas.
        visuals.window_shadow = egui::Shadow {
            offset: egui::vec2(0.0, 10.0),
            blur: 24.0,
            spread: -3.0,
            color: Color32::from_black_alpha(if is_dark { 100 } else { 26 }),
        };
        visuals.popup_shadow = visuals.window_shadow;

        // The chrome is compact, but each interaction state remains visible.
        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.canvas_grid_minor);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(RADIUS_MD);

        visuals.widgets.inactive.bg_fill = self.button_bg;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.separator);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.rounding = egui::Rounding::same(RADIUS_MD);

        visuals.widgets.hovered.bg_fill = self.button_hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.rounding = egui::Rounding::same(RADIUS_MD);

        visuals.widgets.active.bg_fill = self.selection_bg;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.rounding = egui::Rounding::same(RADIUS_MD);

        ctx.set_visuals(visuals);

        ctx.style_mut(|s| {
            s.animation_time = ANIM_MICRO / 1_000.0;
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(8.0, 5.0);
            s.spacing.menu_margin = egui::Margin::same(6.0);
            s.spacing.window_margin = egui::Margin::same(10.0);
            s.spacing.indent = 20.0;
            s.spacing.interact_size = egui::vec2(36.0, 24.0);
        });
    }
}

/// Devuelve el tema activo según el modo del contexto.
///
/// Esta función es el punto de entrada recomendado para todo código que
/// necesite colores semánticos. Reemplaza el patrón `if is_dark { X } else { Y }`
/// con un lookup directo al esquema activo.
///
/// # Ejemplo
///
/// ```ignore
/// use grafito_ui::theme::current_theme;
///
/// let theme = current_theme(ctx);
/// painter.rect_filled(rect, 4.0, theme.accent_muted);
/// ```
pub fn current_theme(ctx: &Context) -> &'static Theme {
    if ctx.style().visuals.dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Tema oscuro. Inicializado en runtime porque algunos campos usan
/// `Color32::from_rgba_unmultiplied` que no es `const fn`.
pub static DARK: once_cell::sync::Lazy<Theme> = once_cell::sync::Lazy::new(|| Theme {
    // Canvas y superficie
    canvas_bg: Color32::from_rgb(18, 18, 20),
    canvas_grid_minor: Color32::from_rgb(34, 34, 38),
    grid_line: Color32::from_rgba_unmultiplied(255, 255, 255, 25),
    grid_minor: Color32::from_rgba_unmultiplied(255, 255, 255, 12),

    // Paneles y chrome
    panel_bg: Color32::from_rgba_unmultiplied(26, 29, 37, 228),
    toolbar_bg: Color32::from_rgba_unmultiplied(28, 32, 42, 236),
    input_bar_bg: Color32::from_rgba_unmultiplied(32, 38, 52, 240),
    sidebar_bg: Color32::from_rgba_unmultiplied(24, 28, 38, 228),
    sidebar_tab_active_bg: Color32::from_rgb(40, 68, 130),
    sidebar_tab_inactive: Color32::from_gray(130),
    sidebar_tab_active: Color32::from_rgb(185, 208, 255),
    status_bar_bg: Color32::from_rgb(22, 22, 26),
    separator: Color32::from_rgb(55, 55, 60),
    input_bg: Color32::from_rgb(20, 20, 24),
    input_text: Color32::from_rgb(240, 240, 245),
    button_bg: Color32::from_rgb(34, 34, 38),
    button_hover: Color32::from_rgb(48, 48, 54),

    // Teclado matemático: los estados seleccionados e inactivos mantienen
    // contraste suficiente contra el panel oscuro.
    keyboard_tab_active_bg: Color32::from_rgb(47, 80, 160),
    keyboard_tab_active_text: Color32::from_rgb(245, 248, 255),
    keyboard_tab_inactive: Color32::from_rgb(174, 180, 195),
    keyboard_key_bg: Color32::from_rgb(40, 43, 52),
    keyboard_key_hover: Color32::from_rgb(58, 64, 79),
    keyboard_key_border: Color32::from_rgb(135, 145, 165),
    keyboard_key_text: Color32::from_rgb(240, 242, 248),
    keyboard_enter_bg: Color32::from_rgb(60, 102, 194),
    keyboard_enter_hover: Color32::from_rgb(64, 107, 200),
    keyboard_enter_text: Color32::from_rgb(245, 248, 255),
    keyboard_delete_hover: Color32::from_rgb(90, 18, 28),
    keyboard_delete_hover_text: Color32::from_rgb(240, 242, 248),

    // Texto
    text_primary: Color32::from_rgb(235, 235, 240),
    text_secondary: Color32::from_gray(160),
    text_tertiary: Color32::from_gray(140),
    text_label: Color32::from_gray(120),

    // Acentos y estados
    accent: Color32::from_rgb(94, 139, 255),
    accent_muted: Color32::from_rgb(37, 55, 92),
    accent_strong: Color32::from_rgb(120, 165, 255),
    success: Color32::from_rgb(46, 212, 122),
    warning: Color32::from_rgb(255, 184, 0),
    danger: Color32::from_rgb(255, 74, 90),
    selection_bg: Color32::from_rgb(47, 76, 145),

    // Toast notifications
    toast_bg: Color32::from_rgba_premultiplied(30, 33, 44, 220),
    toast_border: Color32::from_rgba_unmultiplied(94, 139, 255, 100),
    toast_text: Color32::from_rgb(255, 255, 255),
    toast_info: Color32::from_rgb(80, 150, 255),
    toast_success: Color32::from_rgb(80, 210, 80),
    toast_error: Color32::from_rgb(255, 80, 80),
    toast_cas: Color32::from_rgb(160, 100, 255),

    // Geometría 2D
    axis_2d: Color32::from_rgb(120, 120, 130),
    axis_label: Color32::from_gray(180),
    grid_axis: Color32::from_gray(180),
    snap_indicator: Color32::from_rgb(255, 184, 0),

    // Objetos geométricos (leyenda del panel de álgebra)
    object_point: Color32::from_rgb(112, 161, 255),
    object_line: Color32::from_rgb(180, 180, 190),
    object_function: Color32::from_rgb(46, 212, 122),
    object_conic: Color32::from_rgb(255, 107, 129),
    object_polygon: Color32::from_rgb(239, 68, 68),
    object_label: Color32::WHITE,

    // Highlights y overlays
    highlight: Color32::from_rgba_premultiplied(255, 184, 0, 80),
    ghost_preview: Color32::from_rgba_premultiplied(94, 139, 255, 120),
    newly_created_glow: Color32::from_rgba_premultiplied(94, 139, 255, 180),
    selection_outline: Color32::from_rgb(94, 139, 255),
    hover_overlay: Color32::from_rgba_unmultiplied(255, 255, 255, 12),
});

/// Tema claro. Ver `DARK`.
pub static LIGHT: once_cell::sync::Lazy<Theme> = once_cell::sync::Lazy::new(|| Theme {
    // Canvas y superficie
    canvas_bg: Color32::from_rgb(250, 250, 252),
    canvas_grid_minor: Color32::from_rgb(232, 232, 236),
    grid_line: Color32::from_rgba_unmultiplied(0, 0, 0, 25),
    grid_minor: Color32::from_rgba_unmultiplied(0, 0, 0, 12),

    // Paneles y chrome
    panel_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 228),
    toolbar_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 236),
    input_bar_bg: Color32::from_rgba_unmultiplied(245, 247, 252, 242),
    sidebar_bg: Color32::from_rgba_unmultiplied(248, 250, 255, 230),
    sidebar_tab_active_bg: Color32::from_rgba_unmultiplied(38, 99, 255, 30),
    sidebar_tab_inactive: Color32::from_gray(110),
    sidebar_tab_active: Color32::from_rgb(38, 99, 255),
    status_bar_bg: Color32::from_rgb(244, 244, 248),
    separator: Color32::from_rgb(220, 220, 224),
    input_bg: Color32::from_rgb(244, 244, 248),
    input_text: Color32::from_rgb(30, 30, 35),
    button_bg: Color32::from_rgb(244, 244, 248),
    button_hover: Color32::from_rgb(232, 232, 238),

    // Teclado matemático
    keyboard_tab_active_bg: Color32::from_rgb(38, 99, 255),
    keyboard_tab_active_text: Color32::WHITE,
    keyboard_tab_inactive: Color32::from_rgb(79, 84, 96),
    keyboard_key_bg: Color32::from_rgb(255, 255, 255),
    keyboard_key_hover: Color32::from_rgb(230, 235, 245),
    keyboard_key_border: Color32::from_rgb(105, 115, 135),
    keyboard_key_text: Color32::from_rgb(35, 38, 45),
    keyboard_enter_bg: Color32::from_rgb(20, 70, 220),
    keyboard_enter_hover: Color32::from_rgb(36, 97, 252),
    keyboard_enter_text: Color32::WHITE,
    keyboard_delete_hover: Color32::from_rgb(255, 215, 220),
    keyboard_delete_hover_text: Color32::from_rgb(35, 38, 45),

    // Texto
    text_primary: Color32::from_rgb(40, 40, 45),
    text_secondary: Color32::from_gray(80),
    text_tertiary: Color32::from_gray(110),
    text_label: Color32::from_gray(120),

    // Acentos y estados
    accent: Color32::from_rgb(36, 97, 252),
    accent_muted: Color32::from_rgba_unmultiplied(38, 99, 255, 70),
    accent_strong: Color32::from_rgb(20, 70, 220),
    success: Color32::from_rgb(0, 110, 60),
    warning: Color32::from_rgb(140, 85, 0),
    danger: Color32::from_rgb(190, 30, 50),
    selection_bg: Color32::from_rgba_unmultiplied(38, 99, 255, 30),

    // Toast notifications
    toast_bg: Color32::from_rgba_premultiplied(30, 33, 44, 220),
    toast_border: Color32::from_rgba_unmultiplied(38, 99, 255, 100),
    toast_text: Color32::from_rgb(255, 255, 255),
    toast_info: Color32::from_rgb(80, 150, 255),
    toast_success: Color32::from_rgb(20, 175, 90),
    toast_error: Color32::from_rgb(235, 50, 65),
    toast_cas: Color32::from_rgb(120, 80, 200),

    // Geometría 2D
    axis_2d: Color32::from_rgb(130, 130, 140),
    axis_label: Color32::from_gray(80),
    grid_axis: Color32::from_gray(80),
    snap_indicator: Color32::from_rgb(220, 150, 0),

    // Objetos geométricos (leyenda del panel de álgebra)
    object_point: Color32::from_rgb(38, 99, 255),
    object_line: Color32::from_rgb(90, 90, 100),
    object_function: Color32::from_rgb(20, 175, 90),
    object_conic: Color32::from_rgb(235, 50, 65),
    object_polygon: Color32::from_rgb(200, 50, 50),
    object_label: Color32::BLACK,

    // Highlights y overlays
    highlight: Color32::from_rgba_premultiplied(255, 184, 0, 60),
    ghost_preview: Color32::from_rgba_premultiplied(38, 99, 255, 100),
    newly_created_glow: Color32::from_rgba_premultiplied(38, 99, 255, 160),
    selection_outline: Color32::from_rgb(38, 99, 255),
    hover_overlay: Color32::from_rgba_premultiplied(0, 0, 0, 8),
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_have_distinct_accents() {
        assert_ne!(DARK.accent, LIGHT.accent);
    }

    #[test]
    fn dark_canvas_is_dark() {
        assert!(DARK.canvas_bg.r() < 50);
    }

    #[test]
    fn light_canvas_is_light() {
        assert!(LIGHT.canvas_bg.r() > 200);
    }

    #[test]
    fn current_theme_returns_dark_for_dark_context() {
        let ctx = Context::default();
        DARK.apply(&ctx);
        assert_eq!(current_theme(&ctx).accent, DARK.accent);
    }

    #[test]
    fn current_theme_returns_light_for_light_context() {
        let ctx = Context::default();
        LIGHT.apply(&ctx);
        assert_eq!(current_theme(&ctx).accent, LIGHT.accent);
    }

    #[test]
    fn all_required_tokens_defined() {
        // Si alguno de estos tokens se renombra, este test debe actualizarse.
        // Sirve como documentación de los tokens disponibles.
        let t = &*DARK;
        let _ = t.canvas_bg;
        let _ = t.grid_line;
        let _ = t.panel_bg;
        let _ = t.input_bar_bg;
        let _ = t.keyboard_tab_active_bg;
        let _ = t.keyboard_tab_active_text;
        let _ = t.keyboard_tab_inactive;
        let _ = t.keyboard_key_bg;
        let _ = t.keyboard_key_hover;
        let _ = t.keyboard_key_border;
        let _ = t.keyboard_key_text;
        let _ = t.keyboard_enter_bg;
        let _ = t.keyboard_enter_hover;
        let _ = t.keyboard_enter_text;
        let _ = t.keyboard_delete_hover;
        let _ = t.keyboard_delete_hover_text;
        let _ = t.sidebar_bg;
        let _ = t.status_bar_bg;
        let _ = t.text_primary;
        let _ = t.text_secondary;
        let _ = t.text_tertiary;
        let _ = t.text_label;
        let _ = t.accent;
        let _ = t.accent_muted;
        let _ = t.accent_strong;
        let _ = t.success;
        let _ = t.warning;
        let _ = t.danger;
        let _ = t.toast_bg;
        let _ = t.toast_border;
        let _ = t.toast_text;
        let _ = t.toast_info;
        let _ = t.toast_success;
        let _ = t.toast_error;
        let _ = t.toast_cas;
        let _ = t.axis_2d;
        let _ = t.axis_label;
        let _ = t.grid_axis;
        let _ = t.snap_indicator;
        let _ = t.object_point;
        let _ = t.object_line;
        let _ = t.object_function;
        let _ = t.object_conic;
        let _ = t.object_polygon;
        let _ = t.object_label;
        let _ = t.highlight;
        let _ = t.ghost_preview;
        let _ = t.newly_created_glow;
        let _ = t.selection_outline;
        let _ = t.hover_overlay;
    }

    fn relative_luminance(color: Color32) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };

        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast_ratio(foreground: Color32, background: Color32) -> f64 {
        let foreground = relative_luminance(foreground);
        let background = relative_luminance(background);
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    fn assert_keyboard_state_contrast(theme: &Theme) {
        assert!(
            contrast_ratio(theme.keyboard_tab_active_text, theme.keyboard_tab_active_bg) >= 4.5,
            "active tab text contrast is {}",
            contrast_ratio(theme.keyboard_tab_active_text, theme.keyboard_tab_active_bg),
        );
        assert!(
            contrast_ratio(theme.keyboard_tab_inactive, theme.panel_bg) >= 4.5,
            "inactive tab text contrast is {}",
            contrast_ratio(theme.keyboard_tab_inactive, theme.panel_bg),
        );

        for role in [
            KeyboardKeyRole::Standard,
            KeyboardKeyRole::Delete,
            KeyboardKeyRole::Enter,
        ] {
            for hovered in [false, true] {
                let state = theme.keyboard_key_visuals(role, hovered);
                let name = format!("{role:?} {}", if hovered { "hover" } else { "normal" });
                assert!(
                    contrast_ratio(state.text, state.background) >= 4.5,
                    "{name} text contrast is {}",
                    contrast_ratio(state.text, state.background),
                );
                let (boundary, boundary_kind) = if state.border.width > 0.0 {
                    (
                        contrast_ratio(state.border.color, theme.panel_bg),
                        "border-to-panel",
                    )
                } else {
                    (
                        contrast_ratio(state.background, theme.panel_bg),
                        "fill-to-panel",
                    )
                };
                assert!(
                    boundary >= 3.0,
                    "{name} {boundary_kind} contrast is {boundary}",
                );
            }
        }
    }

    #[test]
    fn keyboard_rendered_states_have_accessible_contrast() {
        assert_keyboard_state_contrast(&DARK);
        assert_keyboard_state_contrast(&LIGHT);
    }

    #[test]
    fn dark_interactive_surfaces_keep_text_readable() {
        assert!(contrast_ratio(DARK.text_primary, DARK.selection_bg) >= 4.5);
        assert!(contrast_ratio(DARK.text_primary, DARK.accent_muted) >= 4.5);
        assert!(contrast_ratio(DARK.text_secondary, DARK.toolbar_bg) >= 4.5);
    }

    #[test]
    fn light_status_colors_have_readable_contrast() {
        assert!(contrast_ratio(LIGHT.success, LIGHT.panel_bg) >= 4.5);
        assert!(contrast_ratio(LIGHT.warning, LIGHT.panel_bg) >= 4.5);
        assert!(contrast_ratio(LIGHT.danger, LIGHT.panel_bg) >= 4.5);
    }

    #[test]
    fn chrome_surfaces_keep_depth_and_readable_text() {
        for theme in [&*DARK, &*LIGHT] {
            assert!(theme.panel_bg.a() <= 232);
            assert!(theme.toolbar_bg.a() <= 236);
            assert!(theme.sidebar_bg.a() <= 232);
            assert!(contrast_ratio(theme.text_primary, theme.panel_bg) >= 4.5);
            assert!(contrast_ratio(theme.text_secondary, theme.toolbar_bg) >= 4.5);
        }
    }

    #[test]
    fn theme_uses_short_state_transition_timing() {
        let ctx = Context::default();
        DARK.apply(&ctx);

        assert_eq!(ctx.style().animation_time, 0.18);
    }

    #[test]
    fn transparent_interaction_tokens_are_not_declared_as_premultiplied() {
        let source = include_str!("theme.rs");

        assert!(
            source.contains("hover_overlay: Color32::from_rgba_unmultiplied(255, 255, 255, 12)")
        );
        assert!(source.contains("selection_bg: Color32::from_rgba_unmultiplied(38, 99, 255, 30)"));
    }
}
