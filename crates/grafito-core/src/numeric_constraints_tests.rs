//! Tests for numeric constraint equations and solver convergence.

use crate::numeric_constraints::{
    AngleEq, CoincidentEq, DistanceEq, EqualLengthEq, HorizontalEq, SymmetryEq, TangentEq,
    VarOrConst, VerticalEq,
};
use crate::numeric_solver::ConstraintEquation;
use crate::*;
use grafito_geometry::Point2;

fn finite_difference_jacobian(eq: &dyn ConstraintEquation, vars: &[f64]) -> Vec<Vec<f64>> {
    let m = eq.dimension();
    let n = vars.len();
    let r0 = eq.residual(vars);
    let mut j = vec![vec![0.0; n]; m];
    for i in 0..n {
        let h = 1e-8 * vars[i].abs().max(1.0);
        let mut vars_plus = vars.to_vec();
        vars_plus[i] += h;
        let r_plus = eq.residual(&vars_plus);
        for k in 0..m {
            j[k][i] = (r_plus[k] - r0[k]) / h;
        }
    }
    j
}

fn jacobian_to_dense(triples: &[(usize, usize, f64)], m: usize, n: usize) -> Vec<Vec<f64>> {
    let mut j = vec![vec![0.0; n]; m];
    for &(row, col, val) in triples {
        if row < m && col < n {
            j[row][col] = val;
        }
    }
    j
}

fn assert_mat_close(a: &[Vec<f64>], b: &[Vec<f64>], tol: f64) {
    assert_eq!(a.len(), b.len(), "row count mismatch");
    for (ra, rb) in a.iter().zip(b.iter()) {
        assert_eq!(ra.len(), rb.len(), "column count mismatch");
        for (va, vb) in ra.iter().zip(rb.iter()) {
            assert!(
                (va - vb).abs() < tol,
                "values differ: analytic={}, finite_diff={}",
                va,
                vb
            );
        }
    }
}

