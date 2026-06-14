//! Numeric constraint equations for Grafito.
//!
//! These equations map high-level geometric constraints (`Distance`, `Angle`,
//! `Tangent`) onto scalar residuals that the Levenberg-Marquardt solver can
//! minimise. Each equation knows which solver variables it touches and treats
//! all other geometry as constants.

use crate::document::ObjField;
use crate::numeric_solver::{ConstraintEquation, VarIndex};
use crate::{GeoObject, ObjectId};
use grafito_geometry::Point2;
use std::collections::HashMap;
use std::f64::consts::PI;

/// Distance constraint between two points.
///
/// Equation: `|A - B| - target = 0`.
pub struct DistanceEq {
    a_idx: VarIndex,
    b_idx: VarIndex,
    target: f64,
}

impl DistanceEq {
    pub fn from_inputs(
        _doc: &crate::Document,
        a: ObjectId,
        b: ObjectId,
        target: f64,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        let a_idx = *var_index.get(&(a, ObjField::PointX))?;
        let _ = *var_index.get(&(b, ObjField::PointX))?;
        Some(Self {
            a_idx,
            b_idx: var_index[&(b, ObjField::PointX)],
            target,
        })
    }
}

impl ConstraintEquation for DistanceEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let ax = vars[self.a_idx];
        let ay = vars[self.a_idx + 1];
        let bx = vars[self.b_idx];
        let by = vars[self.b_idx + 1];
        let dx = ax - bx;
        let dy = ay - by;
        let d = (dx * dx + dy * dy).sqrt();
        vec![d - self.target]
    }
}

/// Angle constraint between two lines.
///
/// Equation: `angle(dir1, dir2) - target = 0`, where `target` is given in
/// degrees and the angle is the smaller angle between the two direction
/// vectors (range `-pi..=pi`).
pub struct AngleEq {
    line1_start: VarIndex,
    line1_end: VarIndex,
    line2_start: VarIndex,
    line2_end: VarIndex,
    target_rad: f64,
}

impl AngleEq {
    pub fn from_inputs(
        doc: &crate::Document,
        inputs: &[ObjectId],
        target_deg: f64,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        if inputs.len() < 2 {
            return None;
        }
        let l1 = doc.get_object(inputs[0])?;
        let l2 = doc.get_object(inputs[1])?;
        let (GeoObject::Line(line1), GeoObject::Line(line2)) = (l1, l2) else {
            return None;
        };
        let line1_start = *var_index.get(&(line1.id, ObjField::LineStartX))?;
        let line1_end = *var_index.get(&(line1.id, ObjField::LineEndX))?;
        let line2_start = *var_index.get(&(line2.id, ObjField::LineStartX))?;
        let line2_end = *var_index.get(&(line2.id, ObjField::LineEndX))?;
        Some(Self {
            line1_start,
            line1_end,
            line2_start,
            line2_end,
            target_rad: target_deg.to_radians(),
        })
    }
}

impl ConstraintEquation for AngleEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let s1 = Point2::new(vars[self.line1_start], vars[self.line1_start + 1]);
        let e1 = Point2::new(vars[self.line1_end], vars[self.line1_end + 1]);
        let s2 = Point2::new(vars[self.line2_start], vars[self.line2_start + 1]);
        let e2 = Point2::new(vars[self.line2_end], vars[self.line2_end + 1]);

        let d1 = normalized_direction(s1, e1);
        let d2 = normalized_direction(s2, e2);
        let cross = d1.x * d2.y - d1.y * d2.x;
        let dot = d1.x * d2.x + d1.y * d2.y;
        let mut residual = cross.atan2(dot) - self.target_rad;
        while residual <= -PI {
            residual += 2.0 * PI;
        }
        while residual > PI {
            residual -= 2.0 * PI;
        }
        vec![residual]
    }
}

/// Tangent constraint between a circle and a line.
///
/// Equation: `distance(circle_center, line) - radius = 0`.
pub struct TangentEq {
    radius_idx: VarIndex,
    center: Point2,
    line_start: VarIndex,
    line_end: VarIndex,
}

impl TangentEq {
    pub fn from_inputs(
        doc: &crate::Document,
        a: ObjectId,
        b: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        let (circle, line) = match (doc.get_object(a)?, doc.get_object(b)?) {
            (GeoObject::Circle(c), GeoObject::Line(l)) => (c, l),
            (GeoObject::Line(l), GeoObject::Circle(c)) => (c, l),
            _ => return None,
        };
        let radius_idx = *var_index.get(&(circle.id, ObjField::CircleRadius))?;
        let line_start = *var_index.get(&(line.id, ObjField::LineStartX))?;
        let line_end = *var_index.get(&(line.id, ObjField::LineEndX))?;
        Some(Self {
            radius_idx,
            center: circle.center,
            line_start,
            line_end,
        })
    }
}

impl ConstraintEquation for TangentEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let r = vars[self.radius_idx];
        let start = Point2::new(vars[self.line_start], vars[self.line_start + 1]);
        let end = Point2::new(vars[self.line_end], vars[self.line_end + 1]);
        let dist = point_to_line_distance(self.center, start, end);
        vec![dist - r]
    }
}

fn point_to_line_distance(p: Point2, start: Point2, end: Point2) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-24 {
        return p.distance(&start);
    }
    ((end.x - start.x) * (start.y - p.y) - (start.x - p.x) * (end.y - start.y)).abs() / len2.sqrt()
}

fn normalized_direction(start: Point2, end: Point2) -> Point2 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        Point2::new(1.0, 0.0)
    } else {
        Point2::new(dx / len, dy / len)
    }
}
