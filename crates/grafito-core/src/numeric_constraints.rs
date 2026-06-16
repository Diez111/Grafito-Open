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
    pub fn new(a_idx: VarIndex, b_idx: VarIndex, target: f64) -> Self {
        Self {
            a_idx,
            b_idx,
            target,
        }
    }

    pub fn from_inputs(
        _doc: &crate::Document,
        a: ObjectId,
        b: ObjectId,
        target: f64,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        let a_idx = *var_index.get(&(a, ObjField::PointX))?;
        let _ = *var_index.get(&(b, ObjField::PointX))?;
        Some(Self::new(a_idx, var_index[&(b, ObjField::PointX)], target))
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

    fn jacobian(&self, vars: &[f64]) -> Vec<(usize, usize, f64)> {
        if vars.is_empty() {
            return vec![
                (0, self.a_idx, 1.0),
                (0, self.a_idx + 1, 1.0),
                (0, self.b_idx, 1.0),
                (0, self.b_idx + 1, 1.0),
            ];
        }
        let ax = vars[self.a_idx];
        let ay = vars[self.a_idx + 1];
        let bx = vars[self.b_idx];
        let by = vars[self.b_idx + 1];
        let dx = ax - bx;
        let dy = ay - by;
        let d = (dx * dx + dy * dy).sqrt().max(1e-12);
        let inv = 1.0 / d;
        vec![
            (0, self.a_idx, dx * inv),
            (0, self.a_idx + 1, dy * inv),
            (0, self.b_idx, -dx * inv),
            (0, self.b_idx + 1, -dy * inv),
        ]
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
    pub fn new(
        line1_start: VarIndex,
        line1_end: VarIndex,
        line2_start: VarIndex,
        line2_end: VarIndex,
        target_deg: f64,
    ) -> Self {
        Self {
            line1_start,
            line1_end,
            line2_start,
            line2_end,
            target_rad: target_deg.to_radians(),
        }
    }

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
        Some(Self::new(
            line1_start,
            line1_end,
            line2_start,
            line2_end,
            target_deg,
        ))
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

    fn jacobian(&self, vars: &[f64]) -> Vec<(usize, usize, f64)> {
        if vars.is_empty() {
            return vec![
                (0, self.line1_start, 1.0),
                (0, self.line1_start + 1, 1.0),
                (0, self.line1_end, 1.0),
                (0, self.line1_end + 1, 1.0),
                (0, self.line2_start, 1.0),
                (0, self.line2_start + 1, 1.0),
                (0, self.line2_end, 1.0),
                (0, self.line2_end + 1, 1.0),
            ];
        }
        let s1 = Point2::new(vars[self.line1_start], vars[self.line1_start + 1]);
        let e1 = Point2::new(vars[self.line1_end], vars[self.line1_end + 1]);
        let s2 = Point2::new(vars[self.line2_start], vars[self.line2_start + 1]);
        let e2 = Point2::new(vars[self.line2_end], vars[self.line2_end + 1]);

        let (u, l1) = normalized_direction_with_length(s1, e1);
        let (v, l2) = normalized_direction_with_length(s2, e2);
        let l1 = l1.max(1e-12);
        let l2 = l2.max(1e-12);

        let cross = u.x * v.y - u.y * v.x;
        let dot = u.x * v.x + u.y * v.y;

        // Gradient of theta = atan2(cross, dot) with respect to the unit vectors.
        // Since cross^2 + dot^2 == 1 for unit vectors, the denominator is 1.
        let dtheta_du_x = dot * v.y - cross * v.x;
        let dtheta_du_y = -dot * v.x - cross * v.y;
        let dtheta_dv_x = -dot * u.y - cross * u.x;
        let dtheta_dv_y = dot * u.x - cross * u.y;

        let dx1 = e1.x - s1.x;
        let dy1 = e1.y - s1.y;
        let il1_3 = 1.0 / (l1 * l1 * l1);

        let du_x_dx1 = -dy1 * dy1 * il1_3;
        let du_x_dy1 = dx1 * dy1 * il1_3;
        let du_x_dx2 = dy1 * dy1 * il1_3;
        let du_x_dy2 = -dx1 * dy1 * il1_3;
        let du_y_dx1 = dx1 * dy1 * il1_3;
        let du_y_dy1 = -dx1 * dx1 * il1_3;
        let du_y_dx2 = -dx1 * dy1 * il1_3;
        let du_y_dy2 = dx1 * dx1 * il1_3;

        let dx2 = e2.x - s2.x;
        let dy2 = e2.y - s2.y;
        let il2_3 = 1.0 / (l2 * l2 * l2);

        let dv_x_dx3 = -dy2 * dy2 * il2_3;
        let dv_x_dy3 = dx2 * dy2 * il2_3;
        let dv_x_dx4 = dy2 * dy2 * il2_3;
        let dv_x_dy4 = -dx2 * dy2 * il2_3;
        let dv_y_dx3 = dx2 * dy2 * il2_3;
        let dv_y_dy3 = -dx2 * dx2 * il2_3;
        let dv_y_dx4 = -dx2 * dy2 * il2_3;
        let dv_y_dy4 = dx2 * dx2 * il2_3;

        // Chain rule: dtheta/dp = dtheta/du * du/dp + dtheta/dv * dv/dp.
        let ds1x = dtheta_du_x * du_x_dx1 + dtheta_du_y * du_y_dx1;
        let ds1y = dtheta_du_x * du_x_dy1 + dtheta_du_y * du_y_dy1;
        let de1x = dtheta_du_x * du_x_dx2 + dtheta_du_y * du_y_dx2;
        let de1y = dtheta_du_x * du_x_dy2 + dtheta_du_y * du_y_dy2;
        let ds2x = dtheta_dv_x * dv_x_dx3 + dtheta_dv_y * dv_y_dx3;
        let ds2y = dtheta_dv_x * dv_x_dy3 + dtheta_dv_y * dv_y_dy3;
        let de2x = dtheta_dv_x * dv_x_dx4 + dtheta_dv_y * dv_y_dx4;
        let de2y = dtheta_dv_x * dv_x_dy4 + dtheta_dv_y * dv_y_dy4;

        vec![
            (0, self.line1_start, ds1x),
            (0, self.line1_start + 1, ds1y),
            (0, self.line1_end, de1x),
            (0, self.line1_end + 1, de1y),
            (0, self.line2_start, ds2x),
            (0, self.line2_start + 1, ds2y),
            (0, self.line2_end, de2x),
            (0, self.line2_end + 1, de2y),
        ]
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
    pub fn new(
        radius_idx: VarIndex,
        center: Point2,
        line_start: VarIndex,
        line_end: VarIndex,
    ) -> Self {
        Self {
            radius_idx,
            center,
            line_start,
            line_end,
        }
    }

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
        Some(Self::new(radius_idx, circle.center, line_start, line_end))
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

    fn jacobian(&self, vars: &[f64]) -> Vec<(usize, usize, f64)> {
        if vars.is_empty() {
            return vec![
                (0, self.radius_idx, 1.0),
                (0, self.line_start, 1.0),
                (0, self.line_start + 1, 1.0),
                (0, self.line_end, 1.0),
                (0, self.line_end + 1, 1.0),
            ];
        }
        let _r = vars[self.radius_idx];
        let start = Point2::new(vars[self.line_start], vars[self.line_start + 1]);
        let end = Point2::new(vars[self.line_end], vars[self.line_end + 1]);

        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt().max(1e-12);

        let n = dx * (start.y - self.center.y) - dy * (start.x - self.center.x);
        let signed = n / len;
        let sign = if signed >= 0.0 { 1.0 } else { -1.0 };
        let inv_len3 = 1.0 / (len2 * len);

        // d(signed)/dstart_x
        let dsx = ((-end.y + self.center.y) * len2 + n * dx) * inv_len3;
        // d(signed)/dstart_y
        let dsy = ((end.x - self.center.x) * len2 + n * dy) * inv_len3;
        // d(signed)/dend_x
        let dex = ((start.y - self.center.y) * len2 - n * dx) * inv_len3;
        // d(signed)/dend_y
        let dey = ((self.center.x - start.x) * len2 - n * dy) * inv_len3;

        let mut triples = Vec::with_capacity(5);
        if self.radius_idx < vars.len() {
            triples.push((0, self.radius_idx, -1.0));
        }
        if self.line_start + 1 < vars.len() {
            triples.push((0, self.line_start, sign * dsx));
            triples.push((0, self.line_start + 1, sign * dsy));
        }
        if self.line_end + 1 < vars.len() {
            triples.push((0, self.line_end, sign * dex));
            triples.push((0, self.line_end + 1, sign * dey));
        }
        triples
    }
}

/// Either a solver variable index or a constant fallback value.
#[derive(Clone, Copy)]
pub struct VarOrConst {
    idx: Option<VarIndex>,
    value: f64,
}

impl VarOrConst {
    pub fn new(idx: Option<VarIndex>, value: f64) -> Self {
        Self { idx, value }
    }

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
    pub fn new(ax: VarOrConst, ay: VarOrConst, bx: VarOrConst, by: VarOrConst) -> Self {
        Self { ax, ay, bx, by }
    }

    pub fn from_inputs(
        doc: &crate::Document,
        a: ObjectId,
        b: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        match (doc.get_object(a)?, doc.get_object(b)?) {
            (GeoObject::Point(_), GeoObject::Point(_)) => Some(Self::new(
                field_or_const(doc, a, ObjField::PointX, var_index),
                field_or_const(doc, a, ObjField::PointY, var_index),
                field_or_const(doc, b, ObjField::PointX, var_index),
                field_or_const(doc, b, ObjField::PointY, var_index),
            )),
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

    fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
        let mut triples = Vec::with_capacity(8);
        if let Some(idx) = self.ax.idx {
            triples.push((0, idx, 1.0));
        }
        if let Some(idx) = self.ay.idx {
            triples.push((1, idx, 1.0));
        }
        if let Some(idx) = self.bx.idx {
            triples.push((0, idx, -1.0));
        }
        if let Some(idx) = self.by.idx {
            triples.push((1, idx, -1.0));
        }
        triples
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
    pub fn new(sy: VarOrConst, ey: VarOrConst) -> Self {
        Self { sy, ey }
    }

    pub fn from_inputs(
        doc: &crate::Document,
        line: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        if !matches!(doc.get_object(line)?, GeoObject::Line(_)) {
            return None;
        }
        Some(Self::new(
            field_or_const(doc, line, ObjField::LineStartY, var_index),
            field_or_const(doc, line, ObjField::LineEndY, var_index),
        ))
    }
}

impl ConstraintEquation for HorizontalEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        vec![self.sy.get(vars) - self.ey.get(vars)]
    }

    fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
        let mut triples = Vec::with_capacity(2);
        if let Some(idx) = self.sy.idx {
            triples.push((0, idx, 1.0));
        }
        if let Some(idx) = self.ey.idx {
            triples.push((0, idx, -1.0));
        }
        triples
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
    pub fn new(sx: VarOrConst, ex: VarOrConst) -> Self {
        Self { sx, ex }
    }

    pub fn from_inputs(
        doc: &crate::Document,
        line: ObjectId,
        var_index: &HashMap<(ObjectId, ObjField), VarIndex>,
    ) -> Option<Self> {
        if !matches!(doc.get_object(line)?, GeoObject::Line(_)) {
            return None;
        }
        Some(Self::new(
            field_or_const(doc, line, ObjField::LineStartX, var_index),
            field_or_const(doc, line, ObjField::LineEndX, var_index),
        ))
    }
}

impl ConstraintEquation for VerticalEq {
    fn dimension(&self) -> usize {
        1
    }

    fn residual(&self, vars: &[f64]) -> Vec<f64> {
        vec![self.sx.get(vars) - self.ex.get(vars)]
    }

    fn jacobian(&self, _vars: &[f64]) -> Vec<(usize, usize, f64)> {
        let mut triples = Vec::with_capacity(2);
        if let Some(idx) = self.sx.idx {
            triples.push((0, idx, 1.0));
        }
        if let Some(idx) = self.ex.idx {
            triples.push((0, idx, -1.0));
        }
        triples
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
    pub fn new(
        a: [VarOrConst; 2],
        b: [VarOrConst; 2],
        c: [VarOrConst; 2],
        d: [VarOrConst; 2],
    ) -> Self {
        Self { a, b, c, d }
    }

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
        Some(Self::new(
            [
                field_or_const(doc, line1, ObjField::LineStartX, var_index),
                field_or_const(doc, line1, ObjField::LineStartY, var_index),
            ],
            [
                field_or_const(doc, line1, ObjField::LineEndX, var_index),
                field_or_const(doc, line1, ObjField::LineEndY, var_index),
            ],
            [
                field_or_const(doc, line2, ObjField::LineStartX, var_index),
                field_or_const(doc, line2, ObjField::LineStartY, var_index),
            ],
            [
                field_or_const(doc, line2, ObjField::LineEndX, var_index),
                field_or_const(doc, line2, ObjField::LineEndY, var_index),
            ],
        ))
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

    fn jacobian(&self, vars: &[f64]) -> Vec<(usize, usize, f64)> {
        if vars.is_empty() {
            let mut triples = Vec::with_capacity(8);
            for vc in self
                .a
                .iter()
                .chain(self.b.iter())
                .chain(self.c.iter())
                .chain(self.d.iter())
            {
                if let Some(idx) = vc.idx {
                    triples.push((0, idx, 1.0));
                }
            }
            return triples;
        }
        let ax = self.a[0].get(vars);
        let ay = self.a[1].get(vars);
        let bx = self.b[0].get(vars);
        let by = self.b[1].get(vars);
        let cx = self.c[0].get(vars);
        let cy = self.c[1].get(vars);
        let dx = self.d[0].get(vars);
        let dy = self.d[1].get(vars);

        let u_x = ax - bx;
        let u_y = ay - by;
        let len1 = (u_x * u_x + u_y * u_y).sqrt();
        let w_x = cx - dx;
        let w_y = cy - dy;
        let len2 = (w_x * w_x + w_y * w_y).sqrt().max(1e-12);

        let il1 = 1.0 / len1.max(1e-12);
        let il2 = 1.0 / len2;

        let mut triples = Vec::with_capacity(8);
        if let Some(idx) = self.a[0].idx {
            triples.push((0, idx, u_x * il1));
        }
        if let Some(idx) = self.a[1].idx {
            triples.push((0, idx, u_y * il1));
        }
        if let Some(idx) = self.b[0].idx {
            triples.push((0, idx, -u_x * il1));
        }
        if let Some(idx) = self.b[1].idx {
            triples.push((0, idx, -u_y * il1));
        }
        if let Some(idx) = self.c[0].idx {
            triples.push((0, idx, -w_x * il2));
        }
        if let Some(idx) = self.c[1].idx {
            triples.push((0, idx, -w_y * il2));
        }
        if let Some(idx) = self.d[0].idx {
            triples.push((0, idx, w_x * il2));
        }
        if let Some(idx) = self.d[1].idx {
            triples.push((0, idx, w_y * il2));
        }
        triples
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        px: VarOrConst,
        py: VarOrConst,
        qx: VarOrConst,
        qy: VarOrConst,
        mx1: VarOrConst,
        my1: VarOrConst,
        mx2: VarOrConst,
        my2: VarOrConst,
    ) -> Self {
        Self {
            px,
            py,
            qx,
            qy,
            mx1,
            my1,
            mx2,
            my2,
        }
    }

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
        Some(Self::new(
            field_or_const(doc, point, ObjField::PointX, var_index),
            field_or_const(doc, point, ObjField::PointY, var_index),
            field_or_const(doc, mirror_point, ObjField::PointX, var_index),
            field_or_const(doc, mirror_point, ObjField::PointY, var_index),
            field_or_const(doc, mirror_line, ObjField::LineStartX, var_index),
            field_or_const(doc, mirror_line, ObjField::LineStartY, var_index),
            field_or_const(doc, mirror_line, ObjField::LineEndX, var_index),
            field_or_const(doc, mirror_line, ObjField::LineEndY, var_index),
        ))
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

    fn jacobian(&self, vars: &[f64]) -> Vec<(usize, usize, f64)> {
        if vars.is_empty() {
            let mut triples = Vec::with_capacity(16);
            for vc in [
                &self.px, &self.py, &self.qx, &self.qy, &self.mx1, &self.my1, &self.mx2, &self.my2,
            ] {
                if let Some(idx) = vc.idx {
                    triples.push((0, idx, 1.0));
                    triples.push((1, idx, 1.0));
                }
            }
            return triples;
        }
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

        // Row 0: cross = dir_x * (mid_y - my1) - dir_y * (mid_x - mx1)
        // Row 1: dot   = (qx - px) * dir_x + (qy - py) * dir_y
        let mut triples = Vec::with_capacity(16);

        // r0 wrt px -> -dir_y * 0.5
        if let Some(idx) = self.px.idx {
            triples.push((0, idx, -dir_y * 0.5));
        }
        // r0 wrt py -> dir_x * 0.5
        if let Some(idx) = self.py.idx {
            triples.push((0, idx, dir_x * 0.5));
        }
        // r0 wrt qx -> -dir_y * 0.5
        if let Some(idx) = self.qx.idx {
            triples.push((0, idx, -dir_y * 0.5));
        }
        // r0 wrt qy -> dir_x * 0.5
        if let Some(idx) = self.qy.idx {
            triples.push((0, idx, dir_x * 0.5));
        }
        // r0 wrt mx1 -> my2 - mid_y
        if let Some(idx) = self.mx1.idx {
            triples.push((0, idx, my2 - mid_y));
        }
        // r0 wrt my1 -> mid_x - mx2
        if let Some(idx) = self.my1.idx {
            triples.push((0, idx, mid_x - mx2));
        }
        // r0 wrt mx2 -> mid_y - my1
        if let Some(idx) = self.mx2.idx {
            triples.push((0, idx, mid_y - my1));
        }
        // r0 wrt my2 -> mx1 - mid_x
        if let Some(idx) = self.my2.idx {
            triples.push((0, idx, mx1 - mid_x));
        }

        // r1 wrt px -> -dir_x
        if let Some(idx) = self.px.idx {
            triples.push((1, idx, -dir_x));
        }
        // r1 wrt py -> -dir_y
        if let Some(idx) = self.py.idx {
            triples.push((1, idx, -dir_y));
        }
        // r1 wrt qx -> dir_x
        if let Some(idx) = self.qx.idx {
            triples.push((1, idx, dir_x));
        }
        // r1 wrt qy -> dir_y
        if let Some(idx) = self.qy.idx {
            triples.push((1, idx, dir_y));
        }
        // r1 wrt mx1 -> px - qx
        if let Some(idx) = self.mx1.idx {
            triples.push((1, idx, px - qx));
        }
        // r1 wrt my1 -> py - qy
        if let Some(idx) = self.my1.idx {
            triples.push((1, idx, py - qy));
        }
        // r1 wrt mx2 -> qx - px
        if let Some(idx) = self.mx2.idx {
            triples.push((1, idx, qx - px));
        }
        // r1 wrt my2 -> qy - py
        if let Some(idx) = self.my2.idx {
            triples.push((1, idx, qy - py));
        }

        triples
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
    normalized_direction_with_length(start, end).0
}

fn normalized_direction_with_length(start: Point2, end: Point2) -> (Point2, f64) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        (Point2::new(1.0, 0.0), len)
    } else {
        (Point2::new(dx / len, dy / len), len)
    }
}