#[test]
fn test_distance_eq_jacobian() {
    let eq = DistanceEq::new(0, 2, 5.0);
    let vars = vec![0.0, 0.0, 3.0, 4.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_angle_eq_jacobian() {
    // line1: (0,0) -> (1,0); line2: (0,0) -> (1,1)
    let eq = AngleEq::new(0, 2, 4, 6, 45.0);
    let vars = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_tangent_eq_jacobian() {
    let eq = TangentEq::new(0, Point2::new(0.0, 0.0), 1, 3);
    let vars = vec![1.0, 0.0, 0.0, 1.0, 0.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_coincident_eq_jacobian() {
    let eq = CoincidentEq::new(
        VarOrConst::new(Some(0), 0.0),
        VarOrConst::new(Some(1), 0.0),
        VarOrConst::new(Some(2), 0.0),
        VarOrConst::new(Some(3), 0.0),
    );
    let vars = vec![1.0, 2.0, 3.0, 4.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_horizontal_eq_jacobian() {
    let eq = HorizontalEq::new(VarOrConst::new(Some(0), 0.0), VarOrConst::new(Some(1), 0.0));
    let vars = vec![1.0, 2.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_vertical_eq_jacobian() {
    let eq = VerticalEq::new(VarOrConst::new(Some(0), 0.0), VarOrConst::new(Some(1), 0.0));
    let vars = vec![1.0, 2.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_equal_length_eq_jacobian() {
    let eq = EqualLengthEq::new(
        [VarOrConst::new(Some(0), 0.0), VarOrConst::new(Some(1), 0.0)],
        [VarOrConst::new(Some(2), 0.0), VarOrConst::new(Some(3), 0.0)],
        [VarOrConst::new(Some(4), 0.0), VarOrConst::new(Some(5), 0.0)],
        [VarOrConst::new(Some(6), 0.0), VarOrConst::new(Some(7), 0.0)],
    );
    let vars = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

#[test]
fn test_symmetry_eq_jacobian() {
    let eq = SymmetryEq::new(
        VarOrConst::new(Some(0), 0.0),
        VarOrConst::new(Some(1), 0.0),
        VarOrConst::new(Some(2), 0.0),
        VarOrConst::new(Some(3), 0.0),
        VarOrConst::new(Some(4), 0.0),
        VarOrConst::new(Some(5), 0.0),
        VarOrConst::new(Some(6), 0.0),
        VarOrConst::new(Some(7), 0.0),
    );
    let vars = vec![1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 1.0, 0.0];
    let analytic = jacobian_to_dense(&eq.jacobian(&vars), eq.dimension(), vars.len());
    let finite = finite_difference_jacobian(&eq, &vars);
    assert_mat_close(&analytic, &finite, 1e-6);
}

fn line_direction(line: &LineObj) -> Point2 {
    Point2::new(line.end.x - line.start.x, line.end.y - line.start.y)
}

fn line_length(line: &LineObj) -> f64 {
    line.start.distance(&line.end)
}

fn angle_between_lines(l1: &LineObj, l2: &LineObj) -> f64 {
    let d1 = line_direction(l1);
    let d2 = line_direction(l2);
    let len1 = (d1.x * d1.x + d1.y * d1.y).sqrt();
    let len2 = (d2.x * d2.x + d2.y * d2.y).sqrt();
    assert!(len1 > 1e-6 && len2 > 1e-6);
    let cos = (d1.x * d2.x + d1.y * d2.y) / (len1 * len2);
    cos.clamp(-1.0, 1.0).acos().to_degrees()
}

#[test]
fn test_solver_square_with_equal_sides() {
    let mut doc = Document::new();
    let ab = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let bc = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
    )));
    let cd = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    )));
    let da = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 1.0),
        Point2::new(0.0, 0.0),
    )));

    doc.add_equal_length_constraint(ab, bc);
    doc.add_equal_length_constraint(bc, cd);
    doc.add_equal_length_constraint(cd, da);
    doc.add_angle_constraint(ab, bc, 90.0);

    doc.re_evaluate_constraints(&[]);

    let GeoObject::Line(lab) = doc.get_object(ab).unwrap() else {
        panic!("expected line ab");
    };
    let GeoObject::Line(lbc) = doc.get_object(bc).unwrap() else {
        panic!("expected line bc");
    };
    let GeoObject::Line(lcd) = doc.get_object(cd).unwrap() else {
        panic!("expected line cd");
    };
    let GeoObject::Line(lda) = doc.get_object(da).unwrap() else {
        panic!("expected line da");
    };

    let len_ab = line_length(lab);
    let len_bc = line_length(lbc);
    let len_cd = line_length(lcd);
    let len_da = line_length(lda);

    assert!((len_ab - len_bc).abs() < 1e-5, "ab != bc");
    assert!((len_bc - len_cd).abs() < 1e-5, "bc != cd");
    assert!((len_cd - len_da).abs() < 1e-5, "cd != da");

    let angle = angle_between_lines(lab, lbc);
    assert!(
        (angle - 90.0).abs() < 1e-3,
        "angle should be 90°, got {}",
        angle
    );
}

#[test]
fn test_solver_triangle_three_angles() {
    let mut doc = Document::new();
    // Three lines whose directions approximate an equilateral triangle.
    let l1 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let l2 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(0.5, 0.866),
    )));
    let l3 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(-0.5, 0.866),
    )));

    // For a triangle the directed angles between consecutive side directions
    // are the exterior angles. An equilateral triangle has exterior angles of
    // 120°, so three 120° constraints should converge to a similar triangle.
    doc.add_angle_constraint(l1, l2, 120.0);
    doc.add_angle_constraint(l2, l3, 120.0);
    doc.add_angle_constraint(l3, l1, 120.0);

    doc.re_evaluate_constraints(&[]);

    let GeoObject::Line(line1) = doc.get_object(l1).unwrap() else {
        panic!("expected line1");
    };
    let GeoObject::Line(line2) = doc.get_object(l2).unwrap() else {
        panic!("expected line2");
    };
    let GeoObject::Line(line3) = doc.get_object(l3).unwrap() else {
        panic!("expected line3");
    };

    let a12 = angle_between_lines(line1, line2);
    let a23 = angle_between_lines(line2, line3);
    let a31 = angle_between_lines(line3, line1);

    assert!((a12 - 120.0).abs() < 1e-3, "angle 12 = {}", a12);
    assert!((a23 - 120.0).abs() < 1e-3, "angle 23 = {}", a23);
    assert!((a31 - 120.0).abs() < 1e-3, "angle 31 = {}", a31);
}

