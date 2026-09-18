#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente P4: cableado de ~105 comandos (Fase 2).
//! Happy path + error honesto por cada comando nuevo + categorías válidas.
//! Regla: cada comando visible en paleta + responde por `process_input`.

use grafito_command::{
    command_registry,
    commands::{process_input, CommandOutcome},
};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_owned();
    process_input(document, &mut input)
}

fn assert_msg(document: &mut Document, command: &str, needle: &str) {
    match run(document, command) {
        CommandOutcome::Message(m) => {
            assert!(m.contains(needle), "{command} → {m} (esperaba '{needle}')")
        }
        CommandOutcome::Ok => panic!("{command} dio Ok, esperaba Message con '{needle}'"),
        CommandOutcome::Error(e) => panic!("{command} dio Error: {e} (esperaba '{needle}')"),
    }
}

fn assert_ok_or_msg(document: &mut Document, command: &str) {
    match run(document, command) {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        CommandOutcome::Error(e) => panic!("{command} dio Error: {e}"),
    }
}

fn assert_err(document: &mut Document, command: &str, needle: &str) {
    match run(document, command) {
        CommandOutcome::Error(e) => assert!(
            e.contains(needle),
            "{command} → Error {e} (esperaba '{needle}')"
        ),
        other => panic!("{command} dio {other:?}, esperaba Error con '{needle}'"),
    }
}

fn palette_visible(canonical: &str) {
    let spec = command_registry::resolve(canonical)
        .unwrap_or_else(|| panic!("{canonical} debe estar registrado"));
    assert!(spec.palette_visible, "{canonical} debe ser visible");
    assert_eq!(spec.canonical, canonical);
}

fn point(document: &mut Document, xy: &str) -> String {
    assert_ok_or_msg(document, &format!("Point[{xy}]"));
    let t = xy.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = t.split(',');
    let ex: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    let ey: f64 = it.next().unwrap_or("0").trim().parse().unwrap_or(f64::NAN);
    document
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Point(p)
                if (p.position.x - ex).abs() < 1e-9 && (p.position.y - ey).abs() < 1e-9 =>
            {
                Some(p.label.clone())
            }
            _ => None,
        })
        .expect("punto fixture por coordenadas")
}

fn rich_2d(document: &mut Document) -> (String, String, String) {
    let a = point(document, "(0, 0)");
    let b = point(document, "(1, 0)");
    let c = point(document, "(0, 1)");
    (a, b, c)
}

// ── Visibilidad ──────────────────────────────────────────────────────────

