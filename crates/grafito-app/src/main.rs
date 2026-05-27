use grafito_core::{Document, GeoObject, ObjectId,
    PointObj, LineObj, CircleObj, PolygonObj, FunctionObj, EllipseObj,
    ParabolaObj, HyperbolaObj,
    Point3DObj, Segment3DObj, Sphere3DObj, Cube3DObj,
    Pyramid3DObj, Surface3DObj,
    SpatialIndex, ConstraintGraph,
};
use grafito_geometry::{Point2, Point3D, ViewTransform, Camera3D, Color};
use grafito_geometry::expr::{eval_function_with_vars, evaluate};
use grafito_geometry::{symbolic, interval};
use grafito_ui::{Tool, algebra_view, properties_panel, toolbar};
use egui::{Pos2, Vec2, Stroke, Shape, Color32, Rect, Sense, Key};
use glam::{Vec2 as GlamVec2, Vec3};
use std::collections::HashMap;
use std::fs;
use rayon::prelude::*;

const MAX_UNDO: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode { D2, D3 }

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

struct GrafitoApp {
    document: Document,
    current_tool: Tool,
    current_view: ViewMode,
    camera: Camera3D,
    animation_running: bool,
    show_grid: bool,
    snap_to_grid: bool,
    exam_mode: bool,
    spatial_index: SpatialIndex,
    constraint_graph: ConstraintGraph,
    pending_points: Vec<Point2>,
    pending_points_3d: Vec<Point3D>,
    last_mouse_pos: Option<Pos2>,
    #[allow(dead_code)]
    hovered_object: Option<ObjectId>,
    selected_object: Option<ObjectId>,
    input_text: String,
    cas_result: String,
    recent_files: Vec<String>,
    undo_stack: Vec<Document>,
    redo_stack: Vec<Document>,
}

