use grafito_core::{
    GeoObject,
    Point3DObj, Segment3DObj, Sphere3DObj, Cube3DObj,
    Pyramid3DObj,
};
use grafito_geometry::{Point3D, Camera3D, Color};
use grafito_geometry::expr::evaluate;
use grafito_ui::Tool;
use egui::{Vec2, Stroke, Color32, Rect};
use glam::Vec3;
use std::collections::HashMap;

use crate::GrafitoApp;

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

impl GrafitoApp {
    pub fn handle_3d_click(&mut self, _ui: &egui::Ui, _response: &egui::Response, _canvas: Rect, w: f32, h: f32) {
        let origin = self.camera.project(&Point3D::new(0.0, 0.0, 0.0), w, h);
        let u = self.camera.project(&Point3D::new(1.0, 0.0, 0.0), w, h);
        let approx_scale = if let (Some(o), Some(u)) = (origin, u) {
            1.0 / ((u.0 - o.0).powi(2) + (u.1 - o.1).powi(2)).sqrt().max(0.01) as f64
        } else { 0.5 };

        // Place objects at a point on the XZ plane (y=0), near camera target
        let t = self.camera.target;
        match self.current_tool {
            Tool::Point => {
                self.save_state();
                let pos = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document.add_object(GeoObject::Point3D(Point3DObj::new(pos)));
            }
            Tool::Line => {
                self.pending_points_3d.push(Point3D::new(t.x as f64, t.y as f64, t.z as f64));
                if self.pending_points_3d.len() == 2 {
                    let a = self.pending_points_3d[0];
                    let b = self.pending_points_3d[1];
                    self.save_state();
                    self.document.add_object(GeoObject::Segment3D(Segment3DObj::new(a, b)));
                    self.pending_points_3d.clear();
                }
            }
            Tool::Circle => {
                self.pending_points_3d.push(Point3D::new(t.x as f64, t.y as f64, t.z as f64));
                if self.pending_points_3d.len() == 2 {
                    let center = self.pending_points_3d[0];
                    let edge = self.pending_points_3d[1];
                    let radius = center.distance(&edge);
                    self.save_state();
                    self.document.add_object(GeoObject::Sphere3D(Sphere3DObj::new(center, radius)));
                    self.pending_points_3d.clear();
                }
            }
            Tool::Polygon => {
                // Cube via polygon tool
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document.add_object(GeoObject::Cube3D(Cube3DObj::new(c, approx_scale * 2.0)));
            }
            Tool::Function => {
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document.add_object(GeoObject::Pyramid3D(Pyramid3DObj::new(
                    Point3D::new(c.x, c.y - approx_scale, c.z), c, approx_scale * 2.0,
                )));
            }
            _ => {}
        }
    }

