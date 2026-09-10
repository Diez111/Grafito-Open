#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Oleada 1 P2: solo S (aliases/wrappers de motor existente).
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

fn make_datatable_label(document: &mut Document) -> String {
    assert_ok_or_message(document, "DataTable[{1, 2, 3}, {2, 4, 6}]");
    first_label_matching(document, |o| matches!(o, GeoObject::DataTable(_)))
}

fn make_point_label(document: &mut Document) -> String {
    assert_ok_or_message(document, "Point[(0, 0)]");
    let labels: Vec<String> = document
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.label.clone()),
            _ => None,
        })
        .collect();
    labels.into_iter().next_back().expect("punto fixture")
}

fn make_cube_label(document: &mut Document) -> String {
    assert_ok_or_message(document, "Cube[0, 0, 0, 2]");
    first_label_matching(document, |o| matches!(o, GeoObject::Cube3D(_)))
}

fn make_circle_label(document: &mut Document) -> String {
    assert_ok_or_message(document, "Circle[(0, 0), 2]");
    first_label_matching(document, |o| matches!(o, GeoObject::Circle(_)))
}

fn find_label(document: &Document, label: &str) -> grafito_core::ObjectId {
    document
        .try_find_object_by_label(label)
        .expect("buscar etiqueta")
        .expect("etiqueta existe")
}

#[test]
fn oleada1_todos_visibles_en_paleta() {
    for canonical in [
        "FitLine",
        "Fit",
        "FitLineX",
        "IntegralBetween",
        "IntegralSymbolic",
        "TaylorPolynomial",
        "Periods",
        "Transpose",
        "Invert",
        "NInvert",
        "SolveCubic",
        "SolveQuartic",
        "Curvature",
        "NIntegral",
        "Dot",
        "Cross",
        "UnitVector",
        "ApplyMatrix",
        "NDerivative",
        "IsDefined",
        "IsInteger",
        "IsPrime",
        "IsInRegion",
        "FormulaText",
        "ScientificText",
        "MixedNumber",
        "Ordinal",
        "SetColor",
        "SetCoords",
        "SetVisible",
        "Surface",
        "Volume",
        "OsculatingCircle",
        "Cell",
        "Column",
        "Row",
    ] {
        palette_must_be_visible(canonical);
    }
    // Total actualizado por Q2 (+3 visibles S): 284 → 287.
    // Frente trig+racionalización (+4 visibles S): 287 → 291.
    // Frente R3.1 (Rename stub→visible): 291 → 292.
    // Frente 3D-A2 (Vista3D visible S): 292 → 293.
    assert_eq!(command_registry::palette_commands().count(), 293);
}

#[test]
fn oleada1_aliases_puros_delegan_al_motor() {
    for canonical in ["FitLine", "Fit", "FitLineX"] {
        let mut document = Document::new();
        let tabla = make_datatable_label(&mut document);
        assert_message_contains(&mut document, &format!("{canonical}[{tabla}]"), "RMSE");
        assert_eq!(command_registry::canonicalize(canonical), Some("FitLinear"));
    }
    for (canonical, cmd) in [
        ("IntegralBetween", "IntegralBetween[x^2, 0, 1]"),
        ("NIntegral", "NIntegral[x^2, 0, 1]"),
    ] {
        let mut document = Document::new();
        assert_message_contains(&mut document, cmd, "Graficado");
        assert_eq!(command_registry::canonicalize(canonical), Some("Integral"));
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "IntegralSymbolic[x^2]", "∫ x^2 dx");
        assert_eq!(
            command_registry::canonicalize("IntegralSymbolic"),
            Some("RischInt")
        );
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "TaylorPolynomial[x^2, x, 0, 2]", "Taylor");
        assert_eq!(
            command_registry::canonicalize("TaylorPolynomial"),
            Some("Taylor")
        );
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "Periods[0.05, -100, 1000, 0]", "Nper");
        assert_eq!(command_registry::canonicalize("Periods"), Some("Nper"));
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "Transpose[[[1, 2], [3, 4]]]", "Transpose");
    }
    for canonical in ["Invert", "NInvert"] {
        let mut document = Document::new();
        assert_message_contains(
            &mut document,
            &format!("{canonical}[[[1, 2], [3, 4]]]"),
            "Inverse",
        );
        assert_eq!(command_registry::canonicalize(canonical), Some("Inverse"));
    }
    {
        let mut document = Document::new();
        assert_message_contains(
            &mut document,
            "SolveCubic[x^3-6*x^2+11*x-6, x]",
            "{1, 2, 3}",
        );
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "SolveQuartic[x^4-5*x^2+4, x]", "1");
    }
    {
        let mut document = Document::new();
        assert_message_contains(&mut document, "Curvature[x^2, 0]", "κ");
        assert_eq!(
            command_registry::canonicalize("Curvature"),
            Some("CurvatureAt")
        );
    }
}

