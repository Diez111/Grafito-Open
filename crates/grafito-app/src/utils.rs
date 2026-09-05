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
    /// Opt-in explícito para Aula/red avanzada (loopback F0 sin red).
    #[serde(default)]
    pub(crate) advanced_red_opt_in: bool,
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
            advanced_red_opt_in: false,
        }
    }
}

pub(crate) const LEGACY_CONFIG_NAME: &str = "grafito_config.json";
pub(crate) const LEGACY_PROFILE_NAME: &str = "grafito_profile.json";

fn xdg_config_home() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            let mut base = std::path::PathBuf::from(home);
            if base.as_os_str().is_empty() {
                base = std::path::PathBuf::from(".");
            }
            base.push(".config");
            base
        })
}

fn xdg_data_home() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            let mut base = std::path::PathBuf::from(home);
            if base.as_os_str().is_empty() {
                base = std::path::PathBuf::from(".");
            }
            base.push(".local/share");
            base
        })
}

/// Intenta migrar el archivo legado `legacy` a `new_path` copiando si el
/// legado existe y el nuevo no. No borra el legado. No hace I/O si
/// no es necesario. Ignora errores de I/O (best-effort).
fn try_migrate_legacy(new_path: &std::path::Path, legacy: &std::path::Path) {
    if new_path.exists() || !legacy.exists() {
        return;
    }
    // No migrar si el nuevo ya existe o el legado falta.
    // Crear directorio padre del nuevo si falta, luego copiar.
    if let Some(parent) = new_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::copy(legacy, new_path);
}

/// Helpers testeables sin tocar env global: resuelven rutas XDG dado un
/// `xdg_home` y un `home` explícitos. `None` simula variable no seteada/vacía.
#[cfg(test)]
pub(crate) fn xdg_config_path_for(
    xdg_config_home: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> std::path::PathBuf {
    let base = xdg_config_home
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut b = home
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            if b.as_os_str().is_empty() {
                b = std::path::PathBuf::from(".");
            }
            b.push(".config");
            b
        });
    base.join("grafito").join(LEGACY_CONFIG_NAME)
}

#[cfg(test)]
pub(crate) fn xdg_data_path_for(
    xdg_data_home: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> std::path::PathBuf {
    let base = xdg_data_home
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut b = home
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            if b.as_os_str().is_empty() {
                b = std::path::PathBuf::from(".");
            }
            b.push(".local/share");
            b
        });
    base.join("grafito").join(LEGACY_PROFILE_NAME)
}

fn config_path() -> std::path::PathBuf {
    let path = xdg_config_home().join("grafito").join(LEGACY_CONFIG_NAME);
    try_migrate_legacy(&path, std::path::Path::new(LEGACY_CONFIG_NAME));
    // Best-effort: asegurar directorio existe para escrituras futuras (no bloquea si falla).
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    path
}

/// Ruta del perfil pedagógico del estudiante (memoria del tutor).
pub(crate) fn profile_path() -> std::path::PathBuf {
    let path = xdg_data_home().join("grafito").join(LEGACY_PROFILE_NAME);
    try_migrate_legacy(&path, std::path::Path::new(LEGACY_PROFILE_NAME));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    path
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
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(err) = std::fs::write(&path, json) {
            log::warn!("No se pudo guardar {}: {err}", path.display());
        }
    }
}

/// Export para tests: expone migración best-effort sin tocar env.
#[cfg(test)]
pub(crate) fn migrate_legacy_for_test(new_path: &std::path::Path, legacy: &std::path::Path) {
    try_migrate_legacy(new_path, legacy);
}

/// Debounce de autosave para la UI (estado puro, sin I/O ni relojes internos).
///
/// El documento se escribe al sidecar `.autosave` (ver
/// `grafito_core::persistence::{write_autosave_sidecar, AUTOSAVE_DEBOUNCE_SECS}`)
/// solo tras [`AUTOSAVE_DEBOUNCE_SECS`] segundos sin edición, para no
/// re-serializar y re-validar el documento en cada keystroke. El caller pasa
/// `now` explícito (epoch segundos) para que sea testeable y determinista.
///
/// Flujo: `mark_dirty(now)` en cada mutación del documento; en el tick
/// (nunca en `Ui::`), si `should_autosave(now)` → escribir el sidecar en
/// background thread y llamar `mark_saved()`; tras `write_document_atomic`
/// exitoso → `mark_saved()` + borrar el sidecar. Recovery al arranque con
/// `grafito_core::persistence::load_autosave_candidate` + diálogo modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutosaveDebouncer {
    /// Epoch de la última edición pendiente de autosave (`None` = limpio).
    last_dirty_epoch: Option<u64>,
    /// Segundos de inactividad requeridos (default [`AUTOSAVE_DEBOUNCE_SECS`]).
    delay_secs: u64,
}