impl GrafitoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(1280.0, 720.0));

        // Demo objects
        document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("A")));
        document.add_object(GeoObject::Point(PointObj::new(Point2::new(3.0, 2.0)).with_label("B")));
        document.add_object(GeoObject::Line(LineObj::new(Point2::new(-2.0, -1.0), Point2::new(4.0, 3.0)).with_label("l")));
        document.add_object(GeoObject::Circle(CircleObj::new(Point2::new(1.0, 1.0), 2.0).with_label("c")));
        document.add_object(GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(-3.0, -2.0),
            Point2::new(-1.0, -3.0),
            Point2::new(-2.0, -1.0),
        ])));
        document.add_object(GeoObject::Function(FunctionObj::new("sin(x)").with_label("f(x)")));
        document.set_variable("a".into(), 2.0);
        document.add_object(GeoObject::Function(FunctionObj::new("a*sin(x)").with_label("g(x)")));
        // 3D demo
        document.add_object(GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0).with_label("C1")));
        document.add_object(GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(2.0, 1.0, 0.0), 1.0).with_label("S1")));
        document.add_object(GeoObject::Ellipse(EllipseObj::new(Point2::new(-1.0, -2.0), 2.0, 1.0).with_label("E1")));

        Self {
            document,
            current_tool: Tool::default(),
            current_view: ViewMode::D2,
            camera: Camera3D::new(1280.0 / 720.0),
            animation_running: false,
            show_grid: true,
            snap_to_grid: false,
            exam_mode: false,
            spatial_index: SpatialIndex::new(),
            constraint_graph: ConstraintGraph::new(),
            pending_points: Vec::new(),
            pending_points_3d: Vec::new(),
            last_mouse_pos: None,
            hovered_object: None,
            selected_object: None,
            input_text: String::new(),
            cas_result: String::new(),
            recent_files: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    fn save_state(&mut self) {
        self.undo_stack.push(self.document.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.document.clone());
            self.document = prev;
            self.selected_object = None;
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.document.clone());
            self.document = next;
            self.selected_object = None;
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected_object {
            self.save_state();
            self.document.remove_object(id);
            self.selected_object = None;
        }
    }

    fn zoom_to_fit(&mut self) {
        let mut bounds: Option<(Point2, Point2)> = None;
        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() { continue; }
            let pts = match obj {
                GeoObject::Point(p) => vec![p.position],
                GeoObject::Line(l) => vec![l.start, l.end],
                GeoObject::Circle(c) => vec![
                    Point2::new(c.center.x - c.radius, c.center.y - c.radius),
                    Point2::new(c.center.x + c.radius, c.center.y + c.radius),
                ],
                GeoObject::Polygon(poly) => poly.vertices.clone(),
                _ => vec![],
            };
            for pt in pts {
                match bounds {
                    None => bounds = Some((pt, pt)),
                    Some((ref mut min, ref mut max)) => {
                        min.x = min.x.min(pt.x); min.y = min.y.min(pt.y);
                        max.x = max.x.max(pt.x); max.y = max.y.max(pt.y);
                    }
                }
            }
        }
        if let Some((min, max)) = bounds {
            let sw = self.document.view().screen_size.x;
            let sh = self.document.view().screen_size.y;
            let margin = 1.2;
            let w = (max.x - min.x).max(0.1) * margin;
            let h = (max.y - min.y).max(0.1) * margin;
            let scale = (sw / w as f32).min(sh / h as f32);
            let cx = (min.x + max.x) * 0.5;
            let cy = (min.y + max.y) * 0.5;
            self.document.view_mut().scale = scale;
            self.document.view_mut().offset = GlamVec2::new(
                -cx as f32 * scale,
                cy as f32 * scale,
            );
        }
    }

    fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        let response = ui.interact(canvas_rect, ui.id().with("canvas"), Sense::click_and_drag());

        // Update view size to match canvas
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

    fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
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

    fn draw_axes(&self, painter: &egui::Painter, canvas_rect: Rect) {
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

    fn draw_objects(&self, painter: &egui::Painter, canvas_rect: Rect) {
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

                    // Parallel evaluation with rayon
                    let samples: Vec<(f64, Option<f64>)> = (0..=steps).into_par_iter().map(|i| {
                        let x = min_x + i as f64 * step;
                        let y = eval_function_with_vars(&fun.expr, x, variables).ok()
                            .filter(|v| v.is_finite() && v.abs() < 1e6);
                        (x, y)
                    }).collect();

                    // Detect asymptotes for safety
                    let asymptotes = interval::detect_asymptotes(&samples);

                    let stroke = Stroke::new(fun.width, to_color32(fun.color));
                    let mut prev_screen: Option<Pos2> = None;
                    for (x, y_opt) in &samples {
                        if let Some(y) = y_opt {
                            let s = view.world_to_screen(Point2::new(*x, *y));
                            let p = canvas_rect.min + Vec2::new(s.x, s.y);
                            if let Some(prev) = prev_screen {
                                // Check asymptote gap
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

                    // Draw asymptotes as dashed vertical lines
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
                            // Actually, let me swap: y = vertex.y + t, x = vertex.x + t^2/4p
                            pb.vertex.y + t
                        };
                        // Simpler: x = vertex.x + t*cos - t^2/4p*sin, y = vertex.y + t*sin + t^2/4p*cos
                        // For vertical: x = vx + t, y = vy + t^2/(4p)
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
                _ => {}  // 3D + other objects handled in respective views
            }
        }
    }
    fn handle_3d_click(&mut self, _ui: &egui::Ui, _response: &egui::Response, _canvas: Rect, w: f32, h: f32) {
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

    fn draw_3d_grid(&self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
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

    fn draw_3d_objects(&self, painter: &egui::Painter, canvas: Rect, w: f32, h: f32) {
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
                            // Just draw 4 vertical lines
                            let _dir = (cyl.top_center.to_vec3() - cyl.base_center.to_vec3()).normalize();
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

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if self.exam_mode { ui.label("Disabled in Exam Mode"); return; }
                    if ui.button("Open...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Grafito Document", &["grafito", "json", "toml"])
                            .pick_file()
                        {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(doc) = serde_json::from_str::<Document>(&content) {
                                    self.document = doc;
                                    self.undo_stack.clear();
                                    self.redo_stack.clear();
                                    let p = path.to_string_lossy().to_string();
                                    if !self.recent_files.contains(&p) {
                                        self.recent_files.insert(0, p);
                                        self.recent_files.truncate(8);
                                    }
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Grafito Document", &["grafito"])
                            .set_file_name("grafito.grafito")
                            .save_file()
                        {
                            if let Ok(json) = serde_json::to_string_pretty(&self.document) {
                                let _ = fs::write(path, json);
                            }
                        }
                        ui.close_menu();
                    }
                    if !self.recent_files.is_empty() {
                        ui.separator();
                        ui.label("Recent:");
                        for f in self.recent_files.clone() {
                            let name = std::path::Path::new(&f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(f.clone());
                            if ui.button(&name).clicked() {
                                if let Ok(content) = fs::read_to_string(&f) {
                                    if let Ok(doc) = serde_json::from_str::<Document>(&content) {
                                        self.document = doc;
                                        self.undo_stack.clear(); self.redo_stack.clear();
                                    }
                                }
                                ui.close_menu();
                            }
                        }
                    }
                    if ui.button("Export SVG...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("SVG Image", &["svg"])
                            .set_file_name("grafito.svg")
                            .save_file()
                        {
                            let svg = export_svg(&self.document);
                            let _ = fs::write(path, svg);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Export TikZ...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("LaTeX TikZ", &["tex", "tikz"])
                            .set_file_name("grafito.tex")
                            .save_file()
                        {
                            let tikz = export_tikz(&self.document);
                            let _ = fs::write(path, tikz);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Export PNG...").clicked() {
                        let w = self.document.view().screen_size.x as u32;
                        let h = self.document.view().screen_size.y as u32;
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG Image", &["png"])
                            .set_file_name("grafito.png")
                            .save_file()
                        {
                            let img = export_png(&self.document, w, h);
                            let _ = img.save(path);
                        }
                        ui.close_menu();
                    }
            });
            if !self.cas_result.is_empty() {
                ui.separator();
                ui.colored_label(Color32::from_rgb(40, 120, 40), &self.cas_result);
            }
        });
        });

        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.undo();
        }
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
            || ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl)
        {
            self.redo();
        }
        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.delete_selected();
        }

        // Top toolbar with view mode tabs
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_view, ViewMode::D2, "2D");
                ui.selectable_value(&mut self.current_view, ViewMode::D3, "3D");
                ui.separator();
                toolbar(ui, &mut self.current_tool);
                ui.separator();
                if ui.button("⛶ Fit").clicked() { self.zoom_to_fit(); }
                ui.checkbox(&mut self.show_grid, "Grid");
                ui.checkbox(&mut self.snap_to_grid, "Snap");
                if ui.checkbox(&mut self.exam_mode, "Exam").changed() && self.exam_mode {
                    self.cas_result = "EXAM MODE: CAS disabled".into();
                }
            });
        });

        // Left: Algebra View + Variables + Sliders
        egui::SidePanel::left("algebra").default_width(200.0).show(ctx, |ui| {
            algebra_view(ui, &self.document, &mut self.selected_object);
            if let Some(id) = self.selected_object {
                ui.separator();
                properties_panel(ui, &mut self.document, id);
            }
            // Variables and sliders
            if !self.document.variables.is_empty() {
                ui.separator();
                ui.heading("Variables");
                ui.checkbox(&mut self.animation_running, "Animation");
                let vars: Vec<(String, f64)> = self.document.variables.clone().into_iter().collect();
                for (name, val) in &vars {
                    let mut v = *val;
                    let range = -10.0..=10.0;
                    ui.horizontal(|ui| {
                        ui.label(name);
                        if ui.add(egui::Slider::new(&mut v, range).step_by(0.1)).changed() {
                            self.document.set_variable(name.clone(), v);
                        }
                    });
                }
                // Auto-increment on animation
                if self.animation_running {
                    for (name, _) in &vars {
                        if let Some(v) = self.document.variables.get(name) {
                            let new_val = (v + 0.02) % 20.0 - 10.0;
                            self.document.set_variable(name.clone(), new_val);
                        }
                    }
                    ctx.request_repaint();
                }
            }
        });

        // Right: Spreadsheet View
        egui::SidePanel::right("spreadsheet").resizable(true).default_width(280.0).show(ctx, |ui| {
            ui.heading("Spreadsheet");
            ui.separator();
            let (rows, cols) = self.document.spreadsheet_dim();
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("sp_grid").striped(true).show(ui, |ui| {
                    ui.label(""); // top-left corner
                    for c in 0..cols { ui.monospace(format!(" {}", (b'A' + c as u8) as char)); }
                    ui.end_row();
                    for r in 0..rows {
                        ui.monospace(format!("{}", r + 1));
                        for c in 0..cols {
                            let mut val = self.document.get_spreadsheet_cell(r, c);
                            let resp = ui.add_sized([60.0, 18.0], egui::TextEdit::singleline(&mut val).font(egui::TextStyle::Monospace));
                            if resp.changed() {
                                self.save_state();
                                self.document.set_spreadsheet_cell(r, c, val.clone());
                                // If cell contains coordinate => create point
                                if let Ok((x, y)) = parse_point_str(&val) {
                                    self.document.add_object(GeoObject::Point(PointObj::new(Point2::new(x, y)).with_label(format!("{}{}", (b'A' + c as u8) as char, r + 1))));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });

        // Bottom: Input Bar
        egui::TopBottomPanel::bottom("input_bar").default_height(40.0).show(ctx, |ui| {
            if self.exam_mode {
                ui.label("EXAM MODE — input disabled");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("Input:");
                let response = ui.text_edit_singleline(&mut self.input_text);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.save_state();
                    self.cas_result = process_input(&mut self.document, &mut self.input_text).unwrap_or_default();
                }
                if ui.button("Enter").clicked() {
                    self.save_state();
                    self.cas_result = process_input(&mut self.document, &mut self.input_text).unwrap_or_default();
                }
            });
        });

        match self.current_view {
            ViewMode::D2 => {
                self.camera.aspect = 1.6; // keep camera alive, update if needed
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas_rect = ui.available_rect_before_wrap();
                    self.handle_canvas_input(ui, canvas_rect);

                    // Right-click context menu
                    let response = ui.interact(canvas_rect, ui.id().with("ctx_menu"), Sense::click());
                    if response.clicked_by(egui::PointerButton::Secondary) {
                        response.context_menu(|ui| {
                            if ui.button("Delete selected").clicked() {
                                self.delete_selected();
                                ui.close_menu();
                            }
                            if ui.button("Zoom to fit").clicked() {
                                self.zoom_to_fit();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Reset view").clicked() {
                                let _sw = self.document.view().screen_size.x;
                                let _sh = self.document.view().screen_size.y;
                                self.document.view_mut().scale = 1.0;
                                self.document.view_mut().offset = GlamVec2::ZERO;
                                ui.close_menu();
                            }
                            ui.checkbox(&mut self.show_grid, "Show Grid");
                            ui.checkbox(&mut self.snap_to_grid, "Snap to Grid");
                        });
                    }

                    let painter = ui.painter();
                    self.draw_grid(painter, canvas_rect);
                    self.draw_axes(painter, canvas_rect);
                    self.draw_objects(painter, canvas_rect);
                });
            }
            ViewMode::D3 => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas_rect = ui.available_rect_before_wrap();
                    let w = canvas_rect.width();
                    let h = canvas_rect.height();
                    self.camera.aspect = w / h.max(1.0);

                    // Context menu
                    let ctx_resp = ui.interact(canvas_rect, ui.id().with("ctx_menu_3d"), Sense::click());
                    if ctx_resp.clicked_by(egui::PointerButton::Secondary) {
                        ctx_resp.context_menu(|ui| {
                            if ui.button("Delete selected").clicked() { self.delete_selected(); ui.close_menu(); }
                            if ui.button("Reset view").clicked() {
                                self.camera = Camera3D::new(w / h.max(1.0));
                                ui.close_menu();
                            }
                        });
                    }

                    // Orbit camera controls with right-button drag + scroll
                    let response = ui.interact(canvas_rect, ui.id().with("canvas3d"), Sense::click_and_drag());
                    if let Some(pos) = response.hover_pos() {
                        if response.dragged_by(egui::PointerButton::Secondary) {
                            if let Some(last) = self.last_mouse_pos {
                                let dx = pos.x - last.x;
                                let dy = pos.y - last.y;
                                self.camera.orbit(dx * 0.005, dy * 0.005);
                            }
                        }
                        if response.dragged_by(egui::PointerButton::Primary) {
                            if let Some(last) = self.last_mouse_pos {
                                let dx = pos.x - last.x;
                                let dy = pos.y - last.y;
                                if self.current_tool == Tool::Select {
                                    self.camera.pan(dx, dy);
                                }
                            }
                        }
                        if response.hovered() {
                            let scroll = ui.input(|i| i.smooth_scroll_delta);
                            if scroll.y != 0.0 {
                                self.camera.zoom(1.0 + scroll.y * 0.005);
                            }
                        }
                        self.last_mouse_pos = Some(pos);
                    }
                    if response.clicked_by(egui::PointerButton::Primary) && self.current_tool != Tool::Select {
                        self.handle_3d_click(ui, &response, canvas_rect, w, h);
                    }

                    // 3D rendering
                    let painter = ui.painter();
                    self.draw_3d_grid(painter, canvas_rect, w, h);
                    self.draw_3d_objects(painter, canvas_rect, w, h);
                });
            }
        }
    }
}

