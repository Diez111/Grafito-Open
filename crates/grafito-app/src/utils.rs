//! Shared helpers and application configuration.
//!
//! Contains color conversion, egui style setup, and persistent config loading
//! (theme, grid visibility, snap-to-grid) used across the desktop app.

use egui::Color32;
use grafito_assistant_types::ProviderProfile;
use grafito_core::persistence::AUTOSAVE_DEBOUNCE_SECS;
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

/// Debounce de autosave para la UI (estado puro, sin I/O ni relojes internos).
///
/// El documento se escribe al sidecar `.autosave` (ver
/// `grafito_core::persistence::{write_autosave_sidecar, AUTOSAVE_DEBOUNCE_SECS}`)
/// solo tras [`AUTOSAVE_DEBOUNCE_SECS`] segundos sin edición, para no
/// re-serializar y re-validar el documento en cada keystroke. El caller pasa
/// `now` explícito (epoch segundos) para que sea testeable y determinista.
///
/// TODO(app.rs — otro agente): instanciar un `AutosaveDebouncer` en el estado
/// de la app; llamar `mark_dirty(now)` en cada mutación del documento; en el
/// tick (nunca en `Ui::`), si `should_autosave(now)` → escribir el sidecar en
/// background thread y llamar `mark_saved()`; tras `write_document_atomic`
/// exitoso → `mark_saved()` + borrar el sidecar. Recovery al arranque con
/// `grafito_core::persistence::load_autosave_candidate` + diálogo modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // TODO otro agente: instanciar en app.rs
pub(crate) struct AutosaveDebouncer {
    /// Epoch de la última edición pendiente de autosave (`None` = limpio).
    last_dirty_epoch: Option<u64>,
    /// Segundos de inactividad requeridos (default [`AUTOSAVE_DEBOUNCE_SECS`]).
    delay_secs: u64,
}

#[allow(dead_code)] // TODO otro agente: instanciar en app.rs
impl AutosaveDebouncer {
    pub(crate) fn new() -> Self {
        Self {
            last_dirty_epoch: None,
            delay_secs: AUTOSAVE_DEBOUNCE_SECS,
        }
    }

    pub(crate) fn with_delay(delay_secs: u64) -> Self {
        Self {
            last_dirty_epoch: None,
            delay_secs,
        }
    }

    /// Marca el documento como editado en `now_epoch` (reinicia la espera).
    pub(crate) fn mark_dirty(&mut self, now_epoch: u64) {
        self.last_dirty_epoch = Some(now_epoch);
    }

    /// ¿Pasó suficiente inactividad para escribir el sidecar?
    /// `false` si está limpio. Reloj sesgado (`now < dirty`) → `false`
    /// (saturating, nunca ofrece con tiempo negativo).
    pub(crate) fn should_autosave(&self, now_epoch: u64) -> bool {
        match self.last_dirty_epoch {
            None => false,
            Some(dirty) => now_epoch.saturating_sub(dirty) >= self.delay_secs,
        }
    }

    /// Limpia el estado tras escribir el sidecar o guardar el documento.
    pub(crate) fn mark_saved(&mut self) {
        self.last_dirty_epoch = None;
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.last_dirty_epoch.is_some()
    }
}

impl Default for AutosaveDebouncer {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn autosave_debouncer_waits_for_inactivity_then_offers_once() {
        let mut debouncer = AutosaveDebouncer::with_delay(5);
        assert!(!debouncer.should_autosave(100), "limpio nunca ofrece");
        assert!(!debouncer.is_dirty());

        debouncer.mark_dirty(100);
        assert!(debouncer.is_dirty());
        assert!(!debouncer.should_autosave(100), "recién editado no ofrece");
        assert!(!debouncer.should_autosave(104), "4s < 5s no ofrece");
        assert!(debouncer.should_autosave(105), "5s de inactividad ofrece");

        // Nueva edición reinicia la espera.
        debouncer.mark_dirty(200);
        assert!(!debouncer.should_autosave(204));
        assert!(debouncer.should_autosave(205));

        // Tras guardar queda limpio hasta la próxima edición.
        debouncer.mark_saved();
        assert!(!debouncer.is_dirty());
        assert!(!debouncer.should_autosave(1_000_000));
    }

    #[test]
    fn autosave_debouncer_default_delay_matches_core_const() {
        let debouncer = AutosaveDebouncer::new();
        assert_eq!(debouncer.delay_secs, AUTOSAVE_DEBOUNCE_SECS);
        assert_eq!(AutosaveDebouncer::default(), debouncer);
        // Reloj sesgado hacia atrás nunca ofrece (saturating).
        let mut skewed = AutosaveDebouncer::with_delay(5);
        skewed.mark_dirty(100);
        assert!(!skewed.should_autosave(50));
        // Delay 0 ofrece de inmediato (útil en tests de integración).
        let mut immediate = AutosaveDebouncer::with_delay(0);
        immediate.mark_dirty(77);
        assert!(immediate.should_autosave(77));
    }
}
