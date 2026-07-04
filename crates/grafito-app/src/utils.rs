//! Shared helpers and application configuration.
//!
//! Contains color conversion, egui style setup, and persistent config loading
//! (theme, grid visibility, snap-to-grid) used across the desktop app.

use egui::Color32;
use grafito_geometry::Color;

use crate::snap::SnapConfig;

pub(crate) fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r * 255.0).clamp(0.0, 255.0) as u8,
        (c.g * 255.0).clamp(0.0, 255.0) as u8,
        (c.b * 255.0).clamp(0.0, 255.0) as u8,
        (c.a * 255.0).clamp(0.0, 255.0) as u8,
    )
}

pub(crate) fn configure_modern_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Smooth corners everywhere
    style.visuals.window_rounding = 8.0.into();
    style.visuals.menu_rounding = 8.0.into();
    style.visuals.widgets.noninteractive.rounding = 6.0.into();
    style.visuals.widgets.inactive.rounding = 6.0.into();
    style.visuals.widgets.hovered.rounding = 6.0.into();
    style.visuals.widgets.active.rounding = 6.0.into();

    // Spacing so it doesn't look cramped
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);

    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 16.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(40),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(40),
    };

    ctx.set_style(style);
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) dark_mode: bool,
    pub(crate) show_grid: bool,
    pub(crate) snap_to_grid: bool,
    #[serde(default)]
    pub(crate) snap: SnapConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dark_mode: false,
            show_grid: true,
            snap_to_grid: false,
            snap: SnapConfig::default(),
        }
    }
}

fn config_path() -> std::path::PathBuf {
    std::path::PathBuf::from("grafito_config.json")
}

pub(crate) fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(config) => config,
            Err(err) => {
                log::warn!("No se pudo parsear {}: {err}", path.display());
                AppConfig::default()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
        Err(err) => {
            log::warn!("No se pudo leer {}: {err}", path.display());
            AppConfig::default()
        }
    }
}

pub(crate) fn save_config(config: &AppConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let path = config_path();
        if let Err(err) = std::fs::write(&path, json) {
            log::warn!("No se pudo guardar {}: {err}", path.display());
        }
    }
}
