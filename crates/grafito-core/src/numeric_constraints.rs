//! Numeric constraint equations for Grafito.
//!
//! These equations map high-level geometric constraints (`Distance`, `Angle`,
//! `Tangent`) onto scalar residuals that the Levenberg-Marquardt solver can
//! minimise. Each equation knows which solver variables it touches and treats
//! all other geometry as constants.

use crate::document::ObjField;
use crate::numeric_solver::{ConstraintEquation, VarIndex};
use crate::{GeoObject, LineKind, ObjectId};
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

/// Either a solver variable index or a constant fallback value.
#[derive(Clone, Copy)]
struct VarOrConst {
    idx: Option<VarIndex>,
    value: f64,
}

impl VarOrConst {
    fn get(&self, vars: &[f64]) -> f64 {
        self.idx.map_or(self.value, |i| vars[i])
    }
}

fn field_or_const(
    doc: &crate::Document,
    id: ObjectId,
    field: ObjField,
    var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
) -> VarOrConst {
    match var_index.get(&(id, field)) {
        Some(&idx) => VarOrConst {
            idx: Some(idx),
            value: 0.0,
        },
        None => VarOrConst {
            idx: None,
            value: doc_field_value(doc, id, field),
        },
    }
}

fn doc_field_value(doc: &crate::Document, id: ObjectId, field: ObjField) -> f64 {
    match (doc.get_object(id), field) {
        (Some(GeoObject::Point(p)), ObjField::PointX) => p.position.x,
        (Some(GeoObject::Point(p)), ObjField::PointY) => p.position.y,
        (Some(GeoObject::Circle(c)), ObjField::CircleRadius) => c.radius,
        (Some(GeoObject::Line(l)), ObjField::LineStartX) => l.start.x,
        (Some(GeoObject::Line(l)), ObjField::LineStartY) => l.start.y,
        (Some(GeoObject::Line(l)), ObjField::LineEndX) => l.end.x,
        (Some(GeoObject::Line(l)), ObjField::LineEndY) => l.end.y,
        _ => 0.0,
    }
}

/// Coincident constraint between two points.
///
/// Equation: `[ax - bx, ay - by] = [0, 0]`.
pub struct CoincidentEq {
    ax: VarOrConst,
    ay: VarOrConst,
    bx: VarOrConst,
    by: VarOrConst,
}

impl CoincidentEq {
    pub fn from_inputs(
        doc: &crate::Document,
        a: ObjectId,
        b: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        match (doc.get_object(a)?, doc.get_object(b)?) {
            (GeoObject::Point(_), GeoObject::Point(_)) => Some(Self {
                ax: field_or_const(doc, a, ObjField::PointX, var_index),
                ay: field_or_const(doc, a, ObjField::PointY, var_index),
                bx: field_or_const(doc, b, ObjField::PointX, var_index),
                by: field_or_const(doc, b, ObjField::PointY, var_index),
            }),
            _ => None,
        }
    }
}

impl ConstraintEquation for CoincidentEq {
    fn dimension(&self) -> usize {
        2
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let ax = self.ax.get(vars);
        let ay = self.ay.get(vars);
        let bx = self.bx.get(vars);
        let by = self.by.get(vars);
        vec![ax - bx, ay - by]
    }
}

/// Horizontal line constraint.
///
/// Equation: `start_y - end_y = 0`.
pub struct HorizontalEq {
    sy: VarOrConst,
    ey: VarOrConst,
}

impl HorizontalEq {
    pub fn from_inputs(
        doc: &crate::Document,
        line: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        if !matches!(doc.get_object(line)?, GeoObject::Line(_)) {
            return None;
        }
        Some(Self {
            sy: field_or_const(doc, line, ObjField::LineStartY, var_index),
            ey: field_or_const(doc, line, ObjField::LineEndY, var_index),
        })
    }
}

impl ConstraintEquation for HorizontalEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        vec![self.sy.get(vars) - self.ey.get(vars)]
    }
}

/// Vertical line constraint.
///
/// Equation: `start_x - end_x = 0`.
pub struct VerticalEq {
    sx: VarOrConst,
    ex: VarOrConst,
}

impl VerticalEq {
    pub fn from_inputs(
        doc: &crate::Document,
        line: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        if !matches!(doc.get_object(line)?, GeoObject::Line(_)) {
            return None;
        }
        Some(Self {
            sx: field_or_const(doc, line, ObjField::LineStartX, var_index),
            ex: field_or_const(doc, line, ObjField::LineEndX, var_index),
        })
    }
}

