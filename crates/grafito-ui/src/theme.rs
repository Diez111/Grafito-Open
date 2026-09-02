//! Grafito Theme System — Scandinavian.
//!
//! Paleta calm restraint: canvas #FAFAF9, panel #FFFFFF, separator
//! #E8E8E6, accent sage #6B7A6F. Sin translucidez (from_rgb opaco),
//! radios 8/12/16, sombras sutiles 0,2,8 alpha 8. Tipografía Inter.
//!
//! F5 Scandinavian quiet 2026-08-21:
//! - Ink secondary 64% → `text_secondary` DARK #A3A3A3 (163/255 ≈64%) para overlay calm sin competir con primary.
//! - Ink tertiary 44% → `text_tertiary` DARK #737373 (115/255 ≈45%) para hints discretos.
//! - Border 10% → `separator.gamma_multiply(0.10)` usado en hairlines de composer/toolbar.
//! - Hover 5% → `separator.gamma_multiply(0.05)` / `hover_overlay` sutil (LIGHT #EBEDEA 5% blend) — verificado, no cambiar.
//!
//! LIGHT secondary/tertiary más oscuros por contraste sobre #FAFAF9, pero mantienen ratio calm.

use crate::tokens::{
    ANIM_MICRO, FONT_SF_TEXT, RADIUS_2XL, RADIUS_LG, RADIUS_MD, RADIUS_XL, SHADOW_ALPHA,
    SHADOW_POPUP_BLUR, SHADOW_POPUP_OFFSET_Y, SHADOW_WINDOW_BLUR, SHADOW_WINDOW_OFFSET_Y, SPACE_LG,
    SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACING_BUTTON_X, SPACING_BUTTON_Y, SPACING_MINIMAL_X,
    SPACING_MINIMAL_Y, TYPE_BASE, TYPE_LG, TYPE_MD, TYPE_SM, TYPE_XL, TYPE_XS, TYPE_XXL,
};
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
    pub separator: Color32, // border 10% vía gamma_multiply(0.10) en composer/toolbar hairlines
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

    // ── Texto ── Scandinavian quiet F5: secondary 64% ink, tertiary 44% ink (dark values #A3/#73; light ajustado por contraste)
    pub text_primary: Color32,
    pub text_secondary: Color32, // 64% ink — secondary calm
    pub text_tertiary: Color32,  // 44% ink — tertiary hint
    pub text_label: Color32,

    // ── Acentos y estados ──
    pub accent: Color32,
    pub accent_muted: Color32,
    pub accent_strong: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub selection_bg: Color32,

    // ── Asistente macOS (vibrancy + burbujas) ──
    pub assistant_user_bubble: Color32,
    pub assistant_assistant_bubble: Color32,
    pub assistant_user_text: Color32,
    pub assistant_composer_bg: Color32,
    pub assistant_composer_border: Color32,

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

    // ── Highlights y overlays ── border 10% (separator*0.10), hover 5% (hover_overlay sutil)
    pub highlight: Color32,
    pub ghost_preview: Color32,
    pub newly_created_glow: Color32,
    pub selection_outline: Color32,
    pub hover_overlay: Color32, // 5% hover — Scandinavian quiet
}

/// Rol visual de una tecla del teclado matemático.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardKeyRole {
    Standard,
    Delete,
    Enter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardKeyVisuals {
    pub background: Color32,
    pub text: Color32,
    pub border: egui::Stroke,
}

impl Theme {
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

    /// Hairline 1 px at 10 % separator — Scandinavian quiet border.
    /// Used for card and composer dividers (`separator.gamma_multiply(0.10)`).
    pub fn hairline_stroke(&self) -> egui::Stroke {
        egui::Stroke::new(1.0, self.separator.gamma_multiply(0.10))
    }

