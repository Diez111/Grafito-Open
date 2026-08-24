#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::Document;

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

fn message(outcome: CommandOutcome) -> String {
    match outcome {
        CommandOutcome::Message(m) => m,
        other => panic!("expected message, got {other:?}"),
    }
}

#[test]
fn am2_symbolic_multivariable_commands() {
    let mut doc = Document::new();

    let jac = message(run(&mut doc, "JacobianMatrix[[x^2*y, x + y], [x, y]]"));
    assert!(jac.contains("JacobianMatrix"), "{jac}");
    assert!(jac.contains("2*x*y") || jac.contains("2 * x * y"), "{jac}");
    assert!(jac.contains("x^2") || jac.contains("x ^ 2"), "{jac}");

    let hessian = message(run(&mut doc, "Hessian[x^2 + y^2, [x, y]]"));
    assert!(hessian.contains("Hessian"), "{hessian}");
    assert!(hessian.matches('2').count() >= 2, "{hessian}");

    let critical = message(run(
        &mut doc,
        "CriticalPoints[x^2 + y^2, [x, y], -2, 2, -2, 2, 15]",
    ));
    assert!(critical.contains("CriticalPoints"), "{critical}");
    assert!(
        critical.contains("minimum") || critical.contains("minimo"),
        "{critical}"
    );
    assert!(
        critical.contains("[0, 0]") || critical.contains("(0, 0)"),
        "{critical}"
    );

    let lagrange = message(run(
        &mut doc,
        "LagrangeMultipliers[x*y, x^2 + y^2 - 1, [x, y], -2, 2, -2, 2, 21]",
    ));
    assert!(lagrange.contains("LagrangeMultipliers"), "{lagrange}");
    assert!(lagrange.contains("lambda"), "{lagrange}");
}

#[test]
fn am2_integrals_fields_and_theorems() {
    let mut doc = Document::new();

    let line_scalar = message(run(
        &mut doc,
        "LineIntegralScalar[1, [cos(t), sin(t)], t, 0, 6.28318530718, 400]",
    ));
    assert!(line_scalar.contains("LineIntegralScalar"), "{line_scalar}");
    assert!(
        line_scalar.contains("6.28") || line_scalar.contains("6.283"),
        "{line_scalar}"
    );

    let line_vector = message(run(
        &mut doc,
        "LineIntegralVector[[-y, x], [cos(t), sin(t)], t, 0, 6.28318530718, 400]",
    ));
    assert!(line_vector.contains("LineIntegralVector"), "{line_vector}");
    assert!(
        line_vector.contains("6.28") || line_vector.contains("6.283"),
        "{line_vector}"
    );

    let triple = message(run(
        &mut doc,
        "TripleIntegral[1, x, 0, 1, y, 0, 1, z, 0, 1, 20]",
    ));
    assert!(triple.contains("TripleIntegral"), "{triple}");
    assert!(triple.contains("1"), "{triple}");

    let surface = message(run(
        &mut doc,
        "SurfaceIntegralScalar[1, [u, v, 0], [u, v], 0, 1, 0, 1, 20]",
    ));
    assert!(surface.contains("SurfaceIntegralScalar"), "{surface}");
    assert!(surface.contains("1"), "{surface}");

    let flux = message(run(
        &mut doc,
        "Flux[[0, 0, 1], [u, v, 0], [u, v], 0, 1, 0, 1, 20]",
    ));
    assert!(flux.contains("Flux"), "{flux}");
    assert!(flux.contains("1"), "{flux}");

    let conservative = message(run(&mut doc, "IsConservative[[2*x*y, x^2], [x, y]]"));
    assert!(
        conservative.contains("true") || conservative.contains("conservative"),
        "{conservative}"
    );

    let potential = message(run(&mut doc, "PotentialFunction[[2*x*y, x^2], [x, y]]"));
    assert!(potential.contains("PotentialFunction"), "{potential}");
    assert!(
        (potential.contains("x^2") || potential.contains("x ^ 2")) && potential.contains("y"),
        "{potential}"
    );

    let green = message(run(&mut doc, "GreenTheorem[[-y, x], x, 0, 1, y, 0, 1, 40]"));
    assert!(green.contains("GreenTheorem"), "{green}");
    assert!(green.contains("2"), "{green}");

    let gauss = message(run(
        &mut doc,
        "GaussOstrogradski[[x, y, z], x, 0, 1, y, 0, 1, z, 0, 1, 12]",
    ));
    assert!(gauss.contains("GaussOstrogradski"), "{gauss}");
    assert!(gauss.contains("3"), "{gauss}");
}

#[test]
fn scalar_line_integral_is_independent_of_parameter_orientation() {
    let mut doc = Document::new();
    let forward = message(run(&mut doc, "LineIntegralScalar[1, [t, 0], t, 0, 2, 200]"));
    let reverse = message(run(&mut doc, "LineIntegralScalar[1, [t, 0], t, 2, 0, 200]"));

    let parse_total = |output: &str| {
        output
            .split('≈')
            .nth(1)
            .expect("line integral output should contain a value")
            .trim()
            .parse::<f64>()
            .expect("line integral value should be numeric")
    };
    let forward = parse_total(&forward);
    let reverse = parse_total(&reverse);
    assert!((forward - 2.0).abs() < 1e-9, "got {forward}");
    assert!((reverse - 2.0).abs() < 1e-9, "got {reverse}");
}