fn process_input(document: &mut Document, input_text: &mut String) -> Option<String> {
    let text = input_text.trim();
    if text.is_empty() {
        return None;
    }
    let mut result: Option<String> = None;

    // CAS commands
    if let Some(cmd) = parse_cas_command(text) {
        match cmd.command.as_str() {
            "Ellipse" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if parts.len() >= 2 {
                    let rx = cmd.args[1].trim().parse().unwrap_or(1.0);
                    let ry = cmd.args[2].trim().parse().unwrap_or(1.0);
                    document.add_object(GeoObject::Ellipse(EllipseObj::new(Point2::new(parts[0], parts[1]), rx, ry)));
                    input_text.clear();
                    return None;
                }
            }
            "RegularPolygon" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if parts.len() >= 2 {
                    let n = cmd.args[1].trim().parse::<usize>().unwrap_or(4).max(3).min(64);
                    let r = cmd.args[2].trim().parse::<f64>().unwrap_or(1.0);
                    let cx = parts[0]; let cy = parts[1];
                    let verts: Vec<Point2> = (0..n).map(|i| {
                        let a = i as f64 / n as f64 * std::f64::consts::TAU;
                        Point2::new(cx + r * a.cos(), cy + r * a.sin())
                    }).collect();
                    document.add_object(GeoObject::Polygon(PolygonObj::new(verts)));
                    input_text.clear();
                    return None;
                }
            }
            "Translate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok((dx, dy))) = (find_object_by_label(document, &cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let new_pos = Point2::new(p.position.x + dx, p.position.y + dy);
                                document.add_object(GeoObject::Point(PointObj::new(new_pos).with_label(format!("{}'", p.label))));
                            }
                            _ => { result = Some("Translate only supports Points".into()); }
                        }
                    }
                } else { result = Some("Usage: Translate[Object, (dx,dy)]".into()); }
                input_text.clear();
                return result;
            }
            "Rotate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok(angle)) = (find_object_by_label(document, &cmd.args[0]), cmd.args[1].trim().parse::<f64>()) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let c = angle.to_radians();
                                let nx = p.position.x * c.cos() - p.position.y * c.sin();
                                let ny = p.position.x * c.sin() + p.position.y * c.cos();
                                document.add_object(GeoObject::Point(PointObj::new(Point2::new(nx, ny)).with_label(format!("{}'", p.label))));
                            }
                            _ => { result = Some("Rotate only supports Points".into()); }
                        }
                    }
                } else { result = Some("Usage: Rotate[Object, angle_degrees]".into()); }
                input_text.clear();
                return result;
            }
            "Surface3D" if cmd.args.len() >= 5 => {
                let expr = cmd.args[0].trim();
                if let (Ok(x_min), Ok(x_max), Ok(y_min), Ok(y_max)) = (
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                    cmd.args[3].trim().parse::<f64>(),
                    cmd.args[4].trim().parse::<f64>(),
                ) {
                    let obj = GeoObject::Surface3D(Surface3DObj::new(expr, (x_min, x_max), (y_min, y_max)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Tangent" => {
                // Tangent[circle_label, point_label] or Tangent[circle, point] 
                // Simplified: Tangent[ (cx,cy), r, (px,py) ]
                if cmd.args.len() >= 3 {
                    if let (Ok((cx, cy)), Ok(r), Ok((px, py))) = (
                        parse_point_str(&cmd.args[0]),
                        cmd.args[1].trim().parse::<f64>(),
                        parse_point_str(&cmd.args[2]),
                    ) {
                        let dx = px - cx; let dy = py - cy;
                        let d = (dx*dx+dy*dy).sqrt();
                        if d > r {
                            let a = r*r/d;
                            let h = (r*r - a*a).sqrt();
                            let pm = Point2::new(cx + a*dx/d, cy + a*dy/d);
                            let perp_x = -h*dy/d; let perp_y = h*dx/d;
                            let t1 = Point2::new(pm.x + perp_x, pm.y + perp_y);
                            let t2 = Point2::new(pm.x - perp_x, pm.y - perp_y);
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(px, py), t1).with_label("T1")));
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(px, py), t2).with_label("T2")));
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "PerpendicularBisector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let mx = (x1 + x2) * 0.5; let my = (y1 + y2) * 0.5;
                    let dx = x2 - x1; let dy = y2 - y1;
                    let p1 = Point2::new(mx - dy * 5.0, my + dx * 5.0);
                    let p2 = Point2::new(mx + dy * 5.0, my - dx * 5.0);
                    document.add_object(GeoObject::Line(LineObj::new(p1, p2).with_label("B")));
                }
                input_text.clear(); return None;
            }
            "AngleBisector" if cmd.args.len() == 3 => {
                if let (Ok((x1, y1)), Ok((xv, yv)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]), parse_point_str(&cmd.args[2])) {
                    let d1 = ((xv-x1).powi(2) + (yv-y1).powi(2)).sqrt();
                    let d2 = ((xv-x2).powi(2) + (yv-y2).powi(2)).sqrt();
                    if d1 > 0.0 && d2 > 0.0 {
                        let ux = (x1 - xv) / d1; let uy = (y1 - yv) / d1;
                        let vx = (x2 - xv) / d2; let vy = (y2 - yv) / d2;
                        let bx = ux + vx; let by = uy + vy;
                        let b_len = (bx*bx + by*by).sqrt();
                        if b_len > 0.0 {
                            let p = Point2::new(xv + bx / b_len * 5.0, yv + by / b_len * 5.0);
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(xv, yv), p).with_label("Ab")));
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "Midpoint" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new((x1+x2)*0.5, (y1+y2)*0.5)).with_label("M"));
                    document.add_object(obj);
                }
                input_text.clear(); return None;
            }
            "Vector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    // Draw vector as directed line from (x1,y1) to (x2,y2) with arrow
                    let obj = GeoObject::Line(LineObj::new(Point2::new(x1, y1), Point2::new(x2, y2)).with_label("v"));
                    document.add_object(obj);
                }
                input_text.clear(); return None;
            }
            "Ray" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let dx = x2 - x1; let dy = y2 - y1;
                    let len = (dx*dx+dy*dy).sqrt().max(0.01);
                    let far = Point2::new(x1 + dx/len * 100.0, y1 + dy/len * 100.0);
                    document.add_object(GeoObject::Line(LineObj::new(Point2::new(x1, y1), far).with_label("r")));
                }
                input_text.clear(); return None;
            }
            "Parabola" if cmd.args.len() >= 2 => {
                if let (Ok((vx, vy)), Ok(p)) = (parse_point_str(&cmd.args[0]), cmd.args[1].trim().parse::<f64>()) {
                    document.add_object(GeoObject::Parabola(ParabolaObj::new(Point2::new(vx, vy), p)));
                }
                input_text.clear(); return None;
            }
            "Hyperbola" if cmd.args.len() >= 3 => {
                if let (Ok((cx, cy)), Ok(a), Ok(b)) = (parse_point_str(&cmd.args[0]), cmd.args[1].trim().parse::<f64>(), cmd.args[2].trim().parse::<f64>()) {
                    document.add_object(GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(cx, cy), a, b)));
                }
                input_text.clear(); return None;
            }
            "Dilate" if cmd.args.len() == 3 => {
                // Dilate[point_label, factor, (cx,cy)] or Dilate[(x,y), factor, (cx,cy)]
                if let (Ok((px, py)), Ok(factor), Ok((cx, cy))) = (
                    parse_point_str(&cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let nx = cx + (px - cx) * factor;
                    let ny = cy + (py - cy) * factor;
                    document.add_object(GeoObject::Point(PointObj::new(Point2::new(nx, ny)).with_label("D'")));
                }
                input_text.clear(); return None;
            }
            "Reflect" if cmd.args.len() == 3 => {
                // Reflect[point, line_ax, line_ay, line_bx, line_by] or Reflect[(x,y), (ax,ay), (bx,by)]
                if let (Ok((px, py)), Ok((ax, ay)), Ok((bx, by))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let dx = bx - ax; let dy = by - ay;
                    let len2 = dx*dx + dy*dy;
                    if len2 > 0.0 {
                        let t = ((px-ax)*dx + (py-ay)*dy) / len2;
                        let cx = ax + t * dx;
                        let cy = ay + t * dy;
                        let rx = 2.0 * cx - px;
                        let ry = 2.0 * cy - py;
                        document.add_object(GeoObject::Point(PointObj::new(Point2::new(rx, ry)).with_label("R'")));
                    }
                }
                input_text.clear(); return None;
            }
            "Locus" if cmd.args.len() == 2 => {
                // Locus[function_expr, x_range] traces a function's path
                let expr = cmd.args[0].trim();
                if let Ok(range) = cmd.args[1].trim().parse::<f64>() {
                    let steps = 200;
                    let mut vertices = Vec::new();
                    for i in 0..=steps {
                        let x = -range + 2.0 * range * i as f64 / steps as f64;
                        let mut vars = HashMap::new();
                        vars.insert("x".to_string(), x);
                        if let Ok(y) = grafito_geometry::expr::evaluate(expr, &vars.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()) {
                            if y.is_finite() && y.abs() < 1e6 {
                                vertices.push(Point2::new(x, y));
                            }
                        }
                    }
                    if vertices.len() >= 2 {
                        let mut poly = PolygonObj::new(vertices);
                        poly.label = "L".to_string();
                        document.add_object(GeoObject::Polygon(poly));
                    }
                }
                input_text.clear(); return None;
            }
            "FunctionInspector" if cmd.args.len() == 1 => {
                let expr = cmd.args[0].trim();
                let v = document.variables.clone();
                let f = |x: f64| {
                    let mut vars: Vec<(String, f64)> = v.iter().map(|(k,val)| (k.clone(), *val)).collect();
                    vars.push(("x".to_string(), x));
                    grafito_geometry::expr::evaluate(expr, &vars).unwrap_or(f64::NAN)
                };
                let mins = find_extrema(&f, -10.0, 10.0, false);
                let maxs = find_extrema(&f, -10.0, 10.0, true);
                let mut res = String::new();
                if let Some((mx, my)) = root_10(&f) {
                    res.push_str(&format!("Root ≈ ({}: {:.4})", mx, my));
                }
                for (mx, my) in &mins { res.push_str(&format!(" Min@({:.2},{:.2})", mx, my)); }
                for (mx, my) in &maxs { res.push_str(&format!(" Max@({:.2},{:.2})", mx, my)); }
                result = Some(if res.is_empty() { "No extrema found in [-10,10]".into() } else { res });
                input_text.clear(); return result;
            }
            "Normal" if cmd.args.len() == 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let expr = format!("exp(-(x-{})^2/(2*{}^2))/({}*sqrt(2*pi))", mu, sigma, sigma);
                document.add_object(GeoObject::Function(FunctionObj::new(expr).with_label(format!("N({},{})", mu, sigma))));
                result = Some(format!("Normal N({},{}) added", mu, sigma));
                input_text.clear(); return result;
            }
            "Binomial" if cmd.args.len() == 3 => {
                let n: usize = cmd.args[0].trim().parse().unwrap_or(10);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: usize = cmd.args[2].trim().parse().unwrap_or(1);
                // P(X=k) = C(n,k) * p^k * (1-p)^(n-k)
                let comb = |n: usize, k: usize| -> f64 {
                    if k > n { return 0.0; }
                    let k = k.min(n - k);
                    let mut result = 1.0;
                    for i in 0..k { result = result * (n - i) as f64 / (i + 1) as f64; }
                    result
                };
                let prob = comb(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32);
                result = Some(format!("P(X={}) = {:.6} (Binom({},{}))", k, prob, n, p));
                input_text.clear(); return result;
            }
            "Poisson" if cmd.args.len() == 2 => {
                let lambda: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: usize = cmd.args[1].trim().parse().unwrap_or(1);
                // P(X=k) = lambda^k * e^(-lambda) / k!
                let mut prob = (-lambda).exp();
                for i in 1..=k { prob *= lambda / i as f64; }
                result = Some(format!("P(X={}) = {:.6} (Poisson({}))", k, prob, lambda));
                input_text.clear(); return result;
            }
            "Curve3D" if cmd.args.len() >= 4 => {
                // Curve3D[(expr_x, expr_y, expr_z), t, t_min, t_max]
                let exprs = cmd.args[0].trim();
                let t_min: f64 = cmd.args[2].trim().parse().unwrap_or(0.0);
                let t_max: f64 = cmd.args[3].trim().parse().unwrap_or(6.28);
                // Store as a polygon with sampled points
                let steps = 200;
                let mut pts = Vec::new();
                for i in 0..=steps {
                    let t = t_min + (t_max - t_min) * i as f64 / steps as f64;
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    let inner = exprs.trim_start_matches('(').trim_end_matches(')');
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() >= 3 {
                        let vals: Vec<f64> = parts.iter().filter_map(|s| {
                            let expr = s.trim();
                            eval_function_with_vars(expr, t, &vars).ok().or_else(|| {
                                evaluate(expr, &vars.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()).ok()
                            })
                        }).collect();
                        if vals.len() >= 3 {
                            pts.push(Point3D::new(vals[0], vals[1], vals[2]));
                        }
                    }
                }
                if pts.len() >= 2 {
                    let mut segs = Vec::new();
                    for i in 1..pts.len() {
                        segs.push((pts[i-1], pts[i]));
                    }
                    // Store as multiple segments
                    for (a, b) in &segs {
                        document.add_object(GeoObject::Segment3D(
                            Segment3DObj::new(*a, *b).with_label("C3")
                        ));
                    }
                }
                input_text.clear(); return None;
            }
            "SetValue" if cmd.args.len() == 2 => {
                if let Some(id) = find_object_by_label(document, &cmd.args[0]) {
                    if let Ok(val) = cmd.args[1].trim().parse::<f64>() {
                        document.set_variable(cmd.args[0].trim().to_string(), val);
                    } else if let Ok((x, y)) = parse_point_str(&cmd.args[1]) {
                        if let Some(obj) = document.get_object_mut(id) {
                            if let GeoObject::Point(p) = obj { p.position = Point2::new(x, y); }
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "Extrude" if cmd.args.len() >= 2 => {
                let height: f64 = cmd.args.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(1.0);
                let id_opt = find_object_by_label(document, &cmd.args[0]);
                let vertices = id_opt.and_then(|id| document.get_object(id).and_then(|obj| {
                    if let GeoObject::Polygon(poly) = obj {
                        if poly.vertices.len() >= 3 { Some(poly.vertices.clone()) }
                        else { None }
                    } else { None }
                }));
                if let Some(verts) = vertices {
                    let base_y = 0.0; let top_y = height;
                    for i in 0..verts.len() {
                        let v = verts[i];
                        let vn = verts[(i+1) % verts.len()];
                        let b = Point3D::new(v.x, base_y, v.y);
                        let t = Point3D::new(v.x, top_y, v.y);
                        let bn = Point3D::new(vn.x, base_y, vn.y);
                        let tn = Point3D::new(vn.x, top_y, vn.y);
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(b, t).with_label("E")));
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(b, bn).with_label("E")));
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(t, tn).with_label("E")));
                    }
                } else {
                    result = Some("Extrude only supports Polygons with 3+ vertices".into());
                }
                input_text.clear(); return result;
            }
            "Script" if cmd.args.len() >= 1 => {
                // ... (already added, simplified code here)
                let commands: Vec<String> = cmd.args[0].split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let mut output = String::new();
                for c in &commands {
                    let mut temp = c.clone();
                    if let Some(res) = process_input(document, &mut temp) { output.push_str(&res); output.push('\n'); }
                }
                result = if output.is_empty() { Some("Script executed".into()) } else { Some(output) };
                input_text.clear(); return result;
            }
            "Simplify" if cmd.args.len() >= 1 => {
                let expr = cmd.args[0].trim();
                // Numeric simplification: evaluate the expression
                let vars: Vec<(String, f64)> = document.variables.iter().map(|(k,v)| (k.clone(), *v)).collect();
                match grafito_geometry::expr::evaluate(expr, &vars) {
                    Ok(val) => result = Some(format!("{} ≈ {}", expr, val)),
                    Err(e) => result = Some(format!("Simplify error: {}", e)),
                }
                input_text.clear(); return result;
            }
            _ => {}
        }
        result = execute_cas_command(document, &cmd);
        input_text.clear();
        return result;
    }

    if let Some((name, rest)) = text.split_once('=') {
        let name = name.trim();
        let rest = rest.trim();
        // Variable assignment: "a = 5"
        if name.chars().all(|c| c.is_alphabetic()) && name.len() == 1 {
            if let Ok(val) = rest.parse::<f64>() {
                document.set_variable(name.to_string(), val);
                input_text.clear();
                return None;
            }
        }
        // f(x) = expr or f = expr (function)
        if is_function_lhs(name) && (contains_var(rest, 'x') || rest.chars().all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c))) {
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(name));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        // "A = (1, 2)" point
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new(x, y)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
        // 3D point: "A = (1, 2, 3)"
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
    } else {
        // Function: expressions containing 'x'
        if contains_var(text, 'x') {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(text).with_label(label));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        // Point: "(1, 2)"
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len()-1];
            // Try 3D first
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            // 2D
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new(x, y)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
    }
    input_text.clear();
    result
}