#[test]
fn p4_todos_visibles_en_paleta() {
    for canonical in [
        "ImplicitDerivative",
        "Iteration",
        "Numeric",
        "ToExponential",
        "LeftSide",
        "RightSide",
        "Factors",
        "AreEqual",
        "RemovableDiscontinuity",
        "InflectionPoint",
        "Dimension",
        "Identity",
        "MatrixRank",
        "RandomBetween",
        "RandomPolynomial",
        "LeftSum",
        "LowerSum",
        "UpperSum",
        "TrapezoidalSum",
        "RectangleSum",
        "ColumnName",
        "DataFunction",
        "Frequency",
        "PointList",
        "RemoveUndefined",
        "SelectedElement",
        "SelectedIndex",
        "ParseToFunction",
        "ParseToNumber",
        "ReadText",
        "ReplaceAll",
        "RotateText",
        "Split",
        "Text",
        "VerticalText",
        "AffineRatio",
        "CrossRatio",
        "AreCongruent",
        "CircularArc",
        "CircularSector",
        "CircumcircularArc",
        "CircumcircularSector",
        "Cubic",
        "Direction",
        "PerpendicularLine",
        "RigidPolygon",
        "Conic",
        "Parameter",
        "PathParameter",
        "Type",
        "Vertex",
        "InteriorAngles",
        "Bottom",
        "Top",
        "Ends",
        "Side",
        "IntersectConic",
        "Variance",
        "HistogramRight",
        "RandomUniform",
        "RandomNormal",
        "RandomBinomial",
        "RandomPoisson",
        "Beta",
        "GammaDist",
        "BetaDist",
        "Cauchy",
        "Pareto",
        "Laplace",
        "Rayleigh",
        "NegBinomial",
        "RunClickScript",
        "RunUpdateScript",
        "SelectObjects",
        "SetActiveView",
        "SetPerspective",
        "SetViewDirection",
        "SetAxesRatio",
        "AxisStepX",
        "AxisStepY",
        "ShowAxes",
        "ShowGrid",
        "SetConditionToShowObject",
        "SetDynamicColor",
        "SetTooltipMode",
        "SetLabelMode",
        "ShowLabel",
        "SetFixed",
        "SetDecoration",
        "SetLevelOfDetail",
        "SetVisibleInView",
        "SetImage",
        "ToolImage",
        "PlaySound",
        "StartRecord",
        "SlowPlot",
        "SetLineOpacity",
        "SetPointSize",
        "ExportImage",
        "GetTime",
        "Name",
        "DynamicCoordinates",
        "Corner",
        "ConstructionStep",
        "SetConstructionStep",
    ] {
        palette_visible(canonical);
    }
    // Frente P4: 516 → 621 totales, 472 → 577 visibles.
    // Frente P4-fix: 4 comandos sin tool pasan a visibles: 577 → 581.
    // Frente P5: +18 visibles: 581 → 599, 621 → 639.
    // Frente P5 (cierre nominal GeoGebra, +18 visibles S): 621 → 639, 581 → 599.
    // Ola 0.2 (Groebner/GroebnerLex parten spec único: +2): 639 → 641, 599 → 601.
    // Ola 0.3 (StartAnimation/StopAnimation/Delete visibles: +3): 601 → 604.
    // Ola 2.3 (ZTest2/FTest visibles): 604 → 606, 641 → 643.
    assert_eq!(command_registry::all().len(), 643);
    assert_eq!(command_registry::palette_commands().count(), 606);
}

#[test]
fn p4_categorias_validas() {
    for spec in command_registry::all() {
        assert!(
            command_registry::is_valid_category(spec.category),
            "{}: categoría '{}' inválida",
            spec.canonical,
            spec.category
        );
    }
}

// ── A) CAS ───────────────────────────────────────────────────────────────

#[test]
fn p4_cas_implicit_derivative() {
    let mut d = Document::new();
    assert_msg(&mut d, "ImplicitDerivative[x^2 + y^2 - 1, x]", "dy");
    assert_msg(
        &mut d,
        "ImplicitDerivative[x^2 + y^2 - 1, x, y, 0, 1]",
        "ImplicitDerivative",
    );
    assert_err(
        &mut d,
        "ImplicitDerivative[x^2, xx, yy, 0, 0, 0]",
        "cantidad de argumentos",
    );
}

#[test]
fn p4_cas_iteration_numeric() {
    let mut d = Document::new();
    assert_msg(&mut d, "Iteration[2*x, x, 1, 3]", "Iteration");
    assert_err(&mut d, "Iteration[2*x, x, 1, 99999]", "n debe");
    assert_msg(&mut d, "Numeric[2 + 3]", "Numeric");
    assert_msg(&mut d, "Numeric[x^2, x, 3]", "Numeric");
    assert_err(&mut d, "Numeric[]", "cantidad de argumentos");
}

#[test]
fn p4_cas_to_exponential_sides() {
    let mut d = Document::new();
    assert_msg(&mut d, "ToExponential[1, 1]", "ToExponential");
    assert_err(&mut d, "ToExponential[1]", "cantidad de argumentos");
    assert_msg(&mut d, "LeftSide[x + 1 = 2]", "LeftSide");
    assert_msg(&mut d, "RightSide[x + 1 = 2]", "RightSide");
    assert_err(&mut d, "LeftSide[x + 1]", "LeftSide");
}

#[test]
fn p4_cas_factors_are_equal() {
    let mut d = Document::new();
    assert_msg(&mut d, "Factors[12]", "Factors");
    assert_err(&mut d, "Factors[0]", "Factors");
    assert_msg(&mut d, "AreEqual[x + x, 2*x]", "AreEqual");
    assert_err(&mut d, "AreEqual[x]", "cantidad de argumentos");
}

