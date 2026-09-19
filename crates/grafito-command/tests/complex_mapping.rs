#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Tests de integración para el comando `ComplexMapping`.
//!
//! Verifica que se puede aplicar una expresión compleja (p.ej. `1/z`)
//! a un objeto del documento y que se crea el `ComplexMappingObj`
//! correspondiente. El render visual se valida manualmente; estos tests
//! sólo cubren la creación correcta del objeto.

use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{
    Document, FunctionObj, GeoObject, ParametricCurve2DObj, PolarCurveObj, PolygonObj,
    RelationOperator, TextObj,
};
use grafito_geometry::Point2;

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
fn complex_mapping_one_over_z_auto_creates_unit_disk_i() {
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, I]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping[1/z, I] should create default I, got {:?}",
        outcome
    );

    let unit_disk_id = doc.objects_iter().find_map(|(id, obj)| match obj {
        GeoObject::ImplicitCurve(ic)
            if ic.label == "I"
                && ic.expr_lhs == "x^2 + y^2"
                && ic.expr_rhs == "1"
                && ic.operator == RelationOperator::Less =>
        {
            Some(*id)
        }
        _ => None,
    });
    let unit_disk_id = unit_disk_id.expect("default implicit disk I should exist");

    let cm = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::ComplexMapping(cm) => Some(cm),
        _ => None,
    });
    let cm = cm.expect("ComplexMapping object should exist");
    assert_eq!(cm.target, unit_disk_id);
    assert!(cm.conformal_cache.is_some());
}

#[test]
fn complex_mapping_auto_i_uses_current_complex_symbol() {
    let mut doc = Document::new();
    let _ = process_input(&mut doc, &mut "ComplexSymbol[w]".to_string());
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/w, I]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexMapping[1/w, I] should work after ComplexSymbol[w], got {:?}",
        outcome
    );
    let cm = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::ComplexMapping(cm) => Some(cm),
        _ => None,
    });
    assert!(cm.and_then(|cm| cm.conformal_cache).is_some());
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
fn complex_mapping_target_faltante_sugiere_alternativa() {
    // README mostraba `ComplexMapping[exp(z), c]` sin crear `c`: el error debe
    // guiar (crear el target o usar la forma de 1 arg con el disco I).
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "ComplexMapping[exp(z), c]".to_string());
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(message.contains("no encontrado"), "mensaje: {message}");
            assert!(
                message.contains("ComplexMapping[expr]") && message.contains("Circle[(0, 0), 3]"),
                "el error sugiere la salida con sintaxis válida: {message}"
            );
        }
        other => panic!("esperaba error honesto, llegó {other:?}"),
    }
    // Y la forma de 1 arg sí funciona en documento vacío.
    let outcome = process_input(&mut doc, &mut "ComplexMapping[exp(z)]".to_string());
    assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
    assert!(doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_))));
}

#[test]
fn complex_mapping_expression_invalida_da_error_honesto() {
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "ComplexMapping[z^]".to_string());
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(
                message.contains("inválida") || message.contains("no se pudo evaluar"),
                "error honesto: {message}"
            );
        }
        other => panic!("esperaba error honesto, llegó {other:?}"),
    }
    assert!(
        !doc.objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_))),
        "no crea objeto muerto"
    );
}

#[test]
fn complex_mapping_afin_y_polinomio_son_validos() {
    // `z` y `z^2+1` no eran reconocidos por la lista corta: el objeto quedaba
    // muerto. Ahora crean un mapeo válido (y el render los dibuja).
    for expr in ["z", "2*z", "z^2+1"] {
        let mut doc = Document::new();
        let outcome = process_input(&mut doc, &mut format!("ComplexMapping[{expr}]"));
        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "ComplexMapping[{expr}] debe crear: {outcome:?}"
        );
        assert!(doc
            .objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_))));
    }
}

#[test]
fn complex_mapping_single_arg_maps_unit_disk() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::Function(FunctionObj::new("x").with_label("f")));
    // 1 argumento: target por defecto = disco unidad "I" (se crea si
    // falta). Debe crear el ComplexMapping, no fallar.
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z]".to_string());
    assert!(
        !matches!(outcome, CommandOutcome::Error(_)),
        "ComplexMapping de 1 arg debe mapear el disco unidad, got {outcome:?}"
    );
    let has_cm = doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_)));
    assert!(has_cm, "single-arg ComplexMapping should create the object");
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

