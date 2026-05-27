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
        let visuals = if is_dark {
            let mut v = egui::Visuals::dark();
            v.panel_fill = self.panel_bg;
            v.window_fill = self.panel_bg;
            v.faint_bg_color = self.canvas_grid_minor;
            v.extreme_bg_color = Color32::from_rgb(12, 14, 18);
            v.code_bg_color = Color32::from_rgb(35, 38, 45);
            v.hyperlink_color = self.accent;
            v.selection.bg_fill = self.selection_bg;
            v.selection.stroke = egui::Stroke::new(1.5, self.accent);
            v.window_rounding = egui::Rounding::same(8.0);
            v.window_shadow = egui::Shadow {
                offset: egui::vec2(4.0, 8.0), blur: 16.0, spread: 0.0,
                color: Color32::from_black_alpha(120),
            };
            v
        } else {
            let mut v = egui::Visuals::light();
            v.panel_fill = self.panel_bg;
            v.window_fill = self.panel_bg;
            v.faint_bg_color = self.canvas_grid_minor;
            v.hyperlink_color = self.accent;
            v.selection.bg_fill = self.selection_bg;
            v.selection.stroke = egui::Stroke::new(1.5, self.accent);
            v.window_rounding = egui::Rounding::same(8.0);
            v.window_shadow = egui::Shadow {
                offset: egui::vec2(2.0, 4.0), blur: 8.0, spread: 0.0,
                color: Color32::from_black_alpha(40),
            };
            v
        };
        ctx.set_visuals(visuals);

        ctx.style_mut(|s| {
            s.animation_time = 0.2;
            s.spacing.item_spacing = egui::vec2(8.0, 4.0);
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            s.spacing.indent = 20.0;
            s.spacing.interact_size = egui::vec2(40.0, 22.0);
        });
    }
}

pub const DARK: Theme = Theme {
    canvas_bg:          Color32::from_rgb(24, 26, 32),
    canvas_grid_minor:  Color32::from_rgba_premultiplied(255, 255, 255, 10),
    panel_bg:           Color32::from_rgb(30, 32, 40),
    toolbar_bg:         Color32::from_rgb(35, 37, 46),
    text_primary:       Color32::from_rgb(215, 215, 230),
    accent:             Color32::from_rgb(80, 150, 255),
    danger:             Color32::from_rgb(255, 80, 80),
    success:            Color32::from_rgb(80, 210, 80),
    selection_bg:       Color32::from_rgb(40, 80, 160),
    input_bg:           Color32::from_rgb(38, 40, 50),
    input_text:         Color32::from_rgb(220, 220, 235),
    button_bg:          Color32::from_rgb(50, 53, 65),
    button_hover:       Color32::from_rgb(60, 63, 78),
    axis_2d:            Color32::from_rgb(100, 105, 115),
    object_point:       Color32::from_rgb(60, 150, 255),
    object_line:        Color32::from_rgb(180, 180, 200),
    object_function:    Color32::from_rgb(60, 150, 255),
    object_conic:       Color32::from_rgb(255, 100, 100),
};

pub const LIGHT: Theme = Theme {
    canvas_bg:          Color32::WHITE,
    canvas_grid_minor:  Color32::from_rgb(230, 230, 235),
    panel_bg:           Color32::from_rgb(248, 248, 252),
    toolbar_bg:         Color32::from_rgb(240, 240, 245),
    text_primary:       Color32::from_rgb(30, 30, 40),
    accent:             Color32::from_rgb(37, 99, 235),
    danger:             Color32::from_rgb(220, 38, 38),
    success:            Color32::from_rgb(22, 163, 74),
    selection_bg:       Color32::from_rgb(200, 220, 255),
    input_bg:           Color32::WHITE,
    input_text:         Color32::from_rgb(30, 30, 40),
    button_bg:          Color32::from_rgb(240, 240, 245),
    button_hover:       Color32::from_rgb(225, 230, 240),
    axis_2d:            Color32::BLACK,
    object_point:       Color32::from_rgb(37, 99, 235),
    object_line:        Color32::from_rgb(50, 50, 55),
    object_function:    Color32::from_rgb(37, 99, 235),
    object_conic:       Color32::from_rgb(200, 60, 60),
};