#[test]
fn oleada1_vectores_y_matrices() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Dot[[1, 2, 3], [4, 5, 6]]", "Dot = 32");
    assert_message_contains(
        &mut document,
        "Cross[[1, 0, 0], [0, 1, 0]]",
        "Cross = [0, 0, 1]",
    );
    assert_message_contains(&mut document, "UnitVector[[3, 4]]", "UnitVector = [0.6");
    assert_message_contains(
        &mut document,
        "ApplyMatrix[[[1, 0], [0, 1]], [5, 7]]",
        "ApplyMatrix",
    );
}

#[test]
fn oleada1_numericos_y_predicados() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "NDerivative[x^2, x, 3]", "NDerivative = 6");
    assert_message_contains(&mut document, "IsDefined[zz_nuevo_oleada1]", "false");
    assert_ok_or_message(&mut document, "SetValue[zz_nuevo_oleada1, 5]");
    assert_message_contains(&mut document, "IsDefined[zz_nuevo_oleada1]", "true");
    assert_message_contains(&mut document, "IsInteger[3]", "true");
    assert_message_contains(&mut document, "IsInteger[3.5]", "false");
    assert_message_contains(&mut document, "IsPrime[7]", "true");
    assert_message_contains(&mut document, "IsPrime[8]", "false");
    let mut document = Document::new();
    let circulo = make_circle_label(&mut document);
    assert_message_contains(
        &mut document,
        &format!("IsInRegion[(0, 0), {circulo}]"),
        "true",
    );
    assert_message_contains(
        &mut document,
        &format!("IsInRegion[(10, 10), {circulo}]"),
        "false",
    );
}

#[test]
fn oleada1_textos_crean_objeto() {
    let mut document = Document::new();
    let before = document.object_count();
    assert_ok_or_message(&mut document, "FormulaText[x^2+1]");
    assert_ok_or_message(&mut document, "ScientificText[12345]");
    assert_ok_or_message(&mut document, "MixedNumber[2.5]");
    assert_ok_or_message(&mut document, "Ordinal[1]");
    assert_eq!(document.object_count(), before + 4);
    let texts: Vec<String> = document
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("1/2")),
        "MixedNumber debe contener 1/2, fue {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "1.º"),
        "Ordinal debe ser 1.º, fue {texts:?}"
    );
}

#[test]
fn oleada1_setters_reflejan_canvas_al_instante() {
    let mut document = Document::new();
    let punto = make_point_label(&mut document);
    assert_message_contains(
        &mut document,
        &format!("SetColor[{punto}, red]"),
        "actualizado",
    );
    assert_message_contains(
        &mut document,
        &format!("SetCoords[{punto}, (5, 6)]"),
        "movido",
    );
    let pos = match document
        .get_object(find_label(&document, &punto))
        .expect("punto")
    {
        GeoObject::Point(p) => p.position,
        other => panic!("esperaba punto, fue {other:?}"),
    };
    assert!((pos.x - 5.0).abs() < 1e-9 && (pos.y - 6.0).abs() < 1e-9);
    assert_message_contains(
        &mut document,
        &format!("SetVisible[{punto}, false]"),
        "oculto",
    );
    assert!(!document
        .get_object(find_label(&document, &punto))
        .expect("punto")
        .is_visible());
    assert_message_contains(
        &mut document,
        &format!("SetVisible[{punto}, true]"),
        "visible",
    );
    assert!(document
        .get_object(find_label(&document, &punto))
        .expect("punto")
        .is_visible());
}

#[test]
fn oleada1_solidos_miden() {
    let mut document = Document::new();
    let cubo = make_cube_label(&mut document);
    assert_message_contains(&mut document, &format!("Surface[{cubo}]"), "Surface");
    assert_message_contains(&mut document, &format!("Volume[{cubo}]"), "Volume");
}

#[test]
fn oleada1_osculating_circle_crea_circulo() {
    let mut document = Document::new();
    let before = document.object_count();
    assert_message_contains(&mut document, "OsculatingCircle[x^2, 1]", "κ");
    assert_eq!(document.object_count(), before + 1);
    assert!(
        document
            .objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::Circle(_))),
        "OsculatingCircle debe crear un círculo"
    );
}

#[test]
fn oleada1_planilla_lee() {
    let mut document = Document::new();
    assert_ok_or_message(&mut document, "FillColumn[A, 5]");
    assert_message_contains(&mut document, "Cell[A1]", "Cell = 5");
    assert_message_contains(&mut document, "Column[A]", "Column = {");
    assert_message_contains(&mut document, "Row[1]", "Row = {");
}