impl AutosaveDebouncer {
    pub(crate) fn new() -> Self {
        Self {
            last_dirty_epoch: None,
            delay_secs: AUTOSAVE_DEBOUNCE_SECS,
        }
    }

    #[cfg(test)]
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

    #[test]
    fn xdg_config_path_respects_xdg_config_home() {
        let xdg = std::path::Path::new("/tmp/xdg_config");
        let home = std::path::Path::new("/home/user");
        let path = xdg_config_path_for(Some(xdg), Some(home));
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/xdg_config/grafito/grafito_config.json")
        );
        // Sin XDG, fallback a HOME/.config
        let fallback = xdg_config_path_for(None, Some(home));
        assert_eq!(
            fallback,
            std::path::PathBuf::from("/home/user/.config/grafito/grafito_config.json")
        );
        // XDG vacío también fallback
        let empty = xdg_config_path_for(Some(std::path::Path::new("")), Some(home));
        assert_eq!(empty, fallback);
    }

    #[test]
    fn xdg_data_path_respects_xdg_data_home() {
        let xdg = std::path::Path::new("/tmp/xdg_data");
        let home = std::path::Path::new("/home/user");
        let path = xdg_data_path_for(Some(xdg), Some(home));
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/xdg_data/grafito/grafito_profile.json")
        );
        let fallback = xdg_data_path_for(None, Some(home));
        assert_eq!(
            fallback,
            std::path::PathBuf::from("/home/user/.local/share/grafito/grafito_profile.json")
        );
        let empty = xdg_data_path_for(Some(std::path::Path::new("")), Some(home));
        assert_eq!(empty, fallback);
    }

    #[test]
    fn legacy_migration_copies_when_new_missing_and_legacy_exists() {
        let tmp = std::env::temp_dir().join(format!(
            "grafito_test_mig_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&tmp);
        let legacy = tmp.join("legacy_grafito_config.json");
        let new_path = tmp.join("xdg/grafito/grafito_config.json");
        // Limpiar restos previos
        let _ = std::fs::remove_file(&new_path);
        let _ = std::fs::remove_file(&legacy);
        std::fs::write(&legacy, br#"{"dark_mode":true}"#).expect("write legacy");
        assert!(legacy.exists());
        assert!(!new_path.exists());
        migrate_legacy_for_test(&new_path, &legacy);
        assert!(new_path.exists(), "migración copia al XDG");
        assert!(legacy.exists(), "legado no borrado (copiar, no mover)");
        let content = std::fs::read_to_string(&new_path).expect("read migrated");
        assert_eq!(content, r#"{"dark_mode":true}"#);
        // No sobrescribir si el nuevo ya existe
        std::fs::write(&legacy, br#"{"dark_mode":false}"#).expect("overwrite legacy");
        migrate_legacy_for_test(&new_path, &legacy);
        let still = std::fs::read_to_string(&new_path).expect("read not overwritten");
        assert_eq!(still, r#"{"dark_mode":true}"#, "no sobrescribe existente");
        // Cleanup
        let _ = std::fs::remove_file(&legacy);
        let _ = std::fs::remove_file(&new_path);
        let _ = std::fs::remove_dir_all(tmp.join("xdg"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_migration_noop_when_legacy_missing() {
        let tmp = std::env::temp_dir().join(format!(
            "grafito_test_mig2_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&tmp);
        let legacy = tmp.join("no_legacy.json");
        let new_path = tmp.join("new.json");
        let _ = std::fs::remove_file(&legacy);
        let _ = std::fs::remove_file(&new_path);
        migrate_legacy_for_test(&new_path, &legacy);
        assert!(!new_path.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn xdg_paths_end_with_grafito_subdir_and_filename() {
        let cfg = xdg_config_path_for(None, Some(std::path::Path::new("/home/alice")));
        assert!(cfg.ends_with("grafito/grafito_config.json"));
        let data = xdg_data_path_for(None, Some(std::path::Path::new("/home/alice")));
        assert!(data.ends_with("grafito/grafito_profile.json"));
        // No relativo CWD
        assert!(cfg.is_absolute());
        assert!(data.is_absolute());
    }
}
