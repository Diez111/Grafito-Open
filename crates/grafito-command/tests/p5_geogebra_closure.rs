//! Frente P5 — cierre nominal GeoGebra: paridad de nombres y comandos nuevos.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

fn run_in(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn ok_in(document: &mut Document, command: &str) -> String {
    match run_in(document, command) {
        CommandOutcome::Message(text) => text,
        CommandOutcome::Ok => String::new(),
        other => panic!("{command} esperaba éxito, llegó {other:?}"),
    }
}

fn err_in(document: &mut Document, command: &str) -> String {
    match run_in(document, command) {
        CommandOutcome::Error(text) => text,
        other => panic!("{command} esperaba error honesto, llegó {other:?}"),
    }
}

// ── Alias GeoGebra → comandos existentes ────────────────────────────────

#[test]
fn p5_geogebra_name_aliases_resolve() {
    use grafito_command::command_registry;
    for (alias, canonical) in [
        ("CorrelationCoefficient", "Correlation"),
        ("InverseTDistribution", "InverseT"),
        ("InverseFDistribution", "InverseF"),
        ("ChiSquaredTest", "ChiSqTest"),
        ("Payment", "Pmt"),
        ("Difference", "PolygonDifference"),
        ("InverseLaplace", "InvLaplaceT"),
        ("Curve", "ParametricCurve2D"),
        ("SetTrace", "Rastro"),
        ("PlotSolve", "Solve"),
        ("SampleSD", "StdDev"),
    ] {
        let spec = command_registry::resolve(alias)
            .unwrap_or_else(|| panic!("alias {alias} sin resolución"));
        assert_eq!(spec.canonical, canonical, "alias {alias}");
    }
}

// ── Comandos nuevos con motor ───────────────────────────────────────────

#[test]
fn p5_sd_and_sample_variance() {
    let mut document = Document::new();
    let sd = ok_in(&mut document, "SD[{1,2,3,4}]");
    assert!(sd.contains("poblacional"), "SD: {sd}");
    assert!(sd.contains("1.118"), "SD √1.25: {sd}");
    let sv = ok_in(&mut document, "SampleVariance[{1,2,3,4}]");
    assert!(sv.contains("muestral"), "SampleVariance: {sv}");
    assert!(sv.contains("1.666"), "÷n−1: {sv}");
    let ssd = ok_in(&mut document, "SampleSD[{1,2,3,4}]");
    assert!(ssd.contains("1.290"), "√(5/3): {ssd}");
    // Un solo dato: poblacional = 0 (correcto); lista vacía es error honesto.
    assert!(ok_in(&mut document, "SD[{1}]").contains("0.000000"));
    assert!(err_in(&mut document, "SD[{}]").contains("vacía"));
}

#[test]
fn p5_groebner_lex_deg_uses_grlex() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "GroebnerLexDeg[{x+y-3, x-y-1}, {x, y}]");
    assert!(out.contains('x') && out.contains('y'), "base: {out}");
    assert!(err_in(&mut document, "GroebnerLexDeg[{x}]").contains("GroebnerLexDeg"));
}

#[test]
fn p5_set_seed_makes_randomness_reproducible() {
    let mut a = Document::new();
    let mut b = Document::new();
    // Sin semilla explícita, documentos frescos ya son deterministas.
    let first_a = ok_in(&mut a, "RandomBetween[1,1000000]");
    let first_b = ok_in(&mut b, "RandomBetween[1,1000000]");
    assert_eq!(first_a, first_b, "determinismo base");
    // Con SetSeed la secuencia cambia y sigue siendo reproducible.
    let seed1 = ok_in(&mut a, "SetSeed[7]");
    assert!(seed1.contains('7'), "semilla: {seed1}");
    let seeded_a = ok_in(&mut a, "RandomBetween[1,1000000]");
    let _ = ok_in(&mut b, "SetSeed[7]");
    let seeded_b = ok_in(&mut b, "RandomBetween[1,1000000]");
    assert_eq!(seeded_a, seeded_b, "misma semilla → mismo valor");
    assert!(err_in(&mut a, "SetSeed[-1]").contains("entero"));
}

#[test]
fn p5_cas_loaded_and_object() {
    let mut document = Document::new();
    assert!(ok_in(&mut document, "CASLoaded[]").contains("verdadero"));
    assert!(matches!(
        run_in(&mut document, "Point[(1,2)]"),
        CommandOutcome::Ok
    ));
    let label = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Point(_)).then(|| o.label().to_string())
        })
        .expect("punto");
    assert_eq!(
        ok_in(&mut document, &format!("Object[{label}]")),
        format!("Object = {label}")
    );
    assert!(err_in(&mut document, "Object[Nope]").contains("no existe"));
}

