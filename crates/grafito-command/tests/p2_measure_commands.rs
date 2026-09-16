//! Frente P2 — medidas, centros, proximidad, envolvente y planos 3D.

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

fn make_square(document: &mut Document) {
    assert!(matches!(
        run_in(document, "Polygon[(0,0),(2,0),(2,2),(0,2)]"),
        CommandOutcome::Ok
    ));
}

fn make_triangle(document: &mut Document) {
    assert!(matches!(
        run_in(document, "Polygon[(0,0),(4,0),(0,3)]"),
        CommandOutcome::Ok
    ));
}

fn poly_label(document: &Document) -> String {
    document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Polygon(_)).then(|| o.label().to_string())
        })
        .expect("polígono creado")
}

/// Etiqueta del punto en (x, y) exacto (el orden de iteración no es el
/// de creación: se busca por coordenadas).
fn find_point(document: &Document, x: f64, y: f64) -> String {
    document
        .objects_iter()
        .find_map(|(_, o)| match o {
            grafito_core::GeoObject::Point(p) if p.position.x == x && p.position.y == y => {
                Some(o.label().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("punto ({x},{y}) no creado"))
}

fn make_points(document: &mut Document, n: usize) -> Vec<String> {
    let coords = [(0.0, 0.0), (4.0, 0.0), (0.0, 3.0), (5.0, 5.0), (1.0, 1.0)];
    for (x, y) in coords.iter().take(n) {
        assert!(
            matches!(
                run_in(document, &format!("Point[({x},{y})]")),
                CommandOutcome::Ok
            ),
            "creando ({x},{y})"
        );
    }
    coords
        .iter()
        .take(n)
        .map(|(x, y)| find_point(document, *x, *y))
        .collect()
}

#[test]
fn p2_area_perimeter_length() {
    let mut document = Document::new();
    make_square(&mut document);
    let p = poly_label(&document);
    assert!(ok_in(&mut document, &format!("Area[{p}]")).contains("= 4"));
    assert!(ok_in(&mut document, &format!("Perimeter[{p}]")).contains("= 8"));
    assert!(ok_in(&mut document, &format!("Length[{p}]")).contains("= 8"));
    assert!(err_in(&mut document, "Area[fantasma]").contains("no encontrado"));
}

#[test]
fn p2_circle_radius_circumference() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Circle[(0,0),2]"),
        CommandOutcome::Ok
    ));
    let c = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Circle(_)).then(|| o.label().to_string())
        })
        .expect("círculo");
    assert!(ok_in(&mut document, &format!("Radius[{c}]")).contains("= 2"));
    let circ = ok_in(&mut document, &format!("Circumference[{c}]"));
    assert!(circ.contains("12.566"), "2πr: {circ}");
}

#[test]
fn p2_height_triangle() {
    let mut document = Document::new();
    let pts = make_points(&mut document, 3);
    // Altura desde el tercero a la recta de los dos primeros: vale 3.
    let out = ok_in(
        &mut document,
        &format!("Height[{},{},{}]", pts[0], pts[1], pts[2]),
    );
    assert!(out.contains("= 3"), "altura: {out}");
}

#[test]
fn p2_triangle_centers() {
    let mut document = Document::new();
    make_triangle(&mut document);
    let p = poly_label(&document);
    // Incentro del 3-4-5 en (1,1).
    let inc = ok_in(&mut document, &format!("TriangleCenter[{p},3]"));
    assert!(inc.contains("(1, 1)"), "incentro: {inc}");
    // Centroide por área.
    let cen = ok_in(&mut document, &format!("Centroid[{p}]"));
    assert!(cen.contains("1.333"), "centroide: {cen}");
    // Baricentro de puntos.
    let pts = make_points(&mut document, 2);
    let bar = ok_in(&mut document, &format!("Barycenter[{},{}]", pts[0], pts[1]));
    assert!(bar.contains("Barycenter"), "baricentro: {bar}");
    assert!(err_in(&mut document, &format!("TriangleCenter[{p},7]")).contains('6'));
}

#[test]
fn p2_trilinear_and_triangle_curve() {
    let mut document = Document::new();
    let pts = make_points(&mut document, 3);
    // Trilineales 1:1:1 = incentro (1,1).
    let tri = ok_in(
        &mut document,
        &format!("Trilinear[1,1,1,{},{},{}]", pts[0], pts[1], pts[2]),
    );
    assert!(tri.contains("(1, 1)"), "incentro trilinear: {tri}");
    // Steiner inellipse: A²+B²+C²−2BC−2CA−2AB = 0.
    let cur = ok_in(
        &mut document,
        &format!(
            "TriangleCurve[{},{},{},A^2+B^2+C^2-2*B*C-2*C*A-2*A*B=0]",
            pts[0], pts[1], pts[2]
        ),
    );
    assert!(cur.contains("TriangleCurve"), "curva: {cur}");
}