impl ConstraintEquation for VerticalEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        vec![self.sx.get(vars) - self.ex.get(vars)]
    }
}

/// Equal length constraint between two line segments.
///
/// Equation: `|A - B| - |C - D| = 0`.
pub struct EqualLengthEq {
    a: [VarOrConst; 2],
    b: [VarOrConst; 2],
    c: [VarOrConst; 2],
    d: [VarOrConst; 2],
}

impl EqualLengthEq {
    pub fn from_inputs(
        doc: &crate::Document,
        line1: ObjectId,
        line2: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        let (GeoObject::Line(l1), GeoObject::Line(l2)) =
            (doc.get_object(line1)?, doc.get_object(line2)?)
        else {
            return None;
        };
        if l1.kind != LineKind::Segment || l2.kind != LineKind::Segment {
            return None;
        }
        Some(Self {
            a: [
                field_or_const(doc, line1, ObjField::LineStartX, var_index),
                field_or_const(doc, line1, ObjField::LineStartY, var_index),
            ],
            b: [
                field_or_const(doc, line1, ObjField::LineEndX, var_index),
                field_or_const(doc, line1, ObjField::LineEndY, var_index),
            ],
            c: [
                field_or_const(doc, line2, ObjField::LineStartX, var_index),
                field_or_const(doc, line2, ObjField::LineStartY, var_index),
            ],
            d: [
                field_or_const(doc, line2, ObjField::LineEndX, var_index),
                field_or_const(doc, line2, ObjField::LineEndY, var_index),
            ],
        })
    }
}

impl ConstraintEquation for EqualLengthEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let ax = self.a[0].get(vars);
        let ay = self.a[1].get(vars);
        let bx = self.b[0].get(vars);
        let by = self.b[1].get(vars);
        let cx = self.c[0].get(vars);
        let cy = self.c[1].get(vars);
        let dx = self.d[0].get(vars);
        let dy = self.d[1].get(vars);
        let len1 = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
        let len2 = ((cx - dx).powi(2) + (cy - dy).powi(2)).sqrt();
        vec![len1 - len2]
    }
}

/// Symmetry constraint: point `q` is the mirror of point `p` across line `m`.
///
/// Residuals:
/// - `cross(m_dir, midpoint(p, q) - m_start) = 0`
/// - `dot(q - p, m_dir) = 0`
pub struct SymmetryEq {
    px: VarOrConst,
    py: VarOrConst,
    qx: VarOrConst,
    qy: VarOrConst,
    mx1: VarOrConst,
    my1: VarOrConst,
    mx2: VarOrConst,
    my2: VarOrConst,
}

impl SymmetryEq {
    pub fn from_inputs(
        doc: &crate::Document,
        point: ObjectId,
        mirror_point: ObjectId,
        mirror_line: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        let (GeoObject::Point(_), GeoObject::Point(_), GeoObject::Line(_)) = (
            doc.get_object(point)?,
            doc.get_object(mirror_point)?,
            doc.get_object(mirror_line)?,
        ) else {
            return None;
        };
        Some(Self {
            px: field_or_const(doc, point, ObjField::PointX, var_index),
            py: field_or_const(doc, point, ObjField::PointY, var_index),
            qx: field_or_const(doc, mirror_point, ObjField::PointX, var_index),
            qy: field_or_const(doc, mirror_point, ObjField::PointY, var_index),
            mx1: field_or_const(doc, mirror_line, ObjField::LineStartX, var_index),
            my1: field_or_const(doc, mirror_line, ObjField::LineStartY, var_index),
            mx2: field_or_const(doc, mirror_line, ObjField::LineEndX, var_index),
            my2: field_or_const(doc, mirror_line, ObjField::LineEndY, var_index),
        })
    }
}

impl ConstraintEquation for SymmetryEq {
    fn dimension(&self) -> usize {
        2
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        let px = self.px.get(vars);
        let py = self.py.get(vars);
        let qx = self.qx.get(vars);
        let qy = self.qy.get(vars);
        let mx1 = self.mx1.get(vars);
        let my1 = self.my1.get(vars);
        let mx2 = self.mx2.get(vars);
        let my2 = self.my2.get(vars);

        let mid_x = (px + qx) * 0.5;
        let mid_y = (py + qy) * 0.5;
        let dir_x = mx2 - mx1;
        let dir_y = my2 - my1;

        let cross = dir_x * (mid_y - my1) - dir_y * (mid_x - mx1);
        let dot = (qx - px) * dir_x + (qy - py) * dir_y;
        vec![cross, dot]
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
