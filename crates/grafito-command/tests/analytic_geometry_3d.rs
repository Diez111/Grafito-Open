#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};
use grafito_geometry::{RegularPolychoron, RegularPolytopeFamily};

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

fn point3d_solutions(doc: &Document) -> Vec<f64> {
    let mut ys = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point3D(p) if p.label.starts_with("Sol3D") => Some(p.position.y),
            _ => None,
        })
        .collect::<Vec<_>>();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys
}

#[test]
fn plane3d_and_line3d_commands_create_objects() {
    let mut doc = Document::new();
    assert!(matches!(
        run(&mut doc, "Plane3D[1, 0, 1, 4]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]"),
        CommandOutcome::Ok
    ));

    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Plane3D(_))));
    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Line3D(_))));
}

#[test]
fn equidistant_from_solves_university_problem_on_y_axis() {
    let mut doc = Document::new();
    run(&mut doc, "Plane3D[1, 0, 1, 4]");
    run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]");

    let outcome = run(&mut doc, "EquidistantFrom[P, L, \"y-axis\"]");
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "expected message, got {:?}",
        outcome
    );

    let ys = point3d_solutions(&doc);
    assert_eq!(ys.len(), 2, "expected two solutions, got {ys:?}");
    let expected = 2.0 * 2.0_f64.sqrt();
    assert!((ys[0] + expected).abs() < 1e-5, "ys={ys:?}");
    assert!((ys[1] - expected).abs() < 1e-5, "ys={ys:?}");
}

#[test]
fn solve3dgeometry_solves_dist_equality_with_point_constraint() {
    let mut doc = Document::new();
    run(&mut doc, "Plane3D[1, 0, 1, 4]");
    run(&mut doc, "Line3D[1, 1, 2, 1, 1, 0]");

    let outcome = run(
        &mut doc,
        "Solve3DGeometry[\"dist(P,P)=dist(P,L)\", y, \"P=(0,y,0)\"]",
    );
    assert!(
        matches!(outcome, CommandOutcome::Message(_)),
        "expected message, got {:?}",
        outcome
    );

    let ys = point3d_solutions(&doc);
    assert_eq!(ys.len(), 2, "expected two solutions, got {ys:?}");
    let expected = 2.0 * 2.0_f64.sqrt();
    assert!((ys[0] + expected).abs() < 1e-5, "ys={ys:?}");
    assert!((ys[1] - expected).abs() < 1e-5, "ys={ys:?}");
}

#[test]
fn plane3d_from_three_points_and_line3d_from_two_points() {
    let mut doc = Document::new();
    run(&mut doc, "Point3D[0, 0, 0]");
    run(&mut doc, "Point3D[1, 0, 0]");
    run(&mut doc, "Point3D[0, 1, 0]");
    run(&mut doc, "Point3D[0, 0, 1]");

    assert!(matches!(
        run(&mut doc, "Plane3D[P, P₁, P₂]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(run(&mut doc, "Line3D[P, P₃]"), CommandOutcome::Ok));

    let plane = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::Plane3D(p) => Some(p),
        _ => None,
    });
    let line = doc.objects_iter().find_map(|(_, obj)| match obj {
        GeoObject::Line3D(l) => Some(l),
        _ => None,
    });

    let plane = plane.expect("Plane3D command should create a plane");
    assert!(plane.a.abs() < 1e-9);
    assert!(plane.b.abs() < 1e-9);
    assert!((plane.c.abs() - 1.0).abs() < 1e-9);

    let line = line.expect("Line3D command should create a line");
    assert!((line.direction.z - 1.0).abs() < 1e-9);
}

#[test]
fn curve3d_uses_the_optional_declared_parameter() {
    let mut doc = Document::new();

    assert!(matches!(
        run(&mut doc, "Curve3D[(s,s^2,2*s),s,0,2]"),
        CommandOutcome::Ok
    ));

    let curve = doc
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::ParametricCurve3D(curve) => Some(curve),
            _ => None,
        })
        .expect("Curve3D must create a parametric curve");
    assert_eq!(curve.parameter, "s");

    let samples =
        grafito_core::parametric_sampling::evaluate_parametric_curve_3d(curve, 2, &doc.variables);
    assert_eq!(samples[2], (2.0, 4.0, 4.0));
}

