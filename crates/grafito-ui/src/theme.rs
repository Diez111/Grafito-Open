//! Grafito Theme System — Dark/Light themes with semantic color tokens.

use egui::Color32;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub canvas_bg: Color32,
    pub canvas_grid_minor: Color32,
    pub panel_bg: Color32,
    pub toolbar_bg: Color32,
    pub text_primary: Color32,
    pub accent: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub selection_bg: Color32,
    pub input_bg: Color32,
    pub input_text: Color32,
    pub button_bg: Color32,
    pub button_hover: Color32,
    pub axis_2d: Color32,
    pub object_point: Color32,
    pub object_line: Color32,
    pub object_function: Color32,
    pub object_conic: Color32,
}

impl Theme {
    pub fn apply(&self, ctx: &egui::Context) {
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
        visuals.window_rounding = egui::Rounding::same(12.0);
        visuals.menu_rounding = egui::Rounding::same(8.0);

        // Premium window aesthetics
        visuals.window_shadow = egui::Shadow {
            offset: egui::vec2(0.0, 8.0),
            blur: 24.0,
            spread: 0.0,
            color: Color32::from_black_alpha(if is_dark { 160 } else { 40 }),
        };
        visuals.popup_shadow = visuals.window_shadow;

        // Widget visuals - Modern soft rounding and subtle borders
        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.canvas_grid_minor);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.inactive.bg_fill = self.button_bg;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::TRANSPARENT);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.hovered.bg_fill = self.button_hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(
            1.0,
            Color32::from_black_alpha(if is_dark { 40 } else { 10 }),
        );
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);

        visuals.widgets.active.bg_fill = self.selection_bg;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent.linear_multiply(0.5));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.rounding = egui::Rounding::same(8.0);

        ctx.set_visuals(visuals);

        // Spacing: Restore to original more compact sizes to avoid breaking layout
        ctx.style_mut(|s| {
            s.animation_time = 0.15;
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(8.0, 4.0);
            s.spacing.menu_margin = egui::Margin::same(6.0);
            s.spacing.window_margin = egui::Margin::same(8.0);
            s.spacing.indent = 20.0;
            s.spacing.interact_size = egui::vec2(36.0, 24.0);
        });
    }
}

pub const DARK: Theme = Theme {
    canvas_bg: Color32::from_rgb(18, 18, 20), // Premium very dark grey/black
    canvas_grid_minor: Color32::from_rgb(34, 34, 38),
    panel_bg: Color32::from_rgb(26, 26, 30), // Deep subtle grey for panels
    toolbar_bg: Color32::from_rgb(26, 26, 30),
    text_primary: Color32::from_rgb(235, 235, 240),
    accent: Color32::from_rgb(94, 139, 255), // Vibrant modern blue
    danger: Color32::from_rgb(255, 74, 90),  // Vibrant red
    success: Color32::from_rgb(46, 212, 122), // Vibrant green
    selection_bg: Color32::from_rgba_premultiplied(94, 139, 255, 40),
    input_bg: Color32::from_rgb(20, 20, 24),
    input_text: Color32::from_rgb(240, 240, 245),
    button_bg: Color32::from_rgb(34, 34, 38),
    button_hover: Color32::from_rgb(48, 48, 54),
    axis_2d: Color32::from_rgb(120, 120, 130),
    object_point: Color32::from_rgb(112, 161, 255),
    object_line: Color32::from_rgb(180, 180, 190),
    object_function: Color32::from_rgb(46, 212, 122),
    object_conic: Color32::from_rgb(255, 107, 129),
};

pub const LIGHT: Theme = Theme {
    canvas_bg: Color32::from_rgb(250, 250, 252), // Off-white modern background
    canvas_grid_minor: Color32::from_rgb(232, 232, 236),
    panel_bg: Color32::from_rgb(255, 255, 255),
    toolbar_bg: Color32::from_rgb(255, 255, 255),
    text_primary: Color32::from_rgb(40, 40, 45),
    accent: Color32::from_rgb(38, 99, 255), // Deep modern blue
    danger: Color32::from_rgb(235, 50, 65),
    success: Color32::from_rgb(20, 175, 90),
    selection_bg: Color32::from_rgba_premultiplied(38, 99, 255, 30),
    input_bg: Color32::from_rgb(244, 244, 248),
    input_text: Color32::from_rgb(30, 30, 35),
    button_bg: Color32::from_rgb(244, 244, 248),
    button_hover: Color32::from_rgb(232, 232, 238),
    axis_2d: Color32::from_rgb(130, 130, 140),
    object_point: Color32::from_rgb(38, 99, 255),
    object_line: Color32::from_rgb(90, 90, 100),
    object_function: Color32::from_rgb(20, 175, 90),
    object_conic: Color32::from_rgb(235, 50, 65),
};
