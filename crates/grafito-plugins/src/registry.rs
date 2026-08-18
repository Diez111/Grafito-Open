//! Registro de plugins cargados y de sus contribuciones activas.

use crate::manifest::*;
use crate::validate::validate_manifest;
use crate::validate::ValidationContext;
use std::fs;
use std::path::{Path, PathBuf};

/// Plugin ya escaneado con su estado de validación y activación.
pub struct LoadedPlugin {
    /// Manifiesto parseado (válido o con errores registrados).
    pub manifest: PluginManifest,
    /// Directorio del plugin.
    pub dir: PathBuf,
    /// Estado de activación actual.
    pub enabled: bool,
    /// Motivo de invalidez, si el manifiesto no pasó la validación.
    pub error: Option<String>,
    /// Fingerprint simple del contenido validado (para evitar recargas).
    pub fingerprint: u64,
}

#[derive(Default)]
pub struct PluginRegistry {
    pub plugins: Vec<LoadedPlugin>,
}

impl PluginRegistry {
    /// Escanea un directorio de plugins (hasta dos niveles) y valida cada uno.
    pub fn load(dir: &Path, ctx: &ValidationContext) -> Self {
        Self::load_many(&[dir], ctx)
    }

    /// Escanea varios directorios (sistema + usuario). El primer directorio
    /// gana ante un id repetido, lo que permite que el usuario reemplace un
    /// plugin por defecto del sistema.
    pub fn load_many(dirs: &[&Path], ctx: &ValidationContext) -> Self {
        let mut candidates = Vec::new();
        for dir in dirs {
            collect_manifests(dir, 0, &mut candidates);
        }
        let mut seen_ids = std::collections::HashSet::new();
        let plugins = candidates
            .into_iter()
            .filter(|path| {
                let id = manifest_id_of(path).unwrap_or_default();
                seen_ids.insert(id)
            })
            .map(|path| load_plugin(&path, ctx))
            .collect();
        Self { plugins }
    }

    pub fn enabled(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.iter().filter(|plugin| plugin.enabled)
    }

    pub fn by_id(&self, id: &str) -> Option<&LoadedPlugin> {
        self.plugins
            .iter()
            .find(|plugin| plugin.manifest.plugin.id == id)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let Some(plugin) = self
            .plugins
            .iter_mut()
            .find(|plugin| plugin.manifest.plugin.id == id)
        else {
            return false;
        };
        if enabled && plugin.error.is_some() {
            return false;
        }
        plugin.enabled = enabled;
        true
    }

    /// Concatena las instrucciones de los plugins activos dentro del presupuesto.
    pub fn instructions_bounded(&self, max_bytes: usize) -> String {
        let mut output = String::new();
        for plugin in self.enabled() {
            let Some(section) = &plugin.manifest.instructions else {
                continue;
            };
            let mut block = String::new();
            for file in &section.files {
                let path = plugin.dir.join(file);
                if let Ok(bytes) = fs::read(&path) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        block.push_str(&text);
                        block.push('\n');
                    }
                }
            }
            if block.chars().count() > section.budget_bytes {
                block = block
                    .chars()
                    .take(section.budget_bytes.saturating_sub(1))
                    .collect::<String>();
                block.push('…');
            }
            if !block.trim().is_empty() {
                output.push_str(&format!("[{}]\n", plugin.manifest.plugin.name));
                output.push_str(&block);
                output.push('\n');
            }
        }
        if output.chars().count() > max_bytes {
            output = output
                .chars()
                .take(max_bytes.saturating_sub(30))
                .collect::<String>();
            output.push_str("\n[instrucciones truncadas]\n");
        }
        output
    }

    /// Identificadores de tools activadas por plugins.
    pub fn enabled_tool_ids(&self) -> Vec<String> {
        self.enabled()
            .flat_map(|plugin| plugin.manifest.tools.iter().map(|tool| tool.id.clone()))
            .collect()
    }

    /// Identificadores de comandos declarados y resueltos por plugins.
    pub fn enabled_command_ids(&self) -> Vec<String> {
        self.enabled()
            .flat_map(|plugin| {
                plugin
                    .manifest
                    .commands
                    .iter()
                    .map(|command| command.id.clone())
            })
            .collect()
    }

    /// Plantillas de escenas aportadas por plugins.
    pub fn enabled_scene_ids(&self) -> Vec<String> {
        self.enabled()
            .flat_map(|plugin| {
                plugin
                    .manifest
                    .scenes
                    .iter()
                    .map(|scene| scene.template.clone())
            })
            .collect()
    }

    /// Motores externos declarados por plugins activos.
    pub fn engines(&self) -> Vec<&EngineSection> {
        self.enabled()
            .filter_map(|plugin| plugin.manifest.engine.as_ref())
            .collect()
    }
}

fn collect_manifests(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_manifests(&path, depth + 1, out);
        } else if let Some(name) = path.file_name() {
            if name == PLUGIN_MANIFEST_FILENAME {
                out.push(path);
            }
        }
    }
}

/// Lee el id de un manifiesto sin cargar el plugin completo (para deduplicar).
fn manifest_id_of(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    parse_manifest(&raw).ok().map(|manifest| manifest.plugin.id)
}