#[test]
fn direct_3d_primitives_accept_finite_numeric_expressions() {
    let mut doc = Document::new();

    assert!(matches!(
        run(&mut doc, "Point3D[pi, 0, 0]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Segment3D[0, 0, 0, 2*pi, 0, 0]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Sphere[0, 0, 0, 2*pi]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Cube[0, 0, 0, 2*pi]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Tetrahedron[0, 0, 0, 2*pi]"),
        CommandOutcome::Ok
    ));

    assert!(doc
        .objects_iter()
        .any(|(_, obj)| matches!(obj, GeoObject::Cube3D(cube) if cube.size > 6.0)));
    assert!(doc.objects_iter().any(|(_, obj)| {
        matches!(obj, GeoObject::Tetrahedron3D(tetrahedron) if tetrahedron.edge_length > 6.0)
    }));
}

#[test]
fn parametric_surface_tuple_syntax_creates_the_requested_heart_surface() {
    let mut doc = Document::new();
    let heart = "Surface3D[((1 - sin(u)) * cos(u) * v, (1 - sin(u)) * sin(u) * v, (1 - cos(u)) * (1 - v/2) - 0.5), 0, 2*pi, 0, 1]";

    assert!(matches!(run(&mut doc, heart), CommandOutcome::Ok));

    let surface = doc.objects_iter().find_map(|(_, object)| match object {
        GeoObject::Surface3D(surface) => Some(surface),
        _ => None,
    });
    let surface = surface.expect("the heart command should create a surface");
    assert!(surface.is_parametric);
    assert_eq!(surface.expr_x, "(1 - sin(u)) * cos(u) * v");
    assert!((surface.u_max - std::f64::consts::TAU).abs() < 1e-12);

    let samples =
        grafito_core::parametric_sampling::evaluate_surface_3d(surface, 12, &doc.variables);
    assert_eq!(samples.len(), 13);
    assert!(samples
        .iter()
        .flatten()
        .any(|point| { point.x.is_finite() && point.y.is_finite() && point.z.is_finite() }));
}

#[test]
fn parametric_surface_rejects_an_invalid_component_before_document_mutation() {
    let mut doc = Document::new();

    let outcome = run(&mut doc, "Surface3D[(sin(u), unknown(u), v), 0, 1, 0, 1]");

    assert!(matches!(outcome, CommandOutcome::Error(_)));
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn parametric_surface_tuple_accepts_xy_parameters_from_assistant_proposals() {
    let mut doc = Document::new();
    let surface = "Surface3D[((16*x^3 - 13*x - 16*x^3*y^2 + 12*x*y^2 + x*y^4) / 30, (5*y - 10*x^2*y - 5*y^3 + 2*x^2*y^3 + y^5) / 30, 0.15*exp(-(3*x^2 + 4*y^2))), -1.3, 1.3, -1.1, 1.1]";

    assert!(matches!(run(&mut doc, surface), CommandOutcome::Ok));

    let surface = doc.objects_iter().find_map(|(_, object)| match object {
        GeoObject::Surface3D(surface) => Some(surface),
        _ => None,
    });
    let surface = surface.expect("the assistant proposal should create a surface");
    assert!(surface.is_parametric);
    assert!(surface.expr_x.contains('u'));
    assert!(surface.expr_y.contains('v'));

    let samples =
        grafito_core::parametric_sampling::evaluate_surface_3d(surface, 12, &doc.variables);
    assert!(samples
        .iter()
        .flatten()
        .any(|point| { point.x.is_finite() && point.y.is_finite() && point.z.is_finite() }));
}

#[test]
fn parametric_surface_rejects_mixed_xy_and_uv_parameters() {
    let mut doc = Document::new();

    let outcome = run(&mut doc, "Surface3D[(u, y, 0), 0, 1, 0, 1]");

    assert!(matches!(outcome, CommandOutcome::Error(_)));
    assert_eq!(doc.object_count(), 0);
}

#[test]
fn regular_polychoron_4d_commands_create_typed_defaults_and_fixed_plane_rotations() {
    let mut doc = Document::new();

    for (command, kind) in [
        ("Pentachoron4D", RegularPolychoron::Pentachoron),
        ("Tesseract4D", RegularPolychoron::Tesseract),
        ("SixteenCell4D", RegularPolychoron::SixteenCell),
        ("TwentyFourCell4D", RegularPolychoron::TwentyFourCell),
        ("OneTwentyCell4D", RegularPolychoron::OneTwentyCell),
        ("SixHundredCell4D", RegularPolychoron::SixHundredCell),
    ] {
        assert!(matches!(
            run(&mut doc, &format!("{command}[]")),
            CommandOutcome::Ok
        ));
        let object = doc
            .objects_iter()
            .find_map(|(_, object)| match object {
                GeoObject::RegularPolychoron4D(polychoron) if polychoron.kind == kind => {
                    Some(polychoron)
                }
                _ => None,
            })
            .expect("each direct 4D command creates its typed selector");
        assert_eq!(object.scale, 1.0);
        assert_eq!(object.rotation_angles, [0.0; 6]);
    }

    let mut rotated = Document::new();
    assert!(matches!(
        run(
            &mut rotated,
            "Tesseract4D[2.5,{0.1,-0.2,0.3,-0.4,0.5,-0.6}]"
        ),
        CommandOutcome::Ok
    ));
    let tesseract = rotated
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::RegularPolychoron4D(polychoron)
                if polychoron.kind == RegularPolychoron::Tesseract =>
            {
                Some(polychoron)
            }
            _ => None,
        })
        .expect("custom Tesseract4D creates a typed selector");
    assert_eq!(tesseract.scale, 2.5);
    assert_eq!(tesseract.rotation_angles, [0.1, -0.2, 0.3, -0.4, 0.5, -0.6]);
}

