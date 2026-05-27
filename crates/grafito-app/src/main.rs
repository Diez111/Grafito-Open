use grafito_core::{Document, GeoObject, PointObj, LineObj, CircleObj, PolygonObj, FunctionObj, ObjectId};
use grafito_geometry::{Point2, ViewTransform, Color};
use grafito_geometry::expr::eval_function;
use grafito_ui::{Tool, algebra_view, properties_panel, toolbar};
use egui::{Pos2, Vec2, Stroke, Shape, Color32, Rect, Sense, Key};
use glam::Vec2 as GlamVec2;
use std::fs;

const MAX_UNDO: usize = 50;

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

        Self {
            document,
            current_tool: Tool::default(),
            pending_points: Vec::new(),
            last_mouse_pos: None,
            hovered_object: None,
            selected_object: None,
            input_text: String::new(),
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
        // Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
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
                    self.save_state();
                    process_input(&mut self.document, &mut self.input_text);
                }
                if ui.button("Enter").clicked() {
                    self.save_state();
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
                    if let Ok(y) = eval_function(&fun.expr, x) {
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
                    if let Ok(y) = eval_function(&fun.expr, mid_x) {
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
                    if let Ok(y) = eval_function(&fun.expr, x) {
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
