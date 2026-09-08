#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Oleada 2 P4: M factible con entrada UI visible (paleta + e2e).
//! Regla de oro: cada comando nuevo visible en paleta + responde por
//! `process_input` + test e2e. Nada muerto.

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

fn assert_ok_or_message(document: &mut Document, command: &str) {
    match run(document, command) {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        CommandOutcome::Error(message) => panic!("{command} dio Error: {message}"),
    }
}

fn palette_must_be_visible(canonical: &str) {
    let spec = command_registry::resolve(canonical)
        .unwrap_or_else(|| panic!("{canonical} debe estar registrado"));
    assert!(
        spec.palette_visible,
        "{canonical} debe ser visible en paleta"
    );
    assert_eq!(spec.canonical, canonical);
}

#[allow(dead_code)]
fn first_label_matching(document: &Document, pred: impl Fn(&GeoObject) -> bool) -> String {
    document
        .objects_iter()
        .find_map(|(_, obj)| {
            if pred(obj) {
                Some(obj.label().to_string())
            } else {
                None
            }
        })
        .expect("fixture no creó el objeto esperado")
}

fn parse_xy(xy: &str) -> (f64, f64) {
    let t = xy.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = t.split(',');
    let x: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    let y: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    (x, y)
}

fn make_point(document: &mut Document, xy: &str) -> String {
    assert_ok_or_message(document, &format!("Point[{xy}]"));
    let (ex, ey) = parse_xy(xy);
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
        .expect("punto fixture por coordenadas")
}

fn make_line(document: &mut Document, a: &str, b: &str) -> String {
    assert_ok_or_message(document, &format!("Line[{a},{b}]"));
    let (ax, ay) = parse_xy(a);
    let (bx, by) = parse_xy(b);
    // La recta guarda start/end normalizados; busca por dirección y punto medio.
    document
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Line(l) => Some((l.label.clone(), l.start, l.end)),
            _ => None,
        })
        .find(|(_, s, e)| {
            let mid = ((s.x + e.x) * 0.5, (s.y + e.y) * 0.5);
            let exp_mid = ((ax + bx) * 0.5, (ay + by) * 0.5);
            (mid.0 - exp_mid.0).abs() < 1e-6 && (mid.1 - exp_mid.1).abs() < 1e-6
        })
        .map(|(label, _, _)| label)
        .expect("recta fixture por punto medio")
}

#[test]
fn oleada2_todos_visibles_en_paleta() {
    for canonical in [
        "Eigenvalues",
        "Eigenvectors",
        "Uniform",
        "Exponential",
        "ChiSquared",
        "InverseExponential",
        "InverseUniform",
        "CFactor",
        "CIFactor",
        "PartialFractions",
        "AreCollinear",
        "AreConcurrent",
        "AreConcyclic",
        "AreParallel",
        "ArePerpendicular",
        "SetCaption",
        "SetLineStyle",
        "SetPointStyle",
        "SetLayer",
        "Dodecahedron",
        "Icosahedron",
        "Octahedron",
        "InfiniteCone",
        "InfiniteCylinder",
    ] {
        palette_must_be_visible(canonical);
    }
    assert_eq!(command_registry::palette_commands().count(), 287);
    assert_eq!(command_registry::all().len(), 333);
}

