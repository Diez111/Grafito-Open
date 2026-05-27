use grafito_core::{Document, GeoObject, PointObj, LineObj, CircleObj, PolygonObj, FunctionObj, ObjectId};
use grafito_geometry::{Point2, ViewTransform, Color};
use grafito_geometry::expr::eval_function;
use grafito_ui::{Tool, algebra_view, properties_panel, toolbar};
use egui::{Pos2, Vec2, Stroke, Shape, Color32, Rect, Sense, Key};
use glam::Vec2 as GlamVec2;

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
    pending_points: Vec<Point2>,
    last_mouse_pos: Option<Pos2>,
    #[allow(dead_code)]
    hovered_object: Option<ObjectId>,
    selected_object: Option<ObjectId>,
    input_text: String,
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

        Self {
            document,
            current_tool: Tool::default(),
            pending_points: Vec::new(),
            last_mouse_pos: None,
            hovered_object: None,
            selected_object: None,
            input_text: String::new(),
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
                        self.document.add_object(GeoObject::Point(PointObj::new(world)));
                    }
                    Tool::Line | Tool::Circle | Tool::Polygon => {
                        self.pending_points.push(world);
                        if self.current_tool == Tool::Line && self.pending_points.len() == 2 {
                            let a = self.pending_points[0];
                            let b = self.pending_points[1];
                            self.document.add_object(GeoObject::Line(LineObj::new(a, b)));
                            self.pending_points.clear();
                        } else if self.current_tool == Tool::Circle && self.pending_points.len() == 2 {
                            let center = self.pending_points[0];
                            let edge = self.pending_points[1];
                            let radius = center.distance(&edge);
                            self.document.add_object(GeoObject::Circle(CircleObj::new(center, radius)));
                            self.pending_points.clear();
                        }
                    }
                    _ => {}
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
                    let steps = 500;
                    let step = (max_x - min_x) / steps as f64;
                    let mut prev_screen: Option<Pos2> = None;
                    let stroke = Stroke::new(fun.width, to_color32(fun.color));
                    for i in 0..=steps {
                        let x = min_x + i as f64 * step;
                        if let Ok(y) = eval_function(&fun.expr, x) {
                            if y.is_finite() && y.abs() < 1e6 {
                                let s = view.world_to_screen(Point2::new(x, y));
                                let p = canvas_rect.min + Vec2::new(s.x, s.y);
                                if let Some(prev) = prev_screen {
                                    painter.line_segment([prev, p], stroke);
                                }
                                prev_screen = Some(p);
                                continue;
                            }
                        }
                        prev_screen = None;
                    }
                    if !fun.label.is_empty() {
                        let mid_x = (min_x + max_x) * 0.5;
                        if let Ok(y) = eval_function(&fun.expr, mid_x) {
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
                GeoObject::Text(_) => {}
            }
        }
    }
}

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            toolbar(ui, &mut self.current_tool);
        });

        // Left: Algebra View
        egui::SidePanel::left("algebra").default_width(200.0).show(ctx, |ui| {
            algebra_view(ui, &self.document, &mut self.selected_object);
            if let Some(id) = self.selected_object {
                ui.separator();
                properties_panel(ui, &mut self.document, id);
            }
        });

        // Bottom: Input Bar
        egui::TopBottomPanel::bottom("input_bar").default_height(40.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Input:");
                let response = ui.text_edit_singleline(&mut self.input_text);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    process_input(&mut self.document, &mut self.input_text);
                }
                if ui.button("Enter").clicked() {
                    process_input(&mut self.document, &mut self.input_text);
                }
            });
        });

        // Central canvas
        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas_rect = ui.available_rect_before_wrap();

            self.handle_canvas_input(ui, canvas_rect);

            let painter = ui.painter();
            self.draw_grid(painter, canvas_rect);
            self.draw_axes(painter, canvas_rect);
            self.draw_objects(painter, canvas_rect);
        });
    }
}

fn process_input(document: &mut Document, input_text: &mut String) {
    let text = input_text.trim();
    if text.is_empty() {
        return;
    }
    if let Some((name, rest)) = text.split_once('=') {
        let name = name.trim();
        let rest = rest.trim();
        // f(x) = expr or f = expr (function)
        if is_function_lhs(name) && (contains_var(rest, 'x') || rest.chars().all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c))) {
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(name));
            document.add_object(obj);
            input_text.clear();
            return;
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
                    return;
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
            return;
        }
        // Point: "(1, 2)"
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new(x, y)));
                    document.add_object(obj);
                    input_text.clear();
                    return;
                }
            }
        }
    }
    input_text.clear();
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
