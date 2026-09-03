//! Validación estricta de manifiestos de plugins (fail-closed).

use crate::manifest::*;

/// Categorías admitidas en la sección plugin.
pub const ALLOWED_CATEGORIES: &[&str] = &[
    CATEGORY_PEDAGOGY,
    CATEGORY_SKILLS,
    CATEGORY_TOOLS,
    CATEGORY_COMMANDS,
    CATEGORY_ENGINE,
];

/// Contexto que la aplicación resuelve para validar referencias externas.
pub struct ValidationContext<'a> {
    /// Devuelve si un id de comando existe en el registro de Grafito.
    pub resolvable_command_ids: &'a dyn Fn(&str) -> bool,
    /// Identificadores de tools incorporadas que los plugins pueden activar.
    pub known_tools: &'a [&'a str],
    /// Identificadores de plantillas de escenas conocidas por el motor.
    pub known_scenes: &'a [&'a str],
}

/// Valida un manifiesto completo; falla ante cualquier referencia no resuelta.
pub fn validate_manifest(manifest: &PluginManifest, ctx: &ValidationContext) -> Result<(), String> {
    validate_header(&manifest.plugin)?;
    if let Some(instructions) = &manifest.instructions {
        validate_instructions(instructions)?;
    }
    for tool in &manifest.tools {
        if !ctx.known_tools.iter().any(|known| *known == tool.id) {
            return Err(format!("plugin enables unknown tool '{}'", tool.id));
        }
    }
    for command in &manifest.commands {
        if !(ctx.resolvable_command_ids)(&command.id) {
            return Err(format!(
                "plugin contributes unregistered command '{}'",
                command.id
            ));
        }
    }
    for scene in &manifest.scenes {
        if !ctx
            .known_scenes
            .iter()
            .any(|known| *known == scene.template)
        {
            return Err(format!(
                "plugin references unknown scene template '{}'",
                scene.template
            ));
        }
    }
    if let Some(engine) = &manifest.engine {
        validate_engine(engine)?;
    }
    Ok(())
}

fn validate_header(header: &PluginHeader) -> Result<(), String> {
    if header.id.is_empty() || header.id.chars().count() > MAX_PLUGIN_ID_CHARS {
        return Err("plugin id is empty or too long".into());
    }
    if header.id.contains('/') || header.id.contains(char::is_whitespace) {
        return Err("plugin id cannot contain path separators or whitespace".into());
    }
    let mut previous: Option<char> = None;
    for character in header.id.chars() {
        let valid = character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '-' | '_');
        if !valid {
            return Err(
                "plugin id uses only lowercase letters, digits, dots, dashes and underscores"
                    .into(),
            );
        }
        if matches!(previous, Some('.')) && matches!(character, '.' | '-' | '_') {
            return Err("plugin id cannot repeat separators".into());
        }
        previous = Some(character);
    }
    if header.id.starts_with('.') || header.id.ends_with('.') {
        return Err("plugin id cannot start or end with a dot".into());
    }
    if !header.category.is_empty() && !ALLOWED_CATEGORIES.contains(&header.category.as_str()) {
        return Err(format!(
            "plugin category '{}' is not allowed",
            header.category
        ));
    }
    if header.name.trim().is_empty() || header.name.chars().count() > MAX_PLUGIN_NAME_CHARS {
        return Err("plugin name is empty or too long".into());
    }
    if header.description.chars().count() > MAX_PLUGIN_DESCRIPTION_CHARS {
        return Err("plugin description exceeds the limit".into());
    }
    if !header.version.is_empty() && !is_plain_semver(&header.version) {
        return Err(format!(
            "plugin version '{}' is not a simple semver",
            header.version
        ));
    }
    if !header.activation.is_empty() && header.activation != "auto" && header.activation != "manual"
    {
        return Err("plugin activation must be auto or manual".into());
    }
    Ok(())
}

fn validate_instructions(instructions: &InstructionsSection) -> Result<(), String> {
    if instructions.files.is_empty() {
        return Err("plugin instructions section lists no files".into());
    }
    if instructions.files.len() > MAX_INSTRUCTIONS_FILES {
        return Err("plugin instructions exceed the file count limit".into());
    }
    for file in &instructions.files {
        if file.is_empty() || file.contains('/') || file.contains('\\') || file.contains("..") {
            return Err(format!(
                "plugin instruction file '{}' is not a bare file name",
                file
            ));
        }
    }
    if instructions.budget_bytes > MAX_INSTRUCTION_BUDGET_BYTES {
        return Err("plugin instruction budget exceeds the limit".into());
    }
    Ok(())
}

