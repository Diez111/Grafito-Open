use crate::constraints::ConstraintGraph;
use crate::{GeoObject, LineKind, ObjectId, RelationOperator};
use grafito_geometry::expr::evaluate;
use grafito_geometry::{distance_point_to_segment, Point2, Point3D, ViewTransform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableMeta {
    pub position: Point2,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub visible: bool,
}

fn to_subscript(n: usize) -> String {
    let s = n.to_string();
    s.chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            _ => c,
        })
        .collect()
}

/// The main document containing all geometric objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    objects: HashMap<ObjectId, GeoObject>,
    view: ViewTransform,
    #[serde(skip)]
    selection: Vec<ObjectId>,
    next_label_number: HashMap<String, usize>,
    pub variables: HashMap<String, f64>,
    pub variable_meta: HashMap<String, VariableMeta>,
    pub spreadsheet: Vec<Vec<String>>,
    #[serde(skip)]
    pub spatial: crate::spatial::SpatialIndex,
    #[serde(skip)]
    pub spatial_dirty: bool,
    pub complex_base_symbol: String,
    #[serde(default)]
    pub constraints: ConstraintGraph,
    #[serde(skip)]
    pub render_quality: crate::RenderQuality,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            objects: HashMap::new(),
            view: ViewTransform::default(),
            selection: Vec::new(),
            next_label_number: HashMap::new(),
            variables: HashMap::new(),
            variable_meta: HashMap::new(),
            spreadsheet: Vec::new(),
            spatial: crate::spatial::SpatialIndex::new(),
            spatial_dirty: true,
            complex_base_symbol: "z".to_string(),
            constraints: ConstraintGraph::new(),
            render_quality: crate::RenderQuality::default(),
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn migrate_complex_symbol(&mut self, new_symbol: &str) {
        let old = self.complex_base_symbol.clone();
        if old == new_symbol {
            return;
        }

        self.complex_base_symbol = new_symbol.to_string();

        let mut updates = Vec::new();
        for (id, obj) in &mut self.objects {
            let label = obj.label();
            if label.starts_with(&old) {
                // Determine if it's the exact old symbol or a subscript variant
                let rest = &label[old.len()..];
                let is_subscript = rest.is_empty()
                    && rest.chars().all(|c| {
                        matches!(c, '₀' | '₁' | '₂' | '₃' | '₄' | '₅' | '₆' | '₇' | '₈' | '₉')
                    });
                if is_subscript {
                    let new_label = format!("{}{}", new_symbol, rest);
                    updates.push((*id, new_label));
                }
            }
        }

        for (id, new_label) in updates {
            if let Some(obj) = self.objects.get_mut(&id) {
                match obj {
                    GeoObject::ComplexGrid(o) => o.label = new_label,
                    GeoObject::ComplexMapping(o) => o.label = new_label,
                    _ => {} // We could update other objects if they were explicitly using the complex name
                }
            }
        }
    }

    pub fn add_object(&mut self, obj: GeoObject) -> ObjectId {
        let id = obj.id();
        // Auto-label if empty
        let obj = if obj.label().is_empty() {
            let mut obj = obj;
            let name = obj.name();
            let base_name = match &obj {
                GeoObject::ComplexGrid(_) | GeoObject::ComplexMapping(_) => {
                    self.complex_base_symbol.clone()
                }
                _ => name
                    .chars()
                    .next()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".to_string()),
            };
            let n = self.next_label_number.entry(base_name.clone()).or_insert(1);
            let label = if *n == 1 {
                base_name
            } else {
                format!("{}{}", base_name, to_subscript(*n - 1))
            };
            *n += 1;
            match &mut obj {
                GeoObject::Point(o) => o.label = label,
                GeoObject::Line(o) => o.label = label,
                GeoObject::Circle(o) => o.label = label,
                GeoObject::Polygon(o) => o.label = label,
                GeoObject::Function(o) => o.label = label,
                GeoObject::Text(o) => o.label = label,
                GeoObject::Ellipse(o) => o.label = label,
                GeoObject::Parabola(o) => o.label = label,
                GeoObject::Hyperbola(o) => o.label = label,
                GeoObject::Point3D(o) => o.label = label,
                GeoObject::Segment3D(o) => o.label = label,
                GeoObject::Sphere3D(o) => o.label = label,
                GeoObject::Cube3D(o) => o.label = label,
                GeoObject::Pyramid3D(o) => o.label = label,
                GeoObject::Cone3D(o) => o.label = label,
                GeoObject::Cylinder3D(o) => o.label = label,
                GeoObject::Surface3D(o) => o.label = label,
                GeoObject::ParametricCurve2D(o) => o.label = label,
                GeoObject::ParametricCurve3D(o) => o.label = label,
                GeoObject::PolarCurve(o) => o.label = label,
                GeoObject::VectorField2D(o) => o.label = label,
                GeoObject::ComplexGrid(o) => o.label = label,
                GeoObject::ComplexMapping(o) => o.label = label,
                GeoObject::ImplicitCurve(o) => o.label = label,
                GeoObject::Attractor3D(o) => o.label = label,
                GeoObject::Fractal2D(o) => o.label = label,
                GeoObject::HyperSurface4D(o) => o.label = label,
                GeoObject::VectorField3D(o) => o.label = label,
                GeoObject::Histogram(o) => o.label = label,
                GeoObject::ScatterPlot(o) => o.label = label,
                GeoObject::BoxPlot(o) => o.label = label,
                GeoObject::RegressionLine(o) => o.label = label,
                GeoObject::Torus3D(o) => o.label = label,
                GeoObject::MoebiusStrip(o) => o.label = label,
                GeoObject::PhasePortrait(o) => o.label = label,
            }
            obj
        } else {
            obj
        };
        self.objects.insert(id, obj);
        self.constraints.add_free_object(id);
        self.spatial_dirty = true;
        id
    }

    pub fn add_constructed_object(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
    ) -> (ObjectId, usize) {
        self.add_constructed_object_with_params(obj, constraint_name, inputs, HashMap::new())
    }

    pub fn add_constructed_object_with_params(
        &mut self,
        obj: GeoObject,
        constraint_name: &str,
        inputs: &[ObjectId],
        params: HashMap<String, f64>,
    ) -> (ObjectId, usize) {
        let id = self.add_object(obj);
        let cons_id =
            self.constraints
                .add_constraint(constraint_name, inputs.to_vec(), vec![id], params);
        (id, cons_id)
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<GeoObject> {
        self.constraints.remove_object(id);
        self.spatial_dirty = true;
        self.objects.remove(&id)
    }

    /// Move a free point and return IDs of all affected objects (via constraint propagation).
    /// The caller is responsible for re-evaluating the constraints in dependency order.
    pub fn move_point(&mut self, id: ObjectId, new_pos: Point2) -> Vec<ObjectId> {
        if !self.constraints.is_free(&id) {
            return vec![];
        }
        let mut affected = vec![id];
        if let Some(GeoObject::Point(p)) = self.get_object_mut(id) {
            p.position = new_pos;
        }
        // Collect all objects downstream of this one
        let constraint_order = self.constraints.get_update_order(&[id]);
        for cons_id in constraint_order {
            if let Some(cons) = self.constraints.get_constraint(cons_id) {
                affected.extend(cons.outputs.iter().cloned());
            }
        }
        affected
    }

    /// Move a free 3D point and return IDs of all affected objects.
    pub fn move_point3d(
        &mut self,
        id: ObjectId,
        new_pos: grafito_geometry::Point3D,
    ) -> Vec<ObjectId> {
        if !self.constraints.is_free(&id) {
            return vec![];
        }
        let mut affected = vec![id];
        if let Some(GeoObject::Point3D(p)) = self.get_object_mut(id) {
            p.position = new_pos;
        }
        let constraint_order = self.constraints.get_update_order(&[id]);
        for cons_id in constraint_order {
            if let Some(cons) = self.constraints.get_constraint(cons_id) {
                affected.extend(cons.outputs.iter().cloned());
            }
        }
        affected
    }

    /// Get the update order for re-evaluating dependent objects when these IDs change.
    pub fn propagation_order(&self, changed: &[ObjectId]) -> Vec<usize> {
        self.constraints.get_update_order(changed)
    }

    pub fn is_free_object(&self, id: &ObjectId) -> bool {
        self.constraints.is_free(id)
    }

    pub fn creator_of(&self, id: &ObjectId) -> Option<&crate::constraints::Constraint> {
        self.constraints.creator_of(id)
    }

    pub fn re_evaluate_constraints(&mut self, order: &[usize]) {
        for cons_id in order {
            if let Some(cons) = self.constraints.get_constraint(*cons_id).cloned() {
                match cons.name.as_str() {
                    "Midpoint" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                        let a = self.get_object(cons.inputs[0]).cloned();
                        let b = self.get_object(cons.inputs[1]).cloned();
                        if let (Some(GeoObject::Point(a)), Some(GeoObject::Point(b))) = (&a, &b) {
                            if let Some(GeoObject::Point(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.position = grafito_geometry::Point2::new(
                                    (a.position.x + b.position.x) * 0.5,
                                    (a.position.y + b.position.y) * 0.5,
                                );
                            }
                        }
                    }
                    "Translate" if !cons.inputs.is_empty() && !cons.outputs.is_empty() => {
                        let obj = self.get_object(cons.inputs[0]).cloned();
                        let dx = cons.params.get("dx").copied().unwrap_or(0.0);
                        let dy = cons.params.get("dy").copied().unwrap_or(0.0);
                        if let Some(GeoObject::Point(p)) = &obj {
                            if let Some(GeoObject::Point(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.position = grafito_geometry::Point2::new(
                                    p.position.x + dx,
                                    p.position.y + dy,
                                );
                            }
                        }
                    }
                    "Rotate" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                        let obj = self.get_object(cons.inputs[0]).cloned();
                        let angle = cons.params.get("angle").copied().unwrap_or(0.0);
                        let angle_rad = angle.to_radians();
                        if let Some(GeoObject::Point(p)) = &obj {
                            if let Some(GeoObject::Point(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.position = grafito_geometry::Point2::new(
                                    p.position.x * angle_rad.cos() - p.position.y * angle_rad.sin(),
                                    p.position.x * angle_rad.sin() + p.position.y * angle_rad.cos(),
                                );
                            }
                        }
                    }
                    "Intersect" if cons.inputs.len() >= 2 => {
                        let a = self.get_object(cons.inputs[0]).cloned();
                        let b = self.get_object(cons.inputs[1]).cloned();
                        if let (Some(a), Some(b)) = (&a, &b) {
                            let pts = doc_intersect(a, b);
                            for (i, out_id) in cons.outputs.iter().enumerate() {
                                if let Some(GeoObject::Point(out)) = self.get_object_mut(*out_id) {
                                    if let Some(pt) = pts.get(i) {
                                        out.position = *pt;
                                    }
                                }
                            }
                        }
                    }
                    "Extrude" if !cons.inputs.is_empty() => {
                        let height = cons.params.get("height").copied().unwrap_or(0.0);
                        if height.abs() < 1e-12 {
                            return;
                        }
                        if let Some(GeoObject::Polygon(poly)) = self.get_object(cons.inputs[0]) {
                            let verts = poly.vertices.clone();
                            if verts.len() < 3 {
                                return;
                            }
                            let base_y = 0.0;
                            let top_y = height;
                            let mut seg_idx = 0;
                            for i in 0..verts.len() {
                                let v = verts[i];
                                let vn = verts[(i + 1) % verts.len()];
                                let b = Point3D::new(v.x, base_y, v.y);
                                let t = Point3D::new(v.x, top_y, v.y);
                                let bn = Point3D::new(vn.x, base_y, vn.y);
                                let tn = Point3D::new(vn.x, top_y, vn.y);
                                if seg_idx < cons.outputs.len() {
                                    if let Some(GeoObject::Segment3D(s)) =
                                        self.get_object_mut(cons.outputs[seg_idx])
                                    {
                                        s.a = b;
                                        s.b = t;
                                    }
                                }
                                seg_idx += 1;
                                if seg_idx < cons.outputs.len() {
                                    if let Some(GeoObject::Segment3D(s)) =
                                        self.get_object_mut(cons.outputs[seg_idx])
                                    {
                                        s.a = b;
                                        s.b = bn;
                                    }
                                }
                                seg_idx += 1;
                                if seg_idx < cons.outputs.len() {
                                    if let Some(GeoObject::Segment3D(s)) =
                                        self.get_object_mut(cons.outputs[seg_idx])
                                    {
                                        s.a = t;
                                        s.b = tn;
                                    }
                                }
                                seg_idx += 1;
                            }
                        }
                    }
                    "Perpendicular" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                        let line_obj = self.get_object(cons.inputs[0]).cloned();
                        let point_obj = self.get_object(cons.inputs[1]).cloned();
                        if let (Some(GeoObject::Line(line)), Some(GeoObject::Point(pt))) =
                            (&line_obj, &point_obj)
                        {
                            if let Some(GeoObject::Line(out)) = self.get_object_mut(cons.outputs[0])
                            {
                                let dx = line.end.x - line.start.x;
                                let dy = line.end.y - line.start.y;
                                out.start = Point2::new(pt.position.x - dy, pt.position.y + dx);
                                out.end = Point2::new(pt.position.x + dy, pt.position.y - dx);
                                out.kind = LineKind::Line;
                            }
                        }
                    }
                    "Parallel" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                        let line_obj = self.get_object(cons.inputs[0]).cloned();
                        let point_obj = self.get_object(cons.inputs[1]).cloned();
                        if let (Some(GeoObject::Line(line)), Some(GeoObject::Point(pt))) =
                            (&line_obj, &point_obj)
                        {
                            if let Some(GeoObject::Line(out)) = self.get_object_mut(cons.outputs[0])
                            {
                                let dx = line.end.x - line.start.x;
                                let dy = line.end.y - line.start.y;
                                out.start = Point2::new(pt.position.x - dx, pt.position.y - dy);
                                out.end = Point2::new(pt.position.x + dx, pt.position.y + dy);
                                out.kind = LineKind::Line;
                            }
                        }
                    }
                    "PointOnObject" if cons.inputs.len() >= 2 && !cons.outputs.is_empty() => {
                        let obj = self.get_object(cons.inputs[0]).cloned();
                        let point = self.get_object(cons.inputs[1]).cloned();
                        if let (Some(obj), Some(GeoObject::Point(pt))) = (&obj, &point) {
                            if let Some(GeoObject::Point(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.position = match obj {
                                    GeoObject::Line(l) => {
                                        project_point_to_line(pt.position, l.start, l.end)
                                    }
                                    GeoObject::Circle(c) => {
                                        project_point_to_circle(pt.position, c.center, c.radius)
                                    }
                                    GeoObject::Polygon(poly) => {
                                        project_point_to_polygon_edges(pt.position, &poly.vertices)
                                    }
                                    _ => pt.position,
                                };
                            }
                        }
                    }
                    "CircleByCenterRadius"
                        if !cons.inputs.is_empty() && !cons.outputs.is_empty() =>
                    {
                        let radius = cons.params.get("radius").copied().unwrap_or(1.0);
                        if let Some(GeoObject::Point(center)) =
                            self.get_object(cons.inputs[0]).cloned()
                        {
                            if let Some(GeoObject::Circle(out)) =
                                self.get_object_mut(cons.outputs[0])
                            {
                                out.center = center.position;
                                out.radius = radius;
                            }
                        }
                    }
                    "CircleByThreePoints" if cons.inputs.len() >= 3 && !cons.outputs.is_empty() => {
                        let a = self.get_object(cons.inputs[0]).cloned();
                        let b = self.get_object(cons.inputs[1]).cloned();
                        let c = self.get_object(cons.inputs[2]).cloned();
                        if let (
                            Some(GeoObject::Point(pa)),
                            Some(GeoObject::Point(pb)),
                            Some(GeoObject::Point(pc)),
                        ) = (&a, &b, &c)
                        {
                            if let Some((center, radius)) =
                                circle_from_three_points(pa.position, pb.position, pc.position)
                            {
                                if let Some(GeoObject::Circle(out)) =
                                    self.get_object_mut(cons.outputs[0])
                                {
                                    out.center = center;
                                    out.radius = radius;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn get_object(&self, id: ObjectId) -> Option<&GeoObject> {
        self.objects.get(&id)
    }

    pub fn get_object_mut(&mut self, id: ObjectId) -> Option<&mut GeoObject> {
        self.spatial_dirty = true;
        self.objects.get_mut(&id)
    }

    pub fn objects(&self) -> &HashMap<ObjectId, GeoObject> {
        &self.objects
    }

    pub fn objects_iter(&self) -> impl Iterator<Item = (&ObjectId, &GeoObject)> {
        self.objects.iter()
    }

    pub fn objects_iter_mut(&mut self) -> impl Iterator<Item = (&ObjectId, &mut GeoObject)> {
        self.spatial_dirty = true;
        self.objects.iter_mut()
    }

    pub fn view(&self) -> &ViewTransform {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut ViewTransform {
        &mut self.view
    }

    pub fn set_view(&mut self, view: ViewTransform) {
        self.view = view;
    }

    pub fn selection(&self) -> &[ObjectId] {
        &self.selection
    }

    pub fn select(&mut self, id: ObjectId) {
        if !self.selection.contains(&id) {
            self.selection.push(id);
        }
    }

    pub fn deselect(&mut self, id: ObjectId) {
        self.selection.retain(|&s| s != id);
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn is_selected(&self, id: ObjectId) -> bool {
        self.selection.contains(&id)
    }

    /// Find object near a screen point (in world coordinates).
    pub fn pick_object(&mut self, world: Point2, tolerance: f64) -> Option<ObjectId> {
        if self.spatial_dirty {
            self.rebuild_spatial_index();
        }
        let candidates = self.spatial.candidates(world.x, world.y, tolerance);
        if candidates.is_empty() {
            // Fallback for objects not in spatial index or if index is empty
            for (id, obj) in &self.objects {
                if !obj.is_visible() {
                    continue;
                }
                if self.check_hit(obj, world, tolerance) {
                    return Some(*id);
                }
            }
            return None;
        }

        for id in candidates {
            if let Some(obj) = self.objects.get(&id) {
                if !obj.is_visible() {
                    continue;
                }
                // Use precise check
                if self.check_hit(obj, world, tolerance) {
                    // For simplicity, just return the first hit or compute actual distance
                    return Some(id);
                }
            }
        }
        None
    }

    fn check_hit(&self, obj: &GeoObject, world: Point2, tolerance: f64) -> bool {
        match obj {
            GeoObject::Point(p) => {
                p.position.distance(&world) <= tolerance.max(p.size as f64 / self.view.scale)
            }
            GeoObject::Line(l) => l.distance_to_point(world) <= tolerance,
            GeoObject::Circle(c) => (c.center.distance(&world) - c.radius).abs() <= tolerance,
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                distance_point_to_polygon(world, &poly.vertices) <= tolerance
            }
            GeoObject::Function(f) => {
                // Evaluate function at world.x and check if world.y is close
                if let Ok(y) =
                    grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), world.x)])
                {
                    (world.y - y).abs() <= tolerance
                } else {
                    false
                }
            }
            GeoObject::Ellipse(el) => {
                // Check if point is near the ellipse boundary
                let dx = world.x - el.center.x;
                let dy = world.y - el.center.y;
                let cos_a = el.angle.cos();
                let sin_a = el.angle.sin();
                let rx = dx * cos_a + dy * sin_a;
                let ry = -dx * sin_a + dy * cos_a;
                let ellipse_eq = (rx / el.rx).powi(2) + (ry / el.ry).powi(2);
                (ellipse_eq - 1.0).abs() <= tolerance / el.rx.min(el.ry)
            }
            GeoObject::Parabola(pb) => {
                // Check if point is near the parabola
                let dx = world.x - pb.vertex.x;
                let dy = world.y - pb.vertex.y;
                let expected_y = if pb.vertical {
                    pb.vertex.y + dx * dx / (4.0 * pb.p)
                } else {
                    pb.vertex.x + dy * dy / (4.0 * pb.p)
                };
                if pb.vertical {
                    (world.y - expected_y).abs() <= tolerance
                } else {
                    (world.x - expected_y).abs() <= tolerance
                }
            }
            GeoObject::Hyperbola(hb) => {
                // Check if point is near the hyperbola
                let dx = world.x - hb.center.x;
                let dy = world.y - hb.center.y;
                let hyperbola_eq = if hb.horizontal {
                    (dx / hb.a).powi(2) - (dy / hb.b).powi(2)
                } else {
                    (dy / hb.a).powi(2) - (dx / hb.b).powi(2)
                };
                (hyperbola_eq - 1.0).abs() <= tolerance / hb.a.min(hb.b)
            }
            GeoObject::Text(txt) => {
                // Simple bounding box check
                let width = txt.content.len() as f64 * txt.font_size as f64 * 0.6;
                let height = txt.font_size as f64;
                world.x >= txt.position.x
                    && world.x <= txt.position.x + width
                    && world.y >= txt.position.y - height
                    && world.y <= txt.position.y
            }
            GeoObject::ParametricCurve2D(pc) => {
                // Sample the curve and check distance to segments
                let steps = 100;
                let dt = (pc.t_max - pc.t_min) / steps as f64;
                let mut prev_point: Option<Point2> = None;
                for i in 0..=steps {
                    let t = pc.t_min + i as f64 * dt;
                    if let (Ok(x), Ok(y)) = (
                        grafito_geometry::expr::evaluate(&pc.expr_x, &[("t".to_string(), t)]),
                        grafito_geometry::expr::evaluate(&pc.expr_y, &[("t".to_string(), t)]),
                    ) {
                        if x.is_finite() && y.is_finite() {
                            let curr_point = Point2::new(x, y);
                            if let Some(prev) = prev_point {
                                if distance_point_to_segment(world, prev, curr_point) <= tolerance {
                                    return true;
                                }
                            }
                            prev_point = Some(curr_point);
                        }
                    }
                }
                false
            }
            GeoObject::PolarCurve(pol) => {
                // Sample the curve and check distance to segments
                let steps = 100;
                let dt = (pol.t_max - pol.t_min) / steps as f64;
                let mut prev_point: Option<Point2> = None;
                for i in 0..=steps {
                    let t = pol.t_min + i as f64 * dt;
                    if let Ok(r) =
                        grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)])
                    {
                        if r.is_finite() {
                            let x = r * t.cos();
                            let y = r * t.sin();
                            let curr_point = Point2::new(x, y);
                            if let Some(prev) = prev_point {
                                if distance_point_to_segment(world, prev, curr_point) <= tolerance {
                                    return true;
                                }
                            }
                            prev_point = Some(curr_point);
                        }
                    }
                }
                false
            }
            GeoObject::ImplicitCurve(ic) => {
                // Evaluate both sides and check if close to the relation
                if let (Ok(lhs), Ok(rhs)) = (
                    grafito_geometry::expr::evaluate(
                        &ic.expr_lhs,
                        &[("x".to_string(), world.x), ("y".to_string(), world.y)],
                    ),
                    grafito_geometry::expr::evaluate(
                        &ic.expr_rhs,
                        &[("x".to_string(), world.x), ("y".to_string(), world.y)],
                    ),
                ) {
                    let diff = (lhs - rhs).abs();
                    match ic.operator {
                        RelationOperator::Eq => diff <= tolerance,
                        RelationOperator::Less => lhs < rhs + tolerance,
                        RelationOperator::Greater => lhs > rhs - tolerance,
                        RelationOperator::LessEq => lhs <= rhs + tolerance,
                        RelationOperator::GreaterEq => lhs >= rhs - tolerance,
                    }
                } else {
                    false
                }
            }
            GeoObject::ScatterPlot(sp) => {
                // Check distance to any point
                for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                    if Point2::new(*x, *y).distance(&world) <= tolerance {
                        return true;
                    }
                }
                false
            }
            GeoObject::RegressionLine(rl) => {
                // Check distance to the line y = slope * x + intercept
                let expected_y = rl.slope * world.x + rl.intercept;
                (world.y - expected_y).abs() <= tolerance
            }
            GeoObject::Histogram(h) => {
                // Check if point is inside any bar
                let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                if max_count <= 0.0 {
                    return false;
                }
                let y_scale = (h.y_max - h.y_min) / max_count;
                for (left, right, count) in &bins {
                    let bar_height = h.y_min + count * y_scale;
                    if world.x >= *left
                        && world.x <= *right
                        && world.y >= h.y_min
                        && world.y <= bar_height
                    {
                        return true;
                    }
                }
                false
            }
            GeoObject::BoxPlot(bp) => {
                // Check if point is inside the box
                if let Some((_, q1, _, q3, _, _)) =
                    grafito_geometry::statistics::boxplot_stats(&bp.data)
                {
                    let half_w = bp.width_box * 0.5;
                    world.x >= bp.position - half_w
                        && world.x <= bp.position + half_w
                        && world.y >= q1
                        && world.y <= q3
                } else {
                    false
                }
            }
            GeoObject::Fractal2D(fr) => {
                // Bounding box check
                world.x >= fr.x_min
                    && world.x <= fr.x_max
                    && world.y >= fr.y_min
                    && world.y <= fr.y_max
            }
            // 3D objects and complex objects - use bounding box or return false
            GeoObject::VectorField2D(_)
            | GeoObject::PhasePortrait(_)
            | GeoObject::ComplexGrid(_)
            | GeoObject::ComplexMapping(_) => false,
            _ => false, // 3D objects require projection, skip for now
        }
    }

    pub fn rebuild_spatial_index(&mut self) {
        let mut items = Vec::new();
        for (id, obj) in &self.objects {
            if !obj.is_visible() {
                continue;
            }
            let (min_x, min_y, max_x, max_y) = match obj {
                GeoObject::Point(p) => (
                    p.position.x - 0.1,
                    p.position.y - 0.1,
                    p.position.x + 0.1,
                    p.position.y + 0.1,
                ),
                GeoObject::Line(l) => (
                    l.start.x.min(l.end.x),
                    l.start.y.min(l.end.y),
                    l.start.x.max(l.end.x),
                    l.start.y.max(l.end.y),
                ),
                GeoObject::Circle(c) => (
                    c.center.x - c.radius,
                    c.center.y - c.radius,
                    c.center.x + c.radius,
                    c.center.y + c.radius,
                ),
                GeoObject::Polygon(poly) => {
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    for v in &poly.vertices {
                        min_x = min_x.min(v.x);
                        min_y = min_y.min(v.y);
                        max_x = max_x.max(v.x);
                        max_y = max_y.max(v.y);
                    }
                    if poly.vertices.is_empty() {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::Function(f) => {
                    let x_min = f.domain_min.unwrap_or(-10.0);
                    let x_max = f.domain_max.unwrap_or(10.0);
                    // Sample function to estimate y bounds
                    let mut y_min = f64::MAX;
                    let mut y_max = f64::MIN;
                    let steps = 50;
                    let dx = (x_max - x_min) / steps as f64;
                    for i in 0..=steps {
                        let x = x_min + i as f64 * dx;
                        if let Ok(y) =
                            grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                        {
                            if y.is_finite() {
                                y_min = y_min.min(y);
                                y_max = y_max.max(y);
                            }
                        }
                    }
                    if y_min > y_max {
                        continue;
                    }
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Ellipse(el) => {
                    let max_r = el.rx.max(el.ry);
                    (
                        el.center.x - max_r,
                        el.center.y - max_r,
                        el.center.x + max_r,
                        el.center.y + max_r,
                    )
                }
                GeoObject::Parabola(pb) => {
                    // Approximate bounding box
                    let range = 10.0;
                    if pb.vertical {
                        (
                            pb.vertex.x - range,
                            pb.vertex.y,
                            pb.vertex.x + range,
                            pb.vertex.y + range,
                        )
                    } else {
                        (
                            pb.vertex.x,
                            pb.vertex.y - range,
                            pb.vertex.x + range,
                            pb.vertex.y + range,
                        )
                    }
                }
                GeoObject::Hyperbola(hb) => {
                    let range = hb.a.max(hb.b) * 3.0;
                    (
                        hb.center.x - range,
                        hb.center.y - range,
                        hb.center.x + range,
                        hb.center.y + range,
                    )
                }
                GeoObject::Text(txt) => {
                    let width = txt.content.len() as f64 * txt.font_size as f64 * 0.6;
                    let height = txt.font_size as f64;
                    (
                        txt.position.x,
                        txt.position.y - height,
                        txt.position.x + width,
                        txt.position.y,
                    )
                }
                GeoObject::ParametricCurve2D(pc) => {
                    // Sample curve to compute bounding box
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (pc.t_max - pc.t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = pc.t_min + i as f64 * dt;
                        if let (Ok(x), Ok(y)) = (
                            grafito_geometry::expr::evaluate(&pc.expr_x, &[("t".to_string(), t)]),
                            grafito_geometry::expr::evaluate(&pc.expr_y, &[("t".to_string(), t)]),
                        ) {
                            if x.is_finite() && y.is_finite() {
                                min_x = min_x.min(x);
                                min_y = min_y.min(y);
                                max_x = max_x.max(x);
                                max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::PolarCurve(pol) => {
                    // Sample curve to compute bounding box
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (pol.t_max - pol.t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = pol.t_min + i as f64 * dt;
                        if let Ok(r) =
                            grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)])
                        {
                            if r.is_finite() {
                                let x = r * t.cos();
                                let y = r * t.sin();
                                min_x = min_x.min(x);
                                min_y = min_y.min(y);
                                max_x = max_x.max(x);
                                max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x {
                        continue;
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::ImplicitCurve(_ic) => {
                    // Use view bounds as approximation
                    let view = &self.view;
                    let x_min = -10.0 / view.scale;
                    let x_max = 10.0 / view.scale;
                    let y_min = -10.0 / view.scale;
                    let y_max = 10.0 / view.scale;
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::ScatterPlot(sp) => {
                    if sp.xs.is_empty() || sp.ys.is_empty() {
                        continue;
                    }
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;
                    for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(*x);
                        max_y = max_y.max(*y);
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::RegressionLine(rl) => {
                    if rl.xs.is_empty() {
                        continue;
                    }
                    let x_min = rl.xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = rl.xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let y1 = rl.slope * x_min + rl.intercept;
                    let y2 = rl.slope * x_max + rl.intercept;
                    let y_min = y1.min(y2);
                    let y_max = y1.max(y2);
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Histogram(h) => {
                    if h.data.is_empty() {
                        continue;
                    }
                    let x_min = h.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = h.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    (x_min, 0.0, x_max, h.data.len() as f64)
                }
                GeoObject::BoxPlot(bp) => {
                    if bp.data.is_empty() {
                        continue;
                    }
                    let y_min = bp.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let y_max = bp.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let half_w = bp.width_box * 0.5;
                    (bp.position - half_w, y_min, bp.position + half_w, y_max)
                }
                GeoObject::Fractal2D(fr) => (fr.x_min, fr.y_min, fr.x_max, fr.y_max),
                GeoObject::VectorField2D(_vf) => {
                    // Use view bounds as approximation
                    let view = &self.view;
                    let x_min = -10.0 / view.scale;
                    let x_max = 10.0 / view.scale;
                    let y_min = -10.0 / view.scale;
                    let y_max = 10.0 / view.scale;
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::ComplexGrid(cg) => (cg.x_min, cg.y_min, cg.x_max, cg.y_max),
                GeoObject::ComplexMapping(_) => {
                    // ComplexMapping doesn't have its own bounds, skip
                    continue;
                }
                GeoObject::PhasePortrait(pp) => (pp.x_min, pp.y_min, pp.x_max, pp.y_max),
                // 3D objects are not indexed in 2D spatial index
                GeoObject::Point3D(_)
                | GeoObject::Segment3D(_)
                | GeoObject::Sphere3D(_)
                | GeoObject::Cube3D(_)
                | GeoObject::Pyramid3D(_)
                | GeoObject::Cone3D(_)
                | GeoObject::Cylinder3D(_)
                | GeoObject::Torus3D(_)
                | GeoObject::MoebiusStrip(_)
                | GeoObject::Surface3D(_)
                | GeoObject::ParametricCurve3D(_)
                | GeoObject::Attractor3D(_)
                | GeoObject::HyperSurface4D(_)
                | GeoObject::VectorField3D(_) => {
                    continue;
                }
            };
            items.push((*id, min_x, min_y, max_x, max_y));
        }
        self.spatial.rebuild(items);
        self.spatial_dirty = false;
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.selection.clear();
        self.next_label_number.clear();
        self.variables.clear();
        self.variable_meta.clear();
        self.spreadsheet.clear();
        self.spatial = crate::spatial::SpatialIndex::new();
        self.spatial_dirty = true;
        self.constraints = ConstraintGraph::new();
    }

    pub fn recompute_bound_parameters(&mut self) {
        let vars: Vec<(String, f64)> = self
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for obj in self.objects.values_mut() {
            match obj {
                GeoObject::Point(p) => {
                    if let Some(expr) = &p.x_expr {
                        if let Ok(x) = evaluate(expr, &vars) {
                            p.position.x = x;
                        }
                    }
                    if let Some(expr) = &p.y_expr {
                        if let Ok(y) = evaluate(expr, &vars) {
                            p.position.y = y;
                        }
                    }
                }
                GeoObject::Circle(c) => {
                    if let Some(expr) = &c.radius_expr {
                        if let Ok(r) = evaluate(expr, &vars) {
                            c.radius = r;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn set_variable(&mut self, name: String, value: f64) {
        self.variables.insert(name, value);
        self.recompute_bound_parameters();
    }

    pub fn get_variable(&self, name: &str) -> Option<f64> {
        self.variables.get(name).copied()
    }

    pub fn variables(&self) -> &HashMap<String, f64> {
        &self.variables
    }

    pub fn get_spreadsheet_cell(&self, row: usize, col: usize) -> String {
        if row < self.spreadsheet.len() && col < self.spreadsheet[row].len() {
            self.spreadsheet[row][col].clone()
        } else {
            String::new()
        }
    }

    pub const MAX_SPREADSHEET_ROWS: usize = 1000;
    pub const MAX_SPREADSHEET_COLS: usize = 1000;

    pub fn set_spreadsheet_cell(
        &mut self,
        row: usize,
        col: usize,
        value: String,
    ) -> Result<(), String> {
        if row >= Self::MAX_SPREADSHEET_ROWS {
            return Err(format!(
                "row {} exceeds maximum {}",
                row,
                Self::MAX_SPREADSHEET_ROWS
            ));
        }
        if col >= Self::MAX_SPREADSHEET_COLS {
            return Err(format!(
                "col {} exceeds maximum {}",
                col,
                Self::MAX_SPREADSHEET_COLS
            ));
        }
        while self.spreadsheet.len() <= row {
            self.spreadsheet.push(Vec::new());
        }
        while self.spreadsheet[row].len() <= col {
            self.spreadsheet[row].push(String::new());
        }
        self.spreadsheet[row][col] = value;
        Ok(())
    }

    pub fn eval_spreadsheet_cell(&self, row: usize, col: usize) -> Option<f64> {
        if row >= self.spreadsheet.len() || col >= self.spreadsheet[row].len() {
            return None;
        }
        let expr = &self.spreadsheet[row][col];
        if expr.is_empty() {
            return None;
        }
        grafito_geometry::expr::evaluate(
            expr,
            &self
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
        )
        .ok()
    }

    pub fn spreadsheet_dim(&self) -> (usize, usize) {
        // Count only rows/cols that have actual non-empty content
        let mut max_row = 0_usize;
        let mut max_col = 0_usize;
        for (r, row) in self.spreadsheet.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if !cell.is_empty() {
                    max_row = max_row.max(r + 1);
                    max_col = max_col.max(c + 1);
                }
            }
        }
        // At least 3×3, plus 1 extra for expansion
        (max_row.max(3) + 1, max_col.max(3) + 1)
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}

fn distance_point_to_polygon(p: Point2, vertices: &[Point2]) -> f64 {
    if vertices.len() < 2 {
        return f64::INFINITY;
    }
    let mut min_dist = f64::INFINITY;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let d = distance_point_to_segment(p, a, b);
        if d < min_dist {
            min_dist = d;
        }
    }
    min_dist
}

fn project_point_to_line(p: Point2, a: Point2, b: Point2) -> Point2 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    Point2::new(a.x + t * dx, a.y + t * dy)
}

fn project_point_to_segment(p: Point2, a: Point2, b: Point2) -> Point2 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return a;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    Point2::new(a.x + t * dx, a.y + t * dy)
}

fn project_point_to_circle(p: Point2, center: Point2, radius: f64) -> Point2 {
    let dx = p.x - center.x;
    let dy = p.y - center.y;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 1e-12 {
        return Point2::new(center.x + radius, center.y);
    }
    Point2::new(center.x + radius * dx / d, center.y + radius * dy / d)
}

fn project_point_to_polygon_edges(p: Point2, vertices: &[Point2]) -> Point2 {
    if vertices.len() < 2 {
        return p;
    }
    let mut best = vertices[0];
    let mut best_dist = f64::INFINITY;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let proj = project_point_to_segment(p, a, b);
        let d = proj.distance(&p);
        if d < best_dist {
            best_dist = d;
            best = proj;
        }
    }
    best
}

fn circle_from_three_points(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = Point2::new(ux, uy);
    let radius = center.distance(&a);
    Some((center, radius))
}

fn doc_intersect(obj_a: &GeoObject, obj_b: &GeoObject) -> Vec<Point2> {
    use grafito_geometry::intersections::{self, IntersectionResult};

    match (obj_a, obj_b) {
        (GeoObject::Line(a), GeoObject::Line(b)) => {
            match intersections::line_line(a.start, a.end, b.start, b.end) {
                IntersectionResult::One(p) => {
                    let t_a = a.param_at_point(p);
                    let t_b = b.param_at_point(p);
                    if a.kind_contains_t(t_a) && b.kind_contains_t(t_b) {
                        vec![p]
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            }
        }
        (GeoObject::Line(l), GeoObject::Circle(c)) | (GeoObject::Circle(c), GeoObject::Line(l)) => {
            match intersections::line_circle(l.start, l.end, c.center, c.radius) {
                IntersectionResult::One(p) => {
                    if l.kind_contains_t(l.param_at_point(p)) {
                        vec![p]
                    } else {
                        vec![]
                    }
                }
                IntersectionResult::Two(p1, p2) => {
                    let mut pts = Vec::new();
                    for p in [p1, p2] {
                        if l.kind_contains_t(l.param_at_point(p)) {
                            pts.push(p);
                        }
                    }
                    pts
                }
                _ => vec![],
            }
        }
        (GeoObject::Circle(c1), GeoObject::Circle(c2)) => {
            match intersections::circle_circle(c1.center, c1.radius, c2.center, c2.radius) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                _ => vec![],
            }
        }
        (GeoObject::Line(l), GeoObject::Function(f))
        | (GeoObject::Function(f), GeoObject::Line(l)) => {
            let slope = if (l.end.x - l.start.x).abs() < 1e-12 {
                0.0
            } else {
                (l.end.y - l.start.y) / (l.end.x - l.start.x)
            };
            let intercept = l.start.y - slope * l.start.x;
            let x_min = f.domain_min.unwrap_or(-10.0);
            let x_max = f.domain_max.unwrap_or(10.0);
            intersections::function_line(&f.expr, slope, intercept, x_min, x_max)
                .into_iter()
                .filter(|p| l.kind_contains_t(l.param_at_point(*p)))
                .collect()
        }
        _ => vec![],
    }
}
