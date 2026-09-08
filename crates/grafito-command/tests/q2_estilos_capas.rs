#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente Q2: estilos reales + capas. Regla de oro: cada comando nuevo
//! visible en paleta + responde por `process_input` + test e2e. Nada muerto.

use grafito_command::{
    command_registry,
    commands::{process_input, CommandOutcome},
};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_owned();
    process_input(document, &mut input)
}

fn assert_message_contains(document: &mut Document, command: &str, needle: &str) {
    match run(document, command) {
        CommandOutcome::Message(message) => assert!(
            message.contains(needle),
            "{command} → {message} (esperaba '{needle}')"
        ),
        CommandOutcome::Ok => panic!("{command} dio Ok, esperaba Message con '{needle}'"),
        CommandOutcome::Error(message) => {
            panic!("{command} dio Error: {message} (esperaba '{needle}')")
        }
    }
}

fn assert_error_contains(document: &mut Document, command: &str, needle: &str) {
    match run(document, command) {
        CommandOutcome::Error(message) => assert!(
            message.contains(needle),
            "{command} → {message} (esperaba error con '{needle}')"
        ),
        other => panic!("{command} debía dar Error con '{needle}', dio {other:?}"),
    }
}

fn assert_ok_or_message(document: &mut Document, command: &str) {
    match run(document, command) {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        CommandOutcome::Error(message) => panic!("{command} dio Error: {message}"),
    }
}

fn make_point(document: &mut Document, xy: &str) -> String {
    assert_ok_or_message(document, &format!("Point[{xy}]"));
    let t = xy.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = t.split(',');
    let ex: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    let ey: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    document
        .objects_iter()
        .find_map(|(_, obj)| match obj {
            GeoObject::Point(p)
                if (p.position.x - ex).abs() < 1e-9 && (p.position.y - ey).abs() < 1e-9 =>
            {
                Some(p.label.clone())
            }
            _ => None,
        })
        .expect("fixture punto no creado")
}

fn make_line(document: &mut Document) -> String {
    assert_ok_or_message(document, "Line[(0, 0), (1, 0)]");
    document
        .objects_iter()
        .find_map(|(_, obj)| match obj {
            GeoObject::Line(l) => Some(l.label.clone()),
            _ => None,
        })
        .expect("fixture línea no creada")
}

#[test]
fn q2_nuevos_visibles_en_paleta_y_resuelven_alias() {
    for canonical in ["SetLineStyle", "SetPointStyle", "SetLayer"] {
        let spec = command_registry::resolve(canonical)
            .unwrap_or_else(|| panic!("{canonical} debe estar registrado"));
        assert!(spec.palette_visible, "{canonical} visible en paleta");
        assert_eq!(spec.canonical, canonical);
    }
    assert_eq!(
        command_registry::canonicalize("estilo_linea"),
        Some("SetLineStyle")
    );
    assert_eq!(
        command_registry::canonicalize("estilo_punto"),
        Some("SetPointStyle")
    );
    assert_eq!(
        command_registry::canonicalize("poner_capa"),
        Some("SetLayer")
    );
}

#[test]
fn q2_set_line_style_cambia_trazo_y_es_honesto() {
    let mut document = Document::new();
    let linea = make_line(&mut document);
    assert_message_contains(
        &mut document,
        &format!("SetLineStyle[{linea}, dashed]"),
        "dashed",
    );
    let estilo = document
        .try_find_object_by_label(&linea)
        .expect("etiqueta única")
        .and_then(|id| document.get_object(id).and_then(|o| o.line_style()));
    assert_eq!(estilo, Some(grafito_core::LineStyle::Dashed));
    // Alias ES + mayúsculas.
    assert_message_contains(
        &mut document,
        &format!("estilo_linea[{linea}, PUNTEADA]"),
        "dotted",
    );
    // Estilo inexistente: error sin mutar.
    assert_error_contains(
        &mut document,
        &format!("SetLineStyle[{linea}, ondulada]"),
        "no soportado",
    );
    // Punto no tiene trazo: error honesto, no fantasma.
    let punto = make_point(&mut document, "(5, 5)");
    assert_error_contains(
        &mut document,
        &format!("SetLineStyle[{punto}, dashed]"),
        "no tiene trazo",
    );
    // Objeto inexistente.
    assert_error_contains(
        &mut document,
        "SetLineStyle[fantasma, dashed]",
        "no encontrado",
    );
}

#[test]
fn q2_set_point_style_cambia_marcador_y_es_honesto() {
    let mut document = Document::new();
    let punto = make_point(&mut document, "(0, 0)");
    assert_message_contains(
        &mut document,
        &format!("SetPointStyle[{punto}, cross]"),
        "cross",
    );
    let estilo = document
        .try_find_object_by_label(&punto)
        .expect("etiqueta única")
        .and_then(|id| document.get_object(id).and_then(|o| o.point_style()));
    assert_eq!(estilo, Some(grafito_core::PointStyle::Cross));
    // Forma inexistente.
    assert_error_contains(
        &mut document,
        &format!("SetPointStyle[{punto}, estrella]"),
        "no soportada",
    );
    // Línea no es punto.
    let linea = make_line(&mut document);
    assert_error_contains(
        &mut document,
        &format!("SetPointStyle[{linea}, cross]"),
        "no es un punto",
    );
}

#[test]
fn q2_set_layer_mueve_con_clamp_honesto() {
    let mut document = Document::new();
    let punto = make_point(&mut document, "(0, 0)");
    assert_message_contains(&mut document, &format!("SetLayer[{punto}, 2]"), "capa 2");
    let id = document
        .try_find_object_by_label(&punto)
        .expect("etiqueta única")
        .expect("existe");
    assert_eq!(document.layer_of(id), 2);
    // Clamp honesto: avisa y aplica 255.
    assert_message_contains(
        &mut document,
        &format!("SetLayer[{punto}, 300]"),
        "clamp a 255",
    );
    assert_eq!(document.layer_of(id), 255);
    // Negativa y texto: error sin mutar (sigue en 255).
    assert_error_contains(&mut document, &format!("SetLayer[{punto}, -1]"), "0..=255");
    assert_error_contains(
        &mut document,
        &format!("SetLayer[{punto}, arriba]"),
        "entero",
    );
    assert_eq!(document.layer_of(id), 255);
    assert_error_contains(&mut document, "SetLayer[fantasma, 1]", "no encontrado");
}