#[test]
fn p4_cas_removable_inflection() {
    let mut d = Document::new();
    assert_msg(
        &mut d,
        "RemovableDiscontinuity[(x^2 - 1)/(x - 1), 1]",
        "RemovableDiscontinuity",
    );
    assert_msg(
        &mut d,
        "RemovableDiscontinuity[(x^2 - 1)/(x - 1), x, 1]",
        "RemovableDiscontinuity",
    );
    assert_err(
        &mut d,
        "RemovableDiscontinuity[x]",
        "cantidad de argumentos",
    );
    assert_msg(&mut d, "InflectionPoint[x^3]", "InflectionPoint");
    assert_msg(&mut d, "InflectionPoint[x^3, x, -2, 2]", "InflectionPoint");
    assert_err(&mut d, "InflectionPoint[x^3, x, 2, -2]", "InflectionPoint");
}

#[test]
fn p4_cas_matrices_azar() {
    let mut d = Document::new();
    assert_msg(&mut d, "Dimension[[[1, 2], [3, 4]]]", "2×2");
    assert_err(&mut d, "Dimension[1]", "Dimension");
    assert_msg(&mut d, "Identity[2]", "Identity");
    assert_err(&mut d, "Identity[0]", "Identity");
    assert_msg(&mut d, "MatrixRank[[[1, 2], [3, 4]]]", "MatrixRank");
    assert_err(&mut d, "MatrixRank[1]", "MatrixRank");
    assert_msg(&mut d, "RandomBetween[1, 6]", "RandomBetween");
    assert_err(&mut d, "RandomBetween[6, 1]", "RandomBetween");
    assert_msg(&mut d, "RandomPolynomial[2]", "RandomPolynomial");
    assert_err(&mut d, "RandomPolynomial[99]", "grado");
}

#[test]
fn p4_cas_riemann() {
    let mut d = Document::new();
    for cmd in [
        "LeftSum[x, x, 0, 1, 4]",
        "LowerSum[x, x, 0, 1, 4]",
        "UpperSum[x, x, 0, 1, 4]",
        "TrapezoidalSum[x, x, 0, 1, 4]",
        "RectangleSum[x, x, 0, 1, 4]",
    ] {
        let name = cmd.split('[').next().unwrap_or(cmd);
        assert_msg(&mut d, cmd, name);
    }
    assert_msg(&mut d, "LowerSum[x, x, 0, 1, 4]", "8 muestras");
    assert_err(&mut d, "LeftSum[x, x, 0, 1]", "cantidad de argumentos");
    assert_err(&mut d, "LeftSum[x, x, 1, 0, 4]", "LeftSum");
}

// ── B) Listas/Texto ──────────────────────────────────────────────────────

#[test]
fn p4_listas_basicas() {
    let mut d = Document::new();
    assert_msg(&mut d, "ColumnName[1]", "A");
    assert_msg(&mut d, "ColumnName[27]", "AA");
    assert_err(&mut d, "ColumnName[0]", "ColumnName");
    assert_msg(&mut d, "DataFunction[x*2, {1, 2, 3}]", "DataFunction");
    assert_err(&mut d, "DataFunction[]", "cantidad de argumentos");
    assert_msg(&mut d, "Frequency[{1, 2, 2, 3}]", "Frequency");
    assert_err(&mut d, "Frequency[]", "cantidad de argumentos");
    assert_msg(&mut d, "RemoveUndefined[{1, 2, 3}]", "RemoveUndefined");
}

#[test]
fn p4_pointlist_crea_puntos() {
    let mut d = Document::new();
    let before = d.object_count();
    assert_msg(&mut d, "PointList[[[0, 0], [1, 1], [2, 0]]]", "3 punto");
    assert_eq!(d.object_count(), before + 3);
    assert_err(&mut d, "PointList[[[0, 0], [1, 2, 3]]]", "PointList");
}

