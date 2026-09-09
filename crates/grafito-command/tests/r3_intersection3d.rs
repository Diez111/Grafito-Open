#![allow(clippy::unwrap_used, clippy::expect_used)]
//! R3.2 `Intersection3D` plano-cubo — cierra el único stub.
//! E2E vía `process_input`: cubo + plano → polígono cuadrado esperado,
//! sin corte honesto y resto de poliedros aún stub.

use grafito_command::command_registry;
use grafito_command::commands::{find_object_by_label, process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    process_input(document, &mut command.to_string())
}

fn doc_cubo_plano() -> (Document, String, String) {
    let mut doc = Document::new();
    assert!(matches!(
        run(&mut doc, "Cube[0, 0, 0, 2]"),
        CommandOutcome::Ok
    ));
    let cubo = doc
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Cube3D(_)).then(|| o.label().to_string()))
        .expect("cubo");
    assert!(matches!(
        run(&mut doc, "Plane3D[0, 0, 1, 0]"),
        CommandOutcome::Ok
    ));
    let plano = doc
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Plane3D(_)).then(|| o.label().to_string()))
        .expect("plano");
    (doc, cubo, plano)
}

#[test]
fn intersection3d_es_visible() {
    let spec = command_registry::resolve("Intersection3D").expect("registrado");
    assert!(spec.palette_visible, "REGLA DE ORO: paleta visible");
    assert!(
        spec.help.contains("Plano-Cubo"),
        "help sin stub: {}",
        spec.help
    );
}

#[test]
fn plano_cubo_da_cuadrado_esperado() {
    let (mut doc, cubo, plano) = doc_cubo_plano();
    let outcome = run(&mut doc, &format!("Intersection3D[{plano}, {cubo}]"));
    let mensaje = match outcome {
        CommandOutcome::Message(m) => m,
        other => panic!("polígono esperado, dio {other:?}"),
    };
    assert!(mensaje.contains("polígono plano-cubo"), "{mensaje}");
    assert!(mensaje.contains("alzado"), "{mensaje}");
    let poly = doc
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Polygon(p) => Some(p),
            _ => None,
        })
        .last()
        .expect("polígono creado");
    assert_eq!(poly.vertices.len(), 4, "{:?}", poly.vertices);
    let mut xs: Vec<f64> = poly.vertices.iter().map(|v| v.x).collect();
    let mut ys: Vec<f64> = poly.vertices.iter().map(|v| v.y).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, exp) in xs.iter().zip([-1.0, -1.0, 1.0, 1.0]) {
        assert!((got - exp).abs() < 1e-9, "x {got} vs {exp}");
    }
    for (got, exp) in ys.iter().zip([-1.0, -1.0, 1.0, 1.0]) {
        assert!((got - exp).abs() < 1e-9, "y {got} vs {exp}");
    }
    // Orden inverso también resuelve (cubo, plano).
    let outcome = run(&mut doc, &format!("Intersection3D[{cubo}, {plano}]"));
    assert!(
        matches!(&outcome, CommandOutcome::Message(m) if m.contains("polígono")),
        "{outcome:?}"
    );
}

#[test]
fn plano_lejos_es_honesto_y_tetra_sigue_stub() {
    let mut doc = Document::new();
    assert!(matches!(
        run(&mut doc, "Cube[0, 0, 0, 2]"),
        CommandOutcome::Ok
    ));
    let cubo = doc
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Cube3D(_)).then(|| o.label().to_string()))
        .expect("cubo");
    assert!(matches!(
        run(&mut doc, "Plane3D[0, 0, 1, -5]"),
        CommandOutcome::Ok
    ));
    let plano = doc
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Plane3D(_)).then(|| o.label().to_string()))
        .expect("plano");
    let outcome = run(&mut doc, &format!("Intersection3D[{plano}, {cubo}]"));
    assert!(
        matches!(&outcome, CommandOutcome::Message(m) if m.contains("no forman polígono")),
        "{outcome:?}"
    );
    // Tetraedro: stub honesto restante.
    let mut doc2 = Document::new();
    assert!(matches!(
        run(&mut doc2, "Tetrahedron[0, 0, 0, 2]"),
        CommandOutcome::Ok
    ));
    let tetra = doc2
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Tetrahedron3D(_)).then(|| o.label().to_string()))
        .expect("tetra");
    assert!(matches!(
        run(&mut doc2, "Plane3D[0, 0, 1, 0]"),
        CommandOutcome::Ok
    ));
    let plano2 = doc2
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::Plane3D(_)).then(|| o.label().to_string()))
        .expect("plano");
    let outcome = run(&mut doc2, &format!("Intersection3D[{plano2}, {tetra}]"));
    assert!(
        matches!(&outcome, CommandOutcome::Message(m) if m.contains("solo cubo")),
        "{outcome:?}"
    );
    let _ = find_object_by_label(&doc2, &plano2);
}