#[derive(Debug)]
struct CasCmd {
    command: String,
    args: Vec<String>,
}

fn parse_cas_command(text: &str) -> Option<CasCmd> {
    let text = text.trim();
    if let Some(open) = text.find('[') {
        let close = text.rfind(']')?;
        let command = text[..open].trim().to_string();
        let inside = &text[open+1..close];
        let args: Vec<String> = split_args(inside).into_iter().map(|s| s.trim().to_string()).collect();
        if command.is_empty() || args.is_empty() { return None; }
        // Only allow known CAS commands
        match command.as_str() {
            "Derivative" | "Integral" | "Solve" | "Limit" | "NSolve" | "Factor" | "Expand" | "Simplify" => {}
            _ => return None,
        }
        Some(CasCmd { command, args })
    } else {
        None
    }
}

fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].to_string());
    args
}

fn execute_cas_command(document: &Document, cmd: &CasCmd) -> Option<String> {
    match cmd.command.as_str() {
        "Derivative" => {
            let expr = cmd.args.get(0)?;
            Some(format!("Derivative[{}]: approx (f(x+h)-f(x))/h with f(x)={}", expr, expr))
        }
        "Integral" => {
            let expr = cmd.args.get(0)?;
            let a: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let b: f64 = cmd.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            let f = move |x: f64| {
                grafito_geometry::expr::eval_function_with_vars(expr, x, &document.variables).unwrap_or(0.0)
            };
            let result = grafito_geometry::cas::integral_auto(f, a, b);
            Some(format!("∫[{}..{}] {} dx = {:.6}", a, b, expr, result))
        }
        "Solve" | "NSolve" => {
            let expr = cmd.args.get(0)?;
            let a: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-10.0);
            let b: f64 = cmd.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10.0);
            let f = move |x: f64| {
                grafito_geometry::expr::eval_function_with_vars(expr, x, &document.variables).unwrap_or(f64::NAN)
            };
            match grafito_geometry::cas::find_root(f, (a, b)) {
                Some(root) => Some(format!("Root of {} in [{:.1}, {:.1}] ≈ {:.6}", expr, a, b, root)),
                None => Some(format!("No root found for {} in [{}, {}]", expr, a, b)),
            }
        }
        "Limit" => {
            let expr = cmd.args.get(0)?;
            let x0: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let f = move |x: f64| {
                eval_function_with_vars(expr, x, &document.variables).unwrap_or(f64::NAN)
            };
            let result = grafito_geometry::cas::limit(f, x0);
            Some(format!("lim[x→{:.1}] {} ≈ {:.6}", x0, expr, result))
        }
        "Factor" => {
            let expr = cmd.args.get(0)?;
            match symbolic::factor(expr) {
                Ok(factors) => Some(format!("{} = {}", expr, factors)),
                Err(e) => Some(format!("Factor error: {}", e)),
            }
        }
        "Expand" => {
            let expr = cmd.args.get(0)?;
            match symbolic::expand(expr) {
                Ok(expanded) => Some(format!("{} = {}", expr, expanded)),
                Err(e) => Some(format!("Expand error: {}", e)),
            }
        }
        "Simplify" => {
            let expr = cmd.args.get(0)?;
            match symbolic::simplify(expr) {
                Ok(simplified) => Some(format!("{} = {}", expr, simplified)),
                Err(e) => Some(format!("Simplify error: {}", e)),
            }
        }
        _ => None,
    }
}

