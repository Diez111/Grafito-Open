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
fn advanced_matrix_factorization_commands() {
    let mut doc = Document::new();

    let transpose = run(&mut doc, "Transpose[[[1, 2], [3, 4]]]");
    assert!(
        matches!(transpose, CommandOutcome::Message(ref m) if m.contains("[1.0000, 3.0000]") && m.contains("[2.0000, 4.0000]")),
        "got {transpose:?}"
    );

    let trace = run(&mut doc, "Trace[[[1, 2], [3, 4]]]");
    assert!(
        matches!(trace, CommandOutcome::Message(ref m) if m.contains("trace = 5")),
        "got {trace:?}"
    );

    let eigenvectors = run(&mut doc, "Eigenvectors[[[2, 0], [0, 3]]]");
    assert!(
        matches!(eigenvectors, CommandOutcome::Message(ref m) if m.contains("lambda = 2") && m.contains("lambda = 3")),
        "got {eigenvectors:?}"
    );

    let cholesky = run(&mut doc, "Cholesky[[[4, 2], [2, 3]]]");
    assert!(
        matches!(cholesky, CommandOutcome::Message(ref m) if m.contains("Cholesky L")),
        "got {cholesky:?}"
    );

    let svd = run(&mut doc, "SVD[[[1, 0], [0, 2]]]");
    assert!(
        matches!(svd, CommandOutcome::Message(ref m) if m.contains("Sigma") && m.contains("2")),
        "got {svd:?}"
    );
}

#[test]
fn multivariable_calculus_commands_cover_am2_workflows() {
    let mut doc = Document::new();

    let gradient = run(&mut doc, "Gradient[x^2 + y^2, [x, y]]");
    assert!(
        matches!(gradient, CommandOutcome::Message(ref m) if m.contains("Gradient") && m.contains("x") && m.contains("y")),
        "got {gradient:?}"
    );

    let directional = run(
        &mut doc,
        "DirectionalDerivative[x^2 + y^2, [x, y], [3, 4], [3, 4]]",
    );
    assert!(
        matches!(directional, CommandOutcome::Message(ref m) if m.contains("DirectionalDerivative = 10")),
        "got {directional:?}"
    );

    let plane = run(&mut doc, "TangentPlane[x^2 + y^2, [1, 2]]");
    assert!(
        matches!(plane, CommandOutcome::Message(ref m) if m.contains("z = 5") && m.contains("2*(x-1)") && m.contains("4*(y-2)")),
        "got {plane:?}"
    );

    let divergence = run(&mut doc, "Divergence[[x, y], [x, y]]");
    assert!(
        matches!(divergence, CommandOutcome::Message(ref m) if m.contains("Divergence") && (m.contains("2") || m.contains("1 + 1"))),
        "got {divergence:?}"
    );

    let curl = run(&mut doc, "Curl[[-y, x], [x, y]]");
    assert!(
        matches!(curl, CommandOutcome::Message(ref m) if m.contains("Curl") && (m.contains("2") || m.contains("1 - (-1)"))),
        "got {curl:?}"
    );

    let integral = run(&mut doc, "DoubleIntegral[1, x, 0, 2, y, 0, 3, 20]");
    assert!(
        matches!(integral, CommandOutcome::Message(ref m) if m.contains("DoubleIntegral") && m.contains("6")),
        "got {integral:?}"
    );

    let surface = run(&mut doc, "SurfaceArea[0, x, 0, 1, y, 0, 1, 20]");
    assert!(
        matches!(surface, CommandOutcome::Message(ref m) if m.contains("SurfaceArea") && m.contains("1")),
        "got {surface:?}"
    );
}

#[test]
fn ode_commands_expose_advanced_solvers() {
    let mut doc = Document::new();

    let rk45 = run(&mut doc, "ODE[y, 0, 1, 1, 20, rk45, 1e-8]");
    assert!(
        matches!(rk45, CommandOutcome::Message(ref m) if m.contains("rk45") && m.contains("points")),
        "got {rk45:?}"
    );

    let backward = run(&mut doc, "ODE[-1000*y, 0, 1, 0.01, 20, backward_euler]");
    assert!(
        matches!(backward, CommandOutcome::Message(ref m) if m.contains("backward_euler") && m.contains("points")),
        "got {backward:?}"
    );

    let system = run(&mut doc, "ODESystem[y, -x, 0, 1, 0, 6.28, 100, rk45, 1e-6]");
    assert!(
        matches!(system, CommandOutcome::Message(ref m) if m.contains("rk45") && m.contains("points")),
        "got {system:?}"
    );
}

#[test]
fn sequence_and_series_commands_cover_basic_convergence_workflows() {
    let mut doc = Document::new();

    let limit = run(&mut doc, "SequenceLimit[(n+1)/n, n]");
    assert!(
        matches!(limit, CommandOutcome::Message(ref m) if m.contains("SequenceLimit") && m.contains("1.000")),
        "got {limit:?}"
    );

    let sum = run(&mut doc, "SeriesSum[n, n, 1, 4]");
    assert!(
        matches!(sum, CommandOutcome::Message(ref m) if m.contains("= 10")),
        "got {sum:?}"
    );

    let ratio = run(&mut doc, "RatioTest[(1/2)^n, n]");
    assert!(
        matches!(ratio, CommandOutcome::Message(ref m) if m.contains("converges") && m.contains("0.5")),
        "got {ratio:?}"
    );

    let root = run(&mut doc, "RootTest[(1/3)^n, n]");
    assert!(
        matches!(root, CommandOutcome::Message(ref m) if m.contains("converges") && m.contains("0.333")),
        "got {root:?}"
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