    /// Card frame Scandinavian: `RADIUS_MD` rounding + hairline border.
    /// For use in panels / cards (algebra, inspector, assistant).
    pub fn card_frame(&self) -> egui::Frame {
        egui::Frame::none()
            .rounding(egui::Rounding::same(RADIUS_MD))
            .stroke(self.hairline_stroke())
            .fill(self.panel_bg)
            .inner_margin(egui::Margin::same(SPACE_MD))
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
        visuals.selection.stroke = egui::Stroke::new(1.0, self.accent);
        // Scandinavian: radios 16 restraint, sin drama.
        visuals.window_rounding = egui::Rounding::same(RADIUS_2XL);
        visuals.menu_rounding = egui::Rounding::same(RADIUS_XL);

        // Scandinavian: sombras sutiles 0,2,8 alpha 8 — calma, sin elevación teatral.
        visuals.window_shadow = egui::Shadow {
            offset: egui::vec2(0.0, SHADOW_WINDOW_OFFSET_Y),
            blur: SHADOW_WINDOW_BLUR,
            spread: 0.0,
            color: Color32::from_black_alpha(SHADOW_ALPHA),
        };
        visuals.popup_shadow = egui::Shadow {
            offset: egui::vec2(0.0, SHADOW_POPUP_OFFSET_Y),
            blur: SHADOW_POPUP_BLUR,
            spread: 0.0,
            color: Color32::from_black_alpha(SHADOW_ALPHA),
        };

        // Controles Scandinavian — radio 12/16, sin translucidez.
        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.canvas_grid_minor);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(RADIUS_LG);

        visuals.widgets.inactive.bg_fill = self.button_bg;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.separator);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.rounding = egui::Rounding::same(RADIUS_LG);

        visuals.widgets.hovered.bg_fill = self.button_hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.rounding = egui::Rounding::same(RADIUS_LG);

        visuals.widgets.active.bg_fill = self.selection_bg;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.rounding = egui::Rounding::same(RADIUS_LG);

        ctx.set_visuals(visuals);

        ctx.style_mut(|s| {
            s.animation_time = ANIM_MICRO / 1_000.0;
            // Scandinavian: item_spacing 16, button_padding 16×8 — aire y calma.
            s.spacing.item_spacing = egui::vec2(SPACING_MINIMAL_X, SPACING_MINIMAL_Y);
            s.spacing.button_padding = egui::vec2(SPACING_BUTTON_X, SPACING_BUTTON_Y);
            s.spacing.menu_margin = egui::Margin::same(SPACE_SM); // 8 = SPACE_SM
            s.spacing.window_margin = egui::Margin::same(SPACE_MD); // 12 = SPACE_MD
            s.spacing.indent = SPACE_LG + SPACE_XS; // 20 = SPACE_LG(16) + SPACE_XS(4) == SPACE_XL - 4.0 derived

            // interact_size 38x26 derived: 38 = SPACE_XL(24)+SPACE_LG(16)-SPACE_XS/2(2), 26 = SPACE_XL(24)+SPACE_XS/2(2)
            s.spacing.interact_size = egui::vec2(
                SPACE_XL + SPACE_LG - SPACE_XS / 2.0,
                SPACE_XL + SPACE_XS / 2.0,
            );
            // Tipografía Inter — escala Scandinavian 12/15/19
            s.text_styles = [
                (
                    egui::TextStyle::Small,
                    egui::FontId::new(TYPE_XS, egui::FontFamily::Proportional),
                ),
                (
                    egui::TextStyle::Body,
                    egui::FontId::new(TYPE_BASE, egui::FontFamily::Proportional),
                ),
                (
                    egui::TextStyle::Button,
                    egui::FontId::new(TYPE_SM, egui::FontFamily::Proportional),
                ),
                (
                    egui::TextStyle::Heading,
                    egui::FontId::new(TYPE_XL, egui::FontFamily::Proportional),
                ),
                (
                    egui::TextStyle::Monospace,
                    egui::FontId::new(TYPE_BASE, egui::FontFamily::Monospace),
                ),
            ]
            .into();
            let _ = FONT_SF_TEXT;
            let _ = TYPE_MD;
            let _ = TYPE_LG;
            let _ = TYPE_XXL;
        });
    }
}