fn load_plugin(path: &Path, ctx: &ValidationContext) -> LoadedPlugin {
    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            return LoadedPlugin {
                manifest: empty_manifest(),
                dir: dir.clone(),
                enabled: false,
                error: Some(format!("cannot read manifest: {error}")),
                fingerprint: 0,
            }
        }
    };
    let manifest = match parse_manifest(&raw) {
        Ok(manifest) => manifest,
        Err(error) => {
            return LoadedPlugin {
                manifest: empty_manifest(),
                dir: dir.clone(),
                enabled: false,
                error: Some(error),
                fingerprint: 0,
            }
        }
    };
    let fingerprint = simple_fingerprint(&raw);
    match validate_manifest(&manifest, ctx) {
        Ok(()) => {
            let enabled = manifest.plugin.activation != "manual";
            LoadedPlugin {
                manifest,
                dir,
                enabled,
                error: None,
                fingerprint,
            }
        }
        Err(error) => LoadedPlugin {
            manifest,
            dir,
            enabled: false,
            error: Some(error),
            fingerprint,
        },
    }
}

/// Parsea el TOML del manifiesto.
pub fn parse_manifest(raw: &str) -> Result<PluginManifest, String> {
    toml::from_str::<PluginManifest>(raw)
        .map_err(|error| format!("invalid plugin manifest: {error}"))
}

fn empty_manifest() -> PluginManifest {
    PluginManifest {
        plugin: PluginHeader {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            category: String::new(),
            description: String::new(),
            activation: "manual".into(),
            min_app_version: String::new(),
        },
        instructions: None,
        tools: Vec::new(),
        commands: Vec::new(),
        scenes: Vec::new(),
        engine: None,
    }
}

fn simple_fingerprint(raw: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in raw.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TOOL_KNOWN: &[&str] = &["evaluate_expr", "grafito_docs"];

    fn ctx() -> ValidationContext<'static> {
        ValidationContext {
            resolvable_command_ids: &|id| id == "Function",
            known_tools: TOOL_KNOWN,
            known_scenes: &["derivative-slope"],
        }
    }

    #[test]
    fn registry_loads_validates_and_enables_plugins_from_a_directory() {
        let dir = std::env::temp_dir().join("grafito_plugins_fixture");
        let plugin_dir = dir.join("calculo-i");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "calculo-i"
name = "Cálculo I"
version = "1.0.0"
category = "pedagogy"
activation = "manual"

[[tools]]
id = "evaluate_expr"
"#,
        )
        .unwrap();

        let registry = PluginRegistry::load(&dir, &ctx());
        assert_eq!(registry.plugins.len(), 1);
        let plugin = registry.by_id("calculo-i").unwrap();
        // Manual plugins start disabled but valid.
        assert!(plugin.error.is_none());
        assert!(!plugin.enabled);

        let mut registry = registry;
        assert!(registry.set_enabled("calculo-i", true));
        assert_eq!(
            registry.enabled_tool_ids(),
            vec!["evaluate_expr".to_string()]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invalid_plugins_are_loaded_with_error_and_cannot_be_enabled() {
        let dir = std::env::temp_dir().join("grafito_plugins_bad_fixture");
        let plugin_dir = dir.join("bad");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "bad!plugin"
name = "Mal"
version = "1.0.0"
category = "skills"

[[tools]]
id = "not_a_tool"
"#,
        )
        .unwrap();

        let mut registry = PluginRegistry::load(&dir, &ctx());
        let plugin = registry.by_id("bad!plugin").unwrap();
        assert!(plugin.error.is_some());
        assert!(!registry.set_enabled("bad!plugin", true));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_many_deduplicates_by_id_and_prefers_the_first_directory() {
        let root = std::env::temp_dir().join("grafito_plugins_many_fixture");
        let system_dir = root.join("system");
        let user_dir = root.join("user");
        fs::create_dir_all(&system_dir).unwrap();
        fs::create_dir_all(&user_dir).unwrap();
        fs::write(
            system_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "shared"
name = "Systemo"
version = "1.0.0"
category = "skills"
activation = "manual"
"#,
        )
        .unwrap();
        fs::write(
            user_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "shared"
name = "Usuario"
version = "1.0.0"
category = "skills"
activation = "auto"
"#,
        )
        .unwrap();

        let registry = PluginRegistry::load_many(&[&user_dir, &system_dir], &ctx());
        assert_eq!(registry.plugins.len(), 1, "same id must deduplicate");
        assert_eq!(registry.plugins[0].manifest.plugin.name, "Usuario");
        assert!(registry.plugins[0].enabled);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn instructions_are_concatenated_and_bounded() {
        let dir = std::env::temp_dir().join("grafito_plugins_docs_fixture");
        let plugin_dir = dir.join("docs");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "docs"
name = "Docs"
version = "1.0.0"
category = "skills"

[instructions]
files = ["intro.md"]
budget_bytes = 64
"#,
        )
        .unwrap();
        fs::write(
            plugin_dir.join("intro.md"),
            "contenido instructivo extenso".repeat(20),
        )
        .unwrap();

        let registry = PluginRegistry::load(&dir, &ctx());
        let instructions = registry.instructions_bounded(256);
        assert!(!instructions.is_empty());
        assert!(instructions.chars().count() <= 256);
        fs::remove_dir_all(&dir).unwrap();
    }
}