fn is_function_lhs(name: &str) -> bool {
    if let Some((id, args)) = name.split_once('(') {
        let id = id.trim();
        let args = args.trim_end_matches(')').trim();
        id.chars().all(|c| c.is_alphabetic())
            && args.len() == 1
            && args.chars().all(|c| c.is_alphabetic())
    } else {
        false
    }
}

fn contains_var(text: &str, var: char) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == var {
            let prev = if i > 0 { chars[i-1] } else { ' ' };
            let next = if i + 1 < chars.len() { chars[i+1] } else { ' ' };
            if !prev.is_alphabetic() && !next.is_alphabetic() {
                return true;
            }
        }
    }
    false
}

fn find_object_by_label(document: &Document, label: &str) -> Option<ObjectId> {
    document.objects_iter().find(|(_, obj)| obj.label() == label.trim()).map(|(id, _)| *id)
}

fn parse_point_str(s: &str) -> Result<(f64, f64), String> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() == 2 {
        Ok((parts[0].parse().map_err(|_| "bad x")?, parts[1].parse().map_err(|_| "bad y")?))
    } else {
        Err("expected (x, y)".into())
    }
}

fn next_function_label(document: &Document) -> String {
    let used: std::collections::HashSet<String> = document.objects_iter()
        .filter_map(|(_, obj)| {
            if let GeoObject::Function(f) = obj {
                Some(f.label.clone())
            } else {
                None
            }
        })
        .collect();
    for c in 'f'..='z' {
        let label = format!("{}(x)", c);
        if !used.contains(&label) {
            return label;
        }
    }
    format!("f{}(x)", document.object_count())
}

