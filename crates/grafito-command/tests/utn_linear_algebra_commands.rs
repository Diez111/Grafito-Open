use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

#[test]
fn intersection3d_plane_plane_creates_line() {
    let mut doc = Document::new();
    run(&mut doc, "Plane3D[1, 0, 0, -1]"); // x=1 => P
    run(&mut doc, "Plane3D[0, 1, 0, -2]"); // y=2 => P₁

    let out = run(&mut doc, "Intersection3D[P, P₁]");
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    assert!(doc.objects_iter().any(|(_, obj)| match obj {
        GeoObject::Line3D(l) =>
            (l.point.x - 1.0).abs() < 1e-8
                && (l.point.y - 2.0).abs() < 1e-8
                && l.direction.z.abs() > 0.9,
        _ => false,
    }));
}

#[test]
fn projection3d_point_plane_creates_projected_point() {
    let mut doc = Document::new();
    run(&mut doc, "Point3D[1, 2, 3]"); // P
    run(&mut doc, "Plane3D[1, 0, 1, 4]"); // P₁

    let out = run(&mut doc, "Projection3D[P, P₁]");
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    assert!(doc.objects_iter().any(|(_, obj)| match obj {
        GeoObject::Point3D(p) =>
            (p.position.x + 3.0).abs() < 1e-8
                && (p.position.y - 2.0).abs() < 1e-8
                && (p.position.z + 1.0).abs() < 1e-8,
        _ => false,
    }));
}

#[test]
fn plane_through_lines_creates_plane_for_intersecting_lines() {
    let mut doc = Document::new();
    run(&mut doc, "Line3D[1, 0, 0, 1, 1, 1]"); // L
    run(&mut doc, "Line3D[1, 0, 0, 0, 1, -1]"); // L₁

    let out = run(&mut doc, "PlaneThroughLines[L, L₁]");
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    assert!(doc.objects_iter().any(|(_, obj)| match obj {
        GeoObject::Plane3D(p) => {
            let f = |x: f64, y: f64, z: f64| p.a * x + p.b * y + p.c * z + p.d;
            f(1.0, 0.0, 0.0).abs() < 1e-8
                && f(2.0, 1.0, 1.0).abs() < 1e-8
                && f(1.0, 1.0, -1.0).abs() < 1e-8
        }
        _ => false,
    }));
}

#[test]
fn matrix_rank_nullspace_and_linear_solve_commands() {
    let mut doc = Document::new();
    let rank_out = run(&mut doc, "Rank[[[1, 2], [2, 4]]]");
    assert!(
        matches!(rank_out, CommandOutcome::Message(ref m) if m.contains("rank = 1")),
        "got {rank_out:?}"
    );

    let ns_out = run(&mut doc, "NullSpace[[[1, 2], [2, 4]]]");
    assert!(
        matches!(ns_out, CommandOutcome::Message(ref m) if m.contains("dimension = 1")),
        "got {ns_out:?}"
    );

    let solve_out = run(&mut doc, "LinearSolve[[[2, 1], [1, 3]], [5, 10]]");
    assert!(
        matches!(solve_out, CommandOutcome::Message(ref m) if m.contains("[1, 3]")),
        "got {solve_out:?}"
    );

    let bad_out = run(&mut doc, "LinearSolve[[[1, 1], [2, 2]], [1, 3]]");
    assert!(
        matches!(bad_out, CommandOutcome::Message(ref m) if m.contains("No solution")),
        "got {bad_out:?}"
    );
}

#[test]
fn p2_commands_detect_dependence_basis_and_equations() {
    let mut doc = Document::new();
    let dep = run(&mut doc, "P2Dependence[{1+x, x+x^2, 1+2*x+x^2}]");
    assert!(
        matches!(dep, CommandOutcome::Message(ref m) if m.contains("Dependent")),
        "got {dep:?}"
    );

    let basis = run(&mut doc, "P2Basis[{1, x, x^2}]");
    assert!(
        matches!(basis, CommandOutcome::Message(ref m) if m.contains("basis of P2")),
        "got {basis:?}"
    );

    let equations = run(&mut doc, "P2Equations[{1, x}]");
    assert!(
        matches!(equations, CommandOutcome::Message(ref m) if m.contains("dimension = 2") && m.contains("a")),
        "got {equations:?}"
    );
}

#[test]
fn subspace_commands_handle_dimension_sum_intersection_and_orthogonal() {
    let mut doc = Document::new();
    let dim = run(
        &mut doc,
        "SubspaceDimension[[[1, 0, 1], [0, 1, 1], [1, 1, 2]]]",
    );
    assert!(
        matches!(dim, CommandOutcome::Message(ref m) if m.contains("dimension = 2")),
        "got {dim:?}"
    );

    let sum = run(
        &mut doc,
        "SubspaceSum[[[1,0,0],[0,1,0]], [[0,1,0],[0,0,1]]]",
    );
    assert!(
        matches!(sum, CommandOutcome::Message(ref m) if m.contains("dim(U + V) = 3")),
        "got {sum:?}"
    );

    let intersection = run(
        &mut doc,
        "SubspaceIntersection[[[1,0,0],[0,1,0]], [[0,1,0],[0,0,1]]]",
    );
    assert!(
        matches!(intersection, CommandOutcome::Message(ref m) if m.contains("dim(U ∩ V) = 1")),
        "got {intersection:?}"
    );

    let orthogonal = run(&mut doc, "OrthogonalComplement[[[1,1,0],[0,1,1]]]");
    assert!(
        matches!(orthogonal, CommandOutcome::Message(ref m) if m.contains("dim(U⊥) = 1")),
        "got {orthogonal:?}"
    );
}

#[test]
fn solve_line3d_parameters_handles_direction_constraints() {
    let mut doc = Document::new();
    let out = run(
        &mut doc,
        "SolveLine3DParameters[[1,h,k], \"perpendicular\", [1,1,0], h, k]",
    );
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("Infinite parameter solutions") && m.contains("x0")),
        "got {out:?}"
    );

    let out = run(
        &mut doc,
        "SolveLine3DParameters[[1,h,k], \"parallel\", [1,2,3], h, k]",
    );
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("Unique parameter solution") && m.contains("h = 2") && m.contains("k = 3")),
        "got {out:?}"
    );
}

#[test]
fn matrix_param_solve_finds_singular_parameter_values() {
    let mut doc = Document::new();
    let out = run(&mut doc, "MatrixParamSolve[[[h, 1], [1, h]], h]");
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("det(A)") && m.contains("-1") && m.contains("1")),
        "got {out:?}"
    );
}

#[test]
fn degenerate_3d_inputs_return_errors() {
    let mut doc = Document::new();
    assert!(matches!(
        run(&mut doc, "Plane3D[0,0,0,1]"),
        CommandOutcome::Error(_)
    ));
    assert!(matches!(
        run(&mut doc, "Line3D[0,0,0,0,0,0]"),
        CommandOutcome::Error(_)
    ));
}
