//! Tests de integración para el comando `ComplexMapping`.
//!
//! Verifica que se puede aplicar una expresión compleja (p.ej. `1/z`)
//! a un objeto del documento y que se crea el `ComplexMappingObj`
//! correspondiente. El render visual se valida manualmente; estos tests
//! sólo cubren la creación correcta del objeto.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{
    Document, FunctionObj, GeoObject, ParametricCurve2DObj, PolarCurveObj, PolygonObj,
};

fn point_obj_count(doc: &Document) -> usize {
    doc.objects_iter()
        .filter(|(_, o)| matches!(o, GeoObject::Point(_)))
        .count()
}

#[test]
fn complex_mapping_inversion_of_polygon_creates_object() {
    use grafito_geometry::Point2;
    let mut doc = Document::new();
    // Cuadrado unitario en el primer cuadrante. El label auto-asignado
    // es "P" (primera letra de "Polygon" + sufijo numérico si hay
    // colisiones).
    let poly = PolygonObj::new(vec![
        Point2::new(0.5, 0.5),
        Point2::new(1.0, 0.5),
        Point2::new(1.0, 1.0),
        Point2::new(0.5, 1.0),
    ]);
    doc.add_object(GeoObject::Polygon(poly));

    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, P]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping should return a message, got {:?}",
        outcome
    );
    let cm = doc
        .objects_iter()
        .find(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(cm.is_some(), "should have created a ComplexMapping object");
    if let Some((_, GeoObject::ComplexMapping(cm))) = cm {
        assert_eq!(cm.expr, "1/z");
        assert!(cm.target != Default::default());
    }
}

#[test]
fn complex_mapping_missing_target_returns_error() {
    let mut doc = Document::new();
    let outcome = process_input(
        &mut doc,
        &mut "ComplexMapping[1/z, inexistente]".to_string(),
    );
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "ComplexMapping on missing target should error, got {:?}",
        outcome
    );
}

#[test]
fn complex_mapping_supports_function_target() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Function(
        FunctionObj::new("sin(x)").with_label("f"),
    ));
    let outcome = process_input(&mut doc, &mut "ComplexMapping[z^2, f]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping[z^2, f] should succeed, got {:?}",
        outcome
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm);
}

#[test]
fn complex_mapping_supports_implicit_target() {
    let mut doc = Document::new();
    // Crear un objeto implícito (no usamos el comando porque queremos
    // verificar la rama del renderer; pero la creación del ComplexMapping
    // no depende de que la cache esté poblada).
    use grafito_core::ImplicitCurveObj;
    use grafito_core::RelationOperator;
    doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "4", RelationOperator::Eq).with_label("c"),
    ));
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping on implicit should succeed, got {:?}",
        outcome
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm);
}

#[test]
fn complex_mapping_supports_polar_target() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::PolarCurve(
        PolarCurveObj::new("1 - cos(t)", 0.0, std::f64::consts::TAU).with_label("p"),
    ));
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, p]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping on polar should succeed, got {:?}",
        outcome
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm);
}

#[test]
fn complex_mapping_supports_parametric_target() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::ParametricCurve2D(
        ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU).with_label("c"),
    ));
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping on parametric should succeed, got {:?}",
        outcome
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm);
}

#[test]
fn complex_mapping_accepts_label_with_parentheses() {
    let mut doc = Document::new();
    // Algunas herramientas exponen el label con "(t)" o "(x)" como sufijo.
    // El comando debe aceptarlo: "Root[f(x)]" y "ComplexMapping[..., f(x)]"
    // apuntan al mismo objeto que tiene label "f".
    doc.add_object(GeoObject::Function(FunctionObj::new("x^2").with_label("f")));
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, f(x)]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping on f(x) should resolve to f, got {:?}",
        outcome
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm);
}

#[test]
fn complex_mapping_wrong_arg_count_does_not_create_object() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Function(FunctionObj::new("x").with_label("f")));
    // Sólo 1 argumento: el normalizer cae en el default porque no es
    // un comando válido de 2 args. Verificamos que NO se crea un
    // ComplexMappingObj (es el comportamiento funcionalmente correcto:
    // un comando mal formado no debe crear objetos fantasma).
    let before = doc
        .objects_iter()
        .filter(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)))
        .count();
    let _ = process_input(&mut doc, &mut "ComplexMapping[1/z]".to_string());
    let after = doc
        .objects_iter()
        .filter(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)))
        .count();
    assert_eq!(
        before, after,
        "malformed ComplexMapping should not create objects"
    );
}

#[test]
fn complex_mapping_does_not_create_extra_points() {
    // Sanity: el comando no debe crear puntos sueltos, sólo un
    // ComplexMappingObj. Esto es importante porque algunos análisis
    // (Root, Extremum) crean puntos, y queremos distinguir.
    let mut doc = Document::new();
    doc.add_object(GeoObject::Function(FunctionObj::new("x").with_label("f")));
    let before = point_obj_count(&doc);
    let _ = process_input(&mut doc, &mut "ComplexMapping[1/z, f]".to_string());
    let after = point_obj_count(&doc);
    assert_eq!(before, after, "ComplexMapping should not create points");
}