    pub fn draw_3d_grid(&self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
        let origin = canvas.min;
        let stroke = Stroke::new(1.0, Color32::from_rgb(180, 180, 180));
        let r: i32 = 5;
        // XZ plane grid
        for i in -r..=r {
            if i == 0 { continue; }
            let a = self.camera.project(&Point3D::new(i as f64, 0.0, -r as f64), w, h);
            let b = self.camera.project(&Point3D::new(i as f64, 0.0, r as f64), w, h);
            let c = self.camera.project(&Point3D::new(-r as f64, 0.0, i as f64), w, h);
            let d = self.camera.project(&Point3D::new(r as f64, 0.0, i as f64), w, h);
            if let (Some(a), Some(b)) = (a, b) {
                painter.line_segment([origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)], stroke);
            }
            if let (Some(c), Some(d)) = (c, d) {
                painter.line_segment([origin + Vec2::new(c.0, c.1), origin + Vec2::new(d.0, d.1)], stroke);
            }
        }
        // Axes
        let red = Stroke::new(2.5, Color32::RED);
        let green = Stroke::new(2.5, Color32::GREEN);
        let blue = Stroke::new(2.5, Color32::BLUE);
        let o = self.camera.project(&Point3D::new(0.0,0.0,0.0), w, h);
        if let Some(o) = o {
            let ox = origin + Vec2::new(o.0, o.1);
            let axis_labels = [
                (Point3D::new(r as f64, 0.0, 0.0), red, "X"),
                (Point3D::new(0.0, r as f64, 0.0), green, "Y"),
                (Point3D::new(0.0, 0.0, r as f64), blue, "Z"),
            ];
            for (dir, color, label) in &axis_labels {
                if let Some(d) = self.camera.project(dir, w, h) {
                    let end = origin + Vec2::new(d.0, d.1);
                    painter.line_segment([ox, end], *color);
                    // Arrow tip
                    painter.circle_filled(end, 3.0, color.color);
                    // Label
                    painter.text(end + Vec2::new(4.0, -4.0), egui::Align2::LEFT_BOTTOM,
                        label, egui::FontId::proportional(14.0), color.color);
                }
            }
        }
    }

    pub fn draw_3d_objects(&self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
        let origin = canvas.min;
        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() { continue; }
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(pt) = self.camera.project(&p.position, w, h) {
                        let pos = origin + Vec2::new(pt.0, pt.1);
                        painter.circle_filled(pos, p.size.min(5.0), to_color32(p.color));
                        if !p.label.is_empty() {
                            painter.text(pos + Vec2::new(6.0, -6.0), egui::Align2::LEFT_BOTTOM, &p.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Segment3D(l) => {
                    if let (Some(a), Some(b)) = (self.camera.project(&l.a, w, h), self.camera.project(&l.b, w, h)) {
                        painter.line_segment([origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)], Stroke::new(l.width, to_color32(l.color)));
                        if !l.label.is_empty() {
                            let mid = (a.0 + b.0) * 0.5;
                            let mid_y = (a.1 + b.1) * 0.5;
                            painter.text(origin + Vec2::new(mid, mid_y - 8.0), egui::Align2::CENTER_BOTTOM, &l.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Sphere3D(s) => {
                    let stroke = Stroke::new(s.width, to_color32(s.color));
                    // Wireframe: 3 orthogonal great circles
                    let center = s.center.to_vec3();
                    let r = s.radius as f32;
                    let axes = [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)];
                    for &(u, v) in &axes {
                        let pts: Vec<(f32, f32)> = Camera3D::circle_points(center, u, v, r, 32)
                            .iter()
                            .filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                            .collect();
                        for i in 0..pts.len() {
                            let i2 = (i + 1) % pts.len();
                            painter.line_segment([
                                origin + Vec2::new(pts[i].0, pts[i].1),
                                origin + Vec2::new(pts[i2].0, pts[i2].1),
                            ], stroke);
                        }
                    }
                    if !s.label.is_empty() {
                        if let Some(pt) = self.camera.project(&Point3D::new(s.center.x, s.center.y + s.radius + 0.3, s.center.z), w, h) {
                            painter.text(origin + Vec2::new(pt.0, pt.1), egui::Align2::CENTER_BOTTOM, &s.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Cube3D(cube) => {
                    let stroke = Stroke::new(cube.width, to_color32(cube.color));
                    let _verts = Cube3DObj::new(cube.center, cube.size).center; // just use the geom
                    let geom = grafito_geometry::Cube3D::new(cube.center, cube.size);
                    let vs = geom.vertices();
                    let edges = [(0,1),(1,2),(2,3),(3,0), (4,5),(5,6),(6,7),(7,4), (0,4),(1,5),(2,6),(3,7)];
                    for &(a, b) in &edges {
                        if let (Some(pa), Some(pb)) = (self.camera.project(&vs[a], w, h), self.camera.project(&vs[b], w, h)) {
                            painter.line_segment([origin + Vec2::new(pa.0, pa.1), origin + Vec2::new(pb.0, pb.1)], stroke);
                        }
                    }
                    if !cube.label.is_empty() {
                        if let Some(pt) = self.camera.project(&Point3D::new(cube.center.x, cube.center.y + cube.size * 0.7, cube.center.z), w, h) {
                            painter.text(origin + Vec2::new(pt.0, pt.1), egui::Align2::CENTER_BOTTOM, &cube.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Pyramid3D(py) => {
                    let stroke = Stroke::new(py.width, to_color32(py.color));
                    let geom = grafito_geometry::Pyramid3D::new(py.base_center, py.apex, py.base_size);
                    let base = geom.base_vertices();
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        let a_proj = self.camera.project(&base[i], w, h);
                        let b_proj = self.camera.project(&base[j], w, h);
                        let apex_proj = self.camera.project(&py.apex, w, h);
                        if let (Some(a), Some(b)) = (a_proj, b_proj) {
                            painter.line_segment([origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)], stroke);
                        }
                        if let (Some(a), Some(ap)) = (a_proj, apex_proj) {
                            painter.line_segment([origin + Vec2::new(a.0, a.1), origin + Vec2::new(ap.0, ap.1)], stroke);
                        }
                    }
                    if !py.label.is_empty() {
                        if let Some(pt) = self.camera.project(&py.apex, w, h) {
                            painter.text(origin + Vec2::new(pt.0, pt.1 + 14.0), egui::Align2::CENTER_TOP, &py.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Cone3D(cone) => {
                    let stroke = Stroke::new(cone.width, to_color32(cone.color));
                    // Base circle
                    let base_pts: Vec<(f32, f32)> = Camera3D::circle_points(
                        cone.base_center.to_vec3(), Vec3::X, Vec3::Z, cone.radius as f32, 32,
                    ).iter().filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h)).collect();
                    for i in 0..base_pts.len() {
                        let j = (i + 1) % base_pts.len();
                        painter.line_segment([origin + Vec2::new(base_pts[i].0, base_pts[i].1), origin + Vec2::new(base_pts[j].0, base_pts[j].1)], stroke);
                    }
                    // Lines from base to apex
                    if let Some(ap) = self.camera.project(&cone.apex, w, h) {
                        for bp in &base_pts {
                            painter.line_segment([origin + Vec2::new(bp.0, bp.1), origin + Vec2::new(ap.0, ap.1)], stroke);
                        }
                    }
                    if !cone.label.is_empty() {
                        if let Some(pt) = self.camera.project(&cone.apex, w, h) {
                            painter.text(origin + Vec2::new(pt.0, pt.1 + 14.0), egui::Align2::CENTER_TOP, &cone.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Cylinder3D(cyl) => {
                    let stroke = Stroke::new(cyl.width, to_color32(cyl.color));
                    // Top and bottom circles
                    for &center in &[cyl.base_center, cyl.top_center] {
                        let pts: Vec<(f32, f32)> = Camera3D::circle_points(
                            center.to_vec3(), Vec3::X, Vec3::Z, cyl.radius as f32, 24,
                        ).iter().filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h)).collect();
                        for i in 0..pts.len() {
                            let j = (i + 1) % pts.len();
                            painter.line_segment([origin + Vec2::new(pts[i].0, pts[i].1), origin + Vec2::new(pts[j].0, pts[j].1)], stroke);
                        }
                        // Vertical lines
                        if let (Some(_a), Some(_b)) = (self.camera.project(&center, w, h), self.camera.project(if center == cyl.base_center { &cyl.top_center } else { &cyl.base_center }, w, h)) {
                            for angle in [0.0, std::f32::consts::PI * 0.5, std::f32::consts::PI, std::f32::consts::PI * 1.5] {
                                let rx = angle.cos() * cyl.radius as f32;
                                let rz = angle.sin() * cyl.radius as f32;
                                let ca = self.camera.project(&Point3D::new(cyl.base_center.x + rx as f64 , cyl.base_center.y, cyl.base_center.z + rz as f64 ), w, h);
                                let cb = self.camera.project(&Point3D::new(cyl.top_center.x + rx as f64 , cyl.top_center.y, cyl.top_center.z + rz as f64 ), w, h);
                                if let (Some(ca), Some(cb)) = (ca, cb) {
                                    painter.line_segment([origin + Vec2::new(ca.0, ca.1), origin + Vec2::new(cb.0, cb.1)], stroke);
                                }
                            }
                        }
                    }
                    if !cyl.label.is_empty() {
                        if let Some(pt) = self.camera.project(&Point3D::new(cyl.top_center.x, cyl.top_center.y + 0.5, cyl.top_center.z), w, h) {
                            painter.text(origin + Vec2::new(pt.0, pt.1), egui::Align2::CENTER_BOTTOM, &cyl.label, egui::FontId::proportional(12.0), Color32::BLACK);
                        }
                    }
                }
                GeoObject::Surface3D(surf) => {
                    let stroke = Stroke::new(surf.width, to_color32(surf.color));
                    let steps = 20;
                    let xs = surf.x_min; let xe = surf.x_max;
                    let ys = surf.y_min; let ye = surf.y_max;
                    let x_step = (xe - xs) / steps as f64;
                    let y_step = (ye - ys) / steps as f64;
                    for i in 0..=steps {
                        let y = ys + i as f64 * y_step;
                        let mut prev: Option<(f32, f32)> = None;
                        for j in 0..=steps {
                            let x = xs + j as f64 * x_step;
                            let mut vars = HashMap::new();
                            vars.insert("x".to_string(), x);
                            vars.insert("y".to_string(), y);
                            if let Ok(z) = evaluate(&surf.expr, &vars.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()) {
                                if z.is_finite() && z.abs() < 100.0 {
                                    if let Some(pt) = self.camera.project(&Point3D::new(x, z, y), w, h) {
                                        if let Some(pp) = prev {
                                            painter.line_segment([origin + Vec2::new(pp.0, pp.1), origin + Vec2::new(pt.0, pt.1)], stroke);
                                        }
                                        prev = Some(pt); continue;
                                    }
                                }
                            }
                            prev = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
