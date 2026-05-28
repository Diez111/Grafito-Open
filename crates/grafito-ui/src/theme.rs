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
        
        // GeoGebra-like flat but slightly elevated look
        visuals.window_shadow = egui::Shadow {
            offset: egui::vec2(0.0, 4.0),
            blur: 16.0,
            spread: 0.0,
            color: Color32::from_black_alpha(if is_dark { 80 } else { 20 }),
        };

        // Widget visuals
        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.canvas_grid_minor);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.inactive.bg_fill = self.button_bg;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);

        visuals.widgets.hovered.bg_fill = self.button_hover;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);

        visuals.widgets.active.bg_fill = self.selection_bg;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.active.rounding = egui::Rounding::same(8.0);

        ctx.set_visuals(visuals);

        // Increase spacing for a cleaner, breathable UI
        ctx.style_mut(|s| {
            s.animation_time = 0.15;
            s.spacing.item_spacing = egui::vec2(12.0, 8.0);
            s.spacing.button_padding = egui::vec2(12.0, 6.0);
            s.spacing.menu_margin = egui::Margin::same(8.0);
            s.spacing.window_margin = egui::Margin::same(12.0);
            s.spacing.indent = 24.0;
            s.spacing.interact_size = egui::vec2(40.0, 28.0);
        });
    }
}

pub const DARK: Theme = Theme {
    canvas_bg:          Color32::from_rgb(30, 41, 59), // slate-800
    canvas_grid_minor:  Color32::from_rgb(51, 65, 85), // slate-700
    panel_bg:           Color32::from_rgb(15, 23, 42), // slate-900
    toolbar_bg:         Color32::from_rgb(15, 23, 42),
    text_primary:       Color32::from_rgb(248, 250, 252), // slate-50
    accent:             Color32::from_rgb(59, 130, 246), // blue-500
    danger:             Color32::from_rgb(239, 68, 68), // red-500
    success:            Color32::from_rgb(34, 197, 94), // green-500
    selection_bg:       Color32::from_rgb(30, 58, 138), // blue-900
    input_bg:           Color32::from_rgb(30, 41, 59),
    input_text:         Color32::from_rgb(248, 250, 252),
    button_bg:          Color32::from_rgb(30, 41, 59),
    button_hover:       Color32::from_rgb(51, 65, 85),
    axis_2d:            Color32::from_rgb(148, 163, 184), // slate-400
    object_point:       Color32::from_rgb(96, 165, 250), // blue-400
    object_line:        Color32::from_rgb(203, 213, 225), // slate-300
    object_function:    Color32::from_rgb(52, 211, 153), // emerald-400
    object_conic:       Color32::from_rgb(248, 113, 113), // red-400
};

pub const LIGHT: Theme = Theme {
    canvas_bg:          Color32::WHITE,
    canvas_grid_minor:  Color32::from_rgb(230, 230, 230),
    panel_bg:           Color32::WHITE,
    toolbar_bg:         Color32::WHITE,
    text_primary:       Color32::from_rgb(32, 33, 36),
    accent:             Color32::from_rgb(101, 87, 210), // GeoGebra purple-blue
    danger:             Color32::from_rgb(220, 38, 38),
    success:            Color32::from_rgb(22, 163, 74),
    selection_bg:       Color32::from_rgb(238, 236, 251), // Light purple selection
    input_bg:           Color32::WHITE,
    input_text:         Color32::from_rgb(32, 33, 36),
    button_bg:          Color32::from_rgb(243, 243, 248),
    button_hover:       Color32::from_rgb(230, 230, 235),
    axis_2d:            Color32::from_rgb(100, 100, 100),
    object_point:       Color32::from_rgb(37, 99, 235),
    object_line:        Color32::from_rgb(71, 85, 105),
    object_function:    Color32::from_rgb(5, 150, 105),
    object_conic:       Color32::from_rgb(220, 38, 38),   // red-600
};

