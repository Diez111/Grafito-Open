//! Estructura declarativa de un plugin de Grafito (grafito-plugin.toml).

use serde::{Deserialize, Serialize};

/// Nombre canónico del manifiesto en el directorio de un plugin.
pub const PLUGIN_MANIFEST_FILENAME: &str = "grafito-plugin.toml";

/// Categoría declarada de un plugin.
pub const CATEGORY_PEDAGOGY: &str = "pedagogy";
pub const CATEGORY_SKILLS: &str = "skills";
pub const CATEGORY_TOOLS: &str = "tools";
pub const CATEGORY_COMMANDS: &str = "commands";
pub const CATEGORY_ENGINE: &str = "engine";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginHeader,
    #[serde(default)]
    pub instructions: Option<InstructionsSection>,
    #[serde(default)]
    pub tools: Vec<ToolEnable>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub scenes: Vec<SceneContribution>,
    #[serde(default)]
    pub engine: Option<EngineSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHeader {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub activation: String,
    #[serde(default)]
    pub min_app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionsSection {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default = "default_instruction_budget")]
    pub budget_bytes: usize,
}

fn default_instruction_budget() -> usize {
    DEFAULT_INSTRUCTION_BUDGET_BYTES
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEnable {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneContribution {
    pub template: String,
}

/// Declaración de un motor externo invocable por IPC (p. ej. animaciones).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSection {
    #[serde(default = "default_engine_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub protocol_version: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_engine_transport() -> String {
    "stdio".to_string()
}

/// Límites de validación.
pub const MAX_PLUGIN_ID_CHARS: usize = 64;
pub const MAX_PLUGIN_NAME_CHARS: usize = 120;
pub const MAX_PLUGIN_DESCRIPTION_CHARS: usize = 512;
pub const MAX_INSTRUCTIONS_FILES: usize = 16;
pub const MAX_INSTRUCTION_FILE_BYTES: usize = 32 * 1024;
pub const DEFAULT_INSTRUCTION_BUDGET_BYTES: usize = 4 * 1024;
pub const MAX_INSTRUCTION_BUDGET_BYTES: usize = 16 * 1024;
pub const MAX_ENGINE_COMMAND_ARGS: usize = 16;