#[test]
fn oleada2_eigen_y_proba() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Eigenvalues[[[1, 0], [0, 2]]]",
        "Eigenvalues",
    );
    assert_message_contains(&mut document, "Eigenvectors[[[1, 0], [0, 2]]]", "lambda");
    // Valores conocidos: diagonal 1,2 → autovalores 1 y 2.
    match run(&mut document, "Eigenvalues[[[1, 0], [0, 2]]]") {
        CommandOutcome::Message(m) => {
            assert!(m.contains('1') && m.contains('2'), "autovalores 1,2: {m}");
        }
        other => panic!("Eigenvalues debe dar Message, dio {other:?}"),
    }
    assert_message_contains(&mut document, "Uniform[0, 1]", "PDF");
    assert_message_contains(&mut document, "Uniform[0, 1, 0.5]", "CDF");
    assert_message_contains(&mut document, "Exponential[1]", "PDF");
    assert_message_contains(&mut document, "Exponential[1, 0.5]", "CDF");
    assert_message_contains(&mut document, "ChiSquared[2]", "PDF");
    assert_message_contains(&mut document, "ChiSquared[2, 1]", "CDF");
    assert_message_contains(&mut document, "InverseExponential[0.5, 1]", "0.693");
    assert_message_contains(&mut document, "InverseUniform[0.5, 0, 10]", "5.000000");
    // Errores honestos.
    match run(&mut document, "Exponential[0]") {
        CommandOutcome::Error(m) => assert!(m.contains("Exponential"), "{m}"),
        other => panic!("Exponential[0] debe dar Error, dio {other:?}"),
    }
    match run(&mut document, "InverseUniform[2, 0, 1]") {
        CommandOutcome::Error(m) => assert!(m.contains("InverseUniform"), "{m}"),
        other => panic!("p fuera de rango debe dar Error, dio {other:?}"),
    }
}

#[test]
fn oleada2_cfactor_y_parciales() {
    let mut document = Document::new();
    // x^2+1 → complejo conjugado con i.
    assert_message_contains(&mut document, "CFactor[x^2+1, x]", "i");
    // Real sigue igual que Factor.
    assert_message_contains(&mut document, "CFactor[x^2-1, x]", "x");
    // Gaussiana: contenido entero + complejo.
    assert_message_contains(&mut document, "CIFactor[2*x^2+2, x]", "i");
    // Parciales con verificación: (2x+3)/((x-1)(x+2)) → A/(x-1)+B/(x+2).
    assert_message_contains(
        &mut document,
        "PartialFractions[(2*x+3)/((x-1)*(x+2))]",
        "/",
    );
    match run(&mut document, "PartialFractions[(2*x+3)/((x-1)*(x+2))]") {
        CommandOutcome::Message(m) => {
            assert!(m.contains("x - 1") || m.contains("x-1"), "lineal 1: {m}");
            assert!(m.contains("x + 2") || m.contains("x+2"), "lineal 2: {m}");
        }
        other => panic!("PartialFractions debe dar Message, dio {other:?}"),
    }
    // Cuadrática irreducible: 1/(x^2+1) → (Bx+C)/(x^2+1).
    assert_message_contains(&mut document, "PartialFractions[1/(x^2+1)]", "x^2");
    // Impropia honesta (grado num 3 ≥ grado den 2).
    match run(&mut document, "PartialFractions[(x^3+1)/(x^2+1)]") {
        CommandOutcome::Error(m) => assert!(m.contains("propia"), "{m}"),
        other => panic!("impropia debe dar Error honesto, dio {other:?}"),
    }
}