#[test]
fn complex_mapping_rejects_targets_without_a_mappable_2d_geometry() {
    let mut doc = Document::new();
    let mut text = TextObj::new("nota", Point2::new(0.0, 0.0));
    text.label = "nota".to_string();
    doc.add_object(GeoObject::Text(text));
    let before = doc
        .objects_iter()
        .filter(|(_, object)| matches!(object, GeoObject::ComplexMapping(_)))
        .count();

    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, nota]".to_string());

    assert!(matches!(outcome, CommandOutcome::Error(_)));
    assert_eq!(
        doc.objects_iter()
            .filter(|(_, object)| matches!(object, GeoObject::ComplexMapping(_)))
            .count(),
        before,
        "unsupported targets must not create invisible mappings"
    );
}

// snippet para pegar temporalmente
#[test]
fn readme_complex_mapping_examples_run_end_to_end() {
    // El bloque del README debe correr tal cual se lee: primero el target
    // (la etiqueta es la automática) y después el mapeo.
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "Circle[(0, 0), 3]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_) | CommandOutcome::Ok),
        "Circle[(0, 0), 3] debe crear: {outcome:?}"
    );
    let label = doc
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::Circle(circle) => Some(circle.label.clone()),
            _ => None,
        })
        .expect("círculo creado");
    assert!(!label.is_empty(), "etiqueta automática presente");
    for expr in ["1/z", "exp(z)", "z^2"] {
        let command = format!("ComplexMapping[{expr}, {label}]");
        let outcome = process_input(&mut doc, &mut command.clone());
        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "{command} debe mapear: {outcome:?}"
        );
    }
    assert!(doc
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::ComplexMapping(_))));
}

#[test]
fn circle_con_etiqueta_inexistente_guia_en_vez_de_fallar_seco() {
    // El toast viejo sugería `Circle[C, 3]`: si alguien lo prueba, el error
    // debe explicar que el centro no existe y cómo crearlo.
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "Circle[C, 3]".to_string());
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(message.contains("no existe"), "mensaje: {message}");
            assert!(
                message.contains("(0, 0)"),
                "guía con coordenadas/creación: {message}"
            );
        }
        other => panic!("esperaba error con guía, llegó {other:?}"),
    }
    // Con el punto C creado, `Circle[C, 3]` es una construcción válida.
    process_input(&mut doc, &mut "C = (0, 0)".to_string());
    let outcome = process_input(&mut doc, &mut "Circle[C, 3]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "Circle[C, 3] con C existente: {outcome:?}"
    );
    // Y `c` en minúscula encuentra la etiqueta `C` (variante única).
    let outcome = process_input(&mut doc, &mut "Circle[c, 2]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "Circle[c, 2] resuelve a C: {outcome:?}"
    );
}

#[test]
fn etiqueta_que_existe_pero_no_es_punto_explica_que_es() {
    // Escenario real: `Circle[(0,0),3]` deja `C` como círculo; después
    // `Circle[C, 3]` debe decir que C es un círculo y cómo seguir.
    let mut doc = Document::new();
    process_input(&mut doc, &mut "Circle[(0, 0), 3]".to_string());
    let outcome = process_input(&mut doc, &mut "Circle[C, 3]".to_string());
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(
                message.contains("no es un punto (es Circle)"),
                "tipo en el mensaje: {message}"
            );
            assert!(message.contains("(0, 0)"), "salida sugerida: {message}");
        }
        other => panic!("esperaba error explicativo, llegó {other:?}"),
    }
}

#[test]
fn target_en_minuscula_encuentra_la_etiqueta_mayuscula() {
    // `Circle[(0,0),3]` auto-etiqueta `C`; el usuario escribe `c`. La
    // búsqueda compleja cae a case-insensitive única (sin cambiar la
    // semántica global de etiquetas).
    let mut doc = Document::new();
    process_input(&mut doc, &mut "Circle[(0, 0), 3]".to_string());
    let outcome = process_input(&mut doc, &mut "ComplexMapping[1/z, c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "`c` debe resolver a `C`: {outcome:?}"
    );
    let outcome = process_input(&mut doc, &mut "ComplexIntegral[1/z, c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "ComplexIntegral con `c`: {outcome:?}"
    );
}

#[test]
fn asignacion_con_comando_da_error_honesto_sin_objeto_basura() {
    // `c = Circle[(0, 0), 3]` no es sintaxis válida: antes creaba una curva
    // implícita basura; ahora responde con error y no toca el documento.
    let mut doc = Document::new();
    let before = doc.object_count();
    let outcome = process_input(&mut doc, &mut "c = Circle[(0, 0), 3]".to_string());
    match outcome {
        CommandOutcome::Error(message) => {
            assert!(message.contains("No se pudo interpretar"), "{message}");
            assert!(message.contains("x/y"), "guía de sintaxis: {message}");
        }
        other => panic!("esperaba error honesto, llegó {other:?}"),
    }
    assert_eq!(doc.object_count(), before, "sin objeto fantasma");
}