#[test]
fn am1_theorem_and_series_commands() {
    let mut doc = Document::new();

    let riemann = message(run(&mut doc, "RiemannSum[x, x, 0, 1, 100, midpoint]"));
    assert!(riemann.contains("RiemannSum"), "{riemann}");
    assert!(
        riemann.contains("0.5") || riemann.contains("0.500"),
        "{riemann}"
    );

    let bolzano = message(run(&mut doc, "BolzanoCheck[x^2 - 2, x, 1, 2]"));
    assert!(bolzano.contains("BolzanoCheck"), "{bolzano}");
    assert!(
        bolzano.contains("true") || bolzano.contains("exists"),
        "{bolzano}"
    );

    let rolle = message(run(&mut doc, "RolleCheck[x^2 - 1, x, -1, 1]"));
    assert!(rolle.contains("RolleCheck"), "{rolle}");
    assert!(rolle.contains("0"), "{rolle}");

    let mvt = message(run(&mut doc, "MeanValueCheck[x^2, x, 0, 2]"));
    assert!(mvt.contains("MeanValueCheck"), "{mvt}");
    assert!(mvt.contains("1"), "{mvt}");

    let lhopital = message(run(&mut doc, "LHopital[sin(x), x, x, 0, 3]"));
    assert!(lhopital.contains("LHopital"), "{lhopital}");
    assert!(lhopital.contains("1"), "{lhopital}");

    let alt = message(run(&mut doc, "AlternatingSeriesTest[(-1)^n/n, n]"));
    assert!(alt.contains("AlternatingSeriesTest"), "{alt}");

    let integral_test = message(run(&mut doc, "IntegralTest[1/n^2, n, 1]"));
    assert!(integral_test.contains("IntegralTest"), "{integral_test}");

    let absolute = message(run(&mut doc, "AbsoluteConvergence[(-1)^n/n^2, n]"));
    assert!(absolute.contains("AbsoluteConvergence"), "{absolute}");
}

#[test]
fn algebra_didactic_commands() {
    let mut doc = Document::new();

    let gj = message(run(&mut doc, "GaussJordan[[[1, 2], [3, 4]]]"));
    assert!(gj.contains("GaussJordan"), "{gj}");
    assert!(gj.contains("RREF"), "{gj}");

    let gjs = message(run(&mut doc, "GaussJordanSolve[[[2, 1], [1, 3]], [5, 10]]"));
    assert!(gjs.contains("GaussJordanSolve"), "{gjs}");
    assert!(gjs.contains("[1, 3]"), "{gjs}");

    let cramer = message(run(&mut doc, "Cramer[[[2, 1], [1, 3]], [5, 10]]"));
    assert!(cramer.contains("Cramer"), "{cramer}");
    assert!(cramer.contains("[1, 3]"), "{cramer}");

    let cofactor = message(run(&mut doc, "Cofactor[[[1, 2], [3, 4]], 1, 1]"));
    assert!(cofactor.contains("Cofactor"), "{cofactor}");
    assert!(cofactor.contains("4"), "{cofactor}");

    let adj = message(run(&mut doc, "Adjugate[[[1, 2], [3, 4]]]"));
    assert!(adj.contains("Adjugate"), "{adj}");
    assert!(adj.contains("4") && adj.contains("-2"), "{adj}");

    let laplace = message(run(&mut doc, "LaplaceExpansion[[[1, 2], [3, 4]], row, 1]"));
    assert!(laplace.contains("LaplaceExpansion"), "{laplace}");
    assert!(laplace.contains("-2"), "{laplace}");

    let cob = message(run(
        &mut doc,
        "ChangeOfBasis[[1, 1], [[1, 0], [0, 1]], [[1, 1], [1, -1]]]",
    ));
    assert!(cob.contains("ChangeOfBasis"), "{cob}");

    let diag = message(run(&mut doc, "Diagonalization[[[2, 0], [0, 3]]]"));
    assert!(diag.contains("Diagonalization"), "{diag}");
    assert!(diag.contains("D"), "{diag}");
}

#[test]
fn gauss_and_cramer_keep_scale_and_solution_classification() {
    let mut doc = Document::new();

    let scaled_gauss = message(run(&mut doc, "GaussJordanSolve[[[1e-12]], [1e-12]]"));
    assert!(scaled_gauss.contains("[1]"), "{scaled_gauss}");

    let scaled_cramer = message(run(&mut doc, "Cramer[[[1e-15]], [1e-15]]"));
    assert!(scaled_cramer.contains("[1]"), "{scaled_cramer}");

    let underdetermined = message(run(&mut doc, "GaussJordanSolve[[[1, 1]], [1]]"));
    assert!(
        underdetermined.contains("Infinite solutions"),
        "{underdetermined}"
    );
}

#[test]
fn diagonalization_rejects_a_defective_matrix() {
    let mut doc = Document::new();

    let outcome = run(&mut doc, "Diagonalization[[[1, 1], [0, 1]]]");
    let CommandOutcome::Error(error) = outcome else {
        panic!("una matriz defectiva no debe anunciar una diagonalización");
    };
    assert!(error.contains("no hay base real completa"), "{error}");
}

#[test]
fn diagonalization_accepts_a_complete_repeated_eigenbasis() {
    let mut doc = Document::new();

    let output = message(run(
        &mut doc,
        "Diagonalization[[[2, 0, 1], [0, 2, 0], [0, 0, 3]]]",
    ));
    assert!(output.contains("A = P*D*P^-1"), "{output}");
}
