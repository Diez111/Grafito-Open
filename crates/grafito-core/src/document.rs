use crate::{GeoObject, ObjectId};
use grafito_geometry::{Point2, ViewTransform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The main document containing all geometric objects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    objects: HashMap<ObjectId, GeoObject>,
    view: ViewTransform,
    #[serde(skip)]
    selection: Vec<ObjectId>,
    next_label_number: HashMap<String, usize>,
    pub variables: HashMap<String, f64>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(&mut self, obj: GeoObject) -> ObjectId {
        let id = obj.id();
        // Auto-label if empty
        let obj = if obj.label().is_empty() {
            let mut obj = obj;
            let name = obj.name();
            let n = self.next_label_number.entry(name.to_string()).or_insert(1);
            let label = format!("{}{}", name.chars().next().unwrap(), *n);
            *n += 1;
            match &mut obj {
                GeoObject::Point(o) => o.label = label,
                GeoObject::Line(o) => o.label = label,
                GeoObject::Circle(o) => o.label = label,
                GeoObject::Polygon(o) => o.label = label,
                GeoObject::Function(o) => o.label = label,
                GeoObject::Text(o) => o.label = label,
            }
            obj
        } else {
            obj
        };
        self.objects.insert(id, obj);
        id
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<GeoObject> {
        self.objects.remove(&id)
    }

    pub fn get_object(&self, id: ObjectId) -> Option<&GeoObject> {
        self.objects.get(&id)
    }

    pub fn get_object_mut(&mut self, id: ObjectId) -> Option<&mut GeoObject> {
        self.objects.get_mut(&id)
    }

    pub fn objects(&self) -> &HashMap<ObjectId, GeoObject> {
        &self.objects
    }

    pub fn objects_iter(&self) -> impl Iterator<Item = (&ObjectId, &GeoObject)> {
        self.objects.iter()
    }

    pub fn objects_iter_mut(&mut self) -> impl Iterator<Item = (&ObjectId, &mut GeoObject)> {
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
    pub fn pick_object(&self, world: Point2, tolerance: f64) -> Option<ObjectId> {
        for (id, obj) in &self.objects {
            if !obj.is_visible() {
                continue;
            }
            let hit = match obj {
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
                _ => false,
            };
            if hit {
                return Some(*id);
            }
        }
        None
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.selection.clear();
        self.next_label_number.clear();
        self.variables.clear();
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
