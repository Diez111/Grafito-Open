use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{
    Document, GeoObject, ImplicitCurveObj, ParametricCurve2DObj, PolarCurveObj, RelationOperator,
    VectorField2DObj,
};

fn count_points(doc: &Document) -> usize {
    doc.objects_iter()
        .filter(|(_, obj)| matches!(obj, GeoObject::Point(_)))
        .count()
}

#[test]
fn root_finds_two_points_on_parabola() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "f(x) = x^2 - 4".to_string());
    let outcome = process_input(&mut doc, &mut "Root[f]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Root should return a message, got {:?}",
        outcome
    );
    assert_eq!(count_points(&doc), 2, "expected two roots");
}

#[test]
fn extremum_finds_minimum_of_parabola() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "g(x) = x^2 - 2*x + 1".to_string());
    let outcome = process_input(&mut doc, &mut "Extremum[g]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Extremum should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points
            .iter()
            .any(|p| (p.x - 1.0).abs() < 1e-3 && (p.y - 0.0).abs() < 1e-3),
        "expected minimum near (1, 0), got {:?}",
        points
    );
}

#[test]
fn inflection_finds_cubic_inflection_point() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "h(x) = x^3".to_string());
    let outcome = process_input(&mut doc, &mut "Inflection[h]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Inflection should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points
            .iter()
            .any(|p| (p.x - 0.0).abs() < 1e-3 && (p.y - 0.0).abs() < 1e-3),
        "expected inflection at (0, 0), got {:?}",
        points
    );
}

#[test]
fn y_intercept_finds_line_crossing() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "r(x) = 2*x + 3".to_string());
    let outcome = process_input(&mut doc, &mut "YIntercept[r]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "YIntercept should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points
            .iter()
            .any(|p| (p.x - 0.0).abs() < 1e-3 && (p.y - 3.0).abs() < 1e-3),
        "expected Y intercept at (0, 3), got {:?}",
        points
    );
}

#[test]
fn analyze_combines_multiple_features() {
    let mut doc = Document::new();
    process_input(&mut doc, &mut "p(x) = x^2 - 4".to_string());
    let outcome = process_input(&mut doc, &mut "Analyze[p]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Analyze should return a message, got {:?}",
        outcome
    );
    assert!(
        count_points(&doc) >= 2,
        "expected at least roots from Analyze"
    );
}

#[test]
fn analysis_on_missing_label_returns_error() {
    let mut doc = Document::new();
    let outcome = process_input(&mut doc, &mut "Root[nonexistent]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Error(_)),
        "Root on missing label should error, got {:?}",
        outcome
    );
}

#[test]
fn root_on_parametric_curve_finds_axis_crossings() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::ParametricCurve2D(
        ParametricCurve2DObj::new("t", "t^2 - 4", -3.0, 3.0).with_label("c"),
    ));
    let outcome = process_input(&mut doc, &mut "Root[c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Root on parametric should return a message, got {:?}",
        outcome
    );
    let xs: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position.x),
            _ => None,
        })
        .collect();
    assert!(xs.iter().any(|x| (x + 2.0).abs() < 1e-2));
    assert!(xs.iter().any(|x| (x - 2.0).abs() < 1e-2));
}

#[test]
fn extremum_on_parametric_curve_finds_minimum() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::ParametricCurve2D(
        ParametricCurve2DObj::new("t", "t^2 - 4", -3.0, 3.0).with_label("c"),
    ));
    let outcome = process_input(&mut doc, &mut "Extremum[c]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Extremum on parametric should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points
            .iter()
            .any(|p| (p.x - 0.0).abs() < 1e-2 && (p.y + 4.0).abs() < 1e-2),
        "expected minimum near (0, -4), got {:?}",
        points
    );
}

#[test]
fn root_on_polar_curve_finds_origin_crossing() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::PolarCurve(
        PolarCurveObj::new("1 - cos(t)", 0.0, std::f64::consts::TAU).with_label("p"),
    ));
    let outcome = process_input(&mut doc, &mut "Root[p]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Root on polar should return a message, got {:?}",
        outcome
    );
}

