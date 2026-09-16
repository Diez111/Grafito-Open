//! Frente P3b — scripting real: vistas, Execute, listeners, tortuga, ajustes.

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

#[test]
fn p3b_zoom_center_pan() {
    let mut document = Document::new();
    assert!(ok_in(&mut document, "ZoomIn[]").contains("escala"));
    assert!(ok_in(&mut document, "ZoomIn[2]").contains("escala"));
    assert!(ok_in(&mut document, "ZoomOut[]").contains("escala"));
    assert!(err_in(&mut document, "ZoomIn[9]").contains("factor"));
    assert!(ok_in(&mut document, "CenterView[3,4]").contains("(3, 4)"));
    assert!(ok_in(&mut document, "Pan[10,-5]").contains("10"));
    assert!(err_in(&mut document, "Pan[200000,0]").contains("absurdo"));
}

#[test]
fn p3b_execute_runs_allowlist_with_rollback() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "Execute[SetValue[a,3]]");
    assert!(out.contains("pasos"), "ejecuta: {out}");
    assert_eq!(document.variables.get("a"), Some(&3.0));
    // Fallo en ejecución revierte lo ya aplicado (ZoomIn[99] falla en runtime).
    let before_vars = document.variables.clone();
    let err = err_in(&mut document, "Execute[SetValue[b,1]; ZoomIn[99]]");
    assert!(err.contains("ZoomIn"), "falla el paso: {err}");
    assert_eq!(document.variables, before_vars, "atómico");
    // Fuera del allowlist se rechaza sin ejecutar nada.
    assert!(err_in(&mut document, "Execute[Delete[A]]").contains("Execute"));
}

#[test]
fn p3b_listeners_store_and_validate() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Point[(0,0)]"),
        CommandOutcome::Ok
    ));
    let label = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Point(_)).then(|| o.label().to_string())
        })
        .expect("punto");
    assert!(ok_in(&mut document, &format!("OnClick[{label},SetValue[a,9]]")).contains("guardado"));
    assert!(ok_in(&mut document, &format!("OnUpdate[{label},SetValue[a,8]]")).contains("P3c"));
    assert!(ok_in(&mut document, "OnLoad[SetValue[a,7]]").contains("P3c"));
    // El guion se valida al guardar.
    assert!(err_in(&mut document, &format!("OnClick[{label},Delete[A]]")).contains("OnClick"));
    assert!(err_in(&mut document, "OnClick[fantasma,SetValue[a,1]]").contains("no existe"));
    // Ejecución del click vía API pública (la piel la llama igual).
    let n =
        grafito_command::ggbscript::run_click_script(&mut document, &label).expect("corre OnClick");
    assert_eq!(n, 1);
    assert_eq!(document.variables.get("a"), Some(&9.0));
}

#[test]
fn p3b_turtle_square() {
    let mut document = Document::new();
    let out = ok_in(&mut document, "Turtle[REPEAT 4 [FD 10 LT 90]]");
    assert!(out.contains("1 trazado"), "cuadrado: {out}");
    assert!(err_in(&mut document, "Turtle[FLIP 10]").contains("desconocida"));
    // Corchete sin cerrar lo rechaza el parser del comando (honesto, con sugerencia).
    assert!(err_in(&mut document, "Turtle[REPEAT 2 [FD 1]").contains("Faltan cierres"));
    // PU/PD parte en dos trazados.
    let two = ok_in(&mut document, "Turtle[FD 5 PU FD 5 PD FD 5]");
    assert!(two.contains("2 trazado"), "partido: {two}");
}

#[test]
fn p3b_styles_layers_update() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Polygon[(0,0),(2,0),(2,2),(0,2)]"),
        CommandOutcome::Ok
    ));
    let p = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Polygon(_)).then(|| o.label().to_string())
        })
        .expect("polígono");
    assert!(ok_in(&mut document, &format!("SetFilling[{p},0.5]")).contains("0.5"));
    assert!(err_in(&mut document, &format!("SetFilling[{p},2]")).contains('1'));
    assert!(ok_in(&mut document, &format!("SetLineThickness[{p},3]")).contains('3'));
    assert!(ok_in(&mut document, "HideLayer[0]").contains("ocultos"));
    assert!(ok_in(&mut document, "ShowLayer[0]").contains("visibles"));
    assert!(err_in(&mut document, "ShowLayer[999]").contains("255"));
    assert!(ok_in(&mut document, "UpdateConstruction[]").contains("recalculadas"));
}