/// Valida que un archivo de instrucciones no escape del directorio del plugin.
/// Uso en runtime: `plugin.dir.join(file).canonicalize()?.starts_with(canonical_plugin_root)`.
/// Fail-closed: si canonicalize falla o el path no está bajo root, log warn y rechazar.
pub fn validate_instruction_path(
    plugin_dir: &std::path::Path,
    file: &str,
) -> Result<std::path::PathBuf, String> {
    let canonical_root = std::fs::canonicalize(plugin_dir).map_err(|e| {
        log::warn!("plugin dir canonicalize failed: {e}");
        format!("plugin dir canonicalize failed: {e}")
    })?;
    let candidate = plugin_dir.join(file);
    let canonical = std::fs::canonicalize(&candidate).map_err(|e| {
        log::warn!("instruction file '{}' canonicalize failed: {e}", file);
        format!("instruction file '{file}' canonicalize failed: {e}")
    })?;
    if !canonical.starts_with(&canonical_root) {
        log::warn!(
            "instruction file '{}' escapes plugin directory (path traversal)",
            file
        );
        return Err(format!(
            "instruction file '{}' escapes plugin directory",
            file
        ));
    }
    Ok(canonical)
}

/// Allowlist de binarios permitidos para `engine.command[0]`.
/// Solo se permiten ejecutables conocidos; cualquier otro (ej. `sh`, `bash`) se rechaza
/// para evitar ejecución arbitraria via `sh -c`.
pub const ALLOWED_ENGINE_BINARIES: &[&str] = &["python3", "grafito-manim"];

/// Límites para `engine.capabilities`.
pub const MAX_ENGINE_CAPABILITIES: usize = 16;
pub const MAX_CAPABILITY_CHARS: usize = 64;

fn validate_engine(engine: &EngineSection) -> Result<(), String> {
    if engine.transport != "stdio" {
        return Err("plugin engine transport must be stdio".into());
    }
    if engine.command.is_empty() || engine.command.len() > MAX_ENGINE_COMMAND_ARGS {
        return Err("plugin engine command is empty or exceeds the argument limit".into());
    }
    // Allowlist: solo binarios conocidos. Rechaza `sh`, `bash`, `cmd`, `powershell`, etc.
    let binary = &engine.command[0];
    if !ALLOWED_ENGINE_BINARIES.contains(&binary.as_str()) {
        return Err(format!(
            "plugin engine command binary '{}' is not in allowlist {:?}",
            binary, ALLOWED_ENGINE_BINARIES
        ));
    }
    for argument in &engine.command {
        if argument.is_empty() || argument.contains('\u{0}') {
            return Err("plugin engine command contains an empty or NUL argument".into());
        }
        // Denylist shell metacaracteres y `sh -c` pattern.
        // Incluso con allowlist, rechazamos inyección via args como `; rm -rf /`, `|`, `&&`, etc.
        if argument.contains(';')
            || argument.contains('&')
            || argument.contains('|')
            || argument.contains('`')
            || argument.contains('$')
            || argument.contains('\n')
            || argument.contains('\r')
        {
            return Err(format!(
                "plugin engine command argument '{argument}' contains shell metacharacters"
            ));
        }
        // Bloquea intento de shell indirection incluso si binario es allowlisted
        // (ej. ["python3", "-c", "import os; os.system(...)"] se permite solo si -c no es shell,
        // pero bloqueamos `sh -c` via allowlist; aquí bloqueamos `-c` suelto si parece shell)
        // Denylist explícito para args que indican shell execution
        let lowered = argument.to_ascii_lowercase();
        if lowered == "sh"
            || lowered == "bash"
            || lowered == "cmd"
            || lowered == "powershell"
            || lowered == "pwsh"
        {
            return Err(format!(
                "plugin engine command argument '{argument}' is denied (shell binary)"
            ));
        }
    }
    // Detecta patrón `sh -c` aunque `sh` sea rechazado por allowlist, documenta razón
    if engine.command.iter().any(|arg| arg == "-c")
        && engine
            .command
            .iter()
            .any(|arg| matches!(arg.as_str(), "sh" | "bash" | "cmd" | "powershell"))
    {
        return Err("plugin engine command contains denied 'sh -c' pattern".into());
    }
    if engine.protocol_version == 0 || engine.protocol_version > 1_000 {
        return Err("plugin engine protocol version is outside the supported range".into());
    }
    // Valida capabilities: longitud y charset ^[a-z0-9-]{1,64}$
    if engine.capabilities.len() > MAX_ENGINE_CAPABILITIES {
        return Err(format!(
            "plugin engine capabilities exceed limit {} (got {})",
            MAX_ENGINE_CAPABILITIES,
            engine.capabilities.len()
        ));
    }
    for cap in &engine.capabilities {
        if cap.is_empty() || cap.len() > MAX_CAPABILITY_CHARS {
            return Err(format!(
                "plugin engine capability '{}' length must be 1..{}",
                cap, MAX_CAPABILITY_CHARS
            ));
        }
        let valid = cap
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            return Err(format!(
                "plugin engine capability '{}' must match ^[a-z0-9-]{{1,64}}$",
                cap
            ));
        }
    }
    Ok(())
}