#[test]
fn p5_copy_free_object_drops_dependencies() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Point[(3,4)]"),
        CommandOutcome::Ok
    ));
    let label = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Point(_)).then(|| o.label().to_string())
        })
        .expect("punto");
    let out = ok_in(&mut document, &format!("CopyFreeObject[{label}]"));
    assert!(out.contains("copia libre"), "{out}");
    // La copia es un punto libre independiente.
    assert_eq!(document.object_count(), 2);
    // Tipos no copiables → error honesto.
    let function_outcome = run_in(&mut document, "Function[x^2]");
    assert!(
        matches!(
            function_outcome,
            CommandOutcome::Ok | CommandOutcome::Message(_)
        ),
        "Function debe crear el objeto: {function_outcome:?}"
    );
    let f = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Function(_)).then(|| o.label().to_string())
        })
        .expect("función");
    let err = err_in(&mut document, &format!("CopyFreeObject[{f}]"));
    assert!(err.contains("no es copiable"), "{err}");
}

#[test]
fn p5_set_background_color_view_and_object() {
    let mut document = Document::new();
    assert!(ok_in(&mut document, "SetBackgroundColor[blue]").contains("fondo"));
    assert!(
        document.variables.contains_key("__view_bg"),
        "variable __view_bg"
    );
    assert!(matches!(
        run_in(&mut document, "Polygon[(0,0),(2,0),(2,2),(0,2)]"),
        CommandOutcome::Ok
    ));
    let poly = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Polygon(_)).then(|| o.label().to_string())
        })
        .expect("polígono");
    assert!(ok_in(&mut document, &format!("SetBackgroundColor[{poly}, red]")).contains("aplicado"));
    assert!(err_in(&mut document, "SetBackgroundColor[violeta]").contains("no soportado"));
    assert!(err_in(
        &mut document,
        &format!("SetBackgroundColor[{poly}, \"2,0,0\"]")
    )
    .contains("0..=1"));
}

#[test]
fn p5_set_spin_speed_validates_range() {
    let mut document = Document::new();
    assert!(ok_in(&mut document, "SetSpinSpeed[90]").contains("90"));
    assert!(document.variables.contains_key("__view_spin_speed"));
    assert!(err_in(&mut document, "SetSpinSpeed[400]").contains("360"));
}

#[test]
fn p5_attach_copy_to_view_creates_copy() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Point[(5,5)]"),
        CommandOutcome::Ok
    ));
    let label = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Point(_)).then(|| o.label().to_string())
        })
        .expect("punto");
    let out = ok_in(&mut document, &format!("AttachCopyToView[{label}, 1]"));
    assert!(out.contains("vista 1"), "{out}");
    assert_eq!(document.object_count(), 2);
    assert!(err_in(&mut document, &format!("AttachCopyToView[{label}, 2]")).contains("vista"));
}

#[test]
fn p5_turtle_stateful_commands_draw_and_rotate() {
    let mut document = Document::new();
    // Reanuda el lápiz y avanza: crea un segmento.
    let fwd = ok_in(&mut document, "TurtleForward[3]");
    assert!(fwd.contains("(3.0000, 0.0000)"), "avance: {fwd}");
    assert_eq!(document.object_count(), 1, "segmento dibujado");
    // Gira 90° y avanza de nuevo: segundo segmento hacia arriba.
    let turn = ok_in(&mut document, "TurtleLeft[90]");
    assert!(turn.contains("90.00"), "giro: {turn}");
    let fwd2 = ok_in(&mut document, "TurtleForward[2]");
    assert!(fwd2.contains("(3.0000, 2.0000)"), "segundo avance: {fwd2}");
    assert_eq!(document.object_count(), 2);
    // Lápiz arriba: se mueve sin dibujar.
    let up = ok_in(&mut document, "TurtleUp[]");
    assert!(up.contains("arriba"), "{up}");
    let _ = ok_in(&mut document, "TurtleBack[1]");
    assert_eq!(document.object_count(), 2, "sin trazo con lápiz arriba");
    // Rumbo persistente entre comandos (estado en el documento).
    assert!(document.variables.contains_key("__turtle_heading"));
}

#[test]
fn p5_all_new_commands_palette_visible() {
    use grafito_command::command_registry;
    for canonical in [
        "GroebnerLexDeg",
        "SD",
        "SampleVariance",
        "SetSeed",
        "CASLoaded",
        "CopyFreeObject",
        "SetBackgroundColor",
        "SetSpinSpeed",
        "AttachCopyToView",
        "Object",
        "Slope",
        "SetValue",
        "TurtleForward",
        "TurtleBack",
        "TurtleLeft",
        "TurtleRight",
        "TurtleUp",
        "TurtleDown",
    ] {
        let spec = command_registry::resolve(canonical)
            .unwrap_or_else(|| panic!("{canonical} no registrado"));
        assert!(spec.palette_visible, "{canonical} debe verse en paleta");
    }
}

#[test]
fn p5_slope_and_set_value_are_registered() {
    let mut document = Document::new();
    // Slope necesita un objeto: error honesto de existencia.
    assert!(err_in(&mut document, "Slope[Nope]").contains("Slope"));
    // SetValue sobre variable libre.
    let out = ok_in(&mut document, "SetValue[a, 4]");
    assert!(
        out.contains("SetValue") || out.contains('4'),
        "SetValue: {out}"
    );
    assert_eq!(document.variables.get("a"), Some(&4.0));
}
