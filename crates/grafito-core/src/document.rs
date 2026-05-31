use crate::{GeoObject, ObjectId, RelationOperator};
use grafito_geometry::{Point2, ViewTransform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn to_subscript(n: usize) -> String {
    let s = n.to_string();
    s.chars().map(|c| match c {
        '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
        '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
        _ => c
    }).collect()
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
    pub spreadsheet: Vec<Vec<String>>,
    #[serde(skip)]
    pub spatial: crate::spatial::SpatialIndex,
    #[serde(skip)]
    pub spatial_dirty: bool,
    pub complex_base_symbol: String,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            objects: HashMap::new(),
            view: ViewTransform::default(),
            selection: Vec::new(),
            next_label_number: HashMap::new(),
            variables: HashMap::new(),
            spreadsheet: Vec::new(),
            spatial: crate::spatial::SpatialIndex::new(),
            spatial_dirty: true,
            complex_base_symbol: "z".to_string(),
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn migrate_complex_symbol(&mut self, new_symbol: &str) {
        let old = self.complex_base_symbol.clone();
        if old == new_symbol { return; }
        
        self.complex_base_symbol = new_symbol.to_string();
        
        let mut updates = Vec::new();
        for (id, obj) in &mut self.objects {
            let label = obj.label();
            if label.starts_with(&old) {
                // Determine if it's the exact old symbol or a subscript variant
                let rest = &label[old.len()..];
                let is_subscript = rest.is_empty() || rest.chars().all(|c| {
                    match c {
                        '₀'|'₁'|'₂'|'₃'|'₄'|'₅'|'₆'|'₇'|'₈'|'₉' => true,
                        _ => false
                    }
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
                GeoObject::ComplexGrid(_) | GeoObject::ComplexMapping(_) => self.complex_base_symbol.clone(),
                _ => name.chars().next().unwrap().to_string(),
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
            }
            obj
        } else {
            obj
        };
        self.objects.insert(id, obj);
        self.spatial_dirty = true;
        id
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<GeoObject> {
        self.spatial_dirty = true;
        self.objects.remove(&id)
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
                if !obj.is_visible() { continue; }
                if self.check_hit(obj, world, tolerance) { return Some(*id); }
            }
            return None;
        }

        for id in candidates {
            if let Some(obj) = self.objects.get(&id) {
                if !obj.is_visible() { continue; }
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
            GeoObject::Point(p) => p.position.distance(&world) <= tolerance.max(p.size as f64 / self.view.scale as f64),
            GeoObject::Line(l) => distance_point_to_segment(world, l.start, l.end) <= tolerance,
            GeoObject::Circle(c) => (c.center.distance(&world) - c.radius).abs() <= tolerance,
            GeoObject::Polygon(poly) => {
                if poly.vertices.len() >= 3 {
                    distance_point_to_polygon(world, &poly.vertices) <= tolerance
                } else {
                    false
                }
            }
            GeoObject::Function(f) => {
                // Evaluate function at world.x and check if world.y is close
                if let Ok(y) = grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), world.x)]) {
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
                world.x >= txt.position.x && world.x <= txt.position.x + width &&
                world.y >= txt.position.y - height && world.y <= txt.position.y
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
                    if let Ok(r) = grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)]) {
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
                    grafito_geometry::expr::evaluate(&ic.expr_lhs, &[("x".to_string(), world.x), ("y".to_string(), world.y)]),
                    grafito_geometry::expr::evaluate(&ic.expr_rhs, &[("x".to_string(), world.x), ("y".to_string(), world.y)]),
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
                if max_count <= 0.0 { return false; }
                let y_scale = (h.y_max - h.y_min) / max_count;
                for (left, right, count) in &bins {
                    let bar_height = h.y_min + count * y_scale;
                    if world.x >= *left && world.x <= *right && world.y >= h.y_min && world.y <= bar_height {
                        return true;
                    }
                }
                false
            }
            GeoObject::BoxPlot(bp) => {
                // Check if point is inside the box
                if let Some((_, q1, _, q3, _, _)) = grafito_geometry::statistics::boxplot_stats(&bp.data) {
                    let half_w = bp.width_box * 0.5;
                    world.x >= bp.position - half_w && world.x <= bp.position + half_w &&
                    world.y >= q1 && world.y <= q3
                } else {
                    false
                }
            }
            GeoObject::Fractal2D(fr) => {
                // Bounding box check
                world.x >= fr.x_min && world.x <= fr.x_max &&
                world.y >= fr.y_min && world.y <= fr.y_max
            }
            // 3D objects and complex objects - use bounding box or return false
            GeoObject::VectorField2D(_) | GeoObject::ComplexGrid(_) | GeoObject::ComplexMapping(_) => false,
            _ => false, // 3D objects require projection, skip for now
        }
    }

    pub fn rebuild_spatial_index(&mut self) {
        let mut items = Vec::new();
        for (id, obj) in &self.objects {
            if !obj.is_visible() { continue; }
            let (min_x, min_y, max_x, max_y) = match obj {
                GeoObject::Point(p) => (p.position.x - 0.1, p.position.y - 0.1, p.position.x + 0.1, p.position.y + 0.1),
                GeoObject::Line(l) => (
                    l.start.x.min(l.end.x), l.start.y.min(l.end.y),
                    l.start.x.max(l.end.x), l.start.y.max(l.end.y)
                ),
                GeoObject::Circle(c) => (
                    c.center.x - c.radius, c.center.y - c.radius,
                    c.center.x + c.radius, c.center.y + c.radius
                ),
                GeoObject::Polygon(poly) => {
                    let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                    for v in &poly.vertices {
                        min_x = min_x.min(v.x); min_y = min_y.min(v.y);
                        max_x = max_x.max(v.x); max_y = max_y.max(v.y);
                    }
                    if poly.vertices.is_empty() { continue; }
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
                        if let Ok(y) = grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)]) {
                            if y.is_finite() {
                                y_min = y_min.min(y);
                                y_max = y_max.max(y);
                            }
                        }
                    }
                    if y_min > y_max { continue; }
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Ellipse(el) => {
                    let max_r = el.rx.max(el.ry);
                    (el.center.x - max_r, el.center.y - max_r, el.center.x + max_r, el.center.y + max_r)
                }
                GeoObject::Parabola(pb) => {
                    // Approximate bounding box
                    let range = 10.0;
                    if pb.vertical {
                        (pb.vertex.x - range, pb.vertex.y, pb.vertex.x + range, pb.vertex.y + range)
                    } else {
                        (pb.vertex.x, pb.vertex.y - range, pb.vertex.x + range, pb.vertex.y + range)
                    }
                }
                GeoObject::Hyperbola(hb) => {
                    let range = hb.a.max(hb.b) * 3.0;
                    (hb.center.x - range, hb.center.y - range, hb.center.x + range, hb.center.y + range)
                }
                GeoObject::Text(txt) => {
                    let width = txt.content.len() as f64 * txt.font_size as f64 * 0.6;
                    let height = txt.font_size as f64;
                    (txt.position.x, txt.position.y - height, txt.position.x + width, txt.position.y)
                }
                GeoObject::ParametricCurve2D(pc) => {
                    // Sample curve to compute bounding box
                    let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (pc.t_max - pc.t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = pc.t_min + i as f64 * dt;
                        if let (Ok(x), Ok(y)) = (
                            grafito_geometry::expr::evaluate(&pc.expr_x, &[("t".to_string(), t)]),
                            grafito_geometry::expr::evaluate(&pc.expr_y, &[("t".to_string(), t)]),
                        ) {
                            if x.is_finite() && y.is_finite() {
                                min_x = min_x.min(x); min_y = min_y.min(y);
                                max_x = max_x.max(x); max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x { continue; }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::PolarCurve(pol) => {
                    // Sample curve to compute bounding box
                    let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                    let steps = 100;
                    let dt = (pol.t_max - pol.t_min) / steps as f64;
                    for i in 0..=steps {
                        let t = pol.t_min + i as f64 * dt;
                        if let Ok(r) = grafito_geometry::expr::evaluate(&pol.expr_r, &[("t".to_string(), t)]) {
                            if r.is_finite() {
                                let x = r * t.cos();
                                let y = r * t.sin();
                                min_x = min_x.min(x); min_y = min_y.min(y);
                                max_x = max_x.max(x); max_y = max_y.max(y);
                            }
                        }
                    }
                    if min_x > max_x { continue; }
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
                    if sp.xs.is_empty() || sp.ys.is_empty() { continue; }
                    let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                    for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                        min_x = min_x.min(*x); min_y = min_y.min(*y);
                        max_x = max_x.max(*x); max_y = max_y.max(*y);
                    }
                    (min_x, min_y, max_x, max_y)
                }
                GeoObject::RegressionLine(rl) => {
                    if rl.xs.is_empty() { continue; }
                    let x_min = rl.xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = rl.xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let y1 = rl.slope * x_min + rl.intercept;
                    let y2 = rl.slope * x_max + rl.intercept;
                    let y_min = y1.min(y2);
                    let y_max = y1.max(y2);
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::Histogram(h) => {
                    if h.data.is_empty() { continue; }
                    let x_min = h.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let x_max = h.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    (x_min, 0.0, x_max, h.data.len() as f64)
                }
                GeoObject::BoxPlot(bp) => {
                    if bp.data.is_empty() { continue; }
                    let y_min = bp.data.iter().cloned().fold(f64::INFINITY, f64::min);
                    let y_max = bp.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let half_w = bp.width_box * 0.5;
                    (bp.position - half_w, y_min, bp.position + half_w, y_max)
                }
                GeoObject::Fractal2D(fr) => {
                    (fr.x_min, fr.y_min, fr.x_max, fr.y_max)
                }
                GeoObject::VectorField2D(_vf) => {
                    // Use view bounds as approximation
                    let view = &self.view;
                    let x_min = -10.0 / view.scale;
                    let x_max = 10.0 / view.scale;
                    let y_min = -10.0 / view.scale;
                    let y_max = 10.0 / view.scale;
                    (x_min, y_min, x_max, y_max)
                }
                GeoObject::ComplexGrid(cg) => {
                    (cg.x_min, cg.y_min, cg.x_max, cg.y_max)
                }
                GeoObject::ComplexMapping(_) => {
                    // ComplexMapping doesn't have its own bounds, skip
                    continue;
                }
                // 3D objects are not indexed in 2D spatial index
                GeoObject::Point3D(_) | GeoObject::Segment3D(_) | GeoObject::Sphere3D(_) |
                GeoObject::Cube3D(_) | GeoObject::Pyramid3D(_) | GeoObject::Cone3D(_) |
                GeoObject::Cylinder3D(_) | GeoObject::Torus3D(_) | GeoObject::MoebiusStrip(_) | 
                GeoObject::Surface3D(_) | GeoObject::ParametricCurve3D(_) |
                GeoObject::Attractor3D(_) | GeoObject::HyperSurface4D(_) | GeoObject::VectorField3D(_) => {
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
        self.spreadsheet.clear();
        self.spatial = crate::spatial::SpatialIndex::new();
        self.spatial_dirty = true;
    }

    pub fn set_variable(&mut self, name: String, value: f64) {
        self.variables.insert(name, value);
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

    pub fn set_spreadsheet_cell(&mut self, row: usize, col: usize, value: String) {
        while self.spreadsheet.len() <= row { self.spreadsheet.push(Vec::new()); }
        while self.spreadsheet[row].len() <= col { self.spreadsheet[row].push(String::new()); }
        self.spreadsheet[row][col] = value;
    }

    pub fn eval_spreadsheet_cell(&self, row: usize, col: usize) -> Option<f64> {
        if row >= self.spreadsheet.len() || col >= self.spreadsheet[row].len() { return None; }
        let expr = &self.spreadsheet[row][col];
        if expr.is_empty() { return None; }
        grafito_geometry::expr::evaluate(expr, &self.variables.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()).ok()
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

fn distance_point_to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let ab2 = abx * abx + aby * aby;
    if ab2 == 0.0 {
        return p.distance(&a);
    }
    let t = ((apx * abx + apy * aby) / ab2).clamp(0.0, 1.0);
    let closest = Point2::new(a.x + t * abx, a.y + t * aby);
    p.distance(&closest)
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
