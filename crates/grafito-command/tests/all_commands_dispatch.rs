//! Barrido de despacho: cada comando registrado (733) debe responder algo
//! honesto al invocarse con `Canonical[]` — jamás "Comando no reconocido".
//!
//! Es el blindaje de visibilidad de P4: un comando puede fallar por aridad
//! o falta de objetos (Error esperado), pero no puede quedar huérfano del
//! dispatcher. Complementa `registry_metadata_is_unique_and_complete` (que
//! cubre metadatos) y `orphan_detection` (specs sin brazo en la lista de
//! handlers del registry).

use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

#[test]
fn every_registered_command_dispatches_without_unknown_error() {
    let mut orphans = Vec::new();
    for spec in command_registry::all() {
        let mut document = Document::new();
        let mut input = format!("{}[]", spec.canonical);
        let outcome = process_input(&mut document, &mut input);
        if let CommandOutcome::Error(message) = &outcome {
            if message.contains("no reconocido") {
                orphans.push(format!("{}: {message}", spec.canonical));
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "comandos sin despachador ({}): {orphans:#?}",
        orphans.len()
    );
}

#[test]
fn every_palette_command_resolves_by_canonical_and_alias() {
    let mut fails = Vec::new();
    for spec in command_registry::palette_commands() {
        if command_registry::resolve(spec.canonical).is_none() {
            fails.push(format!("canónico {}", spec.canonical));
        }
        for alias in spec.aliases {
            match command_registry::resolve(alias) {
                Some(resolved) if resolved.canonical == spec.canonical => {}
                _ => fails.push(format!("alias {alias} -> {}", spec.canonical)),
            }
        }
    }
    assert!(fails.is_empty(), "resolución rota: {fails:#?}");
}
