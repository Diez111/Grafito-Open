//! Frente fantasma+nuevos: los 7 comandos que ya tenían handler en
//! `commands.rs` pero cero apariciones en `command_registry.rs` (no salían en
//! la paleta, la ayuda ni el catálogo del asistente) + los 5 comandos nuevos
//! que delegan en el motor de `grafito-geometry`.
//!
//! Cada test resuelve el spec (visibilidad) Y ejecuta el comando real vía
//! `process_input`. El blindaje de `docs/commands.md` cierra el frente.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::command_registry;
use grafito_command::commands::{parse_cas_command, process_input, CommandOutcome};
use grafito_core::Document;

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

fn assert_registered_visible(canonical: &str) {
    let spec = command_registry::resolve(canonical)
        .unwrap_or_else(|| panic!("{canonical} debe estar en command_registry"));
    assert_eq!(
        spec.canonical, canonical,
        "{canonical} resuelve a otro spec"
    );
    assert!(
        spec.palette_visible,
        "{canonical} debe verse en la paleta de comandos"
    );
    assert!(
        command_registry::canonicalize(canonical).is_some(),
        "{canonical} debe normalizarse en el parser"
    );
}

// ── 7 comandos fantasma: handler existente + spec nuevo ─────────────────────

#[test]
fn improper_integral_registrado_y_ejecuta() {
    assert_registered_visible("ImproperIntegral");
    let mut doc = Document::new();
    let out = run(&mut doc, "ImproperIntegral[x^2, x, 0, 1]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("ImproperIntegral")),
        "got {out:?}"
    );
    // La forma española legacy vive en `cas_parse.rs` y sigue ruteando igual.
    let routed = parse_cas_command("integralimpropia[x^2, x, 0, 1]").expect("alias legacy");
    assert_eq!(routed.command, "ImproperIntegral");
}

#[test]
fn series_sum_registrado_y_ejecuta() {
    assert_registered_visible("SeriesSum");
    let mut doc = Document::new();
    let out = run(&mut doc, "SeriesSum[n, n, 1, 4]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("SeriesSum") && m.contains("= 10")),
        "got {out:?}"
    );
    let routed = parse_cas_command("sumaserie[n, n, 1, 4]").expect("alias legacy");
    assert_eq!(routed.command, "SeriesSum");
}

#[test]
fn sequence_limit_registrado_y_ejecuta() {
    assert_registered_visible("SequenceLimit");
    let mut doc = Document::new();
    let out = run(&mut doc, "SequenceLimit[(n+1)/n, n]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("SequenceLimit")),
        "got {out:?}"
    );
    let routed = parse_cas_command("limitesucesion[(n+1)/n, n]").expect("alias legacy");
    assert_eq!(routed.command, "SequenceLimit");
}

#[test]
fn double_integral_registrado_y_ejecuta() {
    assert_registered_visible("DoubleIntegral");
    let mut doc = Document::new();
    let out = run(&mut doc, "DoubleIntegral[1, x, 0, 2, y, 0, 3, 20]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("DoubleIntegral") && m.contains("6")),
        "got {out:?}"
    );
    let routed = parse_cas_command("integraldoble[1, x, 0, 2, y, 0, 3, 20]").expect("alias legacy");
    assert_eq!(routed.command, "DoubleIntegral");
}

#[test]
fn lagrange_multipliers_registrado_y_ejecuta() {
    assert_registered_visible("LagrangeMultipliers");
    let mut doc = Document::new();
    let out = run(
        &mut doc,
        "LagrangeMultipliers[x*y, x^2 + y^2 - 1, [x, y], -2, 2, -2, 2, 21]",
    );
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("LagrangeMultipliers")),
        "got {out:?}"
    );
    let routed =
        parse_cas_command("multiplicadoreslagrange[x*y, x^2 + y^2 - 1, [x, y], -2, 2, -2, 2, 21]")
            .expect("alias legacy");
    assert_eq!(routed.command, "LagrangeMultipliers");
}

#[test]
fn mean_value_check_registrado_y_ejecuta() {
    assert_registered_visible("MeanValueCheck");
    let mut doc = Document::new();
    let out = run(&mut doc, "MeanValueCheck[x^2, x, 0, 2]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("MeanValueCheck")),
        "got {out:?}"
    );
    let routed = parse_cas_command("teoremalagrange[x^2, x, 0, 2]").expect("alias legacy");
    assert_eq!(routed.command, "MeanValueCheck");
}

#[test]
fn subspace_sum_registrado_y_ejecuta() {
    assert_registered_visible("SubspaceSum");
    let mut doc = Document::new();
    let out = run(
        &mut doc,
        "SubspaceSum[[[1,0,0],[0,1,0]], [[0,1,0],[0,0,1]]]",
    );
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("dim(U + V) = 3")),
        "got {out:?}"
    );
    let routed = parse_cas_command("sumasubespacios[[[1,0,0],[0,1,0]], [[0,1,0],[0,0,1]]]")
        .expect("alias legacy");
    assert_eq!(routed.command, "SubspaceSum");
}

// ── 5 comandos nuevos: spec + brazo delegante al motor ──────────────────────

#[test]
fn poly_gcd_registrado_y_ejecuta() {
    assert_registered_visible("PolyGCD");
    let mut doc = Document::new();
    let out = run(&mut doc, "PolyGCD[x^2 - 1, x^2 + 2*x + 1]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("PolyGCD") && m.contains("x + 1")),
        "got {out:?}"
    );
    // Con variable explícita y entrada no polinómica: error honesto.
    let explicit = run(&mut doc, "PolyGCD[x^2 - 1, x^2 + 2*x + 1, x]");
    assert!(
        matches!(explicit, CommandOutcome::Message(ref m) if m.contains("x + 1")),
        "got {explicit:?}"
    );
    let honest = run(&mut doc, "PolyGCD[1/x, x]");
    assert!(
        matches!(honest, CommandOutcome::Error(ref m) if m.contains("PolyGCD")),
        "got {honest:?}"
    );
}