#[test]
fn generic_regular_polytope_commands_support_dimensions_scales_and_lexicographic_rotations() {
    let mut defaults = Document::new();
    for (command, family, dimension) in [
        ("SimplexND", RegularPolytopeFamily::Simplex, 3),
        ("HypercubeND", RegularPolytopeFamily::Hypercube, 4),
        ("CrossPolytopeND", RegularPolytopeFamily::CrossPolytope, 10),
    ] {
        assert!(matches!(
            run(&mut defaults, &format!("{command}[{dimension}]")),
            CommandOutcome::Ok
        ));
        let object = defaults
            .objects_iter()
            .find_map(|(_, object)| match object {
                GeoObject::RegularPolytopeND(polytope)
                    if polytope.family == family && polytope.dimension == dimension =>
                {
                    Some(polytope)
                }
                _ => None,
            })
            .expect("each generic family creates its typed selector");
        assert_eq!(object.scale, 1.0);
        assert_eq!(
            object.rotation_angles,
            vec![0.0; dimension * (dimension - 1) / 2]
        );
    }

    let mut scaled = Document::new();
    for (command, family, dimension) in [
        ("SimplexND", RegularPolytopeFamily::Simplex, 3),
        ("HypercubeND", RegularPolytopeFamily::Hypercube, 4),
        ("CrossPolytopeND", RegularPolytopeFamily::CrossPolytope, 5),
    ] {
        assert!(matches!(
            run(&mut scaled, &format!("{command}[{dimension},2]")),
            CommandOutcome::Ok
        ));
        assert!(scaled.objects_iter().any(|(_, object)| {
            matches!(object, GeoObject::RegularPolytopeND(polytope)
                if polytope.family == family && polytope.dimension == dimension && polytope.scale == 2.0)
        }));
    }

    let mut rotated = Document::new();
    assert!(matches!(
        run(
            &mut rotated,
            "CrossPolytopeND[5,1.5,{0.1,-0.2,0.3,-0.4,0.5,-0.6,0.7,-0.8,0.9,-1.0}]"
        ),
        CommandOutcome::Ok
    ));
    let polytope = rotated
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::RegularPolytopeND(polytope)
                if polytope.family == RegularPolytopeFamily::CrossPolytope =>
            {
                Some(polytope)
            }
            _ => None,
        })
        .expect("custom generic command creates a typed selector");
    assert_eq!(polytope.dimension, 5);
    assert_eq!(polytope.scale, 1.5);
    assert_eq!(
        polytope.rotation_angles,
        vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8, 0.9, -1.0]
    );
}

#[test]
fn named_4d_aliases_do_not_reinterpret_legacy_or_3d_commands() {
    let mut doc = Document::new();

    assert!(matches!(
        run(&mut doc, "Tetrahedron[0,0,0,2]"),
        CommandOutcome::Ok
    ));
    assert!(matches!(
        run(&mut doc, "Hypercube[]"),
        CommandOutcome::Message(_)
    ));
    assert!(matches!(
        run(&mut doc, "tesseract[]"),
        CommandOutcome::Message(_)
    ));
    assert!(matches!(run(&mut doc, "hypercube4d[]"), CommandOutcome::Ok));

    assert!(doc
        .objects_iter()
        .any(|(_, object)| matches!(object, GeoObject::Tetrahedron3D(_))));
    assert_eq!(
        doc.objects_iter()
            .filter(|(_, object)| matches!(object, GeoObject::HyperSurface4D(_)))
            .count(),
        2,
        "the bare legacy names still create legacy CPU-overlay objects"
    );
    assert!(doc.objects_iter().any(|(_, object)| {
        matches!(object, GeoObject::RegularPolychoron4D(polychoron)
            if polychoron.kind == RegularPolychoron::Tesseract)
    }));
}
