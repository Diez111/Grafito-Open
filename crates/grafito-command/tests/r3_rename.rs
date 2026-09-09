#![allow(clippy::unwrap_used, clippy::expect_used)]
//! R3.1 `Rename[objeto, nuevo]` — cierra `scripting.rename-stub`.
//! E2E vía `process_input`: valida, renombra con `set_label`, colisión honesta,
//! undo transaccional (renombrar de vuelta) y paleta visible.

use grafito_command::command_registry;
use grafito_command::commands::{find_object_by_label, process_input, CommandOutcome};
use grafito_core::Document;

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    process_input(document, &mut command.to_string())
}

fn doc_con_punto(etiqueta: &str) -> Document {
    let mut doc = Document::new();
    let outcome = run(&mut doc, &format!("{etiqueta} = (1, 2)"));
    assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
    assert!(
        find_object_by_label(&doc, etiqueta).is_some(),
        "punto base {etiqueta}"
    );
    doc
}

#[test]
fn rename_es_visible_y_transforma() {
    let spec = command_registry::resolve("Rename").expect("Rename registrado");
    assert_eq!(spec.id, "scripting.rename");
    assert!(spec.palette_visible, "REGLA DE ORO: paleta visible");
    assert_eq!(spec.category, "Dinámica");
    assert!(
        matches!(
            spec.mutation,
            command_registry::MutationClass::TransformsObject
        ),
        "renombrar transforma, no solo consulta"
    );
    assert!(command_registry::resolve("Renombrar").is_some());
}

#[test]
fn rename_e2e_basico_y_undo() {
    let mut doc = doc_con_punto("A");
    let outcome = run(&mut doc, "Rename[A, B]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(find_object_by_label(&doc, "A").is_none(), "A ya no existe");
    assert!(find_object_by_label(&doc, "B").is_some(), "B existe");
    // Undo transaccional: renombrar de vuelta deja el doc como al inicio.
    let outcome = run(&mut doc, "Rename[B, A]");
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(find_object_by_label(&doc, "A").is_some());
    assert!(find_object_by_label(&doc, "B").is_none());
}

#[test]
fn rename_validacion_honesta() {
    let mut doc = doc_con_punto("A");
    // Inexistente.
    assert!(matches!(
        run(&mut doc, "Rename[Z, B]"),
        CommandOutcome::Error(_)
    ));
    // Vacío.
    assert!(matches!(
        run(&mut doc, "Rename[A, \"\"]"),
        CommandOutcome::Error(_)
    ));
    // Colisión.
    let _ = run(&mut doc, "C = (0, 0)");
    assert!(matches!(
        run(&mut doc, "Rename[A, C]"),
        CommandOutcome::Error(m) if m.contains("ya existe")
    ));
    // Largo.
    let largo = "B".repeat(65);
    assert!(matches!(
        run(&mut doc, &format!("Rename[A, {largo}]")),
        CommandOutcome::Error(m) if m.contains("64")
    ));
    // Aridad.
    assert!(matches!(
        run(&mut doc, "Rename[A]"),
        CommandOutcome::Error(m) if m.contains("Rename[objeto")
    ));
    // El doc sigue sano: A y C intactos tras los errores.
    assert!(find_object_by_label(&doc, "A").is_some());
    assert!(find_object_by_label(&doc, "C").is_some());
}