#[test]
fn oleada2_relaciones() {
    let mut document = Document::new();
    let a = make_point(&mut document, "(0, 0)");
    let b = make_point(&mut document, "(1, 1)");
    let c = make_point(&mut document, "(2, 2)");
    let d = make_point(&mut document, "(0, 1)");
    assert_message_contains(
        &mut document,
        &format!("AreCollinear[{a}, {b}, {c}]"),
        "true",
    );
    assert_message_contains(
        &mut document,
        &format!("AreCollinear[{a}, {b}, {d}]"),
        "false",
    );
    // Paralelas horizontales.
    let mut document = Document::new();
    let l1 = make_line(&mut document, "(0, 0)", "(1, 0)");
    let l2 = make_line(&mut document, "(0, 1)", "(1, 1)");
    let l3 = make_line(&mut document, "(0, 0)", "(0, 1)");
    assert_message_contains(&mut document, &format!("AreParallel[{l1}, {l2}]"), "true");
    assert_message_contains(&mut document, &format!("AreParallel[{l1}, {l3}]"), "false");
    assert_message_contains(
        &mut document,
        &format!("ArePerpendicular[{l1}, {l3}]"),
        "true",
    );
    assert_message_contains(
        &mut document,
        &format!("ArePerpendicular[{l1}, {l2}]"),
        "false",
    );
    // Concurrentes en origen (puntos medios distintos a propósito).
    let mut document = Document::new();
    let m1 = make_line(&mut document, "(-2, -2)", "(1, 1)");
    let m2 = make_line(&mut document, "(-2, 2)", "(1, -1)");
    let m3 = make_line(&mut document, "(0, -2)", "(0, 2)");
    assert_message_contains(
        &mut document,
        &format!("AreConcurrent[{m1}, {m2}, {m3}]"),
        "true",
    );
    // Concíclicos en círculo unidad.
    let mut document = Document::new();
    let p1 = make_point(&mut document, "(1, 0)");
    let p2 = make_point(&mut document, "(0, 1)");
    let p3 = make_point(&mut document, "(-1, 0)");
    let p4 = make_point(&mut document, "(0, -1)");
    let p5 = make_point(&mut document, "(0, 0)");
    assert_message_contains(
        &mut document,
        &format!("AreConcyclic[{p1}, {p2}, {p3}, {p4}]"),
        "true",
    );
    assert_message_contains(
        &mut document,
        &format!("AreConcyclic[{p1}, {p2}, {p3}, {p5}]"),
        "false",
    );
}

#[test]
fn oleada2_set_caption() {
    let mut document = Document::new();
    let a = make_point(&mut document, "(0, 0)");
    assert_message_contains(
        &mut document,
        &format!("SetCaption[{a}, MiPunto]"),
        "MiPunto",
    );
    // Visible al instante: buscar por rótulo nuevo.
    assert!(
        document.try_find_object_by_label("MiPunto").is_ok(),
        "rótulo nuevo debe ser encontrable"
    );
    // Colisión honesta.
    let b = make_point(&mut document, "(1, 1)");
    match run(&mut document, &format!("SetCaption[{b}, MiPunto]")) {
        CommandOutcome::Error(m) => assert!(m.contains("ya existe"), "{m}"),
        other => panic!("colisión debe dar Error, dio {other:?}"),
    }
}

#[test]
fn oleada2_solidos_platonicos_e_infinitos() {
    let mut document = Document::new();
    assert_ok_or_message(&mut document, "Dodecahedron[0, 0, 0, 2]");
    assert_ok_or_message(&mut document, "Icosahedron[0, 0, 0, 2]");
    assert_ok_or_message(&mut document, "Octahedron[0, 0, 0, 2]");
    assert_ok_or_message(&mut document, "InfiniteCone[0, 0, 0, 0, 1, 0, 30]");
    assert_ok_or_message(&mut document, "InfiniteCylinder[0, 0, 0, 0, 1, 0, 1]");
    assert!(document
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::Platonic3D(p) if p.kind.as_str() == "Dodecahedron")));
    assert!(document
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::Platonic3D(p) if p.kind.as_str() == "Icosahedron")));
    assert!(document
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::Platonic3D(p) if p.kind.as_str() == "Octahedron")));
    assert!(document
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::InfiniteCone3D(_))));
    assert!(document
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::InfiniteCylinder3D(_))));
    // Errores honestos.
    match run(&mut document, "Dodecahedron[0, 0, 0, 0]") {
        CommandOutcome::Error(m) => assert!(m.contains("Dodecahedron"), "{m}"),
        other => panic!("arista 0 debe dar Error, dio {other:?}"),
    }
    match run(&mut document, "InfiniteCone[0, 0, 0, 0, 0, 0, 30]") {
        CommandOutcome::Error(m) => assert!(m.contains("InfiniteCone"), "{m}"),
        other => panic!("dirección nula debe dar Error, dio {other:?}"),
    }
    match run(&mut document, "InfiniteCone[0, 0, 0, 0, 1, 0, 0]") {
        CommandOutcome::Error(m) => assert!(m.contains("InfiniteCone"), "{m}"),
        other => panic!("ángulo 0 debe dar Error, dio {other:?}"),
    }
}
