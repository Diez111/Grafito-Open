//! Smoke de ejecución (Ola 0.1): cada spec registrado se ejecuta con su
//! ejemplo mínimo (`command_registry::minimal_example`) sobre un documento vacío.
//!
//! Garantía: ningún handler entra en pánico, ningún comando queda huérfano
//! ("no reconocido") y ningún comando falla con "no implementado" salvo las
//! listas pinneadas de abajo.
//!
//! Fallar por aridad, falta de objetos, dominio o recursos ES honesto y el
//! test lo acepta.
//!
//! Nota de I/O: se verificó que ningún spec ejecuta I/O real con su ejemplo
//! mínimo (los únicos 3 con argumento `Path` —SetImage, PlaySound,
//! ExportImage— fallan honesto antes de tocar disco, y están pinneados abajo).
//! Si un futuro comando hiciera I/O con args mínimos, este test lo ejecutaría:
//! en ese caso hay que pinnearlo acá con motivo en vez de dejar el efecto
//! colateral en silencio.
//!
//! Si este test falla con un comando nuevo fuera de las listas, hay tres
//! opciones: implementar el comando, declararlo error honesto permanente
//! (pinnearlo con motivo) o corregir el bug. Nunca agrandar listas en silencio.

use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

/// Comandos que siempre responden "no implementado / no soportado" por diseño
/// (contrato `p4_errores_honestos_permanentes`).
/// La Ola 0.3 implementó Delete/StartAnimation/StopAnimation (12 → 9).
const PINNED_UNIMPLEMENTED: &[&str] = &[
    "SetVisibleInView",
    "SetImage",
    "ToolImage",
    "PlaySound",
    "StartRecord",
    "SlowPlot",
    "ExportImage",
    "ConstructionStep",
    "SetConstructionStep",
];

/// Comandos implementados solo para un subconjunto de tipos: con el ejemplo
/// mínimo responden "no soportado" para ESE tipo, pero computan para otros.
/// Cada entrada documenta el porqué; achicar esta lista es trabajo de paridad.
const KNOWN_PARTIAL: &[&str] = &[
    // Shear rechaza el círculo (su imagen real es una elipse): registry `shear`.
    "Shear",
    // EDOs con coeficientes variables fuera del subset F3c (solo constantes,
    // Euler 2.º y Frobenius); la Ola 2.5 amplía el subset.
    "SolveODE2",
    "ODESystem2",
    "EulerODE",
    // Laplace directa/inversa cubre la tabla F3c; fuera de tabla, error honesto.
    // La Ola 2.6 amplía la tabla.
    "LaplaceT",
    "InvLaplaceT",
    "LaplaceInt",
    // SolveODEN solo orden 1..=8 con coeficientes constantes.
    "SolveODEN",
];

/// Fragmentos (en minúsculas) que indican "nada se computa".
/// Nota: "no soportado"/"not supported" NO están acá a propósito: también los
/// usan errores honestos de dominio ("color no soportado", "objeto no
/// soportado"). Esos casos se pinnean en KNOWN_PARTIAL si son subset real.
fn is_unimplemented_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "no implementado",
        "no implementada",
        "not implemented",
        "unsupported",
    ]
    .iter()
    .any(|fragment| lower.contains(fragment))
}

#[test]
fn every_command_executes_without_panic_or_orphan() {
    let mut orphans = Vec::new();
    let mut surprise_unimplemented = Vec::new();
    let mut executed = 0usize;

    for spec in command_registry::all() {
        let mut document = Document::new();
        let mut input = command_registry::minimal_example(spec);
        executed += 1;
        // Un pánico acá falla el test: es exactamente lo que se quiere cazar.
        let outcome = process_input(&mut document, &mut input);
        if let CommandOutcome::Error(message) = &outcome {
            if message.contains("no reconocido") {
                orphans.push(format!("{}: {message}", spec.canonical));
            } else if is_unimplemented_message(message)
                && !PINNED_UNIMPLEMENTED.contains(&spec.canonical)
                && !KNOWN_PARTIAL.contains(&spec.canonical)
            {
                surprise_unimplemented.push(format!("{}: {message}", spec.canonical));
            }
        }
    }

    assert!(
        orphans.is_empty(),
        "comandos huérfanos ({}): {orphans:#?}",
        orphans.len()
    );
    assert!(
        surprise_unimplemented.is_empty(),
        "comandos con 'no implementado' fuera de las listas ({}): {surprise_unimplemented:#?}",
        surprise_unimplemented.len()
    );
    assert!(
        executed > 600,
        "el smoke ejecutó muy pocos comandos ({executed}): ¿cambió el registro?"
    );
}
