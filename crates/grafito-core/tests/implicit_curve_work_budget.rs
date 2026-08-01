use grafito_core::{
    implicit_curve::{
        evaluate_implicit_curve, marching_squares_from_grid, MAX_IMPLICIT_GRID_SIZE,
        MAX_MARCHING_SQUARES_SEGMENTS, MAX_MARCHING_SQUARES_WORK_UNITS,
    },
    ImplicitCurveObj, RelationOperator,
};
use std::collections::HashMap;

fn checkerboard_rows(grid_size: usize) -> Vec<Vec<f64>> {
    (0..=grid_size)
        .map(|y| {
            (0..=grid_size)
                .map(|x| if (x + y) % 2 == 0 { -1.0 } else { 1.0 })
                .collect()
        })
        .collect()
}

#[test]
fn no_crossing_levels_consume_the_shared_cell_budget() {
    let mut curve = ImplicitCurveObj::new("1", "0", RelationOperator::Eq);
    curve.contour_levels = Some((2..=10).map(f64::from).collect());

    let contours = evaluate_implicit_curve(
        &curve,
        (0.0, 1.0, 0.0, 1.0),
        MAX_IMPLICIT_GRID_SIZE,
        &HashMap::new(),
    );

    let cells_per_level = MAX_IMPLICIT_GRID_SIZE * MAX_IMPLICIT_GRID_SIZE;
    assert_eq!(
        contours.len(),
        MAX_MARCHING_SQUARES_WORK_UNITS / cells_per_level,
        "levels without crossings must still consume one unit for every visited cell"
    );
    assert!(contours.iter().all(|(_, segments)| segments.is_empty()));
}

#[test]
fn dense_level_streams_a_deterministic_segment_prefix() {
    let grid_size = 256;
    let contours = marching_squares_from_grid(
        &checkerboard_rows(grid_size),
        &[0.0],
        0.0,
        0.0,
        grid_size as f64,
        grid_size as f64,
    );

    assert_eq!(contours.len(), 1);
    let segments = &contours[0].1;
    assert_eq!(segments.len(), MAX_MARCHING_SQUARES_SEGMENTS);
    assert_eq!(segments[0].0.x, 1.0);
    assert_eq!(segments[0].0.y, 0.5);
    assert_eq!(segments[0].1.x, 0.5);
    assert_eq!(segments[0].1.y, 0.0);
}

#[test]
fn direct_evaluation_rejects_grids_larger_than_the_public_limit() {
    let curve = ImplicitCurveObj::new("x", "y", RelationOperator::Eq);

    let contours = evaluate_implicit_curve(
        &curve,
        (-1.0, 1.0, -1.0, 1.0),
        MAX_IMPLICIT_GRID_SIZE + 1,
        &HashMap::new(),
    );

    assert!(contours.is_empty());
}