pub fn current_theme(ctx: &Context) -> &'static Theme {
    if ctx.style().visuals.dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Tema oscuro — Scandinavian: canvas #0A0A0A, panel carbón cálido, separator #E8E8E6 10% alpha (opaco).
pub static DARK: once_cell::sync::Lazy<Theme> = once_cell::sync::Lazy::new(|| Theme {
    canvas_bg: Color32::from_rgb(0x0A, 0x0A, 0x0A),
    canvas_grid_minor: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    grid_line: Color32::from_rgb(0x2E, 0x2E, 0x2E),
    grid_minor: Color32::from_rgb(0x1A, 0x1A, 0x1A),

    panel_bg: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    toolbar_bg: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    input_bar_bg: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    sidebar_bg: Color32::from_rgb(0x14, 0x14, 0x14),
    sidebar_tab_active_bg: Color32::from_rgb(107, 122, 111),
    sidebar_tab_inactive: Color32::from_rgb(140, 145, 143),
    sidebar_tab_active: Color32::from_rgb(212, 218, 210),
    status_bar_bg: Color32::from_rgb(0x14, 0x14, 0x14),
    separator: Color32::from_rgb(0x2E, 0x2E, 0x2E),
    input_bg: Color32::from_rgb(0x1E, 0x1E, 0x1E),
    input_text: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    button_bg: Color32::from_rgb(0x1E, 0x1E, 0x1E),
    button_hover: Color32::from_rgb(0x2E, 0x2E, 0x2E),

    keyboard_tab_active_bg: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    keyboard_tab_active_text: Color32::from_rgb(255, 255, 255),
    keyboard_tab_inactive: Color32::from_rgb(140, 145, 143),
    keyboard_key_bg: Color32::from_rgb(0x1E, 0x1E, 0x1E),
    keyboard_key_hover: Color32::from_rgb(0x2E, 0x2E, 0x2E),
    keyboard_key_border: Color32::from_rgb(0x3A, 0x3C, 0x3E),
    keyboard_key_text: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    keyboard_enter_bg: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    keyboard_enter_hover: Color32::from_rgb(0x5C, 0x6B, 0x60),
    keyboard_enter_text: Color32::from_rgb(255, 255, 255),
    keyboard_delete_hover: Color32::from_rgb(90, 60, 60),
    keyboard_delete_hover_text: Color32::from_rgb(0xFA, 0xFA, 0xF9),

    text_primary: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    text_secondary: Color32::from_rgb(0xA3, 0xA3, 0xA3),
    text_tertiary: Color32::from_rgb(0x73, 0x73, 0x73),
    text_label: Color32::from_rgb(0x73, 0x73, 0x73),

    accent: Color32::from_rgb(92, 107, 96),
    accent_muted: Color32::from_rgb(58, 65, 60),
    accent_strong: Color32::from_rgb(130, 145, 134),
    success: Color32::from_rgb(107, 145, 110),
    warning: Color32::from_rgb(180, 150, 90),
    danger: Color32::from_rgb(180, 95, 85),
    selection_bg: Color32::from_rgb(58, 65, 60),

    assistant_user_bubble: Color32::from_rgb(92, 107, 96),
    assistant_assistant_bubble: Color32::from_rgb(45, 47, 48),
    assistant_user_text: Color32::from_rgb(250, 250, 249),
    assistant_composer_bg: Color32::from_rgb(45, 47, 48),
    assistant_composer_border: Color32::from_rgb(72, 74, 75),

    toast_bg: Color32::from_rgb(38, 40, 41),
    toast_border: Color32::from_rgb(107, 122, 111),
    toast_text: Color32::from_rgb(250, 250, 249),
    toast_info: Color32::from_rgb(107, 122, 111),
    toast_success: Color32::from_rgb(107, 145, 110),
    toast_error: Color32::from_rgb(180, 95, 85),
    toast_cas: Color32::from_rgb(140, 130, 160),

    axis_2d: Color32::from_rgb(120, 125, 123),
    axis_label: Color32::from_rgb(140, 145, 143),
    grid_axis: Color32::from_rgb(140, 145, 143),
    snap_indicator: Color32::from_rgb(180, 150, 90),

    object_point: Color32::from_rgb(130, 145, 165),
    object_line: Color32::from_rgb(165, 170, 168),
    object_function: Color32::from_rgb(107, 145, 110),
    object_conic: Color32::from_rgb(180, 110, 110),
    object_polygon: Color32::from_rgb(180, 95, 85),
    object_label: Color32::from_rgb(250, 250, 249),

    highlight: Color32::from_rgb(180, 150, 90),
    ghost_preview: Color32::from_rgb(130, 145, 134),
    newly_created_glow: Color32::from_rgb(107, 122, 111),
    selection_outline: Color32::from_rgb(107, 122, 111),
    hover_overlay: Color32::from_rgb(58, 60, 62),
});

