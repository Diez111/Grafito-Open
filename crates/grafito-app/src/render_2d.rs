use crate::GrafitoApp;
use grafito_core::{
    GeoObject,
    PointObj, LineObj, CircleObj, PolygonObj,
};
use grafito_geometry::{Point2, Color};
use grafito_geometry::expr::eval_function_with_vars;
use grafito_ui::Tool;
use egui::{Pos2, Vec2, Stroke, Shape, Color32, Rect, Sense};
use glam::Vec2 as GlamVec2;

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

#[allow(dead_code)]
impl GrafitoApp {
    pub(crate) fn draw_grid_numbers_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        self.draw_axes(painter, canvas_rect);
    }

    pub(crate) fn draw_complex_objects_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        for (_, obj) in self.document.objects() {
            if matches!(obj, GeoObject::Function(_) | GeoObject::Polygon(_) | GeoObject::Hyperbola(_) | GeoObject::Parabola(_) | GeoObject::Ellipse(_)) {
                self.draw_object(painter, canvas_rect, obj);
            }
        }
    }

    pub(crate) fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        let response = ui.interact(canvas_rect, ui.id().with("canvas"), Sense::click_and_drag());

        self.document.view_mut().screen_size = GlamVec2::new(canvas_rect.width(), canvas_rect.height());

        if let Some(pos) = response.hover_pos() {
            let local = pos - canvas_rect.min;
            let world = self.document.view().screen_to_world(GlamVec2::new(local.x, local.y));

            // ── Point tool: drag-release to place (avoids click sensitivity bug) ──
            if response.drag_stopped() && self.current_tool == Tool::Point {
                self.save_state();
                self.document.add_object(GeoObject::Point(PointObj::new(world)));
                self.tool_ghost = None;
            }

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
                    Tool::Point => {} // handled by drag_released above
                    Tool::Line => {
                        self.pending_points.push(world);
                        if self.pending_points.len() == 2 {
                            let a = self.pending_points[0];
                            let b = self.pending_points[1];
                            self.save_state();
                            self.document.add_object(GeoObject::Line(LineObj::new(a, b)));
                            self.pending_points.clear();
                            self.tool_ghost = None;
                        }
                    }
                    Tool::Circle => {
                        self.pending_points.push(world);
                        if self.pending_points.len() == 2 {
                            let center = self.pending_points[0];
                            let edge = self.pending_points[1];
                            let radius = center.distance(&edge);
                            self.save_state();
                            self.document.add_object(GeoObject::Circle(CircleObj::new(center, radius)));
                            self.pending_points.clear();
                            self.tool_ghost = None;
                        }
                    }
                    Tool::Polygon => {
                        self.pending_points.push(world);
                    }
                    Tool::Function => {}
                    Tool::Point3D | Tool::Sphere3D | Tool::Cube3D => {}
                    Tool::Attractor => {
                        self.input_text = "Lorenz[]".to_string();
                        self.selected_object = None;
                    }
                    Tool::Fractal => {
                        self.input_text = "Mandelbrot[]".to_string();
                        self.selected_object = None;
                    }
                    Tool::Histogram => {
                        self.input_text = "Histogram[{1,2,3,4,5}, 10]".to_string();
                        self.selected_object = None;
                    }
                    Tool::ScatterPlot => {
                        self.input_text = "ScatterPlot[{1,2,3}, {4,5,6}]".to_string();
                        self.selected_object = None;
                    }
                    Tool::Tangent => {
                        self.input_text = "Tangent[(cx,cy), r, (px,py)]".to_string();
                        self.selected_object = None;
                    }
                    Tool::Perpendicular => {
                        self.input_text = "PerpendicularBisector[(x1,y1), (x2,y2)]".to_string();
                        self.selected_object = None;
                    }
                }
            }

            // ── Right-click to close Polygon ─────────────────────────────────
            if response.clicked_by(egui::PointerButton::Secondary) {
                if self.current_tool == Tool::Polygon && self.pending_points.len() >= 3 {
                    self.save_state();
                    let vertices = self.pending_points.clone();
                    self.document.add_object(GeoObject::Polygon(PolygonObj::new(vertices)));
                    self.pending_points.clear();
                    self.tool_ghost = None;
                } else if self.pending_points.len() == 1 {
                    // Cancel single pending point (Line/Circle first point)
                    self.pending_points.clear();
                    self.tool_ghost = None;
                }
            }

            // ── Drag: only pan in Select mode ───────────────────────────────
            if response.dragged() {
                if self.current_tool == Tool::Select {
                    if let Some(last) = self.last_mouse_pos {
                        let delta = pos - last;
                        self.document.view_mut().pan(GlamVec2::new(delta.x, delta.y));
                    }
                }
            }

            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta);
                if scroll.y != 0.0 {
                    let factor = if scroll.y > 0.0 { 1.0 + scroll.y.abs() * 0.001 }
                                 else { 1.0 / (1.0 + scroll.y.abs() * 0.001) };
                    self.document.view_mut().zoom(
                        factor.clamp(0.1, 10.0),
                        GlamVec2::new(local.x, local.y),
                    );
                }
            }

            // ── Tool ghost preview ───────────────────────────────────────────
            self.tool_ghost = None;
            match self.current_tool {
                Tool::Point => {
                    self.tool_ghost = Some(GeoObject::Point(PointObj::new(world)));
                }
                Tool::Line => {
                    if let Some(first) = self.pending_points.first() {
                        self.tool_ghost = Some(GeoObject::Line(
                            LineObj::new(*first, world)
                        ));
                    }
                }
                Tool::Circle => {
                    if let Some(center) = self.pending_points.first() {
                        let radius = center.distance(&world);
                        self.tool_ghost = Some(GeoObject::Circle(
                            CircleObj::new(*center, radius)
                        ));
                    }
                }
                Tool::Polygon => {
                    if let Some(last) = self.pending_points.last() {
                        self.tool_ghost = Some(GeoObject::Line(
                            LineObj::new(*last, world)
                        ));
                    }
                }
                _ => {}
            }

            self.last_mouse_pos = Some(pos);
        }
    }

    pub(crate) fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if !self.show_grid { return; }
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        // Dynamic grid step based on zoom level (target ~100 pixels per major step)
        let pixels_per_unit = self.document.view().scale as f64; // Scale is roughly pixels per unit
        let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;
        
        let major_step = if factor < 2.0 { 1.0 * base }
            else if factor < 5.0 { 2.0 * base }
            else { 5.0 * base };

        let min_x = (world_tl.x / major_step).floor() as i32 - 1;
        let max_x = (world_br.x / major_step).ceil() as i32 + 1;
        let min_y = (world_br.y / major_step).floor() as i32 - 1;
        let max_y = (world_tl.y / major_step).ceil() as i32 + 1;

        // GeoGebra-style: clear, distinct squares without the dense minor mesh
        let grid_color = if self.dark_mode { Color32::from_rgba_unmultiplied(255, 255, 255, 25) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 25) };
        let grid_stroke = Stroke::new(1.0, grid_color);

        for xi in min_x..=max_x {
            let x = xi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(x, min_y as f64 * major_step));
            let b = view.world_to_screen(Point2::new(x, max_y as f64 * major_step));
            painter.line_segment([canvas_rect.min + Vec2::new(a.x, a.y), canvas_rect.min + Vec2::new(b.x, b.y)], grid_stroke);
        }

        for yi in min_y..=max_y {
            let y = yi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(min_x as f64 * major_step, y));
            let b = view.world_to_screen(Point2::new(max_x as f64 * major_step, y));
            painter.line_segment([canvas_rect.min + Vec2::new(a.x, a.y), canvas_rect.min + Vec2::new(b.x, b.y)], grid_stroke);
        }
    }

    pub(crate) fn draw_axes(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let stroke = if self.dark_mode { Stroke::new(1.0, Color32::from_gray(180)) } else { Stroke::new(1.0, Color32::from_gray(80)) };

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

        // Dynamic grid step for axis numbers
        let pixels_per_unit = self.document.view().scale as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;
        
        let major_step = if factor < 2.0 { 1.0 * base }
            else if factor < 5.0 { 2.0 * base }
            else { 5.0 * base };

        let format_num = |v: f64| -> String {
            let rounded = (v * 1000.0).round() / 1000.0;
            format!("{}", rounded)
        };

        let text_color = if self.dark_mode { Color32::from_gray(180) } else { Color32::from_gray(80) };
        let font = egui::FontId::proportional(12.0);

        let min_x = (world_tl.x / major_step).floor() as i32 - 1;
        let max_x = (world_br.x / major_step).ceil() as i32 + 1;
        for xi in min_x..=max_x {
            let x = xi as f64 * major_step;
            if x.abs() < 1e-9 { continue; } // Origin handled separately
            let s = view.world_to_screen(Point2::new(x, x_axis_y));
            let pos = canvas_rect.min + Vec2::new(s.x, s.y);
            painter.line_segment([pos + Vec2::new(0.0, -3.0), pos + Vec2::new(0.0, 3.0)], stroke);
            painter.text(pos + Vec2::new(0.0, 6.0), egui::Align2::CENTER_TOP, format_num(x), font.clone(), text_color);
        }

        let min_y = (world_br.y / major_step).floor() as i32 - 1;
        let max_y = (world_tl.y / major_step).ceil() as i32 + 1;
        for yi in min_y..=max_y {
            let y = yi as f64 * major_step;
            if y.abs() < 1e-9 { continue; }
            let s = view.world_to_screen(Point2::new(y_axis_x, y));
            let pos = canvas_rect.min + Vec2::new(s.x, s.y);
            painter.line_segment([pos + Vec2::new(-3.0, 0.0), pos + Vec2::new(3.0, 0.0)], stroke);
            painter.text(pos + Vec2::new(-6.0, 0.0), egui::Align2::RIGHT_CENTER, format_num(y), font.clone(), text_color);
        }

        let origin = view.world_to_screen(Point2::new(0.0, 0.0));
        let origin_pos = canvas_rect.min + Vec2::new(origin.x, origin.y);
        painter.text(origin_pos + Vec2::new(-6.0, 6.0), egui::Align2::RIGHT_TOP, "0", font, text_color);
    }

    pub(crate) fn draw_objects(&self, painter: &egui::Painter, canvas_rect: Rect) {
        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() { continue; }
            // Skip 3D objects in 2D view — they can't be rendered here
            if matches!(obj,
                GeoObject::Point3D(_) | GeoObject::Segment3D(_) | GeoObject::Sphere3D(_) |
                GeoObject::Cube3D(_) | GeoObject::Pyramid3D(_) | GeoObject::Cone3D(_) |
                GeoObject::Cylinder3D(_) | GeoObject::Surface3D(_) | GeoObject::ParametricCurve3D(_) |
                GeoObject::Attractor3D(_) | GeoObject::HyperSurface4D(_) | GeoObject::VectorField3D(_)
            ) { continue; }
            self.draw_object(painter, canvas_rect, obj);
        }
        
        if let Some(preview) = &self.preview_object {
            let mut preview_clone = preview.clone();
            match &mut preview_clone {
                GeoObject::Function(f) => {
                    f.color = Color { r: 0.4, g: 0.4, b: 0.4, a: 0.8 };
                    f.width = 2.5;
                    f.label = String::new();
                }
                GeoObject::Point(p) => {
                    p.color = Color { r: 0.4, g: 0.4, b: 0.4, a: 0.8 };
                    p.label = String::new();
                }
                _ => {}
            }
            self.draw_object(painter, canvas_rect, &preview_clone);
        }
    }

    pub(crate) fn draw_ripples(&mut self, _painter: &egui::Painter, _current_time: f64) {
        // Disabled
    }

    pub(crate) fn draw_tool_ghost(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if let Some(ghost) = &self.tool_ghost {
            let mut g = ghost.clone();
            // Reduce opacity to create ghost/shadow effect
            let ghost_alpha = 0.3;
            match &mut g {
                GeoObject::Point(p) => { p.color.a = (p.color.a * ghost_alpha).max(0.05); p.size *= 1.3; }
                GeoObject::Line(l) => { l.color.a = (l.color.a * ghost_alpha).max(0.05); l.width *= 0.7; }
                GeoObject::Circle(c) => { c.color.a = (c.color.a * ghost_alpha).max(0.05); c.width *= 0.7; c.fill_color = None; }
                _ => {}
            }
            self.draw_object(painter, canvas_rect, &g);
        }
    }

    pub(crate) fn draw_object(&self, painter: &egui::Painter, canvas_rect: Rect, obj: &GeoObject) {
        let view = self.document.view();
        let label_color = if self.dark_mode { Color32::WHITE } else { Color32::BLACK };
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
                        label_color,
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
                            label_color,
                        );
                    }
                }
                GeoObject::Circle(c) => {
                    let center = view.world_to_screen(c.center);
                    let radius = (c.radius * view.scale as f64) as f32;
                    let radius = radius.max(0.5).min(50000.0);
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
                            label_color,
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
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Function(fun) => {
                    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                    let min_x = fun.domain_min.unwrap_or(world_tl.x);
                    let max_x = fun.domain_max.unwrap_or(world_br.x);
                    
                    let screen_width = canvas_rect.width() as f64;
                    let world_width = max_x - min_x;
                    
                    let mut steps = screen_width.max(400.0).min(4000.0) as usize;
                    let max_world_step = 0.05; 
                    let mut step = world_width / steps as f64;
                    if step > max_world_step {
                        steps = (world_width / max_world_step).ceil() as usize;
                    }
                    steps = steps.min(500_000);
                    step = world_width / steps as f64;
                    
                    let variables = &self.document.variables;
                    let xs = (0..=steps).map(|i| min_x + i as f64 * step);
                    
                    let mut samples: Vec<(f64, Option<f64>)> = Vec::with_capacity(steps + 1);
                    let batch_results = grafito_geometry::expr::eval_batch_1d(&fun.expr, "x", xs.clone(), variables).unwrap_or_default();
                    
                    for (x, y_opt) in xs.zip(batch_results.into_iter().chain(std::iter::repeat(None))) {
                        if let Some(y) = y_opt {
                            if y.is_finite() && y.abs() < 1e50 {
                                samples.push((x, Some(y)));
                                continue;
                            }
                        }
                        // Patch NaN/Inf by evaluating nearby points
                        let eps = 1e-9;
                        if let (Ok(y1), Ok(y2)) = (
                            grafito_geometry::expr::eval_function_with_vars(&fun.expr, x - eps, variables),
                            grafito_geometry::expr::eval_function_with_vars(&fun.expr, x + eps, variables)
                        ) {
                            if y1.is_finite() && y2.is_finite() && (y1 - y2).abs() < 1.0 {
                                samples.push((x, Some((y1 + y2) * 0.5)));
                                continue;
                            }
                        }
                        
                        if let Ok(y) = grafito_geometry::expr::eval_function_with_vars(&fun.expr, x, variables) {
                            if y.is_finite() && y.abs() < 1e50 {
                                samples.push((x, Some(y)));
                                continue;
                            }
                        }
                        samples.push((x, None));
                    }
                    
                    // Refine boundaries using bisection to capture asymptote tails accurately
                    // without falsely extending vertical tangents to infinity.
                    let mut refined_samples = Vec::with_capacity(samples.len() + 100);
                    for i in 0..samples.len() {
                        refined_samples.push(samples[i]);
                        if i + 1 < samples.len() {
                            let (x1, y1_opt) = samples[i];
                            let (x2, y2_opt) = samples[i+1];
                            if y1_opt.is_some() != y2_opt.is_some() {
                                // Boundary detected. Bisect 12 times to get ~4000x closer to the boundary
                                let mut good_x = if y1_opt.is_some() { x1 } else { x2 };
                                let mut bad_x = if y1_opt.is_some() { x2 } else { x1 };
                                let mut best_y = if y1_opt.is_some() { y1_opt.unwrap() } else { y2_opt.unwrap() };
                                
                                for _ in 0..24 {
                                    let mid = (good_x + bad_x) * 0.5;
                                    if let Ok(y) = grafito_geometry::expr::eval_function_with_vars(&fun.expr, mid, variables) {
                                        if y.is_finite() && y.abs() < 1e50 {
                                            good_x = mid;
                                            best_y = y;
                                        } else {
                                            bad_x = mid;
                                        }
                                    } else {
                                        bad_x = mid;
                                    }
                                }
                                
                                // Insert the extremely close point
                                if y1_opt.is_some() {
                                    refined_samples.push((good_x, Some(best_y)));
                                } else {
                                    refined_samples.push((good_x, Some(best_y)));
                                    // Wait, if y1 was None, we are transitioning None -> Some.
                                    // So the boundary point should be placed BEFORE x2 (which is Some).
                                    // The loop order is: push i, then push boundary. So it works perfectly!
                                }
                            }
                        }
                    }
                    let samples = refined_samples;

                    let stroke = Stroke::new(fun.width, to_color32(fun.color));
                    let mut optimized_points = Vec::new();
                    let mut i = 0;
                    while i < samples.len() {
                        let (x, y_opt) = samples[i];
                        if let Some(y) = y_opt {
                            let p = view.world_to_screen(Point2::new(x, y));
                            let px = p.x.round() as i32;
                            let first_y = p.y;
                            let mut min_y = p.y;
                            let mut max_y = p.y;
                            let mut min_j = i;
                            let mut max_j = i;
                            let mut last_y = p.y;
                            
                            let mut j = i + 1;
                            while j < samples.len() {
                                if let Some(y2) = samples[j].1 {
                                    let p2 = view.world_to_screen(Point2::new(samples[j].0, y2));
                                    if (p2.x.round() as i32) == px {
                                        if p2.y < min_y { min_y = p2.y; min_j = j; }
                                        if p2.y > max_y { max_y = p2.y; max_j = j; }
                                        last_y = p2.y;
                                        j += 1;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            optimized_points.push(canvas_rect.min + Vec2::new(px as f32, first_y));
                            if (min_y - first_y).abs() > 1.0 || (max_y - first_y).abs() > 1.0 {
                                if min_j < max_j {
                                    optimized_points.push(canvas_rect.min + Vec2::new(px as f32, min_y));
                                    optimized_points.push(canvas_rect.min + Vec2::new(px as f32, max_y));
                                } else {
                                    optimized_points.push(canvas_rect.min + Vec2::new(px as f32, max_y));
                                    optimized_points.push(canvas_rect.min + Vec2::new(px as f32, min_y));
                                }
                            }
                            optimized_points.push(canvas_rect.min + Vec2::new(px as f32, last_y));
                            i = j;
                        } else {
                            if !optimized_points.is_empty() {
                                painter.add(Shape::line(std::mem::take(&mut optimized_points), stroke));
                            }
                            i += 1;
                        }
                    }
                    if !optimized_points.is_empty() {
                        painter.add(Shape::line(optimized_points, stroke));
                    }

                    if !fun.label.is_empty() {
                        let mid_x = (min_x + max_x) * 0.5;
                        if let Ok(y) = grafito_geometry::expr::eval_function_with_vars(&fun.expr, mid_x, &self.document.variables) {
                            let s = view.world_to_screen(Point2::new(mid_x, y));
                            painter.text(
                                canvas_rect.min + Vec2::new(s.x, s.y + 14.0),
                                egui::Align2::CENTER_TOP,
                                &fun.label,
                                egui::FontId::proportional(12.0),
                                label_color,
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
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y + el.ry as f32 * view.scale as f32 + 14.0),
                            egui::Align2::CENTER_TOP, &el.label, egui::FontId::proportional(12.0), label_color);
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
                            egui::Align2::CENTER_BOTTOM, &pb.label, egui::FontId::proportional(12.0), label_color);
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
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y), egui::Align2::CENTER_BOTTOM, &hb.label, egui::FontId::proportional(12.0), label_color);
                    }
                }
                GeoObject::Text(txt) => {
                    let s = view.world_to_screen(txt.position);
                    painter.text(canvas_rect.min + Vec2::new(s.x, s.y),
                        egui::Align2::LEFT_CENTER, &txt.content,
                        egui::FontId::proportional(txt.font_size.max(8.0)), to_color32(txt.color));
                }
                GeoObject::Histogram(h) => {
                    let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                    let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                    if max_count <= 0.0 { return; }
                    let stroke = Stroke::new(h.width, to_color32(h.color));
                    let fill = h.fill_color.map(to_color32).unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 100));
                    let y_scale = (h.y_max - h.y_min) / max_count;
                    for (left, right, count) in &bins {
                        let bl = view.world_to_screen(Point2::new(*left, h.y_min));
                        let tr = view.world_to_screen(Point2::new(*right, h.y_min + count * y_scale));
                        let rect = Rect::from_min_max(
                            canvas_rect.min + Vec2::new(tr.x, bl.y),
                            canvas_rect.min + Vec2::new(bl.x, tr.y),
                        );
                        painter.rect_filled(rect, 0.0, fill);
                        painter.rect_stroke(rect, 0.0, stroke);
                    }
                }
                GeoObject::ScatterPlot(sp) => {
                    let color = to_color32(sp.color);
                    let r = sp.point_size.max(1.0);
                    for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                        let s = view.world_to_screen(Point2::new(*x, *y));
                        painter.circle_filled(canvas_rect.min + Vec2::new(s.x, s.y), r, color);
                    }
                }
                GeoObject::BoxPlot(bp) => {
                    if let Some((wl, q1, med, q3, wh, outliers)) = grafito_geometry::statistics::boxplot_stats(&bp.data) {
                        let stroke = Stroke::new(bp.width, to_color32(bp.color));
                        let fill = bp.fill_color.map(to_color32).unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 80));
                        let half_w = bp.width_box * 0.5;
                        let bx_min = bp.position - half_w;
                        let bx_max = bp.position + half_w;
                        let s_q1 = view.world_to_screen(Point2::new(bx_min, q1));
                        let s_q3 = view.world_to_screen(Point2::new(bx_max, q3));
                        let box_rect = Rect::from_min_max(
                            canvas_rect.min + Vec2::new(s_q3.x, s_q3.y),
                            canvas_rect.min + Vec2::new(s_q1.x, s_q1.y),
                        );
                        painter.rect_filled(box_rect, 0.0, fill);
                        painter.rect_stroke(box_rect, 0.0, stroke);
                        let s_med_l = view.world_to_screen(Point2::new(bx_min, med));
                        let s_med_r = view.world_to_screen(Point2::new(bx_max, med));
                        painter.line_segment(
                            [canvas_rect.min + Vec2::new(s_med_l.x, s_med_l.y), canvas_rect.min + Vec2::new(s_med_r.x, s_med_r.y)],
                            Stroke::new(bp.width * 2.0, to_color32(bp.color)),
                        );
                        let s_wl = view.world_to_screen(Point2::new(bp.position, wl));
                        let s_q1c = view.world_to_screen(Point2::new(bp.position, q1));
                        painter.line_segment(
                            [canvas_rect.min + Vec2::new(s_wl.x, s_wl.y), canvas_rect.min + Vec2::new(s_q1c.x, s_q1c.y)],
                            stroke,
                        );
                        let s_wh = view.world_to_screen(Point2::new(bp.position, wh));
                        let s_q3c = view.world_to_screen(Point2::new(bp.position, q3));
                        painter.line_segment(
                            [canvas_rect.min + Vec2::new(s_wh.x, s_wh.y), canvas_rect.min + Vec2::new(s_q3c.x, s_q3c.y)],
                            stroke,
                        );
                        let wl_half = half_w * 0.4;
                        let s_wl_l = view.world_to_screen(Point2::new(bp.position - wl_half, wl));
                        let s_wl_r = view.world_to_screen(Point2::new(bp.position + wl_half, wl));
                        painter.line_segment(
                            [canvas_rect.min + Vec2::new(s_wl_l.x, s_wl_l.y), canvas_rect.min + Vec2::new(s_wl_r.x, s_wl_r.y)],
                            stroke,
                        );
                        let s_wh_l = view.world_to_screen(Point2::new(bp.position - wl_half, wh));
                        let s_wh_r = view.world_to_screen(Point2::new(bp.position + wl_half, wh));
                        painter.line_segment(
                            [canvas_rect.min + Vec2::new(s_wh_l.x, s_wh_l.y), canvas_rect.min + Vec2::new(s_wh_r.x, s_wh_r.y)],
                            stroke,
                        );
                        for &o in &outliers {
                            let s_o = view.world_to_screen(Point2::new(bp.position, o));
                            painter.circle_stroke(canvas_rect.min + Vec2::new(s_o.x, s_o.y), 3.0, stroke);
                        }
                    }
                }
                GeoObject::RegressionLine(rl) => {
                    let stroke = Stroke::new(rl.width, to_color32(rl.color));
                    let x0 = rl.x_min;
                    let x1 = rl.x_max;
                    let y0 = rl.slope * x0 + rl.intercept;
                    let y1 = rl.slope * x1 + rl.intercept;
                    let s0 = view.world_to_screen(Point2::new(x0, y0));
                    let s1 = view.world_to_screen(Point2::new(x1, y1));
                    painter.line_segment(
                        [canvas_rect.min + Vec2::new(s0.x, s0.y), canvas_rect.min + Vec2::new(s1.x, s1.y)],
                        stroke,
                    );
                    let pt_color = to_color32(rl.color);
                    for (x, y) in rl.xs.iter().zip(rl.ys.iter()) {
                        let s = view.world_to_screen(Point2::new(*x, *y));
                        painter.circle_filled(canvas_rect.min + Vec2::new(s.x, s.y), 4.0, pt_color);
                    }
                    if !rl.label.is_empty() {
                        let s = view.world_to_screen(Point2::new((x0 + x1) * 0.5, rl.slope * (x0 + x1) * 0.5 + rl.intercept));
                        painter.text(canvas_rect.min + Vec2::new(s.x, s.y - 12.0),
                            egui::Align2::CENTER_BOTTOM, &rl.label, egui::FontId::proportional(11.0), to_color32(rl.color));
                    }
                }
                GeoObject::Fractal2D(fr) => {
                    use grafito_geometry::fractals::{compute_fractal, fractal_color_hsv, FractalType};
                    let fractal_type = match fr.fractal_type.as_str() {
                        "julia" if fr.params.len() >= 2 => FractalType::Julia { cr: fr.params[0], ci: fr.params[1], max_iter: fr.max_iter },
                        "burning_ship" => FractalType::BurningShip { max_iter: fr.max_iter },
                        "tricorn" => FractalType::Tricorn { max_iter: fr.max_iter },
                        _ => FractalType::Mandelbrot { max_iter: fr.max_iter },
                    };
                    let res = fr.resolution.min(400).max(20);
                    let pixels = compute_fractal(&fractal_type, fr.x_min, fr.x_max, fr.y_min, fr.y_max, res, res);
                    let dx = (fr.x_max - fr.x_min) / res as f64;
                    let dy = (fr.y_max - fr.y_min) / res as f64;
                    for px in &pixels {
                        let (r, g, b, a) = fractal_color_hsv(px.iter, px.max_iter, px.smooth_value);
                        let bl = view.world_to_screen(Point2::new(px.x, px.y));
                        let tr = view.world_to_screen(Point2::new(px.x + dx, px.y + dy));
                        let rect = Rect::from_min_max(
                            canvas_rect.min + Vec2::new(bl.x, tr.y),
                            canvas_rect.min + Vec2::new(tr.x, bl.y),
                        );
                        painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(
                            (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8,
                        ));
                    }
                }
                GeoObject::ParametricCurve2D(pc) => {
                    let steps = 500;
                    let dt = (pc.t_max - pc.t_min) / steps as f64;
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=steps {
                        let t = pc.t_min + i as f64 * dt;
                        if let (Ok(x), Ok(y)) = (
                            eval_function_with_vars(&pc.expr_x, t, &self.document.variables),
                            eval_function_with_vars(&pc.expr_y, t, &self.document.variables),
                        ) {
                            if x.is_finite() && y.is_finite() {
                                let screen = view.world_to_screen(Point2::new(x, y));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment([prev_pos, pos], Stroke::new(pc.width, to_color32(pc.color)));
                                }
                                prev = Some(pos);
                            } else {
                                prev = None;
                            }
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::PolarCurve(pol) => {
                    let steps = 500;
                    let dt = (pol.t_max - pol.t_min) / steps as f64;
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=steps {
                        let t = pol.t_min + i as f64 * dt;
                        if let Ok(r) = eval_function_with_vars(&pol.expr_r, t, &self.document.variables) {
                            if r.is_finite() {
                                let x = r * t.cos();
                                let y = r * t.sin();
                                let screen = view.world_to_screen(Point2::new(x, y));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment([prev_pos, pos], Stroke::new(pol.width, to_color32(pol.color)));
                                }
                                prev = Some(pos);
                            } else {
                                prev = None;
                            }
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::VectorField2D(vf) => {
                    let grid_size = 20;
                    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                    let dx = (world_br.x - world_tl.x) / grid_size as f64;
                    let dy = (world_br.y - world_tl.y) / grid_size as f64;
                    let arrow_length = dx.min(dy) * 0.8;
                    
                    for i in 0..grid_size {
                        for j in 0..grid_size {
                            let x = world_tl.x + (i as f64 + 0.5) * dx;
                            let y = world_tl.y + (j as f64 + 0.5) * dy;
                            
                            let vars = vec![("x".to_string(), x), ("y".to_string(), y)];
                            if let (Ok(u), Ok(v)) = (
                                grafito_geometry::expr::evaluate(&vf.expr_u, &vars),
                                grafito_geometry::expr::evaluate(&vf.expr_v, &vars),
                            ) {
                                if u.is_finite() && v.is_finite() {
                                    let mag = (u * u + v * v).sqrt();
                                    if mag > 1e-10 {
                                        let nu = u / mag * arrow_length;
                                        let nv = v / mag * arrow_length;
                                        
                                        let start = view.world_to_screen(Point2::new(x, y));
                                        let end = view.world_to_screen(Point2::new(x + nu, y + nv));
                                        let start_pos = canvas_rect.min + Vec2::new(start.x, start.y);
                                        let end_pos = canvas_rect.min + Vec2::new(end.x, end.y);
                                        
                                        painter.line_segment([start_pos, end_pos], Stroke::new(1.5, to_color32(vf.color)));
                                        
                                        // Arrow head
                                        let angle = (nv as f32).atan2(nu as f32);
                                        let head_len = arrow_length as f32 * 0.3;
                                        let head1 = end_pos + Vec2::new(
                                            -head_len * (angle - 0.4).cos(),
                                            -head_len * (angle - 0.4).sin(),
                                        );
                                        let head2 = end_pos + Vec2::new(
                                            -head_len * (angle + 0.4).cos(),
                                            -head_len * (angle + 0.4).sin(),
                                        );
                                        painter.line_segment([end_pos, head1], Stroke::new(1.5, to_color32(vf.color)));
                                        painter.line_segment([end_pos, head2], Stroke::new(1.5, to_color32(vf.color)));
                                    }
                                }
                            }
                        }
                    }
                }
                GeoObject::ImplicitCurve(ic) => {
                    // Marching squares for implicit curves
                    let grid_size = 100;
                    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                    let dx = (world_br.x - world_tl.x) / grid_size as f64;
                    let dy = (world_br.y - world_tl.y) / grid_size as f64;
                    
                    // Evaluate function on grid
                    let mut values = vec![vec![0.0f64; grid_size + 1]; grid_size + 1];
                    for i in 0..=grid_size {
                        for j in 0..=grid_size {
                            let x = world_tl.x + i as f64 * dx;
                            let y = world_tl.y + j as f64 * dy;
                            let vars = vec![("x".to_string(), x), ("y".to_string(), y)];
                            if let (Ok(lhs), Ok(rhs)) = (
                                grafito_geometry::expr::evaluate(&ic.expr_lhs, &vars),
                                grafito_geometry::expr::evaluate(&ic.expr_rhs, &vars),
                            ) {
                                let val = match ic.operator {
                                    grafito_core::RelationOperator::Eq => lhs - rhs,
                                    grafito_core::RelationOperator::Less => lhs - rhs,
                                    grafito_core::RelationOperator::Greater => rhs - lhs,
                                    grafito_core::RelationOperator::LessEq => lhs - rhs,
                                    grafito_core::RelationOperator::GreaterEq => rhs - lhs,
                                };
                                values[i][j] = if val.is_finite() { val } else { f64::NAN };
                            } else {
                                values[i][j] = f64::NAN;
                            }
                        }
                    }
                    
                    // Marching squares
                    for i in 0..grid_size {
                        for j in 0..grid_size {
                            let v00 = values[i][j];
                            let v10 = values[i + 1][j];
                            let v01 = values[i][j + 1];
                            let v11 = values[i + 1][j + 1];
                            
                            if v00.is_nan() || v10.is_nan() || v01.is_nan() || v11.is_nan() {
                                continue;
                            }
                            
                            let s00 = v00 >= 0.0;
                            let s10 = v10 >= 0.0;
                            let s01 = v01 >= 0.0;
                            let s11 = v11 >= 0.0;
                            
                            let case = (s00 as u8) | ((s10 as u8) << 1) | ((s01 as u8) << 2) | ((s11 as u8) << 3);
                            
                            if case == 0 || case == 15 {
                                continue; // All same sign
                            }
                            
                            let x0 = world_tl.x + i as f64 * dx;
                            let y0 = world_tl.y + j as f64 * dy;
                            let x1 = x0 + dx;
                            let y1 = y0 + dy;
                            
                            // Interpolate edge crossings
                            let interp = |va: f64, vb: f64, pa: f64, pb: f64| -> f64 {
                                let t = va / (va - vb);
                                pa + t * (pb - pa)
                            };
                            
                            let mut points = Vec::new();
                            
                            // Bottom edge (v00 -> v10)
                            if s00 != s10 {
                                let x = interp(v00, v10, x0, x1);
                                points.push((x, y0));
                            }
                            // Right edge (v10 -> v11)
                            if s10 != s11 {
                                let y = interp(v10, v11, y0, y1);
                                points.push((x1, y));
                            }
                            // Top edge (v01 -> v11)
                            if s01 != s11 {
                                let x = interp(v01, v11, x0, x1);
                                points.push((x, y1));
                            }
                            // Left edge (v00 -> v01)
                            if s00 != s01 {
                                let y = interp(v00, v01, y0, y1);
                                points.push((x0, y));
                            }
                            
                            // Draw line segments
                            if points.len() >= 2 {
                                for k in (0..points.len()).step_by(2) {
                                    if k + 1 < points.len() {
                                        let p1 = view.world_to_screen(Point2::new(points[k].0, points[k].1));
                                        let p2 = view.world_to_screen(Point2::new(points[k + 1].0, points[k + 1].1));
                                        let pos1 = canvas_rect.min + Vec2::new(p1.x, p1.y);
                                        let pos2 = canvas_rect.min + Vec2::new(p2.x, p2.y);
                                        painter.line_segment([pos1, pos2], Stroke::new(ic.width, to_color32(ic.color)));
                                    }
                                }
                            }
                        }
                    }
                }
                GeoObject::ComplexGrid(cg) => {
                    // Draw deformed grid under complex mapping
                    use num_complex::Complex64;
                    use std::collections::HashMap;
                    
                    let grid_lines = cg.density;
                    let dx = (cg.x_max - cg.x_min) / grid_lines as f64;
                    let dy = (cg.y_max - cg.y_min) / grid_lines as f64;
                    
                    // Parse the complex expression once
                    let expr = match grafito_geometry::complex_expr::parse(&cg.expr) {
                        Ok(e) => e,
                        Err(_) => return,
                    };
                    
                    // Convert document variables to complex
                    let mut vars: HashMap<String, Complex64> = HashMap::new();
                    for (name, val) in &self.document.variables {
                        vars.insert(name.clone(), Complex64::new(*val, 0.0));
                    }
                    
                    // Draw horizontal lines (constant imaginary part)
                    for j in 0..=grid_lines {
                        let y = cg.y_min + j as f64 * dy;
                        let mut prev: Option<Pos2> = None;
                        for i in 0..=grid_lines * 4 {
                            let x = cg.x_min + i as f64 * dx / 4.0;
                            vars.insert("z".to_string(), Complex64::new(x, y));
                            
                            if let Ok(result) = expr.eval(&vars) {
                                if result.re.is_finite() && result.im.is_finite() {
                                    let screen = view.world_to_screen(Point2::new(result.re, result.im));
                                    let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                    if let Some(prev_pos) = prev {
                                        painter.line_segment([prev_pos, pos], Stroke::new(1.0, to_color32(cg.color)));
                                    }
                                    prev = Some(pos);
                                } else {
                                    prev = None;
                                }
                            } else {
                                prev = None;
                            }
                        }
                    }
                    
                    // Draw vertical lines (constant real part)
                    for i in 0..=grid_lines {
                        let x = cg.x_min + i as f64 * dx;
                        let mut prev: Option<Pos2> = None;
                        for j in 0..=grid_lines * 4 {
                            let y = cg.y_min + j as f64 * dy / 4.0;
                            vars.insert("z".to_string(), Complex64::new(x, y));
                            
                            if let Ok(result) = expr.eval(&vars) {
                                if result.re.is_finite() && result.im.is_finite() {
                                    let screen = view.world_to_screen(Point2::new(result.re, result.im));
                                    let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                    if let Some(prev_pos) = prev {
                                        painter.line_segment([prev_pos, pos], Stroke::new(1.0, to_color32(cg.color)));
                                    }
                                    prev = Some(pos);
                                } else {
                                    prev = None;
                                }
                            } else {
                                prev = None;
                            }
                        }
                    }
                }
                GeoObject::ComplexMapping(cm) => {
                    // Get target object and apply complex mapping
                    use num_complex::Complex64;
                    use std::collections::HashMap;
                    
                    // Parse the complex expression once
                    let expr = match grafito_geometry::complex_expr::parse(&cm.expr) {
                        Ok(e) => e,
                        Err(_) => return,
                    };
                    
                    // Convert document variables to complex
                    let mut vars: HashMap<String, Complex64> = HashMap::new();
                    for (name, val) in &self.document.variables {
                        vars.insert(name.clone(), Complex64::new(*val, 0.0));
                    }
                    
                    if let Some(target) = self.document.get_object(cm.target) {
                        match target {
                            GeoObject::Polygon(poly) => {
                                let mut transformed_verts = Vec::new();
                                for vert in &poly.vertices {
                                    let z = Complex64::new(vert.x, vert.y);
                                    vars.insert("z".to_string(), z);
                                    
                                    if let Ok(result) = expr.eval(&vars) {
                                        if result.re.is_finite() && result.im.is_finite() {
                                            transformed_verts.push(Point2::new(result.re, result.im));
                                        }
                                    }
                                }
                                
                                if transformed_verts.len() >= 3 {
                                    let points: Vec<Pos2> = transformed_verts.iter()
                                        .map(|v| {
                                            let s = view.world_to_screen(*v);
                                            canvas_rect.min + Vec2::new(s.x, s.y)
                                        })
                                        .collect();
                                    
                                    let stroke = Stroke::new(2.0, to_color32(cm.color));
                                    for i in 0..points.len() {
                                        let j = (i + 1) % points.len();
                                        painter.line_segment([points[i], points[j]], stroke);
                                    }
                                }
                            }
                            GeoObject::Line(line) => {
                                let steps = 50;
                                let dx = (line.end.x - line.start.x) / steps as f64;
                                let dy = (line.end.y - line.start.y) / steps as f64;
                                let mut prev: Option<Pos2> = None;
                                
                                for i in 0..=steps {
                                    let x = line.start.x + i as f64 * dx;
                                    let y = line.start.y + i as f64 * dy;
                                    let z = Complex64::new(x, y);
                                    vars.insert("z".to_string(), z);
                                    
                                    if let Ok(result) = expr.eval(&vars) {
                                        if result.re.is_finite() && result.im.is_finite() {
                                            let screen = view.world_to_screen(Point2::new(result.re, result.im));
                                            let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                            if let Some(prev_pos) = prev {
                                                painter.line_segment([prev_pos, pos], Stroke::new(2.0, to_color32(cm.color)));
                                            }
                                            prev = Some(pos);
                                        } else {
                                            prev = None;
                                        }
                                    } else {
                                        prev = None;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
    }
}
