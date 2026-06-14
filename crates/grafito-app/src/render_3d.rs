use egui::{Color32, Rect, Stroke, Vec2};
use glam::Vec3;
use grafito_core::{Cube3DObj, GeoObject, Point3DObj, Pyramid3DObj, Segment3DObj, Sphere3DObj};
use grafito_geometry::{Camera3D, Point3D};
use grafito_ui::Tool;

use crate::{to_color32, GrafitoApp};

fn project_segment(
    camera: &Camera3D,
    a: &Point3D,
    b: &Point3D,
    screen_w: f32,
    screen_h: f32,
) -> Option<((f32, f32), (f32, f32))> {
    let mvp = camera.mvp();
    let mut clip_a = mvp * a.to_vec3().extend(1.0);
    let mut clip_b = mvp * b.to_vec3().extend(1.0);

    let near = camera.near;

    if clip_a.w < near && clip_b.w < near {
        return None;
    }

    if clip_a.w < near {
        let t = (near - clip_a.w) / (clip_b.w - clip_a.w);
        clip_a = clip_a + t * (clip_b - clip_a);
    } else if clip_b.w < near {
        let t = (near - clip_b.w) / (clip_a.w - clip_b.w);
        clip_b = clip_b + t * (clip_a - clip_b);
    }

    let ndc_ax = clip_a.x / clip_a.w;
    let ndc_ay = clip_a.y / clip_a.w;
    let ndc_bx = clip_b.x / clip_b.w;
    let ndc_by = clip_b.y / clip_b.w;

    if ndc_ax.abs() > 5.0 && ndc_bx.abs() > 5.0 && ndc_ax.signum() == ndc_bx.signum() {
        return None;
    }
    if ndc_ay.abs() > 5.0 && ndc_by.abs() > 5.0 && ndc_ay.signum() == ndc_by.signum() {
        return None;
    }

    let sax = (ndc_ax + 1.0) * 0.5 * screen_w;
    let say = (1.0 - ndc_ay) * 0.5 * screen_h;
    let sbx = (ndc_bx + 1.0) * 0.5 * screen_w;
    let sby = (1.0 - ndc_by) * 0.5 * screen_h;

    Some(((sax, say), (sbx, sby)))
}