/// Acepta versiones de tres componentes ([major].[minor].[patch]).
fn is_plain_semver(version: &str) -> bool {
    let components = version.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        crate::registry::parse_manifest(
            r#"[plugin]
id = "utn.calculo-i"
name = "Cálculo I UTN"
version = "1.0.0"
category = "pedagogy"
description = "Lecciones de cálculo"
activation = "auto"

[instructions]
files = ["intro.md"]
budget_bytes = 2048

[[tools]]
id = "evaluate_expr"

[[commands]]
id = "Function"

[[scenes]]
template = "derivative-slope"
"#,
        )
        .unwrap()
    }

    fn ctx() -> ValidationContext<'static> {
        ValidationContext {
            resolvable_command_ids: &|id| id == "Function" || id == "Tetrahedron",
            known_tools: &["evaluate_expr", "grafito_docs"],
            known_scenes: &["derivative-slope", "riemann"],
        }
    }

    #[test]
    fn valid_manifest_passes_all_checks() {
        assert!(validate_manifest(&valid_manifest(), &ctx()).is_ok());
    }

    #[test]
    fn unknown_tools_and_scenes_fail_closed() {
        let mut manifest = valid_manifest();
        manifest.tools[0].id = "run_script".into();
        assert!(validate_manifest(&manifest, &ctx()).is_err());

        let mut manifest = valid_manifest();
        manifest.scenes[0].template = "not-a-scene".into();
        assert!(validate_manifest(&manifest, &ctx()).is_err());
    }

    #[test]
    fn unregistered_commands_are_rejected() {
        let mut manifest = valid_manifest();
        manifest.commands[0].id = "DeleteEverything".into();
        assert!(validate_manifest(&manifest, &ctx()).is_err());
    }

    #[test]
    fn bad_ids_and_categories_are_rejected() {
        let mut manifest = valid_manifest();
        manifest.plugin.id = "utn/calculo".into();
        assert!(validate_manifest(&manifest, &ctx()).is_err());

        let mut manifest = valid_manifest();
        manifest.plugin.category = "malware".into();
        assert!(validate_manifest(&manifest, &ctx()).is_err());
    }

    #[test]
    fn engine_section_validates_transport_and_version() {
        let manifest = crate::registry::parse_manifest(
            r#"[plugin]
id = "grafito.manim-engine"
name = "Motor Manim"
version = "1.0.0"
category = "engine"

[engine]
transport = "stdio"
command = ["python3", "-m", "grafito_engine"]
protocol_version = 1
"#,
        )
        .unwrap();
        assert!(validate_manifest(&manifest, &ctx()).is_ok());

        let mut invalid = manifest.clone();
        invalid.engine.as_mut().unwrap().protocol_version = 0;
        assert!(validate_manifest(&invalid, &ctx()).is_err());

        let mut invalid = manifest;
        invalid.engine.as_mut().unwrap().transport = "http".into();
        assert!(validate_manifest(&invalid, &ctx()).is_err());
    }
}