fn find_extrema<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64, find_max: bool) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let steps = 200;
    let step = (b - a) / steps as f64;
    let mut prev = f(a);
    for i in 1..steps {
        let x = a + i as f64 * step;
        let curr = f(x);
        let next = f(x + step);
        if find_max {
            if curr > prev && curr > next && curr.is_finite() {
                pts.push((x, curr));
            }
        } else {
            if curr < prev && curr < next && curr.is_finite() {
                pts.push((x, curr));
            }
        }
        prev = curr;
    }
    pts
}

fn root_10<F: Fn(f64) -> f64>(f: &F) -> Option<(f64, f64)> {
    for x0 in -10..=10 {
        if let Ok(r) = grafito_geometry::cas::newton_root_auto(f, x0 as f64) {
            if r >= -10.0 && r <= 10.0 {
                let fx = f(r);
                if fx.abs() < 0.1 { return Some((r, fx)); }
            }
        }
    }
    None
}

fn export_tikz(document: &Document) -> String {
    let view = document.view();
    let mut out = String::from("% Grafito TikZ Export\n\\begin{tikzpicture}[scale=1]\n");
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                out.push_str(&format!("  \\fill[black] ({:.2},{:.2}) circle (2pt);\n", s.x, s.y));
                if !p.label.is_empty() { out.push_str(&format!("  \\node[above right] at ({:.2},{:.2}) {{{}}};\n", s.x, s.y, p.label)); }
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start); let b = view.world_to_screen(l.end);
                out.push_str(&format!("  \\draw ({:.2},{:.2}) -- ({:.2},{:.2});\n", a.x, a.y, b.x, b.y));
                if !l.label.is_empty() { out.push_str(&format!("  \\node[above] at ({:.2},{:.2}) {{{}}};\n", (a.x+b.x)/2., (a.y+b.y)/2., l.label)); }
            }
            GeoObject::Circle(c) => {
                let cen = view.world_to_screen(c.center);
                let r = c.radius as f32 * view.scale;
                out.push_str(&format!("  \\draw ({:.2},{:.2}) circle ({:.2});\n", cen.x, cen.y, r));
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let pts: Vec<String> = poly.vertices.iter().map(|v| {
                    let s = view.world_to_screen(*v); format!("({:.2},{:.2})", s.x, s.y)
                }).collect();
                out.push_str(&format!("  \\draw {} -- cycle;\n", pts.join(" -- ")));
            }
            GeoObject::Ellipse(el) => {
                let cen = view.world_to_screen(el.center);
                out.push_str(&format!("  \\draw ({:.2},{:.2}) ellipse ({:.2} and {:.2});\n",
                    cen.x, cen.y, el.rx as f32 * view.scale, el.ry as f32 * view.scale));
            }
            _ => {}
        }
    }
    out.push_str("\\end{tikzpicture}\n");
    out
}