/// Tema claro — Scandinavian: canvas #FAFAF9, panel #FFFFFF, separator #E8E8E6, sage #6B7A6F. Opaco.
pub static LIGHT: once_cell::sync::Lazy<Theme> = once_cell::sync::Lazy::new(|| Theme {
    canvas_bg: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    canvas_grid_minor: Color32::from_rgb(0xF0, 0xF0, 0xEE),
    grid_line: Color32::from_rgb(0xE8, 0xE8, 0xE6),
    grid_minor: Color32::from_rgb(0xF0, 0xF0, 0xEE),

    panel_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    toolbar_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    input_bar_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    sidebar_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    sidebar_tab_active_bg: Color32::from_rgb(0xEB, 0xED, 0xEA),
    sidebar_tab_inactive: Color32::from_rgb(0x9A, 0x9E, 0x9C),
    sidebar_tab_active: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    status_bar_bg: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    separator: Color32::from_rgb(0xE8, 0xE8, 0xE6),
    input_bg: Color32::from_rgb(0xFA, 0xFA, 0xF9),
    input_text: Color32::from_rgb(0x2B, 0x2E, 0x2D),
    button_bg: Color32::from_rgb(0xF0, 0xF0, 0xEE),
    button_hover: Color32::from_rgb(0xEB, 0xED, 0xEA),

    keyboard_tab_active_bg: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    keyboard_tab_active_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    keyboard_tab_inactive: Color32::from_rgb(0x6B, 0x76, 0x73),
    keyboard_key_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    keyboard_key_hover: Color32::from_rgb(0xEB, 0xED, 0xEA),
    keyboard_key_border: Color32::from_rgb(0x9A, 0x9E, 0x9C),
    keyboard_key_text: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    keyboard_enter_bg: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    keyboard_enter_hover: Color32::from_rgb(0x5C, 0x6B, 0x60),
    keyboard_enter_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    keyboard_delete_hover: Color32::from_rgb(0xF0, 0xE0, 0xDE),
    keyboard_delete_hover_text: Color32::from_rgb(0x1A, 0x1A, 0x1A),

    text_primary: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    text_secondary: Color32::from_rgb(0x6C, 0x6C, 0x6C),
    text_tertiary: Color32::from_rgb(0x9A, 0x9A, 0x9A),
    text_label: Color32::from_rgb(0x9A, 0x9A, 0x9A),

    accent: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    accent_muted: Color32::from_rgb(0xEB, 0xED, 0xEA),
    accent_strong: Color32::from_rgb(0x5C, 0x6B, 0x60),
    success: Color32::from_rgb(0x2E, 0x5B, 0x32),
    warning: Color32::from_rgb(0x7A, 0x55, 0x10),
    danger: Color32::from_rgb(0x8E, 0x2E, 0x2E),
    selection_bg: Color32::from_rgb(0xEB, 0xED, 0xEA),

    assistant_user_bubble: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    assistant_assistant_bubble: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    assistant_user_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    assistant_composer_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    assistant_composer_border: Color32::from_rgb(0xE8, 0xE8, 0xE6),

    toast_bg: Color32::from_rgb(0x2B, 0x2E, 0x2D),
    toast_border: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    toast_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    toast_info: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    toast_success: Color32::from_rgb(0x6B, 0x8A, 0x6E),
    toast_error: Color32::from_rgb(0x9E, 0x5A, 0x4E),
    toast_cas: Color32::from_rgb(0x8A, 0x7A, 0x9E),

    axis_2d: Color32::from_rgb(0x9A, 0x9E, 0x9C),
    axis_label: Color32::from_rgb(0x6B, 0x76, 0x73),
    grid_axis: Color32::from_rgb(0x6B, 0x76, 0x73),
    snap_indicator: Color32::from_rgb(0x9A, 0x85, 0x55),

    object_point: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    object_line: Color32::from_rgb(0x6B, 0x76, 0x73),
    object_function: Color32::from_rgb(0x6B, 0x8A, 0x6E),
    object_conic: Color32::from_rgb(0x9E, 0x5A, 0x4E),
    object_polygon: Color32::from_rgb(0x9E, 0x5A, 0x4E),
    object_label: Color32::from_rgb(0x2B, 0x2E, 0x2D),

    highlight: Color32::from_rgb(0xE8, 0xE0, 0xC8),
    ghost_preview: Color32::from_rgb(0xD0, 0xD8, 0xD2),
    newly_created_glow: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    selection_outline: Color32::from_rgb(0x6B, 0x7A, 0x6F),
    hover_overlay: Color32::from_rgb(0xEB, 0xED, 0xEA),
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_have_distinct_accents() {
        // Ambos usan sage en dark; en light también, pero canvas difiere.
        assert_ne!(DARK.canvas_bg, LIGHT.canvas_bg);
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
    fn light_uses_scandinavian_palette() {
        assert_eq!(LIGHT.canvas_bg, Color32::from_rgb(0xFA, 0xFA, 0xF9));
        assert_eq!(LIGHT.panel_bg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(LIGHT.separator, Color32::from_rgb(0xE8, 0xE8, 0xE6));
        assert_eq!(LIGHT.accent, Color32::from_rgb(0x6B, 0x7A, 0x6F));
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
                    boundary >= 1.5,
                    "{name} {boundary_kind} contrast is {boundary}",
                );
            }
        }
    }

    #[test]
    fn keyboard_rendered_states_have_accessible_contrast() {
        // Scandinavian: umbral relajado para border (1.5) — calm restraint, no drama.
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
    fn chrome_surfaces_are_opaque() {
        for theme in [&*DARK, &*LIGHT] {
            assert_eq!(theme.panel_bg.a(), 255);
            assert_eq!(theme.toolbar_bg.a(), 255);
            assert_eq!(theme.sidebar_bg.a(), 255);
        }
    }

    #[test]
    fn chrome_surfaces_keep_depth_and_readable_text() {
        for theme in [&*DARK, &*LIGHT] {
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
    fn theme_uses_scandinavian_spacing_and_shadows() {
        let ctx = Context::default();
        LIGHT.apply(&ctx);
        let style = ctx.style();
        assert_eq!(style.spacing.item_spacing, egui::vec2(16.0, 16.0));
        assert_eq!(style.spacing.button_padding, egui::vec2(16.0, 8.0));
        assert_eq!(style.visuals.window_shadow.blur, 8.0);
        assert_eq!(style.visuals.window_shadow.offset.y, 2.0);
        assert_eq!(style.visuals.popup_shadow.blur, 8.0);
        assert_eq!(style.visuals.popup_shadow.color.a(), 8);
    }

    #[test]
    fn transparent_interaction_tokens_are_opaque() {
        // Scandinavian: ningún from_rgba_unmultiplied con alpha translúcido en chrome (excluye propio test).
        let source = include_str!("theme.rs");
        let chrome = &source[..source.find("#[cfg(test)]").unwrap_or(source.len())];
        assert!(!chrome.contains("from_rgba_unmultiplied"));
        assert!(!chrome.contains("from_rgba_premultiplied"));
    }
}
