//! Shared helpers and application configuration.
//!
//! Contains color conversion, egui style setup, and persistent config loading
//! (theme, grid visibility, snap-to-grid) used across the desktop app.

use egui::Color32;
use grafito_assistant_types::ProviderProfile;
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

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) dark_mode: bool,
    pub(crate) show_grid: bool,
    pub(crate) snap_to_grid: bool,
    #[serde(default)]
    pub(crate) snap: SnapConfig,
    #[serde(default = "default_assistant_provider")]
    pub(crate) assistant_provider: ProviderProfile,
    #[serde(default = "default_assistant_model")]
    pub(crate) assistant_model: String,
    #[serde(default)]
    pub(crate) allow_fusion_fallback: bool,
    /// Plugins habilitados explícitamente por el usuario (manuales).
    #[serde(default)]
    pub(crate) enabled_plugins: Vec<String>,
    /// Plugins automáticos que el usuario desactivó.
    #[serde(default)]
    pub(crate) disabled_plugins: Vec<String>,
    /// Respuestas en línea sin cartel de autorización (permiso completo).
    #[serde(default = "default_full_permission")]
    pub(crate) assistant_full_permission: bool,
    /// Modo agente (loop con herramientas) para el asistente.
    #[serde(default)]
    pub(crate) assistant_agent_mode: bool,
    /// Onboarding 30s ya visto (Scandinavian, sin laberinto).
    #[serde(default)]
    pub(crate) onboarding_completed: bool,
}

fn default_full_permission() -> bool {
    true
}

const fn default_assistant_provider() -> ProviderProfile {
    ProviderProfile::OpenCodeGo
}

fn default_assistant_model() -> String {
    "deepseek-v4-flash".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dark_mode: false,
            show_grid: true,
            snap_to_grid: false,
            snap: SnapConfig::default(),
            assistant_provider: default_assistant_provider(),
            assistant_model: default_assistant_model(),
            allow_fusion_fallback: false,
            enabled_plugins: Vec::new(),
            disabled_plugins: Vec::new(),
            assistant_full_permission: default_full_permission(),
            assistant_agent_mode: false,
            onboarding_completed: false,
        }
    }
}

fn config_path() -> std::path::PathBuf {
    std::path::PathBuf::from("grafito_config.json")
}

/// Ruta del perfil pedagógico del estudiante (memoria del tutor).
pub(crate) fn profile_path() -> std::path::PathBuf {
    std::path::PathBuf::from("grafito_profile.json")
}

/// Carga el perfil persistido; ante cualquier error devuelve un perfil vacío.
pub(crate) fn load_profile() -> grafito_profile::StudentProfile {
    std::fs::read_to_string(profile_path())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Directorio de plugins del asistente (configurable por entorno).
pub(crate) fn plugins_dir() -> std::path::PathBuf {
    std::env::var_os("GRAFITO_PLUGINS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("plugins"))
}

/// Directorio de plugins instalados de fábrica (paquete).
pub(crate) fn system_plugins_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/usr/share/grafito/plugins")
}

/// Directorio de plugins a nivel usuario (instalación sin root).
pub(crate) fn user_data_plugins_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut base = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
            base.push(".local/share");
            base
        })
        .join("grafito/plugins")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_configuration_defaults_assistant_preferences() {
        let config: AppConfig =
            serde_json::from_str(r#"{"dark_mode":false,"show_grid":true,"snap_to_grid":false}"#)
                .unwrap();

        assert_eq!(config.assistant_provider, ProviderProfile::OpenCodeGo);
        assert_eq!(config.assistant_model, "deepseek-v4-flash");
        assert!(!config.allow_fusion_fallback);
    }
}
