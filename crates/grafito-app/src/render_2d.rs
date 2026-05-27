use crate::GrafitoApp;
use grafito_core::{
    GeoObject,
    PointObj, LineObj, CircleObj,
};
use grafito_geometry::{Point2, Color};
use grafito_geometry::expr::eval_function_with_vars;
use grafito_geometry::interval;
use grafito_ui::Tool;
use egui::{Pos2, Vec2, Stroke, Shape, Color32, Rect, Sense};
use glam::Vec2 as GlamVec2;
use rayon::prelude::*;

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

impl GrafitoApp {
    pub(crate) fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        let response = ui.interact(canvas_rect, ui.id().with("canvas"), Sense::click_and_drag());

        self.document.view_mut().screen_size = GlamVec2::new(canvas_rect.width(), canvas_rect.height());

        if let Some(pos) = response.hover_pos() {
            let local = pos - canvas_rect.min;
            let world = self.document.view().screen_to_world(GlamVec2::new(local.x, local.y));

            if response.clicked() {
                match self.current_tool {
                    Tool::Select => {
                        let tolerance = 10.0 / self.document.view().scale;
                        if let Some(id) = self.document.pick_object(world, tolerance as f64) {
                            self.document.clear_selection();
                            self.document.select(id);
                            self.selected_object = Some(id);
                        } else {
                            self.document.clear_selection();
                            self.selected_object = None;
                        }
                    }
                    Tool::Point => {
                        self.save_state();
                        self.document.add_object(GeoObject::Point(PointObj::new(world)));
                    }
                    Tool::Line | Tool::Circle | Tool::Polygon => {
                        self.pending_points.push(world);
                        if self.current_tool == Tool::Line && self.pending_points.len() == 2 {
                            let a = self.pending_points[0];
                            let b = self.pending_points[1];
                            self.save_state();
                            self.document.add_object(GeoObject::Line(LineObj::new(a, b)));
                            self.pending_points.clear();
                        } else if self.current_tool == Tool::Circle && self.pending_points.len() == 2 {
                            let center = self.pending_points[0];
                            let edge = self.pending_points[1];
                            let radius = center.distance(&edge);
                            self.save_state();
                            self.document.add_object(GeoObject::Circle(CircleObj::new(center, radius)));
                            self.pending_points.clear();
                        }
                    }
                    Tool::Function => {}
                }
            }

            if response.dragged() {
                if let Some(last) = self.last_mouse_pos {
                    let delta = pos - last;
                    self.document.view_mut().pan(GlamVec2::new(delta.x, delta.y));
                }
            }

            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta);
                if scroll.y != 0.0 {
                    let factor = 1.0 + scroll.y * 0.001;
                    self.document.view_mut().zoom(
                        factor.clamp(0.5, 2.0),
                        GlamVec2::new(local.x, local.y),
                    );
                }
            }

            self.last_mouse_pos = Some(pos);
        }
    }

    pub(crate) fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if !self.show_grid { return; }
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let min_x = world_tl.x.floor() as i32 - 1;
        let max_x = world_br.x.ceil() as i32 + 1;
        let min_y = world_br.y.floor() as i32 - 1;
        let max_y = world_tl.y.ceil() as i32 + 1;

        let color = to_color32(Color::LIGHT_GRAY);
        let stroke = Stroke::new(1.0, color);

        for x in min_x..=max_x {
            let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
            let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
            painter.line_segment(
                [canvas_rect.min + Vec2::new(a.x, a.y), canvas_rect.min + Vec2::new(b.x, b.y)],
                stroke,
            );
        }

        for y in min_y..=max_y {
            let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
            let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
            painter.line_segment(
                [canvas_rect.min + Vec2::new(a.x, a.y), canvas_rect.min + Vec2::new(b.x, b.y)],
                stroke,
            );
        }
    }

    pub(crate) fn draw_axes(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let stroke = Stroke::new(2.0, Color32::BLACK);

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        painter.line_segment(
            [canvas_rect.min + Vec2::new(x_axis_a.x, x_axis_a.y), canvas_rect.min + Vec2::new(x_axis_b.x, x_axis_b.y)],
            stroke,
        );

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        painter.line_segment(
            [canvas_rect.min + Vec2::new(y_axis_a.x, y_axis_a.y), canvas_rect.min + Vec2::new(y_axis_b.x, y_axis_b.y)],
            stroke,
        );
    }

    pub(crate) fn draw_objects(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let view = self.document.view();
        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point(p) => {
                    let screen = view.world_to_screen(p.position);
                    let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                    let size = p.size.max(1.0);
                    let color = to_color32(p.color);
                    painter.circle_filled(pos, size, color);
                    if !p.label.is_empty() {
                        painter.text(
                            pos + Vec2::new(size + 2.0, -size - 2.0),
                            egui::Align2::LEFT_BOTTOM,
                            &p.label,
                            egui::FontId::proportional(12.0),
                            Color32::BLACK,
                        );
                    }
                }
                GeoObject::Line(l) => {
                    let a = view.world_to_screen(l.start);
                    let b = view.world_to_screen(l.end);
                    let stroke = Stroke::new(l.width, to_color32(l.color));
                    painter.line_segment(
                        [canvas_rect.min + Vec2::new(a.x, a.y), canvas_rect.min + Vec2::new(b.x, b.y)],
                        stroke,
                    );
                    if !l.label.is_empty() {
                        let mid = (a + b) * 0.5;
                        painter.text(
                            canvas_rect.min + Vec2::new(mid.x, mid.y) + Vec2::new(0.0, -8.0),
                            egui::Align2::CENTER_BOTTOM,
                            &l.label,
                            egui::FontId::proportional(12.0),
                            Color32::BLACK,
                        );
                    }
                }
                GeoObject::Circle(c) => {
                    let center = view.world_to_screen(c.center);
                    let radius = (c.radius as f32) * view.scale;
                    let pos = canvas_rect.min + Vec2::new(center.x, center.y);
                    let stroke = Stroke::new(c.width, to_color32(c.color));
                    if let Some(fill) = c.fill_color {
                        painter.circle_filled(pos, radius, to_color32(fill));
                    }
                    painter.circle_stroke(pos, radius, stroke);
                    if !c.label.is_empty() {
                        painter.text(
                            pos + Vec2::new(radius + 2.0, -radius - 2.0),
                            egui::Align2::LEFT_BOTTOM,
                            &c.label,
                            egui::FontId::proportional(12.0),
                            Color32::BLACK,
                        );
                    }
                }
                GeoObject::Polygon(poly) => {
                    if poly.vertices.len() >= 3 {
                        let points: Vec<_> = poly.vertices.iter()
                            .map(|v| {
                                let s = view.world_to_screen(*v);
                                canvas_rect.min + Vec2::new(s.x, s.y)
                            })
                            .collect();
                        let stroke = Stroke::new(poly.width, to_color32(poly.color));
                        if let Some(fill) = poly.fill_color {
                            painter.add(Shape::convex_polygon(points.clone(), to_color32(fill), stroke));
                        } else {
                            painter.add(Shape::convex_polygon(points.clone(), Color32::TRANSPARENT, stroke));
                        }
                        if !poly.label.is_empty() {
                            let cx: f32 = points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32;
                            let cy: f32 = points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32;
                            painter.text(
                                Pos2::new(cx, cy),
                                egui::Align2::CENTER_CENTER,
                                &poly.label,
                                egui::FontId::proportional(12.0),
                                Color32::BLACK,
                            );
                        }
                    }
                }
                GeoObject::Function(fun) => {
                    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                    let min_x = fun.domain_min.unwrap_or(world_tl.x);
                    let max_x = fun.domain_max.unwrap_or(world_br.x);
                    let steps = 1000;
                    let step = (max_x - min_x) / steps as f64;
                    let variables = &self.document.variables;

                    let samples: Vec<(f64, Option<f64>)> = (0..=steps).into_par_iter().map(|i| {
                        let x = min_x + i as f64 * step;
                        let y = eval_function_with_vars(&fun.expr, x, variables).ok()
                            .filter(|v| v.is_finite() && v.abs() < 1e6);
                        (x, y)
                    }).collect();

                    let asymptotes = interval::detect_asymptotes(&samples);

                    let stroke = Stroke::new(fun.width, to_color32(fun.color));
                    let mut prev_screen: Option<Pos2> = None;
                    for (x, y_opt) in &samples {
                        if let Some(y) = y_opt {
                            let s = view.world_to_screen(Point2::new(*x, *y));
                            let p = canvas_rect.min + Vec2::new(s.x, s.y);
                            if let Some(prev) = prev_screen {
                                let gap = (p.x - prev.x).abs();
                                if gap < 300.0 {
                                    painter.line_segment([prev, p], stroke);
                                }
                            }
                            prev_screen = Some(p);
                        } else {
                            prev_screen = None;
                        }
                    }

                    for ax in &asymptotes {
                        let s = view.world_to_screen(Point2::new(*ax, 0.0));
                        let dash_stroke = Stroke::new(1.0, Color32::from_rgb(180, 100, 100));
                        painter.line_segment([
                            canvas_rect.min + Vec2::new(s.x, 0.0),
                            canvas_rect.min + Vec2::new(s.x, canvas_rect.height()),
                        ], dash_stroke);
                    }
                    if !fun.label.is_empty() {
                        let mid_x = (min_x + max_x) * 0.5;
                        if let Ok(y) = eval_function_with_vars(&fun.expr, mid_x, &self.document.variables) {
                            let s = view.world_to_screen(Point2::new(mid_x, y));
                            painter.text(
                                canvas_rect.min + Vec2::new(s.x, s.y + 14.0),
                                egui::Align2::CENTER_TOP,
                                &fun.label,
                                egui::FontId::proportional(12.0),
                                Color32::BLACK,
                            );
                        }
                    }
                }
                GeoObject::Ellipse(el) => {
                    let stroke = Stroke::new(el.width, to_color32(el.color));
                    let n = 64;
                    let mut pts = Vec::with_capacity(n);
                    for i in 0..n {
                        let t = i as f64 / n as f64 * std::f64::consts::TAU;
                        let x = el.center.x + el.rx * t.cos() * el.angle.cos() - el.ry * t.sin() * el.angle.sin();
                        let y = el.center.y + el.rx * t.cos() * el.angle.sin() + el.ry * t.sin() * el.angle.cos();
                        let s = view.world_to_screen(Point2::new(x, y));
                        pts.push(canvas_rect.min + Vec2::new(s.x, s.y));
                    }
                    if let Some(fill) = el.fill_color {
                        painter.add(Shape::convex_polygon(pts.clone(), to_color32(fill), Stroke::NONE));
                    }
                    for i in 0..n {
                        let j = (i + 1) % n;
                        painter.line_segment([pts[i], pts[j]], stroke);
                    }
                    if !el.label.is_empty() {
                        let s = view.world_to_screen(el.center);
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y + el.ry as f32 * view.scale + 14.0),
                            egui::Align2::CENTER_TOP, &el.label, egui::FontId::proportional(12.0), Color32::BLACK);
                    }
                }
                GeoObject::Parabola(pb) => {
                    let _v = view.world_to_screen(pb.vertex);
                    let stroke = Stroke::new(pb.width, to_color32(pb.color));
                    let steps = 64;
                    let x_range = 10.0 / view.scale as f64;
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=steps {
                        let t = -x_range + 2.0 * x_range * i as f64 / steps as f64;
                        let _x = pb.vertex.x + t;
                        let _y = if pb.vertical {
                            pb.vertex.y + t * t / (4.0 * pb.p.max(0.001))
                        } else {
                            pb.vertex.y + t
                        };
                        let (sx, sy) = if pb.vertical {
                            (pb.vertex.x + t, pb.vertex.y + t*t / (4.0 * pb.p.max(0.001)))
                        } else {
                            (pb.vertex.x + t*t / (4.0 * pb.p.max(0.001)), pb.vertex.y + t)
                        };
                        let s = view.world_to_screen(Point2::new(sx, sy));
                        let p = canvas_rect.min + Vec2::new(s.x, s.y);
                        if let Some(prev_p) = prev {
                            if (p.x - prev_p.x).abs() < 300.0 {
                                painter.line_segment([prev_p, p], stroke);
                            }
                        }
                        prev = Some(p);
                    }
                    if !pb.label.is_empty() {
                        let s = view.world_to_screen(Point2::new(pb.vertex.x, pb.vertex.y - 1.0));
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y - 8.0),
                            egui::Align2::CENTER_BOTTOM, &pb.label, egui::FontId::proportional(12.0), Color32::BLACK);
                    }
                }
                GeoObject::Hyperbola(hb) => {
                    let stroke = Stroke::new(hb.width, to_color32(hb.color));
                    let range = 8.0 / view.scale as f64;
                    let n = 64;
                    // Right branch
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=n {
                        let x = hb.center.x + hb.a + range * i as f64 / n as f64;
                        let dx = x - hb.center.x;
                        if dx > hb.a {
                            let y_off = hb.b * ((dx/hb.a).powi(2) - 1.0).sqrt();
                            for &sign in &[1.0f64, -1.0] {
                                let y = hb.center.y + sign * y_off;
                                let s = view.world_to_screen(Point2::new(x, y));
                                let p = canvas_rect.min + Vec2::new(s.x, s.y);
                                if let Some(prev_p) = prev {
                                    if (p.x - prev_p.x).abs() < 300.0 {
                                        painter.line_segment([prev_p, p], stroke);
                                    }
                                }
                                prev = Some(p);
                            }
                        }
                    }
                    // Left branch
                    prev = None;
                    for i in 0..=n {
                        let x = hb.center.x - hb.a - range * i as f64 / n as f64;
                        let dx = (x - hb.center.x).abs();
                        if dx > hb.a {
                            let y_off = hb.b * ((dx/hb.a).powi(2) - 1.0).sqrt();
                            for &sign in &[1.0f64, -1.0] {
                                let y = hb.center.y + sign * y_off;
                                let s = view.world_to_screen(Point2::new(x, y));
                                let p = canvas_rect.min + Vec2::new(s.x, s.y);
                                if let Some(prev_p) = prev {
                                    if (p.x - prev_p.x).abs() < 300.0 { painter.line_segment([prev_p, p], stroke); }
                                }
                                prev = Some(p);
                            }
                        }
                    }
                    if !hb.label.is_empty() {
                        let s = view.world_to_screen(Point2::new(hb.center.x, hb.center.y + hb.b + 0.5));
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y), egui::Align2::CENTER_BOTTOM, &hb.label, egui::FontId::proportional(12.0), Color32::BLACK);
                    }
                }
                GeoObject::Text(txt) => {
                    let s = view.world_to_screen(txt.position);
                    painter.text(canvas_rect.min + Vec2::new(s.x, s.y),
                        egui::Align2::LEFT_CENTER, &txt.content,
                        egui::FontId::proportional(txt.font_size.max(8.0)), to_color32(txt.color));
                }
                _ => {}
            }
        }
    }
}