#[test]
fn analyze_implicit_circle_finds_axis_intersections() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::ImplicitCurve(
        ImplicitCurveObj::new("x^2 + y^2", "4", RelationOperator::Eq).with_label("circ"),
    ));
    let outcome = process_input(&mut doc, &mut "Analyze[circ]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Analyze on implicit should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points
            .iter()
            .any(|p| (p.x - 2.0).abs() < 1e-2 && p.y.abs() < 1e-2),
        "expected root near (2, 0), got {:?}",
        points
    );
    assert!(
        points
            .iter()
            .any(|p| p.x.abs() < 1e-2 && (p.y - 2.0).abs() < 1e-2),
        "expected Y intercept near (0, 2), got {:?}",
        points
    );
}

#[test]
fn root_on_vector_field_finds_equilibrium() {
    let mut doc = Document::new();
    doc.add_object(GeoObject::VectorField2D(
        VectorField2DObj::new("x", "y").with_label("vf"),
    ));
    let outcome = process_input(&mut doc, &mut "Root[vf]".to_string());
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "Root on vector field should return a message, got {:?}",
        outcome
    );
    let points: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.position),
            _ => None,
        })
        .collect();
    assert!(
        points.iter().any(|p| p.x.abs() < 1e-2 && p.y.abs() < 1e-2),
        "expected equilibrium near (0, 0), got {:?}",
        points
    );
}

// ============================================================================
// Tests de las herramientas de cálculo (TangentAt, NormalAt, ArcLength,
// CurvatureAt, VolumeOfRevolution, SurfaceOfRevolution).
// ============================================================================

fn extract_message(out: &CommandOutcome) -> Option<&str> {
    match out {
        CommandOutcome::Message(s) => Some(s.as_str()),
        CommandOutcome::Error(s) => Some(s.as_str()),
        CommandOutcome::Ok => None,
    }
}

#[test]
fn tangent_at_creates_line_and_reports_slope() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "TangentAt[x^2, 1]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    assert!(msg.contains("Tangente"), "got {msg}");
    // Debe crear un objeto Line (la tangente).
    assert!(
        doc.objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::Line(_))),
        "esperaba una línea tangente"
    );
}

#[test]
fn normal_at_creates_perpendicular_line() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "NormalAt[x^2, 1]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    assert!(msg.contains("Normal"), "got {msg}");
    assert!(
        doc.objects_iter()
            .any(|(_, o)| matches!(o, GeoObject::Line(_))),
        "esperaba una línea normal"
    );
}

#[test]
fn arc_length_command_returns_sqrt2_for_line() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "ArcLength[x, 0, 1]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    let expected = 2.0_f64.sqrt();
    assert!(msg.contains(&format!("{expected:.6}")), "got {msg}");
}

#[test]
fn curvature_at_command_reports_kappa_for_parabola() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "CurvatureAt[x^2, 0]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    assert!(msg.contains("Curvatura"), "got {msg}");
    // κ(0) = 2 para y = x².
    assert!(
        msg.contains("2.000000"),
        "esperaba κ=2 en el mensaje: {msg}"
    );
}

#[test]
fn volume_of_revolution_command_returns_pi_over_3() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "VolumeOfRevolution[x, 0, 1]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    let expected = std::f64::consts::PI / 3.0;
    assert!(msg.contains(&format!("{expected:.6}")), "got {msg}");
}

#[test]
fn surface_of_revolution_command_returns_pi_sqrt2() {
    let mut doc = Document::new();
    let out = process_input(&mut doc, &mut "SurfaceOfRevolution[x, 0, 1]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    let expected = std::f64::consts::PI * 2.0_f64.sqrt();
    assert!(msg.contains(&format!("{expected:.6}")), "got {msg}");
}

#[test]
fn tangent_at_with_document_variable() {
    // f(x) = a*x con a = 3 → en x = 2, f' = 3, tangente de pendiente 3.
    let mut doc = Document::new();
    doc.set_variable("a".to_string(), 3.0);
    let out = process_input(&mut doc, &mut "TangentAt[a*x, 2]".to_string());
    let msg = extract_message(&out).expect("mensaje");
    assert!(msg.contains("3.0000"), "esperaba pendiente 3 en: {msg}");
}