fn svg_color(c: Color) -> String {
    format!(
        "rgb({},{},{})",
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8
    )
}

fn export_svg(document: &Document) -> String {
    use std::fmt::Write;
    let view = document.view();
    let w = view.screen_size.x as u32;
    let h = view.screen_size.y as u32;
    let mut svg = String::new();
    let _ = writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" style="background:white">"#);

    // Grid
    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(GlamVec2::new(w as f32, h as f32));
    let min_x = world_tl.x.floor() as i32 - 1;
    let max_x = world_br.x.ceil() as i32 + 1;
    let min_y = world_br.y.floor() as i32 - 1;
    let max_y = world_tl.y.ceil() as i32 + 1;
    for x in min_x..=max_x {
        let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
        let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
        let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="rgb(217,217,217)" stroke-width="1"/>"#, a.x, a.y, b.x, b.y);
    }
    for y in min_y..=max_y {
        let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
        let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
        let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="rgb(217,217,217)" stroke-width="1"/>"#, a.x, a.y, b.x, b.y);
    }

    // Axes
    let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
    let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);
    let ax_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
    let ax_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
    let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="2"/>"#, ax_a.x, ax_a.y, ax_b.x, ax_b.y);
    let ay_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
    let ay_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
    let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="2"/>"#, ay_a.x, ay_a.y, ay_b.x, ay_b.y);

    // Objects
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{}" fill="{}"/>"#, s.x, s.y, p.size, svg_color(p.color));
                if !p.label.is_empty() {
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, s.x + p.size as f32 + 2.0, s.y - p.size as f32 - 2.0, p.label);
                }
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start);
                let b = view.world_to_screen(l.end);
                let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{}"/>"#, a.x, a.y, b.x, b.y, svg_color(l.color), l.width);
                if !l.label.is_empty() {
                    let mx = (a.x + b.x) * 0.5;
                    let my = (a.y + b.y) * 0.5;
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, mx, my - 8.0, l.label);
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = (c.radius as f32) * view.scale;
                if let Some(fill) = c.fill_color {
                    let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{}"/>"#, center.x, center.y, r, svg_color(fill));
                }
                let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{}" stroke-width="{}"/>"#, center.x, center.y, r, svg_color(c.color), c.width);
                if !c.label.is_empty() {
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, center.x + r + 2.0, center.y - r - 2.0, c.label);
                }
            }
            GeoObject::Polygon(poly) => {
                if poly.vertices.len() >= 3 {
                    let pts: Vec<String> = poly.vertices.iter()
                        .map(|v| {
                            let s = view.world_to_screen(*v);
                            format!("{:.1},{:.1}", s.x, s.y)
                        })
                        .collect();
                    let pts_str = pts.join(" ");
                    let fill = poly.fill_color.map_or("none".to_string(), |c| svg_color(c));
                    let _ = writeln!(svg, r#"<polygon points="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#, pts_str, fill, svg_color(poly.color), poly.width);
                    if !poly.label.is_empty() {
                        let cx: f32 = poly.vertices.iter().map(|v| view.world_to_screen(*v).x).sum::<f32>() / poly.vertices.len() as f32;
                        let cy: f32 = poly.vertices.iter().map(|v| view.world_to_screen(*v).y).sum::<f32>() / poly.vertices.len() as f32;
                        let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, cx, cy, poly.label);
                    }
                }
            }
            GeoObject::Function(fun) => {
                let min_x = fun.domain_min.unwrap_or(world_tl.x);
                let max_x = fun.domain_max.unwrap_or(world_br.x);
                let steps = 500;
                let step = (max_x - min_x) / steps as f64;
                let mut points = Vec::new();
                for i in 0..=steps {
                    let x = min_x + i as f64 * step;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, x, &document.variables) {
                        if y.is_finite() && y.abs() < 1e6 {
                            let s = view.world_to_screen(Point2::new(x, y));
                            points.push(format!("{:.1},{:.1}", s.x, s.y));
                        } else if !points.is_empty() {
                            let pts_str = points.join(" ");
                            let _ = writeln!(svg, r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}"/>"#, pts_str, svg_color(fun.color), fun.width);
                            points.clear();
                        }
                    }
                }
                if !points.is_empty() {
                    let pts_str = points.join(" ");
                    let _ = writeln!(svg, r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}"/>"#, pts_str, svg_color(fun.color), fun.width);
                }
                if !fun.label.is_empty() {
                    let mid_x = (min_x + max_x) * 0.5;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, mid_x, &document.variables) {
                        let s = view.world_to_screen(Point2::new(mid_x, y));
                        let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, s.x, s.y + 14.0, fun.label);
                    }
                }
            }
            _ => {}
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn export_png(document: &Document, width: u32, height: u32) -> image::RgbaImage {
    use image::Rgba;
    let mut img = image::RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let view = document.view();
    let grid_color = [217u8, 217, 217, 255];
    let black = [0u8, 0, 0, 255];

    fn to_rgba(c: [u8; 4]) -> Rgba<u8> { Rgba(c) }

    // Bresenham line
    fn draw_line(img: &mut image::RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
        let (w, h) = (img.width() as i32, img.height() as i32);
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x >= 0 && x < w && y >= 0 && y < h {
                img.put_pixel(x as u32, y as u32, to_rgba(color));
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }

    fn fill_circle(img: &mut image::RgbaImage, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
        let (w, h) = (img.width() as i32, img.height() as i32);
        let r2 = r * r;
        for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
            let dy = y - cy;
            let dx = ((r2 - dy*dy) as f64).sqrt() as i32;
            let x0 = (cx - dx).max(0);
            let x1 = (cx + dx).min(w - 1);
            for x in x0..=x1 {
                img.put_pixel(x as u32, y as u32, to_rgba(color));
            }
        }
    }

    // Grid
    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(GlamVec2::new(width as f32, height as f32));
    let min_x = world_tl.x.floor() as i32 - 1;
    let max_x = world_br.x.ceil() as i32 + 1;
    let min_y = world_br.y.floor() as i32 - 1;
    let max_y = world_tl.y.ceil() as i32 + 1;

    for x in min_x..=max_x {
        let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
        let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
        draw_line(&mut img, a.x as i32, a.y as i32, b.x as i32, b.y as i32, grid_color);
    }
    for y in min_y..=max_y {
        let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
        let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
        draw_line(&mut img, a.x as i32, a.y as i32, b.x as i32, b.y as i32, grid_color);
    }

    // Axes
    let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
    let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);
    let ax_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
    let ax_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
    draw_line(&mut img, ax_a.x as i32, ax_a.y as i32, ax_b.x as i32, ax_b.y as i32, black);
    let ay_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
    let ay_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
    draw_line(&mut img, ay_a.x as i32, ay_a.y as i32, ay_b.x as i32, ay_b.y as i32, black);

    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                let r = p.size.max(1.0) as i32;
                let c = [(p.color.r*255.0) as u8, (p.color.g*255.0) as u8, (p.color.b*255.0) as u8, 255];
                fill_circle(&mut img, s.x as i32, s.y as i32, r, c);
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start);
                let b = view.world_to_screen(l.end);
                let c = [(l.color.r*255.0) as u8, (l.color.g*255.0) as u8, (l.color.b*255.0) as u8, 255];
                // Draw thick line: multiple parallel lines
                let w = (l.width/2.0).max(0.5) as i32;
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let len = (dx*dx + dy*dy).sqrt().max(0.001);
                let nx = -dy / len;
                let ny = dx / len;
                for t in -w..=w {
                    let offset_x = (nx * t as f32) as i32;
                    let offset_y = (ny * t as f32) as i32;
                    draw_line(&mut img, a.x as i32 + offset_x , a.y as i32 + offset_y , b.x as i32 + offset_x , b.y as i32 + offset_y , c);
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = (c.radius as f32 * view.scale).max(0.5) as i32;
                if let Some(fill) = c.fill_color {
                    let fc = [(fill.r*255.0) as u8, (fill.g*255.0) as u8, (fill.b*255.0) as u8, 255];
                    fill_circle(&mut img, center.x as i32, center.y as i32, r, fc);
                }
                // Circle outline via midpoint circle
                let cc = [(c.color.r*255.0) as u8, (c.color.g*255.0) as u8, (c.color.b*255.0) as u8, 255];
                let (cx, cy) = (center.x as i32, center.y as i32);
                let (w, h) = (width as i32, height as i32);
                for d in 0.max(r - (c.width/2.0) as i32)..=r + (c.width/2.0) as i32 {
                    let mut x = 0i32;
                    let mut y = d;
                    let mut p_val = 1 - d;
                    while x <= y {
                        for (px, py) in &[(cx+x,cy+y),(cx-x,cy+y),(cx+x,cy-y),(cx-x,cy-y),(cx+y,cy+x),(cx-y,cy+x),(cx+y,cy-x),(cx-y,cy-x)] {
                            if *px >= 0 && *px < w && *py >= 0 && *py < h {
                                img.put_pixel(*px as u32, *py as u32, to_rgba(cc));
                            }
                        }
                        x += 1;
                        if p_val < 0 { p_val += 2*x + 1; }
                        else { y -= 1; p_val += 2*(x - y) + 1; }
                    }
                }
            }
            GeoObject::Polygon(poly) => {
                if poly.vertices.len() >= 3 {
                    let pts: Vec<(i32, i32)> = poly.vertices.iter()
                        .map(|v| {
                            let s = view.world_to_screen(*v);
                            (s.x as i32, s.y as i32)
                        }).collect();
                    // Fill polygon with scanline algorithm
                    if let Some(fill) = poly.fill_color {
                        let fc = [(fill.r*255.0) as u8, (fill.g*255.0) as u8, (fill.b*255.0) as u8, 200];
                        let mut y_pts: Vec<i32> = pts.iter().map(|p| p.1).collect();
                        y_pts.sort();
                        let y_min = y_pts[0].max(0);
                        let y_max = y_pts[y_pts.len()-1].min(img.height() as i32 - 1);
                        for y in y_min..=y_max {
                            let mut xs: Vec<i32> = Vec::new();
                            for i in 0..pts.len() {
                                let (x0, y0) = pts[i];
                                let (x1, y1) = pts[(i+1)%pts.len()];
                                if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                                    let t = (y - y0) as f32 / (y1 - y0) as f32;
                                    xs.push((x0 as f32 + t * (x1 - x0) as f32) as i32);
                                }
                            }
                            xs.sort();
                            for i in (0..xs.len()-1).step_by(2) {
                                let x0 = xs[i].max(0);
                                let x1 = xs[i+1].min(img.width() as i32 - 1);
                                for x in x0..=x1 {
                                    img.put_pixel(x as u32, y as u32, to_rgba(fc));
                                }
                            }
                        }
                    }
                    // Stroke
                    let sc = [(poly.color.r*255.0) as u8, (poly.color.g*255.0) as u8, (poly.color.b*255.0) as u8, 255];
                    for i in 0..pts.len() {
                        let a = pts[i];
                        let b = pts[(i+1)%pts.len()];
                        draw_line(&mut img, a.0, a.1, b.0, b.1, sc);
                    }
                }
            }
            GeoObject::Function(fun) => {
                let (w, h) = (width as i32, height as i32);
                let min_x = fun.domain_min.unwrap_or(world_tl.x);
                let max_x = fun.domain_max.unwrap_or(world_br.x);
                let steps = 500;
                let step = (max_x - min_x) / steps as f64;
                let fc = [(fun.color.r*255.0) as u8, (fun.color.g*255.0) as u8, (fun.color.b*255.0) as u8, 255];
                let mut prev: Option<(i32, i32)> = None;
                for i in 0..=steps {
                    let x = min_x + i as f64 * step;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, x, &document.variables) {
                        if y.is_finite() && y.abs() < 1e6 {
                            let s = view.world_to_screen(Point2::new(x, y));
                            let curr = (s.x as i32, s.y as i32);
                            if curr.0 >= 0 && curr.0 < w && curr.1 >= 0 && curr.1 < h {
                                if let Some(prev_p) = prev {
                                    draw_line(&mut img, prev_p.0, prev_p.1, curr.0, curr.1, fc);
                                }
                                prev = Some(curr);
                            } else {
                                prev = None;
                            }
                            continue;
                        }
                    }
                    prev = None;
                }
            }
            _ => {}
        }
    }
    img
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Grafito",
        options,
        Box::new(|cc| Ok(Box::new(GrafitoApp::new(cc)))),
    ).unwrap();
}