#[test]
fn p4_seleccion() {
    let mut d = Document::new();
    let a = point(&mut d, "(0, 0)");
    let b = point(&mut d, "(5, 5)");
    assert_ok_or_msg(&mut d, &format!("SelectObjects[{{{a}}}]"));
    assert_msg(
        &mut d,
        &format!("SelectedIndex[{{{a}, {b}}}]"),
        "SelectedIndex",
    );
    assert_msg(
        &mut d,
        &format!("SelectedElement[{{{a}, {b}}}]"),
        "SelectedElement",
    );
    d.clear_selection();
    assert_err(
        &mut d,
        &format!("SelectedIndex[{{{a}, {b}}}]"),
        "SelectedIndex",
    );
    assert_err(&mut d, "SelectedIndex[]", "cantidad de argumentos");
}

#[test]
fn p4_texto_parse() {
    let mut d = Document::new();
    assert_msg(&mut d, "ParseToFunction[x^2 + 1, x]", "ParseToFunction");
    assert_err(&mut d, "ParseToFunction[+, x]", "ParseToFunction");
    assert_msg(&mut d, "ParseToNumber[3.5]", "ParseToNumber");
    assert_err(&mut d, "ParseToNumber[hola]", "ParseToNumber");
    assert_msg(&mut d, "ReplaceAll[hola mundo, o, 0]", "ReplaceAll");
    assert_err(&mut d, "ReplaceAll[a, b]", "cantidad de argumentos");
    assert_msg(&mut d, "Split[a-b-c, -]", "Split");
    assert_err(&mut d, "Split[a]", "cantidad de argumentos");
}

#[test]
fn p4_texto_crea_y_lee() {
    let mut d = Document::new();
    assert_msg(&mut d, "Text[hola]", "creado");
    let label: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Text(t) if t.content == "hola" => Some(t.label.clone()),
            _ => None,
        })
        .expect("texto creado");
    assert_msg(&mut d, &format!("ReadText[{label}]"), "hola");
    assert_err(&mut d, "ReadText[inexistente_xyz]", "ReadText");
    assert_msg(&mut d, "VerticalText[ab]", "creado");
    assert_msg(&mut d, "RotateText[giro, 90]", "creado");
    // Rotación en radianes (90° = π/2) en el objeto.
    let rot = d
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Text(t) if t.content == "giro" => Some(t.rotation),
            _ => None,
        })
        .last()
        .expect("texto rotado");
    assert!(
        (rot - std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "rotación {rot} debe ser π/2"
    );
    assert_err(&mut d, "RotateText[giro]", "cantidad de argumentos");
}

// ── C) Geometría ─────────────────────────────────────────────────────────

#[test]
fn p4_geo_razones() {
    let mut d = Document::new();
    // Trío colineal en el eje X para la razón afín.
    let a = point(&mut d, "(0, 0)");
    let b = point(&mut d, "(2, 0)");
    let c = point(&mut d, "(5, 0)");
    assert_msg(
        &mut d,
        &format!("AffineRatio[{a}, {b}, {c}]"),
        "AffineRatio",
    );
    assert_err(
        &mut d,
        &format!("AffineRatio[{a}, {b}]"),
        "cantidad de argumentos",
    );
    let e = point(&mut d, "(7, 0)");
    assert_msg(
        &mut d,
        &format!("CrossRatio[{a}, {b}, {c}, {e}]"),
        "CrossRatio",
    );
    assert_err(&mut d, "CrossRatio[A]", "cantidad de argumentos");
}

