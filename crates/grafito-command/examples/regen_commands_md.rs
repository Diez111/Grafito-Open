//! Regenera la porción generada de `docs/commands.md` desde el registry.
//!
//! Uso: `cargo run -p grafito-command --example regen_commands_md`
//!
//! Reescribe el archivo empalmando `command_registry::render_markdown()`
//! antes de `## Valores validos` (las notas runtime se conservan tal cual).
//! El test `markdown_reference_is_the_registry_projection` verifica el
//! resultado: si este ejemplo no se corrió tras cambiar el registry, ese test
//! falla mostrando el diff.

use std::path::PathBuf;

const RUNTIME_VALIDITY_NOTES: &str = "\n## Valores validos\n";

fn run() -> Result<(), String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs_path = manifest.join("../../docs/commands.md");
    let documentation = std::fs::read_to_string(&docs_path)
        .map_err(|error| format!("docs/commands.md debe existir: {error}"))?;
    let (_, notes) = documentation
        .split_once(RUNTIME_VALIDITY_NOTES)
        .ok_or_else(|| "docs debe contener las notas runtime".to_string())?;
    let mut generated = grafito_command::command_registry::render_markdown();
    if !generated.ends_with('\n') {
        return Err("render_markdown debe terminar en newline".to_string());
    }
    generated.pop();
    let updated = format!("{generated}{RUNTIME_VALIDITY_NOTES}{notes}");
    std::fs::write(&docs_path, updated)
        .map_err(|error| format!("no se pudo escribir docs/commands.md: {error}"))?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("regen_commands_md: {error}");
        std::process::exit(1);
    }
    println!("docs/commands.md regenerado desde el registry");
}
