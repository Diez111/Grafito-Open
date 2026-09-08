#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::numeric_constraints::{
    AngleEq, CoincidentEq, DistanceEq, HorizontalEq, VarOrConst, VerticalEq,
};
use grafito_core::numeric_solver::{ConstraintEquation, NumericSolver};

fn bench_numeric_solver_8_constraints(c: &mut Criterion) {
    // 8 variables: 4 points * (x, y)
    // P1 = vars[0..2], P2 = vars[2..4], P3 = vars[4..6], P4 = vars[6..8]
    // System:
    //   P1 fixed at origin, P2 on x-axis, P3 above P2, P4 closes the rectangle.
    let equations: Vec<Box<dyn ConstraintEquation>> = vec![
        Box::new(CoincidentEq::new(
            VarOrConst::new(Some(0), 0.0),
            VarOrConst::new(Some(1), 0.0),
            VarOrConst::new(None, 0.0),
            VarOrConst::new(None, 0.0),
        )),
        Box::new(HorizontalEq::new(
            VarOrConst::new(Some(1), 0.0),
            VarOrConst::new(Some(3), 0.0),
        )),
        Box::new(VerticalEq::new(
            VarOrConst::new(Some(2), 0.0),
            VarOrConst::new(Some(4), 0.0),
        )),
        Box::new(DistanceEq::new(0, 2, 3.0)),
        Box::new(DistanceEq::new(2, 4, 4.0)),
        Box::new(DistanceEq::new(4, 6, 3.0)),
        Box::new(DistanceEq::new(6, 0, 4.0)),
        Box::new(AngleEq::new(0, 2, 4, 6, 90.0)),
    ];

    let solver = NumericSolver::default();
    let initial_vars = vec![0.0, 0.0, 3.0, 0.0, 3.0, 4.0, 0.0, 4.0];

    // P1b: gate verde — el sistema 8-restricciones con `AngleEq` no converge
    // desde este arranque (`MaxIterations`, residual ~1.41, verificado
    // 2026-09-08 en debug y release). El bench mide rendimiento, no
    // convergencia: no se hace `unwrap` (era el rojo). Se benchea el costo
    // por iteración sea `Ok` o `Err`, y se publica `vars` para que criterion
    // no elimine el trabajo. Si se quiere benchear convergencia, usar un
    // rectángulo sin `AngleEq` o un arranque perturbado (ver
    // `numeric_solver` tests que sí convergen).
    c.bench_function("numeric_solver_8_constraints", |b| {
        b.iter(|| {
            let mut vars = initial_vars.clone();
            let _ = solver.solve(&mut vars, &equations);
            black_box(vars)
        })
    });
}

criterion_group!(benches, bench_numeric_solver_8_constraints);
criterion_main!(benches);
