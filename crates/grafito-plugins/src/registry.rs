//! Registro de plugins cargados y de sus contribuciones activas.

use crate::manifest::*;
use crate::validate::ValidationContext;
use crate::validate::{validate_instruction_path, validate_manifest};
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

    /// Escanea varios directorios. El primer directorio gana ante un id repetido
    /// (deduplicación por `seen_ids`).
    ///
    /// # Precedencia recomendada (fail-closed, system > user > CWD_explicit_only)
    ///
    /// Por seguridad, el directorio de trabajo (`./plugins` o `plugins/`) **solo**
    /// debe incluirse si `GRAFITO_PLUGINS_DIR` está seteado explícitamente. De lo
    /// contrario un plugin en CWD podría hacer shadowing accidental (o malicioso)
    /// de un plugin del sistema instalado en `/usr/share/grafito/plugins`.
    ///
    /// Orden recomendado para `dirs` (de mayor a menor prioridad, primero gana):
    /// 1. `system` (`/usr/share/grafito/plugins`) — **no** debería ser shadowed por
    ///    CWD no explícito.
    /// 2. `user` (`~/.local/share/grafito/plugins` o `XDG_DATA_HOME/grafito/plugins`)
    /// 3. `CWD_explicit` (`$GRAFITO_PLUGINS_DIR` **solo** si la variable está seteada)
    ///
    /// Si se desea que el usuario pueda reemplazar plugins del sistema (comportamiento
    /// histórico), pásese `[user, system]` en ese orden. Para desarrollo con CWD
    /// explícito, pásese `[cwd_explicit, user, system]`. Documentamos que CWD sin
    /// `GRAFITO_PLUGINS_DIR` no debe usarse; ver `crate::utils::plugins_dir()`.
    ///
    /// # Seguridad
    /// - `collect_manifests` rechaza directorios symlink (evita escape via symlink).
    /// - `instructions_bounded` usa `validate_instruction_path` (fail-closed) + `O_NOFOLLOW`.
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
            // Fail-closed: si canonicalize del root falla, se omite el plugin completo.
            // Anterior fallback `unwrap_or_else(|_| dir.clone())` permitía TOCTOU bypass;
            // ahora fail-closed (registry.rs:88-141).
            let canonical_root = match fs::canonicalize(&plugin.dir) {
                Ok(p) => p,
                Err(error) => {
                    log::warn!(
                        "plugin '{}' dir no canonicalizable (fail-closed): {error}",
                        plugin.manifest.plugin.id
                    );
                    continue;
                }
            };
            for file in &section.files {
                // Unifica lógica con `validate_instruction_path` (elimina duplicado inline).
                // `validate_instruction_path` es fail-closed: canonicalize + starts_with.
                let canonical = match validate_instruction_path(&plugin.dir, file) {
                    Ok(p) => {
                        // Defensa en profundidad: verifica de nuevo contra canonical_root
                        // por si plugin_dir cambió entre llamadas.
                        if !p.starts_with(&canonical_root) {
                            log::warn!(
                                "plugin '{}' instruction file '{}' escapa del directorio (path traversal)",
                                plugin.manifest.plugin.id,
                                file
                            );
                            continue;
                        }
                        p
                    }
                    Err(error) => {
                        log::warn!(
                            "plugin '{}' instruction file '{}' rechazado: {error}",
                            plugin.manifest.plugin.id,
                            file
                        );
                        continue;
                    }
                };
                // TOCTOU mitigation: abrir con O_NOFOLLOW (unix) y verificar symlink_metadata en fallback.
                // Esto evita que un atacante reemplace el archivo por un symlink entre canonicalize y open.
                #[cfg(unix)]
                let fh = {
                    use std::os::unix::fs::OpenOptionsExt;
                    match std::fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(&canonical)
                    {
                        Ok(file) => file,
                        Err(error) => {
                            // ELOOP indica symlink con O_NOFOLLOW — fail-closed
                            log::warn!(
                                "plugin '{}' instruction file '{}' open failed (O_NOFOLLOW): {error}",
                                plugin.manifest.plugin.id,
                                file
                            );
                            continue;
                        }
                    }
                };
                #[cfg(not(unix))]
                let fh = {
                    // Fallback sin libc: symlink_metadata check antes de open (documentado)
                    match fs::symlink_metadata(&canonical) {
                        Ok(meta) if meta.file_type().is_symlink() => {
                            log::warn!(
                                "plugin '{}' instruction file '{}' es symlink (rechazado)",
                                plugin.manifest.plugin.id,
                                file
                            );
                            continue;
                        }
                        Ok(meta) if !meta.is_file() => {
                            log::warn!(
                                "plugin '{}' instruction file '{}' no es archivo regular",
                                plugin.manifest.plugin.id,
                                file
                            );
                            continue;
                        }
                        Err(error) => {
                            log::warn!(
                                "plugin '{}' instruction file '{}' metadata failed: {error}",
                                plugin.manifest.plugin.id,
                                file
                            );
                            continue;
                        }
                        _ => {}
                    }
                    match std::fs::OpenOptions::new().read(true).open(&canonical) {
                        Ok(file) => file,
                        Err(_) => continue,
                    }
                };
                // Presupuesto OOM: OpenOptions + take(budget+1) + try_reserve + reject > MAX_INSTRUCTION_FILE_BYTES
                // Usa constante MAX_INSTRUCTION_FILE_BYTES (32 KiB) en lugar de hardcode 10_001.
                const PER_FILE_LIMIT_PLUS_ONE: u64 = (MAX_INSTRUCTION_FILE_BYTES as u64) + 1;
                let per_file_limit = MAX_INSTRUCTION_FILE_BYTES;
                let per_file_limit_plus_one =
                    usize::try_from(PER_FILE_LIMIT_PLUS_ONE).unwrap_or(usize::MAX);
                let mut limited = fh.take(PER_FILE_LIMIT_PLUS_ONE);
                let mut bytes = Vec::new();
                if bytes.try_reserve(per_file_limit_plus_one).is_err() {
                    log::warn!(
                        "plugin '{}' instruction file '{}' no pudo reservar memoria",
                        plugin.manifest.plugin.id,
                        file
                    );
                    continue;
                }
                use std::io::Read as _;
                if limited.read_to_end(&mut bytes).is_err() {
                    continue;
                }
                if bytes.len() > per_file_limit {
                    log::warn!(
                        "plugin '{}' instruction file '{}' excede {} ({} bytes)",
                        plugin.manifest.plugin.id,
                        file,
                        per_file_limit,
                        bytes.len()
                    );
                    continue;
                }
                let text = match String::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                if text.len() > per_file_limit {
                    log::warn!(
                        "plugin '{}' instruction file '{}' texto excede {}",
                        plugin.manifest.plugin.id,
                        file,
                        per_file_limit
                    );
                    continue;
                }
                if block.try_reserve(text.len().saturating_add(1)).is_err() {
                    log::warn!(
                        "plugin '{}' instruction block sin memoria",
                        plugin.manifest.plugin.id
                    );
                    continue;
                }
                block.push_str(&text);
                block.push('\n');
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
        // Mitigación symlink traversal (registry.rs:228-245): rechaza symlink dirs.
        // Usa symlink_metadata para no seguir symlinks; si es symlink, se ignora.
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            // Rechaza symlinks tanto a archivos como a directorios para evitar TOCTOU
            // y escape de directorio (ej. plugin que symlinkea a /etc).
            log::warn!("collect_manifests: symlink rechazado '{}'", path.display());
            continue;
        }
        if meta.is_dir() {
            collect_manifests(&path, depth + 1, out);
        } else if meta.is_file() {
            if let Some(name) = path.file_name() {
                if name == PLUGIN_MANIFEST_FILENAME {
                    out.push(path);
                }
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
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

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
    fn load_many_precedence_system_over_user_over_cwd_explicit_shadowing() {
        // Documenta precedencia recomendada system > user > CWD_explicit_only.
        // CWD solo si GRAFITO_PLUGINS_DIR está seteado; test demuestra shadowing first-wins.
        // Si se pasa [system, user, cwd_explicit], system gana. Si se pasa [cwd_explicit, user, system], cwd gana.
        let root = std::env::temp_dir().join("grafito_plugins_precedence_fixture");
        let system_dir = root.join("system");
        let user_dir = root.join("user");
        let cwd_dir = root.join("cwd");
        for dir in [&system_dir, &user_dir, &cwd_dir] {
            fs::create_dir_all(dir).unwrap();
        }
        for (dir, name) in [
            (&system_dir, "System"),
            (&user_dir, "User"),
            (&cwd_dir, "CwdExplicit"),
        ] {
            fs::write(
                dir.join(PLUGIN_MANIFEST_FILENAME),
                format!(
                    r#"[plugin]
id = "shadowed"
name = "{name}"
version = "1.0.0"
category = "skills"
activation = "auto"
"#
                ),
            )
            .unwrap();
        }

        // Caso 1: system > user > cwd_explicit (system primero gana) — seguro, evita shadowing por CWD no explícito
        let registry = PluginRegistry::load_many(&[&system_dir, &user_dir, &cwd_dir], &ctx());
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(
            registry.plugins[0].manifest.plugin.name, "System",
            "system first should win when passed first"
        );

        // Caso 2: cwd_explicit > user > system (CWD explícito gana) — para desarrollo con GRAFITO_PLUGINS_DIR
        let registry = PluginRegistry::load_many(&[&cwd_dir, &user_dir, &system_dir], &ctx());
        assert_eq!(registry.plugins[0].manifest.plugin.name, "CwdExplicit");

        // Caso 3: user > system (histórico: usuario reemplaza sistema)
        let registry = PluginRegistry::load_many(&[&user_dir, &system_dir], &ctx());
        assert_eq!(registry.plugins[0].manifest.plugin.name, "User");

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

    #[test]
    fn collect_manifests_rejects_symlink_dirs() {
        let root = std::env::temp_dir().join("grafito_plugins_symlink_fixture");
        let real_dir = root.join("real");
        let link_dir = root.join("link");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(
            real_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "real"
name = "Real"
version = "1.0.0"
category = "skills"
activation = "auto"
"#,
        )
        .unwrap();
        // Crea symlink `link -> real` y verifica que collect_manifests no lo sigue
        #[cfg(unix)]
        {
            let _ = fs::remove_file(&link_dir);
            let _ = fs::remove_dir_all(&link_dir);
            symlink(&real_dir, &link_dir).unwrap();
            let registry = PluginRegistry::load(&root, &ctx());
            // Debe encontrar solo el plugin real una vez, no duplicado via symlink
            let ids: Vec<_> = registry
                .plugins
                .iter()
                .map(|p| p.manifest.plugin.id.clone())
                .collect();
            assert_eq!(
                ids.iter().filter(|id| *id == "real").count(),
                1,
                "symlink dir must be rejected, not traversed"
            );
            fs::remove_file(&link_dir).unwrap();
        }
        #[cfg(not(unix))]
        {
            // En no-unix, symlink test se omite pero el código usa symlink_metadata fallback
            let registry = PluginRegistry::load(&root, &ctx());
            assert_eq!(registry.plugins.len(), 1);
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn instructions_reject_symlink_file_via_o_nofollow() {
        let dir = std::env::temp_dir().join("grafito_plugins_symlink_file_fixture");
        let plugin_dir = dir.join("plug");
        fs::create_dir_all(&plugin_dir).unwrap();
        let target = dir.join("outside.md");
        fs::write(&target, "contenido externo").unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "plug"
name = "Plug"
version = "1.0.0"
category = "skills"

[instructions]
files = ["evil.md"]
budget_bytes = 4096
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            let evil = plugin_dir.join("evil.md");
            let _ = fs::remove_file(&evil);
            symlink(&target, &evil).unwrap();
            let registry = PluginRegistry::load(&dir, &ctx());
            let instructions = registry.instructions_bounded(8192);
            // validate_instruction_path canonicaliza y verifica starts_with, y O_NOFOLLOW bloquea symlink
            // El contenido externo no debe aparecer
            assert!(
                !instructions.contains("contenido externo"),
                "symlink instruction file should be rejected (O_NOFOLLOW/canonicalize)"
            );
            let _ = fs::remove_file(&evil);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn instructions_respect_max_instruction_file_bytes_constant() {
        let dir = std::env::temp_dir().join("grafito_plugins_budget_fixture");
        let plugin_dir = dir.join("budget");
        fs::create_dir_all(&plugin_dir).unwrap();
        // Crea archivo que excede MAX_INSTRUCTION_FILE_BYTES (32 KiB)
        let oversized = "a".repeat(MAX_INSTRUCTION_FILE_BYTES + 1);
        fs::write(plugin_dir.join("big.md"), &oversized).unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILENAME),
            r#"[plugin]
id = "budget"
name = "Budget"
version = "1.0.0"
category = "skills"

[instructions]
files = ["big.md"]
budget_bytes = 4096
"#,
        )
        .unwrap();
        let registry = PluginRegistry::load(&dir, &ctx());
        let instructions = registry.instructions_bounded(100_000);
        assert!(
            !instructions.contains(&"a".repeat(100)),
            "oversized file > MAX_INSTRUCTION_FILE_BYTES should be rejected"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
