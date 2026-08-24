#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::uninlined_format_args)]
//! Sistema de plugins declarativos de Grafito.
//!
//! Un plugin es un directorio con un manifiesto grafito-plugin.toml que aporta
//! instrucciones (skill packs), activación de tools incorporadas, comandos o
//! plantillas ya conocidas, y motores externos invocables por IPC. En v1 los
//! plugins son declarativos: no cargan binarios ni definen handlers dinámicos.

pub mod manifest;
pub mod registry;
pub mod validate;

pub use manifest::{
    CommandContribution, EngineSection, InstructionsSection, PluginHeader, PluginManifest,
    SceneContribution, ToolEnable, CATEGORY_COMMANDS, CATEGORY_ENGINE, CATEGORY_PEDAGOGY,
    CATEGORY_SKILLS, CATEGORY_TOOLS, PLUGIN_MANIFEST_FILENAME,
};
pub use registry::{parse_manifest, LoadedPlugin, PluginRegistry};
pub use validate::{validate_manifest, ValidationContext};