#[test]
fn p4_geo_congruencia() {
    let mut d = Document::new();
    assert_ok_or_msg(&mut d, "Segment[(0, 0), (1, 0)]");
    assert_ok_or_msg(&mut d, "Segment[(0, 0), (1, 0)]");
    let segs: Vec<String> = d
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Line(_) => Some(o.label().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(segs.len(), 2);
    assert_msg(
        &mut d,
        &format!("AreCongruent[{}, {}]", segs[0], segs[1]),
        "AreCongruent",
    );
    assert_ok_or_msg(&mut d, "Polygon[(0, 0), (1, 0), (0, 1)]");
    assert_err(&mut d, "AreCongruent[solo_uno]", "cantidad de argumentos");
    assert_err(
        &mut d,
        &format!("AreCongruent[{}, {}]", segs[0], "inexistente"),
        "AreCongruent",
    );
}

#[test]
fn p4_geo_arcos() {
    let mut d = Document::new();
    let (a, b, c) = rich_2d(&mut d);
    let n0 = d.object_count();
    assert_msg(
        &mut d,
        &format!("CircularArc[{a}, 1, 0, 1.57]"),
        "CircularArc",
    );
    assert_eq!(d.object_count(), n0 + 1);
    assert_msg(
        &mut d,
        &format!("CircularSector[{a}, 1, 0, 1.57]"),
        "CircularSector",
    );
    assert_msg(
        &mut d,
        &format!("CircumcircularArc[{a}, {b}, {c}]"),
        "CircumcircularArc",
    );
    assert_msg(
        &mut d,
        &format!("CircumcircularSector[{a}, {b}, {c}]"),
        "CircumcircularSector",
    );
    assert_err(
        &mut d,
        &format!("CircularArc[{a}, 1, 0]"),
        "cantidad de argumentos",
    );
    assert_err(
        &mut d,
        &format!("CircularArc[{a}, -1, 0, 1]"),
        "CircularArc",
    );
}

#[test]
fn p4_geo_cubica_conica() {
    let mut d = Document::new();
    // 9 puntos sobre (y − x²)·(x − 5) = 0 (set conocido del motor, no singular).
    for (x, y) in [
        (0.0, 0.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (2.0, 4.0),
        (-2.0, 4.0),
        (5.0, 0.0),
        (5.0, 25.0),
        (5.0, -3.0),
        (3.0, 9.0),
    ] {
        assert_ok_or_msg(&mut d, &format!("Point[({x}, {y})]"));
    }
    let pts: Vec<String> = d
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Point(p) => Some(p.label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pts.len(), 9);
    assert_msg(
        &mut d,
        &format!(
            "Cubic[{}, {}, {}, {}, {}, {}, {}, {}, {}]",
            pts[0], pts[1], pts[2], pts[3], pts[4], pts[5], pts[6], pts[7], pts[8]
        ),
        "Cubic",
    );
    assert_err(&mut d, "Cubic[A]", "cantidad de argumentos");
    // Cónica por 5 puntos.
    assert_msg(
        &mut d,
        &format!(
            "Conic[{}, {}, {}, {}, {}]",
            pts[0], pts[1], pts[2], pts[3], pts[4]
        ),
        "Conic",
    );
    assert_err(&mut d, "Conic[A, B]", "cantidad de argumentos");
}

#[test]
fn p4_geo_direccion_perpendicular() {
    let mut d = Document::new();
    let (a, b, _) = rich_2d(&mut d);
    assert_msg(&mut d, &format!("Direction[{a}, {b}]"), "Direction");
    assert_ok_or_msg(&mut d, &format!("Line[{a}, {b}]"));
    let linea: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Line(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("recta");
    assert_msg(&mut d, &format!("Direction[{linea}]"), "Direction");
    assert_msg(
        &mut d,
        &format!("PerpendicularLine[{a}, {linea}]"),
        "PerpendicularLine",
    );
    assert_msg(
        &mut d,
        &format!("PerpendicularLine[{a}, {a}, {b}]"),
        "PerpendicularLine",
    );
    assert_err(&mut d, "Direction[]", "cantidad de argumentos");
    assert_err(&mut d, "PerpendicularLine[A]", "cantidad de argumentos");
}

#[test]
fn p4_geo_rigida_tipo_vertice() {
    let mut d = Document::new();
    let (a, b, c) = rich_2d(&mut d);
    assert_msg(
        &mut d,
        &format!("RigidPolygon[{a}, {b}, {c}]"),
        "RigidPolygon",
    );
    assert_err(
        &mut d,
        &format!("RigidPolygon[{a}, {b}]"),
        "cantidad de argumentos",
    );
    assert_msg(&mut d, &format!("Type[{a}]"), "Point");
    assert_err(&mut d, "Type[inexistente]", "Type");
    assert_ok_or_msg(&mut d, "Polygon[(0, 0), (2, 0), (0, 2)]");
    let pol: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Polygon(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("polígono");
    let n0 = d.object_count();
    assert_msg(&mut d, &format!("Vertex[{pol}]"), "Vertex");
    assert!(d.object_count() > n0);
    assert_msg(&mut d, &format!("InteriorAngles[{pol}]"), "InteriorAngles");
    assert_msg(
        &mut d,
        &format!("PathParameter[{a}, {pol}]"),
        "PathParameter",
    );
    assert_err(&mut d, "Vertex[inexistente]", "Vertex");
}

#[test]
fn p4_geo_parametro_conicas() {
    let mut d = Document::new();
    assert_ok_or_msg(&mut d, "Parabola[(0, 0), 1]");
    assert_ok_or_msg(&mut d, "Ellipse[(0, 0), 3, 2]");
    assert_ok_or_msg(&mut d, "Hyperbola[(0, 0), 3, 2]");
    let parab: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Parabola(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("parábola");
    let elip: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Ellipse(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("elipse");
    let hiper: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Hyperbola(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("hipérbola");
    assert_msg(&mut d, &format!("Parameter[{parab}]"), "Parameter");
    assert_msg(&mut d, &format!("Parameter[{elip}]"), "Parameter");
    assert_msg(&mut d, &format!("Parameter[{hiper}]"), "Parameter");
    assert_err(&mut d, "Parameter[inexistente]", "Parameter");
}

#[test]
fn p4_geo_solidos() {
    let mut d = Document::new();
    assert_ok_or_msg(&mut d, "Cube[0, 0, 0, 2]");
    let cubo: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Cube3D(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("cubo");
    assert_msg(&mut d, &format!("Bottom[{cubo}]"), "Bottom");
    assert_msg(&mut d, &format!("Top[{cubo}]"), "Top");
    assert_ok_or_msg(&mut d, "Sphere[0, 0, 0, 1]");
    let esfera: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Sphere3D(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("esfera");
    assert_ok_or_msg(&mut d, "Plane3D[0, 0, 1, -0.5]");
    let plano: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Plane3D(p)
                if p.a.abs() < 1e-12
                    && p.b.abs() < 1e-12
                    && (p.c - 1.0).abs() < 1e-12
                    && (p.d + 0.5).abs() < 1e-12 =>
            {
                Some(p.label.clone())
            }
            _ => None,
        })
        .expect("plano z=0.5");
    assert_msg(
        &mut d,
        &format!("IntersectConic[{esfera}, {plano}]"),
        "IntersectConic",
    );
    assert_ok_or_msg(&mut d, "Cylinder[0, 0, 0, 1, 2]");
    let cil: String = d
        .objects_iter()
        .find_map(|(_, o)| match o {
            GeoObject::Cylinder3D(_) => Some(o.label().to_string()),
            _ => None,
        })
        .expect("cilindro");
    assert_msg(&mut d, &format!("Ends[{cil}]"), "Ends");
    assert_msg(&mut d, &format!("Side[{cil}]"), "Side");
    assert_err(&mut d, "Bottom[inexistente]", "Bottom");
    assert_err(&mut d, &format!("Ends[{cubo}]"), "Ends");
    assert_err(
        &mut d,
        &format!("IntersectConic[{cubo}, {plano}]"),
        "IntersectConic",
    );
}

// ── D) Stats/Prob ────────────────────────────────────────────────────────

#[test]
fn p4_stats_nuevos() {
    let mut d = Document::new();
    assert_msg(&mut d, "Variance[{1, 2, 3, 4}]", "Variance");
    assert_err(&mut d, "Variance[]", "cantidad de argumentos");
    assert_err(&mut d, "Variance[{}]", "Variance");
    assert_msg(&mut d, "HistogramRight[{1, 2, 2, 3}]", "HistogramRight");
    assert_msg(&mut d, "HistogramRight[{1, 2, 2, 3}, 2]", "HistogramRight");
    assert_err(&mut d, "HistogramRight[{1}, 0]", "bins");
    assert_msg(&mut d, "RandomUniform[0, 1]", "RandomUniform");
    assert_err(&mut d, "RandomUniform[1, 0]", "RandomUniform");
    assert_msg(&mut d, "RandomNormal[0, 1]", "RandomNormal");
    assert_err(&mut d, "RandomNormal[0, -1]", "RandomNormal");
    assert_msg(&mut d, "RandomBinomial[10, 0.5]", "RandomBinomial");
    assert_err(&mut d, "RandomBinomial[10, 2]", "RandomBinomial");
    assert_msg(&mut d, "RandomPoisson[3]", "RandomPoisson");
    assert_err(&mut d, "RandomPoisson[-1]", "RandomPoisson");
}

#[test]
fn p4_distribuciones_huerfanas_visibles() {
    let mut d = Document::new();
    assert_msg(&mut d, "Beta[2, 3]", "B(");
    assert_err(&mut d, "Beta[1]", "cantidad de argumentos");
    assert_msg(&mut d, "GammaDist[2, 3]", "Gamma(");
    assert_msg(&mut d, "BetaDist[2, 3]", "Beta(");
    assert_msg(&mut d, "Cauchy[0, 1]", "Cauchy");
    assert_msg(&mut d, "Pareto[1, 2]", "Pareto");
    assert_msg(&mut d, "Laplace[0, 1]", "Laplace");
    assert_msg(&mut d, "Rayleigh[1]", "Rayleigh");
    assert_msg(&mut d, "NegBinomial[5, 0.5]", "NegBin(");
    assert_err(&mut d, "Cauchy[0, -1]", "Cauchy");
}

// ── E) Scripting/Display ─────────────────────────────────────────────────

#[test]
fn p4_scripting_click_update_select() {
    let mut d = Document::new();
    let a = point(&mut d, "(0, 0)");
    assert_ok_or_msg(&mut d, &format!("OnClick[{a}, SetValue[x, 1]]"));
    assert_msg(&mut d, &format!("RunClickScript[{a}]"), "RunClickScript");
    assert_err(&mut d, "RunClickScript[inexistente]", "RunClickScript");
    assert_ok_or_msg(&mut d, &format!("OnUpdate[{a}, SetValue[x, 2]]"));
    assert_msg(&mut d, &format!("RunUpdateScript[{a}]"), "RunUpdateScript");
    assert_err(&mut d, "RunUpdateScript[inexistente]", "RunUpdateScript");
    assert_msg(&mut d, &format!("SelectObjects[{{{a}}}]"), "SelectObjects");
    assert_err(&mut d, "SelectObjects[]", "cantidad de argumentos");
    assert_err(&mut d, "SelectObjects[{inexistente_xyz}]", "no existen");
}

#[test]
fn p4_vistas_direcciones() {
    let mut d = Document::new();
    assert_msg(&mut d, "SetActiveView[G2]", "validada");
    assert_msg(&mut d, "SetPerspective[G3]", "validada");
    assert_err(&mut d, "SetPerspective[inexistente]", "SetPerspective");
    assert_msg(&mut d, "SetViewDirection[front]", "validada");
    assert_err(&mut d, "SetViewDirection[inexistente]", "SetViewDirection");
    assert_msg(&mut d, "SetAxesRatio[1, 1]", "SetAxesRatio");
    assert!(d.variables.contains_key("__view_axes_rx"));
    assert_err(&mut d, "SetAxesRatio[-1, 1]", "SetAxesRatio");
    assert_msg(&mut d, "AxisStepX[0.5]", "AxisStepX");
    assert_msg(&mut d, "AxisStepY[0.5]", "AxisStepY");
    assert_err(&mut d, "AxisStepX[-1]", "AxisStepX");
    assert_msg(&mut d, "ShowAxes[true]", "ShowAxes");
    assert_msg(&mut d, "ShowGrid[false]", "ShowGrid");
    assert_err(&mut d, "ShowAxes[quizás]", "ShowAxes");
}

#[test]
fn p4_display_flags() {
    let mut d = Document::new();
    let a = point(&mut d, "(0, 0)");
    assert_msg(
        &mut d,
        &format!("SetConditionToShowObject[{a}, x > 0]"),
        "SetConditionToShowObject",
    );
    assert!(d
        .display_flags
        .get(&a)
        .and_then(|f| f.condition.clone())
        .is_some());
    assert_err(
        &mut d,
        &format!("SetConditionToShowObject[{a}, x >]"),
        "SetConditionToShowObject",
    );
    assert_msg(
        &mut d,
        &format!("SetDynamicColor[{a}, 1, 0, 0]"),
        "SetDynamicColor",
    );
    assert!(d
        .display_flags
        .get(&a)
        .and_then(|f| f.dynamic_color.clone())
        .is_some());
    assert_msg(&mut d, &format!("SetTooltipMode[{a}, 2]"), "SetTooltipMode");
    assert_msg(&mut d, &format!("SetLabelMode[{a}, false]"), "SetLabelMode");
    assert_msg(&mut d, &format!("ShowLabel[{a}, false]"), "ShowLabel");
    assert!(d.display_flags.get(&a).is_some_and(|f| !f.show_label));
    assert_msg(&mut d, &format!("SetFixed[{a}, true]"), "SetFixed");
    assert!(d.display_flags.get(&a).is_some_and(|f| f.locked));
    assert_msg(&mut d, &format!("SetFixed[{a}, false]"), "SetFixed");
    assert_msg(&mut d, &format!("SetDecoration[{a}, 2]"), "SetDecoration");
    assert_msg(
        &mut d,
        &format!("SetLevelOfDetail[{a}, 1]"),
        "SetLevelOfDetail",
    );
    assert_err(
        &mut d,
        &format!("SetLevelOfDetail[{a}, 9]"),
        "SetLevelOfDetail",
    );
    assert_err(&mut d, "SetFixed[inexistente, true]", "SetFixed");
}

#[test]
fn p4_errores_honestos_permanentes() {
    let mut d = Document::new();
    let a = point(&mut d, "(0, 0)");
    assert_err(
        &mut d,
        &format!("SetVisibleInView[{a}, vista1]"),
        "una sola vista",
    );
    assert_err(&mut d, &format!("SetImage[{a}, foto.png]"), "sin pipeline");
    assert_err(&mut d, &format!("ToolImage[{a}]"), "no disponible");
    assert_err(&mut d, "PlaySound[cancion.mp3]", "sin pipeline");
    assert_err(&mut d, "StartRecord[]", "sin grabación");
    assert_err(&mut d, &format!("SlowPlot[{a}]"), "sin camino limpio");
    assert_err(
        &mut d,
        "ExportImage[salida.png]",
        "la escritura la hace la UI",
    );
    assert_err(&mut d, "ExportImage[../fuera.png]", "ExportImage");
    assert_err(&mut d, "ConstructionStep[1]", "requiere cableado UI");
    assert_err(&mut d, "SetConstructionStep[1]", "requiere cableado UI");
    assert_err(&mut d, "ConstructionStep[0]", "1..=500");
}

#[test]
fn p4_estilos_lecturas() {
    let mut d = Document::new();
    let a = point(&mut d, "(0, 0)");
    assert_msg(
        &mut d,
        &format!("SetLineOpacity[{a}, 0.5]"),
        "SetLineOpacity",
    );
    assert_err(&mut d, &format!("SetLineOpacity[{a}, 5]"), "SetLineOpacity");
    assert_msg(&mut d, &format!("SetPointSize[{a}, 4]"), "SetPointSize");
    assert_err(&mut d, &format!("SetPointSize[{a}, 999]"), "SetPointSize");
    assert_msg(&mut d, "GetTime[]", "GetTime");
    assert_err(&mut d, "GetTime[x]", "GetTime");
    assert_msg(&mut d, &format!("Name[{a}]"), "Name");
    assert_err(&mut d, "Name[inexistente]", "Name");
    assert_msg(
        &mut d,
        &format!("DynamicCoordinates[{a}]"),
        "DynamicCoordinates",
    );
    assert_msg(&mut d, "Corner[1]", "Corner");
    assert_err(&mut d, "Corner[9]", "Corner");
}