#[test]
fn test_solver_rhombus() {
    let mut doc = Document::new();
    // Four lines roughly forming a rhombus.
    let ab = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let bc = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(1.0, 0.0),
        Point2::new(1.5, 0.866),
    )));
    let cd = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(1.5, 0.866),
        Point2::new(0.5, 0.866),
    )));
    let da = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.5, 0.866),
        Point2::new(0.0, 0.0),
    )));

    doc.add_equal_length_constraint(ab, bc);
    doc.add_equal_length_constraint(bc, cd);
    doc.add_equal_length_constraint(cd, da);
    doc.add_equal_length_constraint(da, ab);
    // Fix one interior angle to 60° so the shape is a specific rhombus.
    doc.add_angle_constraint(ab, bc, 60.0);

    doc.re_evaluate_constraints(&[]);

    let GeoObject::Line(lab) = doc.get_object(ab).unwrap() else {
        panic!("expected line ab");
    };
    let GeoObject::Line(lbc) = doc.get_object(bc).unwrap() else {
        panic!("expected line bc");
    };
    let GeoObject::Line(lcd) = doc.get_object(cd).unwrap() else {
        panic!("expected line cd");
    };
    let GeoObject::Line(lda) = doc.get_object(da).unwrap() else {
        panic!("expected line da");
    };

    let len_ab = line_length(lab);
    let len_bc = line_length(lbc);
    let len_cd = line_length(lcd);
    let len_da = line_length(lda);

    assert!((len_ab - len_bc).abs() < 1e-5);
    assert!((len_bc - len_cd).abs() < 1e-5);
    assert!((len_cd - len_da).abs() < 1e-5);

    let angle = angle_between_lines(lab, lbc);
    assert!(
        (angle - 60.0).abs() < 1e-3,
        "angle should be 60°, got {}",
        angle
    );
}

#[test]
fn test_solver_overconstrained_system() {
    let mut doc = Document::new();
    // Three lines roughly forming an equilateral triangle.
    let l1 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
    )));
    let l2 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(0.5, 0.866),
    )));
    let l3 = doc.add_object(GeoObject::Line(LineObj::new(
        Point2::new(0.0, 0.0),
        Point2::new(-0.5, 0.866),
    )));

    // Overconstrain: all sides equal and all exterior angles 120° (equilateral
    // triangle). The system is overconstrained but consistent.
    doc.add_equal_length_constraint(l1, l2);
    doc.add_equal_length_constraint(l2, l3);
    doc.add_equal_length_constraint(l3, l1);
    doc.add_angle_constraint(l1, l2, 120.0);
    doc.add_angle_constraint(l2, l3, 120.0);
    doc.add_angle_constraint(l3, l1, 120.0);

    doc.re_evaluate_constraints(&[]);

    let GeoObject::Line(line1) = doc.get_object(l1).unwrap() else {
        panic!("expected line1");
    };
    let GeoObject::Line(line2) = doc.get_object(l2).unwrap() else {
        panic!("expected line2");
    };
    let GeoObject::Line(line3) = doc.get_object(l3).unwrap() else {
        panic!("expected line3");
    };

    let len1 = line_length(line1);
    let len2 = line_length(line2);
    let len3 = line_length(line3);

    assert!((len1 - len2).abs() < 1e-5);
    assert!((len2 - len3).abs() < 1e-5);

    let a12 = angle_between_lines(line1, line2);
    let a23 = angle_between_lines(line2, line3);
    let a31 = angle_between_lines(line3, line1);

    assert!((a12 - 120.0).abs() < 1e-3);
    assert!((a23 - 120.0).abs() < 1e-3);
    assert!((a31 - 120.0).abs() < 1e-3);
}