impl GrafitoApp {
    pub fn handle_3d_click(
        &mut self,
        _ui: &egui::Ui,
        _response: &egui::Response,
        _canvas: Rect,
        w: f32,
        h: f32,
    ) {
        let origin = self.camera.project(&Point3D::new(0.0, 0.0, 0.0), w, h);
        let u = self.camera.project(&Point3D::new(1.0, 0.0, 0.0), w, h);
        let approx_scale = if let (Some(o), Some(u)) = (origin, u) {
            1.0 / ((u.0 - o.0).powi(2) + (u.1 - o.1).powi(2)).sqrt().max(0.01) as f64
        } else {
            0.5
        };

        // Place objects at a point on the XZ plane (y=0), near camera target
        let t = self.camera.target;
        match self.current_tool {
            Tool::Point => {
                self.save_state();
                let pos = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Point3D(Point3DObj::new(pos)));
            }
            Tool::Line => {
                self.pending_points_3d
                    .push(Point3D::new(t.x as f64, t.y as f64, t.z as f64));
                if self.pending_points_3d.len() == 2 {
                    let a = self.pending_points_3d[0];
                    let b = self.pending_points_3d[1];
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Segment3D(Segment3DObj::new(a, b)));
                    self.pending_points_3d.clear();
                }
            }
            Tool::Circle => {
                self.pending_points_3d
                    .push(Point3D::new(t.x as f64, t.y as f64, t.z as f64));
                if self.pending_points_3d.len() == 2 {
                    let center = self.pending_points_3d[0];
                    let edge = self.pending_points_3d[1];
                    let radius = center.distance(&edge);
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Sphere3D(Sphere3DObj::new(center, radius)));
                    self.pending_points_3d.clear();
                }
            }
            Tool::Polygon => {
                // Cube via polygon tool
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Cube3D(Cube3DObj::new(c, approx_scale * 2.0)));
            }
            Tool::Function => {
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Pyramid3D(Pyramid3DObj::new(
                        Point3D::new(c.x, c.y - approx_scale, c.z),
                        c,
                        approx_scale * 2.0,
                    )));
            }
            Tool::Point3D => {
                self.save_state();
                let pos = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Point3D(Point3DObj::new(pos)));
            }
            Tool::Sphere3D => {
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Sphere3D(Sphere3DObj::new(c, approx_scale * 1.5)));
            }
            Tool::Cube3D => {
                self.save_state();
                let c = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                self.document
                    .add_object(GeoObject::Cube3D(Cube3DObj::new(c, approx_scale * 2.0)));
            }
            Tool::Attractor => {
                self.input_text = "Lorenz[]".to_string();
            }
            Tool::Fractal => {
                self.input_text = "Mandelbrot[]".to_string();
            }
            _ => {}
        }
    }

    pub fn draw_3d_grid(&self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
        let origin = canvas.min;

        // Dynamic step calculation based on camera distance
        let fov_rad = self.camera.fov.to_radians();
        let frustum_height = 2.0 * self.camera.distance * (fov_rad * 0.5).tan();
        let pixels_per_unit = (h / frustum_height) as f64;
        let target_world_step = 120.0 / pixels_per_unit.max(1e-30);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;

        let major_step = if factor < 2.0 {
            1.0 * base
        } else if factor < 5.0 {
            2.0 * base
        } else {
            5.0 * base
        };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

        // Grid colors (soft grey, adapting to dark mode)
        let major_color = if self.dark_mode {
            Color32::from_rgba_unmultiplied(255, 255, 255, 28)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 35)
        };
        let major_stroke = Stroke::new(0.5, major_color);

        // Grid range: center around camera target projection on XZ plane
        let center_x = self.camera.target.x as f64;
        let center_z = self.camera.target.z as f64;
        let aspect = w / h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let start_x = ((center_x - view_range) / major_step).floor() * major_step;
        let end_x = ((center_x + view_range) / major_step).ceil() * major_step;
        let start_z = ((center_z - view_range) / major_step).floor() * major_step;
        let end_z = ((center_z + view_range) / major_step).ceil() * major_step;

        let line_count_x = ((end_x - start_x) / major_step).round() as i64;
        let line_count_z = ((end_z - start_z) / major_step).round() as i64;

        if self.show_grid && line_count_x <= 200 && line_count_z <= 200 {
            // Draw grid lines parallel to Z axis (varying z, fixed x)
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * major_step;
                let stroke = major_stroke;
                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                if let Some((a, b)) = project_segment(&self.camera, &p1, &p2, w, h) {
                    painter.line_segment(
                        [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                        stroke,
                    );
                }
            }

            // Draw grid lines parallel to X axis (varying x, fixed z)
            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * major_step;
                let stroke = major_stroke;
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                if let Some((a, b)) = project_segment(&self.camera, &p1, &p2, w, h) {
                    painter.line_segment(
                        [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                        stroke,
                    );
                }
            }
        }

        // Draw Axes
        let axis_len = view_range;
        let red_stroke = Stroke::new(2.0, Color32::from_rgb(220, 50, 50));
        let green_stroke = Stroke::new(2.0, Color32::from_rgb(50, 180, 50));
        let blue_stroke = Stroke::new(2.0, Color32::from_rgb(50, 50, 220));

        // X Axis
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            w,
            h,
        ) {
            painter.line_segment(
                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                red_stroke,
            );
        }
        // Y Axis (vertical)
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            w,
            h,
        ) {
            painter.line_segment(
                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                green_stroke,
            );
        }
        // Z Axis
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            w,
            h,
        ) {
            painter.line_segment(
                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                blue_stroke,
            );
        }

        // Axis labels
        let label_font = egui::FontId::proportional(14.0);
        if let Some(pos) = self.camera.project(&Point3D::new(axis_len, 0.0, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "X",
                label_font.clone(),
                red_stroke.color,
            );
        }
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, axis_len, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "Y",
                label_font.clone(),
                green_stroke.color,
            );
        }
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, axis_len), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "Z",
                label_font.clone(),
                blue_stroke.color,
            );
        }

        // Draw Axis Numbers
        let precision = if major_step > 0.0 {
            let log = major_step.log10();
            if log < 0.0 {
                (log.abs().ceil() as usize + 2).clamp(1, 14)
            } else {
                0
            }
        } else {
            2
        };
        let format_num = |v: f64| -> String {
            if v.abs() < major_step * 1e-5 {
                return "0".to_string();
            }
            let mut s = format!("{:.*}", precision, v);
            if s.contains('.') {
                s = s.trim_end_matches('0').to_string();
                s = s.trim_end_matches('.').to_string();
            }
            if s.is_empty() || s == "-" {
                "0".to_string()
            } else {
                s
            }
        };

        let text_color = if self.dark_mode {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(80)
        };
        let font = egui::FontId::proportional(11.0);
        let tick_stroke = Stroke::new(1.0, text_color);

        // Numbers on X Axis (Z=0, Y=0)
        let start_x_num = ((center_x - view_range) / major_step).floor() * major_step;
        let end_x_num = ((center_x + view_range) / major_step).ceil() * major_step;
        let num_count_x = ((end_x_num - start_x_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_x <= 500 {
            for xi in 0..=num_count_x {
                let x = start_x_num + xi as f64 * major_step;
                if x.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = x - cam_pos.x as f64;
                let dy = 0.0 - cam_pos.y as f64;
                let dz = 0.0 - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.distance as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(x, 0.0, -tick_size),
                    &Point3D::new(x, 0.0, tick_size),
                    w,
                    h,
                ) {
                    painter.line_segment(
                        [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                        tick_stroke,
                    );
                }
                if let Some(pos) = self.camera.project(&Point3D::new(x, 0.0, 0.0), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 50.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(0.0, 6.0),
                        egui::Align2::CENTER_TOP,
                        format_num(x),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Numbers on Y Axis (vertical: X=0, Z=0)
        let center_y = self.camera.target.y as f64;
        let start_y_num = ((center_y - view_range) / major_step).floor() * major_step;
        let end_y_num = ((center_y + view_range) / major_step).ceil() * major_step;
        let num_count_y = ((end_y_num - start_y_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_y <= 500 {
            for yi in 0..=num_count_y {
                let y = start_y_num + yi as f64 * major_step;
                if y.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = 0.0 - cam_pos.x as f64;
                let dy = y - cam_pos.y as f64;
                let dz = 0.0 - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.distance as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(-tick_size, y, 0.0),
                    &Point3D::new(tick_size, y, 0.0),
                    w,
                    h,
                ) {
                    painter.line_segment(
                        [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                        tick_stroke,
                    );
                }
                if let Some(pos) = self.camera.project(&Point3D::new(0.0, y, 0.0), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 40.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(-6.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        format_num(y),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Numbers on Z Axis (X=0, Y=0)
        let start_z_num = ((center_z - view_range) / major_step).floor() * major_step;
        let end_z_num = ((center_z + view_range) / major_step).ceil() * major_step;
        let num_count_z = ((end_z_num - start_z_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_z <= 500 {
            for zi in 0..=num_count_z {
                let z = start_z_num + zi as f64 * major_step;
                if z.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = 0.0 - cam_pos.x as f64;
                let dy = 0.0 - cam_pos.y as f64;
                let dz = z - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.distance as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(-tick_size, 0.0, z),
                    &Point3D::new(tick_size, 0.0, z),
                    w,
                    h,
                ) {
                    painter.line_segment(
                        [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                        tick_stroke,
                    );
                }
                if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, z), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 50.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format_num(z),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Origin "0" label
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(-6.0, 6.0),
                egui::Align2::RIGHT_TOP,
                "0",
                font.clone(),
                text_color,
            );
        }
    }

    pub fn draw_3d_objects(&mut self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
        let origin = canvas.min;
        let label_color = if self.dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        };

        // Depth sorting: collect objects with their distance to camera
        let camera_pos = self.camera.position();
        let mut objects_with_depth: Vec<(f32, &GeoObject)> = Vec::new();

        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }

            // Calculate distance from camera to object center
            let distance = match obj {
                GeoObject::Point3D(p) => (p.position.to_vec3() - camera_pos).length(),
                GeoObject::Segment3D(l) => {
                    let center = (l.a.to_vec3() + l.b.to_vec3()) * 0.5;
                    (center - camera_pos).length()
                }
                GeoObject::Sphere3D(s) => (s.center.to_vec3() - camera_pos).length(),
                GeoObject::Cube3D(c) => (c.center.to_vec3() - camera_pos).length(),
                GeoObject::Pyramid3D(p) => {
                    let center = (p.base_center.to_vec3() + p.apex.to_vec3()) * 0.5;
                    (center - camera_pos).length()
                }
                GeoObject::Cone3D(c) => {
                    let center = (c.base_center.to_vec3() + c.apex.to_vec3()) * 0.5;
                    (center - camera_pos).length()
                }
                GeoObject::Cylinder3D(c) => {
                    let center = (c.base_center.to_vec3() + c.top_center.to_vec3()) * 0.5;
                    (center - camera_pos).length()
                }
                GeoObject::Torus3D(t) => (t.center.to_vec3() - camera_pos).length(),
                GeoObject::MoebiusStrip(m) => (m.center.to_vec3() - camera_pos).length(),
                GeoObject::Surface3D(s) => {
                    let center = Vec3::new(
                        (s.x_min + s.x_max) as f32 * 0.5,
                        0.0,
                        (s.y_min + s.y_max) as f32 * 0.5,
                    );
                    (center - camera_pos).length()
                }
                GeoObject::ParametricCurve3D(c) => {
                    // Use midpoint of parameter range as approximation
                    let t_mid = (c.t_min + c.t_max) * 0.5;
                    if let (Ok(x), Ok(y), Ok(z)) = (
                        grafito_geometry::expr::evaluate(&c.expr_x, &[("t".to_string(), t_mid)]),
                        grafito_geometry::expr::evaluate(&c.expr_y, &[("t".to_string(), t_mid)]),
                        grafito_geometry::expr::evaluate(&c.expr_z, &[("t".to_string(), t_mid)]),
                    ) {
                        let center = Vec3::new(x as f32, z as f32, y as f32);
                        (center - camera_pos).length()
                    } else {
                        1000.0 // Fallback distance
                    }
                }
                GeoObject::Attractor3D(a) => {
                    // Use first point as approximation
                    let center = Vec3::new(a.x0 as f32, a.z0 as f32, a.y0 as f32);
                    (center - camera_pos).length()
                }
                GeoObject::HyperSurface4D(_h) => {
                    // Use origin as approximation
                    Vec3::new(0.0, 0.0, 0.0).distance(camera_pos)
                }
                GeoObject::VectorField3D(v) => {
                    let center = Vec3::new(
                        (v.x_min + v.x_max) as f32 * 0.5,
                        (v.z_min + v.z_max) as f32 * 0.5,
                        (v.y_min + v.y_max) as f32 * 0.5,
                    );
                    (center - camera_pos).length()
                }
                _ => continue, // Skip non-3D objects
            };

            objects_with_depth.push((distance, obj));
        }

        // Sort by distance (far to near - painter's algorithm)
        objects_with_depth
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Render objects in sorted order
        for (_, obj) in objects_with_depth {
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(pt) = self.camera.project(&p.position, w, h) {
                        let pos = origin + Vec2::new(pt.0, pt.1);
                        painter.circle_filled(pos, p.size.min(5.0), to_color32(p.color));
                        if !p.label.is_empty() {
                            painter.text(
                                pos + Vec2::new(6.0, -6.0),
                                egui::Align2::LEFT_BOTTOM,
                                &p.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Segment3D(l) => {
                    if let Some((a, b)) = project_segment(&self.camera, &l.a, &l.b, w, h) {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            Stroke::new(l.width, to_color32(l.color)),
                        );
                        if !l.label.is_empty() {
                            let mid = (a.0 + b.0) * 0.5;
                            let mid_y = (a.1 + b.1) * 0.5;
                            painter.text(
                                origin + Vec2::new(mid, mid_y - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                &l.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Sphere3D(s) => {
                    // Wireframe with lighting: 3 orthogonal great circles
                    let center = s.center.to_vec3();
                    let r = s.radius as f32;
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize(); // Light from upper-right

                    let axes = [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)];
                    for &(u, v) in &axes {
                        let pts_3d: Vec<Vec3> = Camera3D::circle_points(center, u, v, r, 32);

                        let pts_screen: Vec<Option<(f32, f32)>> = pts_3d
                            .iter()
                            .map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                            .collect();

                        for i in 0..pts_3d.len() {
                            let i2 = (i + 1) % pts_3d.len();

                            if let (Some(p1), Some(p2)) = (pts_screen[i], pts_screen[i2]) {
                                // Calculate normal at midpoint of segment
                                let mid_3d = (pts_3d[i] + pts_3d[i2]) * 0.5;
                                let normal = (mid_3d - center).normalize();

                                // Apply lighting
                                let lit_color =
                                    grafito_render::calculate_lighting(s.color, normal, light_dir);
                                let stroke = Stroke::new(s.width, to_color32(lit_color));

                                painter.line_segment(
                                    [
                                        origin + Vec2::new(p1.0, p1.1),
                                        origin + Vec2::new(p2.0, p2.1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                    if !s.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(s.center.x, s.center.y + s.radius + 0.3, s.center.z),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &s.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cube3D(cube) => {
                    let geom = grafito_geometry::Cube3D::new(cube.center, cube.size);
                    let vs = geom.vertices();
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Edges with their face normals for lighting
                    let edges_with_normals = [
                        // Bottom face (normal: -Y)
                        ((0, 1), Vec3::new(0.0, -1.0, 0.0)),
                        ((1, 2), Vec3::new(0.0, -1.0, 0.0)),
                        ((2, 3), Vec3::new(0.0, -1.0, 0.0)),
                        ((3, 0), Vec3::new(0.0, -1.0, 0.0)),
                        // Top face (normal: +Y)
                        ((4, 5), Vec3::new(0.0, 1.0, 0.0)),
                        ((5, 6), Vec3::new(0.0, 1.0, 0.0)),
                        ((6, 7), Vec3::new(0.0, 1.0, 0.0)),
                        ((7, 4), Vec3::new(0.0, 1.0, 0.0)),
                        // Vertical edges (use average of adjacent face normals)
                        ((0, 4), Vec3::new(-1.0, 0.0, -1.0).normalize()),
                        ((1, 5), Vec3::new(1.0, 0.0, -1.0).normalize()),
                        ((2, 6), Vec3::new(1.0, 0.0, 1.0).normalize()),
                        ((3, 7), Vec3::new(-1.0, 0.0, 1.0).normalize()),
                    ];

                    for &((a, b), normal) in &edges_with_normals {
                        if let (Some(pa), Some(pb)) = (
                            self.camera.project(&vs[a], w, h),
                            self.camera.project(&vs[b], w, h),
                        ) {
                            let lit_color =
                                grafito_render::calculate_lighting(cube.color, normal, light_dir);
                            let stroke = Stroke::new(cube.width, to_color32(lit_color));
                            painter.line_segment(
                                [
                                    origin + Vec2::new(pa.0, pa.1),
                                    origin + Vec2::new(pb.0, pb.1),
                                ],
                                stroke,
                            );
                        }
                    }
                    if !cube.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(
                                cube.center.x,
                                cube.center.y + cube.size * 0.7,
                                cube.center.z,
                            ),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &cube.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Pyramid3D(py) => {
                    let geom =
                        grafito_geometry::Pyramid3D::new(py.base_center, py.apex, py.base_size);
                    let base = geom.base_vertices();
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Render base edges (normal: -Y)
                    let base_normal = Vec3::new(0.0, -1.0, 0.0);
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        let a_proj = self.camera.project(&base[i], w, h);
                        let b_proj = self.camera.project(&base[j], w, h);
                        if let (Some(a), Some(b)) = (a_proj, b_proj) {
                            let lit_color = grafito_render::calculate_lighting(
                                py.color,
                                base_normal,
                                light_dir,
                            );
                            let stroke = Stroke::new(py.width, to_color32(lit_color));
                            painter.line_segment(
                                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                                stroke,
                            );
                        }
                    }

                    // Render lateral edges (calculate normal for each triangular face)
                    let apex_proj = self.camera.project(&py.apex, w, h);
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        let a_proj = self.camera.project(&base[i], w, h);

                        // Calculate face normal using cross product
                        let v1 = base[j].to_vec3() - base[i].to_vec3();
                        let v2 = py.apex.to_vec3() - base[i].to_vec3();
                        let face_normal = v1.cross(v2).normalize();

                        if let (Some(a), Some(ap)) = (a_proj, apex_proj) {
                            let lit_color = grafito_render::calculate_lighting(
                                py.color,
                                face_normal,
                                light_dir,
                            );
                            let stroke = Stroke::new(py.width, to_color32(lit_color));
                            painter.line_segment(
                                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(ap.0, ap.1)],
                                stroke,
                            );
                        }
                    }

                    if !py.label.is_empty() {
                        if let Some(pt) = self.camera.project(&py.apex, w, h) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1 + 14.0),
                                egui::Align2::CENTER_TOP,
                                &py.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cone3D(cone) => {
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Base circle (normal: -Y)
                    let base_normal = Vec3::new(0.0, -1.0, 0.0);
                    let base_pts_3d: Vec<Vec3> = Camera3D::circle_points(
                        cone.base_center.to_vec3(),
                        Vec3::X,
                        Vec3::Z,
                        cone.radius as f32,
                        32,
                    );
                    let base_pts: Vec<(f32, f32)> = base_pts_3d
                        .iter()
                        .filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                        .collect();

                    for i in 0..base_pts.len() {
                        let j = (i + 1) % base_pts.len();
                        let lit_color =
                            grafito_render::calculate_lighting(cone.color, base_normal, light_dir);
                        let stroke = Stroke::new(cone.width, to_color32(lit_color));
                        painter.line_segment(
                            [
                                origin + Vec2::new(base_pts[i].0, base_pts[i].1),
                                origin + Vec2::new(base_pts[j].0, base_pts[j].1),
                            ],
                            stroke,
                        );
                    }

                    // Lines from base to apex (calculate lateral surface normal)
                    if let Some(ap) = self.camera.project(&cone.apex, w, h) {
                        for bp_3d in &base_pts_3d {
                            let bp_3d = *bp_3d;
                            if let Some(bp) = self.camera.project(&Point3D::from_vec3(bp_3d), w, h)
                            {
                                // Calculate lateral surface normal at this point
                                let radial = (bp_3d - cone.base_center.to_vec3()).normalize();
                                let axial =
                                    (cone.apex.to_vec3() - cone.base_center.to_vec3()).normalize();
                                let lateral_normal = (radial + axial * 0.5).normalize();

                                let lit_color = grafito_render::calculate_lighting(
                                    cone.color,
                                    lateral_normal,
                                    light_dir,
                                );
                                let stroke = Stroke::new(cone.width, to_color32(lit_color));
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(bp.0, bp.1),
                                        origin + Vec2::new(ap.0, ap.1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }

                    if !cone.label.is_empty() {
                        if let Some(pt) = self.camera.project(&cone.apex, w, h) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1 + 14.0),
                                egui::Align2::CENTER_TOP,
                                &cone.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cylinder3D(cyl) => {
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Top and bottom circles with their normals
                    let circles = [
                        (cyl.base_center, Vec3::new(0.0, -1.0, 0.0)), // Bottom (normal: -Y)
                        (cyl.top_center, Vec3::new(0.0, 1.0, 0.0)),   // Top (normal: +Y)
                    ];

                    for &(center, normal) in &circles {
                        let pts_3d: Vec<Vec3> = Camera3D::circle_points(
                            center.to_vec3(),
                            Vec3::X,
                            Vec3::Z,
                            cyl.radius as f32,
                            24,
                        );
                        let pts: Vec<(f32, f32)> = pts_3d
                            .iter()
                            .filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                            .collect();

                        for i in 0..pts.len() {
                            let j = (i + 1) % pts.len();
                            let lit_color =
                                grafito_render::calculate_lighting(cyl.color, normal, light_dir);
                            let stroke = Stroke::new(cyl.width, to_color32(lit_color));
                            painter.line_segment(
                                [
                                    origin + Vec2::new(pts[i].0, pts[i].1),
                                    origin + Vec2::new(pts[j].0, pts[j].1),
                                ],
                                stroke,
                            );
                        }
                    }

                    // Vertical lines with radial normals
                    if let (Some(_a), Some(_b)) = (
                        self.camera.project(&cyl.base_center, w, h),
                        self.camera.project(&cyl.top_center, w, h),
                    ) {
                        for angle in [
                            0.0,
                            std::f32::consts::PI * 0.5,
                            std::f32::consts::PI,
                            std::f32::consts::PI * 1.5,
                        ] {
                            let rx = angle.cos() * cyl.radius as f32;
                            let rz = angle.sin() * cyl.radius as f32;

                            // Radial normal pointing outward
                            let radial_normal = Vec3::new(angle.cos(), 0.0, angle.sin());

                            let ca = self.camera.project(
                                &Point3D::new(
                                    cyl.base_center.x + rx as f64,
                                    cyl.base_center.y,
                                    cyl.base_center.z + rz as f64,
                                ),
                                w,
                                h,
                            );
                            let cb = self.camera.project(
                                &Point3D::new(
                                    cyl.top_center.x + rx as f64,
                                    cyl.top_center.y,
                                    cyl.top_center.z + rz as f64,
                                ),
                                w,
                                h,
                            );

                            if let (Some(ca), Some(cb)) = (ca, cb) {
                                let lit_color = grafito_render::calculate_lighting(
                                    cyl.color,
                                    radial_normal,
                                    light_dir,
                                );
                                let stroke = Stroke::new(cyl.width, to_color32(lit_color));
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(ca.0, ca.1),
                                        origin + Vec2::new(cb.0, cb.1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }

                    if !cyl.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(
                                cyl.top_center.x,
                                cyl.top_center.y + 0.5,
                                cyl.top_center.z,
                            ),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &cyl.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Torus3D(torus) => {
                    let stroke = Stroke::new(torus.width, to_color32(torus.color));
                    let steps_u = 24;
                    let steps_v = 12;
                    let r_maj = torus.r_major;
                    let r = torus.r_minor;
                    let mut pts = vec![vec![(0.0_f32, 0.0_f32); steps_v + 1]; steps_u + 1];
                    let mut valid = vec![vec![false; steps_v + 1]; steps_u + 1];
                    for i in 0..=steps_u {
                        let u = (i as f64) * std::f64::consts::PI * 2.0 / (steps_u as f64);
                        for j in 0..=steps_v {
                            let v = (j as f64) * std::f64::consts::PI * 2.0 / (steps_v as f64);
                            let x = torus.center.x + (r_maj + r * v.cos()) * u.cos();
                            let z = torus.center.z + (r_maj + r * v.cos()) * u.sin();
                            let y = torus.center.y + r * v.sin();
                            if let Some(p) = self.camera.project(&Point3D::new(x, y, z), w, h) {
                                pts[i][j] = (p.0, p.1);
                                valid[i][j] = true;
                            }
                        }
                    }
                    for i in 0..steps_u {
                        for j in 0..steps_v {
                            if valid[i][j] && valid[i + 1][j] {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i + 1][j].0, pts[i + 1][j].1),
                                    ],
                                    stroke,
                                );
                            }
                            if valid[i][j] && valid[i][j + 1] {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i][j + 1].0, pts[i][j + 1].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                }
                GeoObject::MoebiusStrip(moe) => {
                    let stroke = Stroke::new(moe.width, to_color32(moe.color));
                    let steps_u = 32;
                    let steps_v = 8;
                    let r_maj = moe.radius;
                    let w_max = moe.width_r;
                    let mut pts = vec![vec![(0.0_f32, 0.0_f32); steps_v + 1]; steps_u + 1];
                    let mut valid = vec![vec![false; steps_v + 1]; steps_u + 1];
                    for i in 0..=steps_u {
                        let u = (i as f64) * std::f64::consts::PI * 2.0 / (steps_u as f64);
                        for j in 0..=steps_v {
                            let v = -w_max / 2.0 + w_max * (j as f64) / (steps_v as f64);
                            let x = moe.center.x + (r_maj + v * (u / 2.0).cos()) * u.cos();
                            let z = moe.center.z + (r_maj + v * (u / 2.0).cos()) * u.sin();
                            let y = moe.center.y + v * (u / 2.0).sin();
                            if let Some(p) = self.camera.project(&Point3D::new(x, y, z), w, h) {
                                pts[i][j] = (p.0, p.1);
                                valid[i][j] = true;
                            }
                        }
                    }
                    for i in 0..steps_u {
                        for j in 0..=steps_v {
                            if valid[i][j] && valid[i + 1][j] {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i + 1][j].0, pts[i + 1][j].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                    for i in 0..=steps_u {
                        for j in 0..steps_v {
                            if valid[i][j] && valid[i][j + 1] {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i][j + 1].0, pts[i][j + 1].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                }
                GeoObject::Surface3D(surf) => {
                    if surf.solid {
                        // Solid Gouraud-shaded surface
                        let res = surf.mesh_res.clamp(8, 50);
                        let xs = surf.x_min;
                        let xe = surf.x_max;
                        let ys = surf.y_min;
                        let ye = surf.y_max;
                        let dx = (xe - xs) / res as f64;
                        let dy = (ye - ys) / res as f64;

                        // Evaluate all z values
                        let mut pts = Vec::with_capacity((res + 1) * (res + 1));
                        for i in 0..=res {
                            for j in 0..=res {
                                pts.push((xs + j as f64 * dx, ys + i as f64 * dy));
                            }
                        }
                        if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                            &surf.expr,
                            pts.iter().copied(),
                            &self.document.variables,
                        ) {
                            let light_dir = glam::Vec3::new(0.5, 0.8, 0.3).normalize();
                            let ambient = 0.4;
                            let base = to_color32(surf.color);
                            // Triangulate and draw depth-sorted faces
                            for i in 0..res {
                                for j in 0..res {
                                    let idx00 = i * (res + 1) + j;
                                    let idx10 = i * (res + 1) + j + 1;
                                    let idx01 = (i + 1) * (res + 1) + j;
                                    let idx11 = (i + 1) * (res + 1) + j + 1;

                                    let (x0, y0) = pts[idx00];
                                    let z0 = z_vals[idx00].unwrap_or(f64::NAN);
                                    let (x1, y1) = pts[idx10];
                                    let z1 = z_vals[idx10].unwrap_or(f64::NAN);
                                    let (x2, y2) = pts[idx01];
                                    let z2 = z_vals[idx01].unwrap_or(f64::NAN);
                                    let (x3, y3) = pts[idx11];
                                    let z3 = z_vals[idx11].unwrap_or(f64::NAN);

                                    if !z0.is_finite()
                                        || !z1.is_finite()
                                        || !z2.is_finite()
                                        || !z3.is_finite()
                                        || z0.abs() > 100.0
                                        || z1.abs() > 100.0
                                        || z2.abs() > 100.0
                                        || z3.abs() > 100.0
                                    {
                                        continue;
                                    }

                                    // Compute two triangle normals
                                    let v00 = glam::Vec3::new(x0 as f32, z0 as f32, y0 as f32);
                                    let v10 = glam::Vec3::new(x1 as f32, z1 as f32, y1 as f32);
                                    let v01 = glam::Vec3::new(x2 as f32, z2 as f32, y2 as f32);
                                    let v11 = glam::Vec3::new(x3 as f32, z3 as f32, y3 as f32);

                                    let n1 = (v10 - v00).cross(v01 - v00).normalize();
                                    let n2 = (v11 - v10).cross(v01 - v10).normalize();

                                    let shade1 = (ambient
                                        + (1.0 - ambient) * n1.dot(light_dir).max(0.0))
                                    .clamp(0.0, 1.0);
                                    let shade2 = (ambient
                                        + (1.0 - ambient) * n2.dot(light_dir).max(0.0))
                                    .clamp(0.0, 1.0);

                                    let c1 = Color32::from_rgba_unmultiplied(
                                        (base.r() as f32 * shade1) as u8,
                                        (base.g() as f32 * shade1) as u8,
                                        (base.b() as f32 * shade1) as u8,
                                        255,
                                    );
                                    let c2 = Color32::from_rgba_unmultiplied(
                                        (base.r() as f32 * shade2) as u8,
                                        (base.g() as f32 * shade2) as u8,
                                        (base.b() as f32 * shade2) as u8,
                                        255,
                                    );

                                    // Project and draw triangle 1
                                    if let (Some(p0), Some(p1), Some(p2)) = (
                                        self.camera.project(&Point3D::new(x0, z0, y0), w, h),
                                        self.camera.project(&Point3D::new(x1, z1, y1), w, h),
                                        self.camera.project(&Point3D::new(x2, z2, y2), w, h),
                                    ) {
                                        let pts1 = vec![
                                            origin + Vec2::new(p0.0, p0.1),
                                            origin + Vec2::new(p1.0, p1.1),
                                            origin + Vec2::new(p2.0, p2.1),
                                        ];
                                        painter.add(egui::Shape::convex_polygon(
                                            pts1,
                                            c1,
                                            Stroke::new(0.5, c1),
                                        ));
                                    }
                                    // Project and draw triangle 2
                                    if let (Some(p1), Some(p2), Some(p3)) = (
                                        self.camera.project(&Point3D::new(x1, z1, y1), w, h),
                                        self.camera.project(&Point3D::new(x2, z2, y2), w, h),
                                        self.camera.project(&Point3D::new(x3, z3, y3), w, h),
                                    ) {
                                        let pts2 = vec![
                                            origin + Vec2::new(p1.0, p1.1),
                                            origin + Vec2::new(p2.0, p2.1),
                                            origin + Vec2::new(p3.0, p3.1),
                                        ];
                                        painter.add(egui::Shape::convex_polygon(
                                            pts2,
                                            c2,
                                            Stroke::new(0.5, c2),
                                        ));
                                    }
                                }
                            }
                        }
                        if !surf.label.is_empty() {
                            if let Some(pt) = self.camera.project(
                                &Point3D::new(
                                    (surf.x_min + surf.x_max) * 0.5,
                                    1.0,
                                    (surf.y_min + surf.y_max) * 0.5,
                                ),
                                w,
                                h,
                            ) {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1),
                                    egui::Align2::CENTER_BOTTOM,
                                    &surf.label,
                                    egui::FontId::proportional(12.0),
                                    label_color,
                                );
                            }
                        }
                        return;
                    }
                    // Original wireframe rendering
                    let stroke = Stroke::new(surf.width, to_color32(surf.color));
                    let steps = 20;
                    let xs = surf.x_min;
                    let xe = surf.x_max;
                    let ys = surf.y_min;
                    let ye = surf.y_max;
                    let x_step = (xe - xs) / steps as f64;
                    let y_step = (ye - ys) / steps as f64;
                    // Collect all (x, y) points for X-axis lines
                    let mut pts_x = Vec::with_capacity((steps + 1) * (steps + 1));
                    for i in 0..=steps {
                        let y = ys + i as f64 * y_step;
                        for j in 0..=steps {
                            let x = xs + j as f64 * x_step;
                            pts_x.push((x, y));
                        }
                    }

                    if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                        &surf.expr,
                        pts_x.iter().copied(),
                        &self.document.variables,
                    ) {
                        for i in 0..=steps {
                            let mut prev: Option<(f32, f32)> = None;
                            for j in 0..=steps {
                                let idx = i * (steps + 1) + j;
                                let (x, y) = pts_x[idx];
                                if let Some(z) = z_vals[idx] {
                                    if z.is_finite() && z.abs() < 100.0 {
                                        if let Some(pt) =
                                            self.camera.project(&Point3D::new(x, z, y), w, h)
                                        {
                                            if let Some(pp) = prev {
                                                painter.line_segment(
                                                    [
                                                        origin + Vec2::new(pp.0, pp.1),
                                                        origin + Vec2::new(pt.0, pt.1),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                            prev = Some(pt);
                                            continue;
                                        }
                                    }
                                }
                                prev = None;
                            }
                        }
                    }

                    // Collect all (x, y) points for Y-axis lines
                    let mut pts_y = Vec::with_capacity((steps + 1) * (steps + 1));
                    for j in 0..=steps {
                        let x = xs + j as f64 * x_step;
                        for i in 0..=steps {
                            let y = ys + i as f64 * y_step;
                            pts_y.push((x, y));
                        }
                    }

                    if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                        &surf.expr,
                        pts_y.iter().copied(),
                        &self.document.variables,
                    ) {
                        for j in 0..=steps {
                            let mut prev: Option<(f32, f32)> = None;
                            for i in 0..=steps {
                                let idx = j * (steps + 1) + i;
                                let (x, y) = pts_y[idx];
                                if let Some(z) = z_vals[idx] {
                                    if z.is_finite() && z.abs() < 100.0 {
                                        if let Some(pt) =
                                            self.camera.project(&Point3D::new(x, z, y), w, h)
                                        {
                                            if let Some(pp) = prev {
                                                painter.line_segment(
                                                    [
                                                        origin + Vec2::new(pp.0, pp.1),
                                                        origin + Vec2::new(pt.0, pt.1),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                            prev = Some(pt);
                                            continue;
                                        }
                                    }
                                }
                                prev = None;
                            }
                        }
                    }
                }
                GeoObject::ParametricCurve3D(curve) => {
                    let stroke = Stroke::new(curve.width, to_color32(curve.color));
                    let variables = &self.document.variables;
                    let steps = 500;
                    let dt = (curve.t_max - curve.t_min) / steps as f64;
                    let ts = (0..=steps).map(|i| curve.t_min + i as f64 * dt);

                    let xs = grafito_geometry::expr::eval_batch_1d(
                        &curve.expr_x,
                        "t",
                        ts.clone(),
                        variables,
                    )
                    .unwrap_or_default();
                    let ys = grafito_geometry::expr::eval_batch_1d(
                        &curve.expr_y,
                        "t",
                        ts.clone(),
                        variables,
                    )
                    .unwrap_or_default();
                    let zs = grafito_geometry::expr::eval_batch_1d(
                        &curve.expr_z,
                        "t",
                        ts.clone(),
                        variables,
                    )
                    .unwrap_or_default();

                    let mut prev: Option<(f32, f32)> = None;
                    for i in 0..=steps {
                        if let (Some(Some(x)), Some(Some(y)), Some(Some(z))) =
                            (xs.get(i), ys.get(i), zs.get(i))
                        {
                            if let Some(pt) = self.camera.project(&Point3D::new(*x, *z, *y), w, h) {
                                if let Some(pp) = prev {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pp.0, pp.1),
                                            origin + Vec2::new(pt.0, pt.1),
                                        ],
                                        stroke,
                                    );
                                }
                                prev = Some(pt);
                                continue;
                            }
                        }
                        prev = None;
                    }
                }
                GeoObject::Attractor3D(att) => {
                    use grafito_geometry::attractors::{integrate_attractor, AttractorType};
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};

                    // Calculate hash of attractor parameters
                    let mut hasher = DefaultHasher::new();
                    att.attractor_type.hash(&mut hasher);
                    for p in &att.params {
                        p.to_bits().hash(&mut hasher);
                    }
                    att.x0.to_bits().hash(&mut hasher);
                    att.y0.to_bits().hash(&mut hasher);
                    att.z0.to_bits().hash(&mut hasher);
                    att.dt.to_bits().hash(&mut hasher);
                    att.steps.hash(&mut hasher);
                    att.skip.hash(&mut hasher);
                    let param_hash = hasher.finish();

                    // Check cache or compute new points
                    let pts = if let Some((cached_hash, cached_pts)) =
                        self.attractor_cache.get(&att.id)
                    {
                        if *cached_hash == param_hash {
                            cached_pts.clone()
                        } else {
                            let atype = match att.attractor_type.as_str() {
                                "lorenz" => {
                                    let p = &att.params;
                                    AttractorType::Lorenz {
                                        sigma: p.first().copied().unwrap_or(10.0),
                                        rho: p.get(1).copied().unwrap_or(28.0),
                                        beta: p.get(2).copied().unwrap_or(8.0 / 3.0),
                                    }
                                }
                                "rossler" => {
                                    let p = &att.params;
                                    AttractorType::Rossler {
                                        a: p.first().copied().unwrap_or(0.2),
                                        b: p.get(1).copied().unwrap_or(0.2),
                                        c: p.get(2).copied().unwrap_or(5.7),
                                    }
                                }
                                "thomas" => AttractorType::Thomas {
                                    b: att.params.first().copied().unwrap_or(0.208186),
                                },
                                "aizawa" => {
                                    let p = &att.params;
                                    AttractorType::Aizawa {
                                        a: p.first().copied().unwrap_or(0.95),
                                        b: p.get(1).copied().unwrap_or(0.7),
                                        c: p.get(2).copied().unwrap_or(0.6),
                                        d: p.get(3).copied().unwrap_or(3.5),
                                        e: p.get(4).copied().unwrap_or(0.25),
                                        f: p.get(5).copied().unwrap_or(0.1),
                                    }
                                }
                                "chen" => {
                                    let p = &att.params;
                                    AttractorType::Chen {
                                        a: p.first().copied().unwrap_or(35.0),
                                        b: p.get(1).copied().unwrap_or(3.0),
                                        c: p.get(2).copied().unwrap_or(28.0),
                                    }
                                }
                                "halvorsen" => AttractorType::Halvorsen {
                                    a: att.params.first().copied().unwrap_or(1.89),
                                },
                                "dadras" => {
                                    let p = &att.params;
                                    AttractorType::Dadras {
                                        p: p.first().copied().unwrap_or(3.0),
                                        q: p.get(1).copied().unwrap_or(2.7),
                                        r: p.get(2).copied().unwrap_or(1.7),
                                        s: p.get(3).copied().unwrap_or(2.0),
                                        e: p.get(4).copied().unwrap_or(9.0),
                                    }
                                }
                                "chua" => {
                                    let p = &att.params;
                                    AttractorType::Chua {
                                        alpha: p.first().copied().unwrap_or(15.6),
                                        beta: p.get(1).copied().unwrap_or(28.0),
                                        m0: p.get(2).copied().unwrap_or(-1.143),
                                        m1: p.get(3).copied().unwrap_or(-0.714),
                                    }
                                }
                                _ => AttractorType::lorenz(),
                            };
                            let new_pts = integrate_attractor(
                                &atype, att.x0, att.y0, att.z0, att.dt, att.steps, att.skip,
                            );
                            self.attractor_cache
                                .insert(att.id, (param_hash, new_pts.clone()));
                            new_pts
                        }
                    } else {
                        let atype = match att.attractor_type.as_str() {
                            "lorenz" => {
                                let p = &att.params;
                                AttractorType::Lorenz {
                                    sigma: p.first().copied().unwrap_or(10.0),
                                    rho: p.get(1).copied().unwrap_or(28.0),
                                    beta: p.get(2).copied().unwrap_or(8.0 / 3.0),
                                }
                            }
                            "rossler" => {
                                let p = &att.params;
                                AttractorType::Rossler {
                                    a: p.first().copied().unwrap_or(0.2),
                                    b: p.get(1).copied().unwrap_or(0.2),
                                    c: p.get(2).copied().unwrap_or(5.7),
                                }
                            }
                            "thomas" => AttractorType::Thomas {
                                b: att.params.first().copied().unwrap_or(0.208186),
                            },
                            "aizawa" => {
                                let p = &att.params;
                                AttractorType::Aizawa {
                                    a: p.first().copied().unwrap_or(0.95),
                                    b: p.get(1).copied().unwrap_or(0.7),
                                    c: p.get(2).copied().unwrap_or(0.6),
                                    d: p.get(3).copied().unwrap_or(3.5),
                                    e: p.get(4).copied().unwrap_or(0.25),
                                    f: p.get(5).copied().unwrap_or(0.1),
                                }
                            }
                            "chen" => {
                                let p = &att.params;
                                AttractorType::Chen {
                                    a: p.first().copied().unwrap_or(35.0),
                                    b: p.get(1).copied().unwrap_or(3.0),
                                    c: p.get(2).copied().unwrap_or(28.0),
                                }
                            }
                            "halvorsen" => AttractorType::Halvorsen {
                                a: att.params.first().copied().unwrap_or(1.89),
                            },
                            "dadras" => {
                                let p = &att.params;
                                AttractorType::Dadras {
                                    p: p.first().copied().unwrap_or(3.0),
                                    q: p.get(1).copied().unwrap_or(2.7),
                                    r: p.get(2).copied().unwrap_or(1.7),
                                    s: p.get(3).copied().unwrap_or(2.0),
                                    e: p.get(4).copied().unwrap_or(9.0),
                                }
                            }
                            "chua" => {
                                let p = &att.params;
                                AttractorType::Chua {
                                    alpha: p.first().copied().unwrap_or(15.6),
                                    beta: p.get(1).copied().unwrap_or(28.0),
                                    m0: p.get(2).copied().unwrap_or(-1.143),
                                    m1: p.get(3).copied().unwrap_or(-0.714),
                                }
                            }
                            _ => AttractorType::lorenz(),
                        };
                        let new_pts = integrate_attractor(
                            &atype, att.x0, att.y0, att.z0, att.dt, att.steps, att.skip,
                        );
                        self.attractor_cache
                            .insert(att.id, (param_hash, new_pts.clone()));
                        new_pts
                    };

                    let stroke = Stroke::new(att.width, to_color32(att.color));
                    let mut prev: Option<(f32, f32)> = None;
                    for pt in &pts {
                        let scaled_pt = Point3D::new(pt.x * 0.2, pt.y * 0.2, pt.z * 0.2);
                        if let Some(sp) = self.camera.project(&scaled_pt, w, h) {
                            if let Some(pp) = prev {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pp.0, pp.1),
                                        origin + Vec2::new(sp.0, sp.1),
                                    ],
                                    stroke,
                                );
                            }
                            prev = Some(sp);
                        } else {
                            prev = None;
                        }
                    }

                    if !att.label.is_empty() {
                        if let Some(first) = pts.first() {
                            if let Some(pt) = self.camera.project(first, w, h) {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1 - 10.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &att.label,
                                    egui::FontId::proportional(12.0),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::HyperSurface4D(hs) => {
                    let stroke = Stroke::new(hs.width, to_color32(hs.color));
                    let angles = &hs.rotation_angles;
                    let a_xy = angles.first().copied().unwrap_or(0.3);
                    let a_xz = angles.get(1).copied().unwrap_or(0.5);
                    let a_xw = angles.get(2).copied().unwrap_or(0.7);
                    let scale = hs.params.first().copied().unwrap_or(1.0);
                    match hs.surface_type.as_str() {
                        "hypercube" => {
                            let mut verts_4d: Vec<[f64; 4]> = Vec::with_capacity(16);
                            for i in 0..16u8 {
                                verts_4d.push([
                                    if i & 1 != 0 { scale } else { -scale },
                                    if i & 2 != 0 { scale } else { -scale },
                                    if i & 4 != 0 { scale } else { -scale },
                                    if i & 8 != 0 { scale } else { -scale },
                                ]);
                            }
                            let cos_xy = a_xy.cos();
                            let sin_xy = a_xy.sin();
                            let cos_xz = a_xz.cos();
                            let sin_xz = a_xz.sin();
                            let cos_xw = a_xw.cos();
                            let sin_xw = a_xw.sin();
                            let projected: Vec<Point3D> = verts_4d
                                .iter()
                                .map(|v| {
                                    let mut p = *v;
                                    let nx = p[0] * cos_xy - p[1] * sin_xy;
                                    let ny = p[0] * sin_xy + p[1] * cos_xy;
                                    p[0] = nx;
                                    p[1] = ny;
                                    let nx = p[0] * cos_xz - p[2] * sin_xz;
                                    let nz = p[0] * sin_xz + p[2] * cos_xz;
                                    p[0] = nx;
                                    p[2] = nz;
                                    let nx = p[0] * cos_xw - p[3] * sin_xw;
                                    let nw = p[0] * sin_xw + p[3] * cos_xw;
                                    p[0] = nx;
                                    p[3] = nw;
                                    let w_factor = 1.0 / (3.0 - p[3] / scale);
                                    Point3D::new(p[0] * w_factor, p[1] * w_factor, p[2] * w_factor)
                                })
                                .collect();
                            let edges: Vec<(usize, usize)> = (0..16usize)
                                .flat_map(|i| {
                                    (0..16usize)
                                        .filter(move |&j| j > i && (i ^ j).count_ones() == 1)
                                        .map(move |j| (i, j))
                                })
                                .collect();
                            for &(a, b) in &edges {
                                if let (Some(pa), Some(pb)) = (
                                    self.camera.project(&projected[a], w, h),
                                    self.camera.project(&projected[b], w, h),
                                ) {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pa.0, pa.1),
                                            origin + Vec2::new(pb.0, pb.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                            if !hs.label.is_empty() {
                                if let Some(pt) = self.camera.project(&projected[0], w, h) {
                                    painter.text(
                                        origin + Vec2::new(pt.0, pt.1 - 10.0),
                                        egui::Align2::CENTER_BOTTOM,
                                        &hs.label,
                                        egui::FontId::proportional(12.0),
                                        label_color,
                                    );
                                }
                            }
                        }
                        "hypersphere" => {
                            let res = hs.resolution.clamp(8, 30);
                            let mut pts_3d: Vec<Vec<Point3D>> = Vec::new();
                            for i in 0..=res {
                                let phi = std::f64::consts::PI * i as f64 / res as f64;
                                let mut ring = Vec::new();
                                for j in 0..=res * 2 {
                                    let theta = std::f64::consts::TAU * j as f64 / (res * 2) as f64;
                                    let mut p = [
                                        scale * phi.sin() * theta.cos(),
                                        scale * phi.sin() * theta.sin(),
                                        scale * phi.cos(),
                                        0.0,
                                    ];
                                    let cos_xy = a_xy.cos();
                                    let sin_xy = a_xy.sin();
                                    let nx = p[0] * cos_xy - p[1] * sin_xy;
                                    let ny = p[0] * sin_xy + p[1] * cos_xy;
                                    p[0] = nx;
                                    p[1] = ny;
                                    let cos_xw = a_xw.cos();
                                    let sin_xw = a_xw.sin();
                                    let nx2 = p[0] * cos_xw - p[3] * sin_xw;
                                    let nw = p[0] * sin_xw + p[3] * cos_xw;
                                    p[0] = nx2;
                                    p[3] = nw;
                                    let w_factor = 1.0 / (3.0 - p[3] / scale);
                                    ring.push(Point3D::new(
                                        p[0] * w_factor,
                                        p[1] * w_factor,
                                        p[2] * w_factor,
                                    ));
                                }
                                pts_3d.push(ring);
                            }
                            for ring in &pts_3d {
                                let mut prev: Option<(f32, f32)> = None;
                                for pt in ring {
                                    if let Some(sp) = self.camera.project(pt, w, h) {
                                        if let Some(pp) = prev {
                                            painter.line_segment(
                                                [
                                                    origin + Vec2::new(pp.0, pp.1),
                                                    origin + Vec2::new(sp.0, sp.1),
                                                ],
                                                stroke,
                                            );
                                        }
                                        prev = Some(sp);
                                    } else {
                                        prev = None;
                                    }
                                }
                            }
                            for j in 0..pts_3d.first().map(|r| r.len()).unwrap_or(0) {
                                let mut prev: Option<(f32, f32)> = None;
                                for ring in &pts_3d {
                                    if let Some(pt) = ring.get(j) {
                                        if let Some(sp) = self.camera.project(pt, w, h) {
                                            if let Some(pp) = prev {
                                                painter.line_segment(
                                                    [
                                                        origin + Vec2::new(pp.0, pp.1),
                                                        origin + Vec2::new(sp.0, sp.1),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                            prev = Some(sp);
                                        } else {
                                            prev = None;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                GeoObject::VectorField3D(vf) => {
                    let stroke = Stroke::new(1.0, to_color32(vf.color));
                    let variables = &self.document.variables;
                    let n = vf.density.clamp(2, 10);
                    let dx = (vf.x_max - vf.x_min) / n as f64;
                    let dy = (vf.y_max - vf.y_min) / n as f64;
                    let dz = (vf.z_max - vf.z_min) / n as f64;
                    let arrow_scale = dx.min(dy.min(dz)) * 0.4;
                    let vars: Vec<(String, f64)> =
                        variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    for i in 0..=n {
                        for j in 0..=n {
                            for k in 0..=n {
                                let x = vf.x_min + i as f64 * dx;
                                let y = vf.y_min + j as f64 * dy;
                                let z = vf.z_min + k as f64 * dz;
                                let mut local_vars = vars.clone();
                                local_vars.push(("x".into(), x));
                                local_vars.push(("y".into(), y));
                                local_vars.push(("z".into(), z));
                                let u = grafito_geometry::expr::evaluate(&vf.expr_u, &local_vars)
                                    .unwrap_or(0.0);
                                let v = grafito_geometry::expr::evaluate(&vf.expr_v, &local_vars)
                                    .unwrap_or(0.0);
                                let w_val =
                                    grafito_geometry::expr::evaluate(&vf.expr_w, &local_vars)
                                        .unwrap_or(0.0);
                                if !u.is_finite() || !v.is_finite() || !w_val.is_finite() {
                                    continue;
                                }
                                let mag = (u * u + v * v + w_val * w_val).sqrt();
                                if mag < 1e-10 {
                                    continue;
                                }
                                let nu = u / mag * arrow_scale;
                                let nv = v / mag * arrow_scale;
                                let nw = w_val / mag * arrow_scale;
                                let start = Point3D::new(x, z, y);
                                let end = Point3D::new(x + nu, z + nw, y + nv);
                                if let (Some(pa), Some(pb)) = (
                                    self.camera.project(&start, w, h),
                                    self.camera.project(&end, w, h),
                                ) {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pa.0, pa.1),
                                            origin + Vec2::new(pb.0, pb.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