#[test]
fn p2_conic_axes() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Ellipse[(0,0),3,2]"),
        CommandOutcome::Ok
    ));
    let e = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Ellipse(_)).then(|| o.label().to_string())
        })
        .expect("elipse");
    assert!(ok_in(&mut document, &format!("MajorAxis[{e}]")).contains("= 6"));
    assert!(ok_in(&mut document, &format!("MinorAxis[{e}]")).contains("= 4"));
    assert!(ok_in(&mut document, &format!("SemiMajorAxisLength[{e}]")).contains("= 3"));
    assert!(ok_in(&mut document, &format!("SemiMinorAxisLength[{e}]")).contains("= 2"));
    let ecc = ok_in(&mut document, &format!("LinearEccentricity[{e}]"));
    assert!(ecc.contains("2.236"), "√5: {ecc}");
    assert!(ok_in(&mut document, &format!("ConjugateDiameter[{e}]")).contains("= 4"));
}

#[test]
fn p2_proximity() {
    let mut document = Document::new();
    make_square(&mut document);
    let p = poly_label(&document);
    let pts = make_points(&mut document, 5);
    let (inside, outside) = (pts[4].clone(), pts[3].clone());
    // Adentro cuenta.
    assert!(ok_in(&mut document, &format!("PointIn[{inside},{p}]")).contains("verdadero"));
    assert!(ok_in(&mut document, &format!("PointIn[{outside},{p}]")).contains("falso"));
    // Cercano al borde.
    let cp = ok_in(&mut document, &format!("ClosestPoint[{outside},{p}]"));
    assert!(cp.contains("ClosestPoint"), "cercano: {cp}");
    // Región: adentro devuelve el punto.
    let rg = ok_in(&mut document, &format!("ClosestPointRegion[{inside},{p}]"));
    assert!(rg.contains("(1, 1)"), "región: {rg}");
    // Aleatorio determinista: dos historiales idénticos (mismos comandos en
    // el mismo orden → misma versión), mismo punto.
    let r1 = ok_in(&mut document, &format!("RandomPointIn[{p}]"));
    let mut document2 = Document::new();
    make_square(&mut document2);
    make_points(&mut document2, 5);
    let p2 = poly_label(&document2);
    let pts2: Vec<String> = [(1.0, 1.0), (5.0, 5.0)]
        .iter()
        .map(|(x, y)| find_point(&document2, *x, *y))
        .collect();
    let _ = ok_in(&mut document2, &format!("ClosestPoint[{},{}]", pts2[1], p2));
    let _ = ok_in(
        &mut document2,
        &format!("ClosestPointRegion[{},{}]", pts2[0], p2),
    );
    let r2 = ok_in(&mut document2, &format!("RandomPointIn[{p2}]"));
    assert_eq!(r1, r2, "determinista");
}

#[test]
fn p2_intersect_path_line_circle() {
    let mut document = Document::new();
    assert!(matches!(
        run_in(&mut document, "Line[(0,0),(1,0)]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run_in(&mut document, "Circle[(0,0),2]"),
        CommandOutcome::Ok
    ));
    let line = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Line(_)).then(|| o.label().to_string())
        })
        .expect("recta");
    let circle = document
        .objects_iter()
        .find_map(|(_, o)| {
            matches!(o, grafito_core::GeoObject::Circle(_)).then(|| o.label().to_string())
        })
        .expect("círculo");
    let out = ok_in(&mut document, &format!("IntersectPath[{line},{circle}]"));
    assert!(out.contains("IntersectPath = {"), "intersección: {out}");
    assert!(out.matches(',').count() >= 1, "dos puntos: {out}");
}

#[test]
fn p2_envelope_parabola() {
    let mut document = Document::new();
    // Familia 2tx − y + t² = 0 envuelve y = −x².
    let out = ok_in(&mut document, "Envelope[2*t,-1,t^2,-2,2]");
    assert!(out.contains("created"), "envolvente: {out}");
    let bad = err_in(&mut document, "Envelope[sin(t),cos(t),tan(t),0,0]");
    assert!(!bad.is_empty());
}

#[test]
fn p2_planes_3d() {
    let mut document = Document::new();
    for pt in ["Point3D[0,0,0]", "Point3D[2,0,0]", "Point3D[0,0,5]"] {
        assert!(
            matches!(run_in(&mut document, pt), CommandOutcome::Ok),
            "{pt}"
        );
    }
    let pts: Vec<String> = document
        .objects_iter()
        .filter(|(_, o)| matches!(o, grafito_core::GeoObject::Point3D(_)))
        .map(|(_, o)| o.label().to_string())
        .collect();
    assert_eq!(pts.len(), 3);
    let bi = ok_in(
        &mut document,
        &format!("PlaneBisector[{},{}]", pts[0], pts[1]),
    );
    assert!(bi.contains("created"), "bisector: {bi}");
    let pp = ok_in(
        &mut document,
        &format!("PerpendicularPlane[{},{},{}]", pts[0], pts[1], pts[2]),
    );
    assert!(pp.contains("created"), "perpendicular: {pp}");
    assert!(err_in(
        &mut document,
        &format!("PlaneBisector[{},{}]", pts[0], pts[0])
    )
    .contains("distintos"));
}
