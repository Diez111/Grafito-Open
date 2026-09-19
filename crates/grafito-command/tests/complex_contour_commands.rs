//! Contorno complejo: `ComplexIntegral`/`Gauss` validan el tipo de curva y
//! aceptan el trazo a mano alzada como target (antes: no-op silencioso).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{CircleObj, Document, GeoObject, PencilObj, PointObj};
use grafito_geometry::Point2;

fn run_in(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_string();
    process_input(document, &mut input)
}

fn has_complex_integral(document: &Document) -> bool {
    document
        .objects_iter()
        .any(|(_, object)| matches!(object, GeoObject::ComplexIntegral(_)))
}

#[test]
fn trazo_a_mano_es_contorno_valido() {
    let mut document = Document::new();
    let pencil = GeoObject::Pencil(
        PencilObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
        ])
        .with_label("trazo"),
    );
    document.try_add_object(pencil).expect("trazo");
    let outcome = run_in(&mut document, "ComplexIntegral[1/z, trazo]");
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "trazo aceptado: {outcome:?}"
    );
    assert!(has_complex_integral(&document), "objeto creado");
}

#[test]
fn circulo_y_gauss_aceptan_contorno() {
    let mut document = Document::new();
    document
        .try_add_object(GeoObject::Circle(
            CircleObj::new(Point2::new(0.0, 0.0), 1.0).with_label("c"),
        ))
        .expect("círculo");
    let outcome = run_in(&mut document, "Gauss[1/z, c]");
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Gauss sobre círculo: {outcome:?}"
    );
    assert!(has_complex_integral(&document));
}

#[test]
fn objeto_que_no_es_curva_da_error_honesto() {
    let mut document = Document::new();
    document
        .try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
        ))
        .expect("punto");
    let outcome = run_in(&mut document, "ComplexIntegral[1/z, A]");
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(
                message.contains("no es una curva"),
                "error honesto: {message}"
            );
        }
        other => panic!("esperaba error honesto, llegó {other:?}"),
    }
    assert!(!has_complex_integral(&document), "no crea objeto fantasma");
}
