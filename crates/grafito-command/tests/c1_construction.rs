#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente C1 — huecos de construcción: Polyline abierta, Pyramid 3D y
//! compás fino (Compasses ya cubría ambas formas: se fija con tests).

use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run_on(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn run(command: &str) -> CommandOutcome {
    run_on(&mut Document::new(), command)
}

#[test]
fn c1_polyline_creates_open_chain() {
    let mut document = Document::new();
    match run_on(&mut document, "Polyline[(0,0), (1,0), (1,1)]") {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        other => panic!("Polyline válida debe crear objeto, dio {other:?}"),
    }
    let found = document
        .objects_iter()
        .find(|(_, obj)| matches!(obj, GeoObject::Polyline(_)));
    let (_, obj) = found.expect("el documento debe contener la polilínea");
    match obj {
        GeoObject::Polyline(line) => {
            assert_eq!(line.points.len(), 3);
            assert!(!matches!(obj, GeoObject::Polygon(_)));
        }
        other => panic!("esperaba Polyline, llegó {}", other.name()),
    }
}

#[test]
fn c1_polyline_needs_two_points_and_honest_errors() {
    match run("Polyline[(0,0)]") {
        CommandOutcome::Error(message) => assert!(
            message.contains("Polyline") || message.contains("argumentos"),
            "error honesto, fue: {message}"
        ),
        other => panic!("1 punto debe dar Error, dio {other:?}"),
    }
    match run("Polyline[(0,0), (x, 1)]") {
        CommandOutcome::Error(message) => assert!(message.contains("Polyline")),
        other => panic!("punto no finito debe dar Error, dio {other:?}"),
    }
}

#[test]
fn c1_polyline_registry_metadata() {
    let spec = command_registry::resolve("Polyline").expect("Polyline registrada");
    assert_eq!(spec.dispatch_key, "Polyline");
    assert!(!spec.palette_visible, "primitiva oculta como Polygon");
    assert_eq!(
        command_registry::canonicalize("polilinea"),
        Some("Polyline")
    );
    assert!(spec.accepts_argument_count(2));
    assert!(spec.accepts_argument_count(8192));
    assert!(!spec.accepts_argument_count(1));
    assert!(!spec.accepts_argument_count(8193));
}

#[test]
fn c1_pyramid_creates_square_pyramid() {
    let mut document = Document::new();
    match run_on(&mut document, "Pyramid[0, 0, 0, 2, 3]") {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        other => panic!("Pyramid válida debe crear objeto, dio {other:?}"),
    }
    let (_, obj) = document
        .objects_iter()
        .find(|(_, obj)| matches!(obj, GeoObject::Pyramid3D(_)))
        .expect("el documento debe contener la pirámide");
    match obj {
        GeoObject::Pyramid3D(py) => {
            assert_eq!(py.base_size, 2.0);
            assert_eq!((py.apex.x, py.apex.y, py.apex.z), (0.0, 3.0, 0.0));
            let volume =
                grafito_core::symbolic::solids::solid_volume(obj).expect("volumen exacto base²h/3");
            assert!((volume - 4.0).abs() < 1e-9, "V=2²·3/3=4, fue {volume}");
        }
        other => panic!("esperaba Pyramid3D, llegó {}", other.name()),
    }
}

#[test]
fn c1_pyramid_rejects_nonpositive_and_bad_arity() {
    for command in ["Pyramid[0, 0, 0, -2, 3]", "Pyramid[0, 0, 0, 2, 0]"] {
        match run(command) {
            CommandOutcome::Error(message) => assert!(message.contains("Pyramid")),
            other => panic!("{command} debe dar Error, dio {other:?}"),
        }
    }
    match run("Pyramid[0, 0, 0, 2]") {
        CommandOutcome::Error(_) => {}
        other => panic!("aridad 4 debe dar Error, dio {other:?}"),
    }
}

#[test]
fn c1_pyramid_registry_metadata() {
    let spec = command_registry::resolve("Pyramid").expect("Pyramid registrada");
    assert_eq!(spec.dispatch_key, "Pyramid");
    assert!(!spec.palette_visible, "sólido 3D oculto como Cube/Cone");
    assert_eq!(command_registry::canonicalize("piramide"), Some("Pyramid"));
    assert!(spec.accepts_argument_count(5));
    assert!(!spec.accepts_argument_count(4));
}

#[test]
fn c1_compass_fine_already_exists_both_forms() {
    // Compás fino = Compasses: centro+punto (radio por segmento) y
    // centro+radio numérico. Ambas formas ya existían: se fijan aquí.
    let mut document = Document::new();
    match run_on(&mut document, "Compasses[(0,0), (3,4)]") {
        CommandOutcome::Message(text) => assert!(text.contains("r=5.000"), "fue: {text}"),
        other => panic!("Compasses punto debe dar r=5, dio {other:?}"),
    }
    match run_on(&mut document, "Compasses[(0,0), 2]") {
        CommandOutcome::Message(text) => assert!(text.contains("r=2.000"), "fue: {text}"),
        other => panic!("Compasses radio debe dar r=2, dio {other:?}"),
    }
    let circles: Vec<_> = document
        .objects_iter()
        .filter(|(_, obj)| matches!(obj, GeoObject::Circle(_)))
        .collect();
    assert_eq!(circles.len(), 2);
    let spec = command_registry::resolve("Compasses").expect("Compasses registrada");
    assert!(spec.accepts_argument_count(2));
}