#[test]
fn resultant_registrado_y_ejecuta() {
    assert_registered_visible("Resultant");
    let mut doc = Document::new();
    // Elimina x de x² + y² - 1 y x - y → 2·y² - 1 (el signo lo fija el
    // determinante de Sylvester del motor: Res(f, g) = (-1)^(m·n)·Res(g, f)).
    // Ajuste fantasma-2: el motor de grafito-geometry cambió la convención de
    // signo (antes -2·y² + 1); se documenta el contrato vigente.
    let out = run(&mut doc, "Resultant[x^2 + y^2 - 1, x - y, x]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("Resultant") && m.contains("2*y^2 - 1")),
        "got {out:?}"
    );
    // Sin raíz común: resultante constante (convención del motor: -2).
    let constant = run(&mut doc, "Resultant[x^2 + 1, x - 1, x]");
    assert!(
        matches!(constant, CommandOutcome::Message(ref m) if m.contains("= -2") || m.contains("= 2")),
        "got {constant:?}"
    );
    // Con raíz común la resultante es idénticamente nula: error honesto.
    let degenerate = run(&mut doc, "Resultant[x^2 - 1, x - 1, x]");
    assert!(
        matches!(degenerate, CommandOutcome::Error(ref m) if m.contains("Resultant")),
        "got {degenerate:?}"
    );
}

#[test]
fn residue_registrado_y_ejecuta() {
    assert_registered_visible("Residue");
    let mut doc = Document::new();
    let out = run(&mut doc, "Residue[1/x, x, 0]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("Residue") && m.contains("polo simple") && m.contains("= 1")),
        "got {out:?}"
    );
    let analytic = run(&mut doc, "Residue[x^2 + 1, x, 0]");
    assert!(
        matches!(analytic, CommandOutcome::Message(ref m) if m.contains("analítica")),
        "got {analytic:?}"
    );
    // Singularidad esencial: el motor la declara sin inventar un residuo.
    let honest = run(&mut doc, "Residue[exp(1/x), x, 0]");
    assert!(
        matches!(honest, CommandOutcome::Error(ref m) if m.contains("Residue")),
        "got {honest:?}"
    );
}

#[test]
fn principal_part_registrado_y_ejecuta() {
    assert_registered_visible("PrincipalPart");
    let mut doc = Document::new();
    let out = run(&mut doc, "PrincipalPart[1/x^2, x, 0]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("PrincipalPart") && m.contains("1/x^2")),
        "got {out:?}"
    );
    // Polo desplazado: el denominador usa (x - x0).
    let shifted = run(&mut doc, "PrincipalPart[1/(x-1), x, 1]");
    assert!(
        matches!(shifted, CommandOutcome::Message(ref m) if m.contains("(x - 1)")),
        "got {shifted:?}"
    );
    // Ruido ~1e-10 del motor en la fórmula de derivadas: el término que
    // formatea a cero no se muestra (sin "-0/(x - 1)").
    let noisy = run(&mut doc, "PrincipalPart[1/(x-1)^2, x, 1]");
    assert!(
        matches!(noisy, CommandOutcome::Message(ref m) if m.contains("1/(x - 1)^2") && !m.contains("-0")),
        "got {noisy:?}"
    );
    // Sin polo: parte vacía informada como 0, sin inventar términos.
    let analytic = run(&mut doc, "PrincipalPart[x^2 + 1, x, 0]");
    assert!(
        matches!(analytic, CommandOutcome::Message(ref m) if m.contains("sin polo")),
        "got {analytic:?}"
    );
}

#[test]
fn step_by_step_registrado_y_ejecuta() {
    assert_registered_visible("StepByStep");
    let mut doc = Document::new();
    // Forma anidada: la traza pedagógica completa del stepper.
    let out = run(&mut doc, "StepByStep[Derivative[x^3 + x, x]]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("StepByStep") && m.contains("pasos") && m.contains('→')),
        "got {out:?}"
    );
    // Forma plana: op + operandos.
    let flat = run(&mut doc, "StepByStep[Solve, x^2 - 4, x]");
    assert!(
        matches!(flat, CommandOutcome::Message(ref m) if m.contains("pasos")),
        "got {flat:?}"
    );
    // Operación fuera del subset: lista explícita de las soportadas.
    let honest = run(&mut doc, "StepByStep[NoExiste[x]]");
    assert!(
        matches!(honest, CommandOutcome::Error(ref m) if m.contains("no tiene pasos soportados")),
        "got {honest:?}"
    );
}

// ── Blindaje docs↔código: los 12 comandos en la referencia generada ─────────

#[test]
fn docs_commands_lista_los_doce_comandos() {
    let docs = include_str!("../../../docs/commands.md");
    for canonical in [
        // 7 fantasma
        "ImproperIntegral",
        "SeriesSum",
        "SequenceLimit",
        "DoubleIntegral",
        "LagrangeMultipliers",
        "MeanValueCheck",
        "SubspaceSum",
        // 5 nuevos
        "PolyGCD",
        "Resultant",
        "Residue",
        "PrincipalPart",
        "StepByStep",
    ] {
        assert!(
            docs.contains(canonical),
            "docs/commands.md debe documentar {canonical}"
        );
    }
}
