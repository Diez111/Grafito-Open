use crate::GrafitoApp;
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_core::parametric_sampling;
use grafito_core::{CircleObj, GeoObject, LineObj, PointObj, PolygonObj, RenderQuality};
use grafito_geometry::expr::{eval_function_with_vars, eval_integral_batch, prepare_function_ast};
use grafito_geometry::{Color, Point2};
use grafito_ui::Tool;
use std::time::Instant;

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

/// HSL to RGB conversion for domain coloring
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let hue_to_rgb = |p: f64, q: f64, mut t: f64| -> f64 {
        while t < 0.0 {
            t += 1.0;
        }
        while t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

/// Thermal colormap for heat maps: blue → cyan → green → yellow → red
fn thermal_colormap(t: f64) -> (f64, f64, f64) {
    let t = t.clamp(0.0, 1.0);
    let r = (t * 3.0 - 1.5).clamp(0.0, 1.0).min(1.0);
    let g = (1.5 - (t * 3.0 - 1.5).abs()).clamp(0.0, 1.0);
    let b = (1.5 - t * 3.0).clamp(0.0, 1.0);
    (r, g, b)
}

/// Convert integer exponent to Unicode superscript (e.g. 3 → "³", -2 → "⁻²")
fn snap_world_to_grid(world: Point2, scale: f64) -> Point2 {
    let pixels_per_unit = scale;
    let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
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
    Point2::new(
        (world.x / major_step).round() * major_step,
        (world.y / major_step).round() * major_step,
    )
}

fn superscript(exp: i32) -> String {
    let digits: Vec<char> = exp.to_string().chars().collect();
    let mut result = String::new();
    for &c in &digits {
        result.push(match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            _ => c,
        });
    }
    result
}

#[allow(dead_code)]
impl GrafitoApp {
    fn handle_canvas_primary_click(&mut self, world: Point2) {
        match self.current_tool {
            Tool::Select => {
                let tolerance = 10.0 / self.document.view().scale;
                if let Some(id) = self.document.pick_object(world, tolerance) {
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
                self.document
                    .add_object(GeoObject::Point(PointObj::new(world)));
                self.tool_ghost = None;
            }
            Tool::Line => {
                self.pending_points.push(world);
                if self.pending_points.len() == 2 {
                    let a = self.pending_points[0];
                    let b = self.pending_points[1];
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Line(LineObj::new(a, b)));
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
                    self.document
                        .add_object(GeoObject::Circle(CircleObj::new(center, radius)));
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
                let mut cmd = "Lorenz[]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Fractal => {
                let mut cmd = "Mandelbrot[]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Histogram => {
                let mut cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ScatterPlot => {
                let mut cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Tangent => {
                self.pending_points.push(world);
                if self.pending_points.len() == 2 {
                    let p1 = self.pending_points[0];
                    let p2 = self.pending_points[1];
                    let mut cmd = format!(
                        "Tangent[({:.2}, {:.2}), 1, ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    crate::commands::process_input(&mut self.document, &mut cmd);
                    self.pending_points.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::Perpendicular => {
                self.pending_points.push(world);
                if self.pending_points.len() == 2 {
                    let p1 = self.pending_points[0];
                    let p2 = self.pending_points[1];
                    let mut cmd = format!(
                        "PerpendicularBisector[({:.2}, {:.2}), ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    crate::commands::process_input(&mut self.document, &mut cmd);
                    self.pending_points.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::DomainColoring => {
                let mut cmd = "DomainColoring[z^2 + 1, -3, 3, -3, 3, 200]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::HeatMap => {
                let mut cmd = "HeatMap[x^2 + y^2, -5, 5, -5, 5, 150]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ComplexGrid => {
                let mut cmd = "ComplexGrid[sin(z), -3, 3, -3, 3]".to_string();
                crate::commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Locus
            | Tool::Midpoint
            | Tool::Slider
            | Tool::Button
            | Tool::Distance
            | Tool::Angle
            | Tool::Area
            | Tool::Slope
            | Tool::Image
            | Tool::Segment
            | Tool::Ray
            | Tool::Vector
            | Tool::RegularPolygon => {
                let mut state = self.tool_state.clone();
                let result = crate::tool_dispatcher::dispatch_tool(
                    self.current_tool,
                    &mut state,
                    &mut self.document,
                    world,
                );
                self.tool_state = state;
                if result.reset_tool {
                    self.current_tool = Tool::Select;
                }
                if let Some(msg) = result.message {
                    self.cas_result = msg;
                }
                for obj in result.objects {
                    self.document.add_object(obj);
                }
            }
        }
    }

    fn update_tool_ghost(&mut self, world: Point2) {
        self.tool_ghost = None;
        let pts = &self.tool_state.pending;
        match self.current_tool {
            Tool::Point | Tool::Point3D => {
                self.tool_ghost = Some(GeoObject::Point(PointObj::new(world)));
            }
            Tool::Line | Tool::Distance | Tool::Perpendicular | Tool::Midpoint => {
                if let Some(first) = pts.first() {
                    self.tool_ghost = Some(GeoObject::Line(LineObj::new(*first, world)));
                }
            }
            Tool::Circle | Tool::Tangent => {
                if let Some(center) = pts.first() {
                    let radius = center.distance(&world);
                    self.tool_ghost = Some(GeoObject::Circle(CircleObj::new(*center, radius)));
                }
            }
            Tool::Polygon => {
                if let Some(last) = pts.last() {
                    self.tool_ghost = Some(GeoObject::Line(LineObj::new(*last, world)));
                }
            }
            Tool::Angle if pts.len() == 1 => {
                self.tool_ghost = Some(GeoObject::Line(LineObj::new(pts[0], world)));
            }
            Tool::Angle if pts.len() == 2 => {
                self.tool_ghost = Some(GeoObject::Line(LineObj::new(pts[1], world)));
            }
            Tool::Slider => {
                let bar = LineObj::new(
                    Point2::new(world.x - 1.5, world.y),
                    Point2::new(world.x + 1.5, world.y),
                );
                self.tool_ghost = Some(GeoObject::Line(bar));
            }
            Tool::Locus => {}
            _ => {}
        }
    }

    pub(crate) fn draw_grid_numbers_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        self.draw_axes(painter, canvas_rect);
    }

    pub(crate) fn draw_complex_objects_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        for obj in self.document.objects().values() {
            if matches!(
                obj,
                GeoObject::Function(_)
                    | GeoObject::Polygon(_)
                    | GeoObject::Hyperbola(_)
                    | GeoObject::Parabola(_)
                    | GeoObject::Ellipse(_)
            ) {
                self.draw_object(painter, canvas_rect, obj);
            }
        }
    }

    pub(crate) fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        const CLICK_THRESHOLD: f32 = 3.0;

        let response = ui.interact(canvas_rect, ui.id().with("canvas"), Sense::click_and_drag());

        self.document.view_mut().screen_size =
            GlamVec2::new(canvas_rect.width(), canvas_rect.height());

        let space_pressed = ui.input(|i| i.key_down(egui::Key::Space));
        let pointer = ui.input(|i| i.pointer.clone());

        // Current pointer position: prefer interaction point during drag, then hover, then global
        let current_pos = response
            .interact_pointer_pos()
            .or(response.hover_pos())
            .or(pointer.latest_pos());
        let pointer_in_canvas = current_pos
            .map(|p| canvas_rect.contains(p))
            .unwrap_or(false);

        // ── Drag lifecycle: start / distance / stop ──────────────────────────
        if response.drag_started() {
            self.canvas_drag_start = current_pos;
            self.canvas_is_panning = false;
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            self.document.render_quality = RenderQuality::Preview;
        }

        let drag_distance = self
            .canvas_drag_start
            .and_then(|s| current_pos.map(|p| (p - s).length()))
            .unwrap_or(0.0);
        let became_drag = drag_distance > CLICK_THRESHOLD;
        if became_drag {
            self.canvas_is_panning = true;
        }

        // Compute world position at pointer for tools / hover
        let world_at_pointer = current_pos.map(|pos| {
            let local = pos - canvas_rect.min;
            self.document
                .view()
                .screen_to_world(GlamVec2::new(local.x, local.y))
        });

        // ── Determine panning ────────────────────────────────────────────────
        let mut panning = false;
        let mut pan_delta = Vec2::ZERO;

        // Right-click is reserved for polygon closing / cancel when a polygon is in progress.
        let can_pan_with_right =
            self.current_tool != Tool::Polygon || self.pending_points.is_empty();
        let pan_button_pressed = pointer_in_canvas
            && (pointer.button_down(egui::PointerButton::Middle)
                || (pointer.button_down(egui::PointerButton::Secondary) && can_pan_with_right));

        // 1. Space + primary drag: universal pan
        if space_pressed && response.dragged_by(egui::PointerButton::Primary) {
            panning = true;
            pan_delta = response.drag_delta();
        }
        // 2. Middle/right button drag: universal pan (direct pointer reading)
        else if pan_button_pressed {
            let delta = pointer.delta();
            if delta != Vec2::ZERO {
                panning = true;
                pan_delta = delta;
            }
        }
        // 3. Primary drag: pan unless we are moving a free point in Select mode
        else if response.dragged_by(egui::PointerButton::Primary) {
            let moving_point = self.current_tool == Tool::Select
                && self
                    .selected_object
                    .map(|id| self.document.is_free_object(&id))
                    .unwrap_or(false);
            if !moving_point {
                panning = true;
                pan_delta = response.drag_delta();
            }
        }

        // Apply pan
        if panning && pan_delta != Vec2::ZERO {
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            self.document.render_quality = RenderQuality::Preview;
            self.document
                .view_mut()
                .pan(GlamVec2::new(pan_delta.x, pan_delta.y));
        }

        // ── Move free point in Select mode (primary drag, not panning) ───────
        if !panning
            && self.current_tool == Tool::Select
            && response.dragged_by(egui::PointerButton::Primary)
        {
            if let (Some(sel_id), Some(pos)) = (self.selected_object, current_pos) {
                if self.document.is_free_object(&sel_id) {
                    let local = pos - canvas_rect.min;
                    let mut world = self
                        .document
                        .view()
                        .screen_to_world(GlamVec2::new(local.x, local.y));
                    if self.snap_to_grid {
                        world = snap_world_to_grid(world, self.document.view().scale);
                    }
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        self.save_state();
                    }
                    self.document.move_point(sel_id, world);
                    let order = self.document.propagation_order(&[sel_id]);
                    self.document.re_evaluate_constraints(&order);
                }
            }
        }

        // ── Cursor feedback ──────────────────────────────────────────────────
        if panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if space_pressed && pointer_in_canvas {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        } else if self.current_tool == Tool::Select && response.hover_pos().is_some() {
            if let Some(world) = world_at_pointer {
                let tolerance = 10.0 / self.document.view().scale;
                if self.document.pick_object(world, tolerance).is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }

        // ── Clicks (ignore if this was a pan gesture) ────────────────────────
        let is_click = !self.canvas_is_panning && drag_distance <= CLICK_THRESHOLD;

        if response.clicked_by(egui::PointerButton::Primary) && is_click {
            if let Some(world) = world_at_pointer {
                let world = if self.snap_to_grid {
                    snap_world_to_grid(world, self.document.view().scale)
                } else {
                    world
                };
                self.handle_canvas_primary_click(world);
            }
        }

        // Right-click: close polygon / cancel pending point (only if not a pan)
        if response.clicked_by(egui::PointerButton::Secondary) && is_click {
            if self.current_tool == Tool::Polygon && self.pending_points.len() >= 3 {
                self.save_state();
                let vertices = self.pending_points.clone();
                self.document
                    .add_object(GeoObject::Polygon(PolygonObj::new(vertices)));
                self.pending_points.clear();
                self.tool_ghost = None;
            } else if !self.pending_points.is_empty() {
                // Cancel single pending point (Line/Circle first point)
                self.pending_points.clear();
                self.tool_ghost = None;
            }
        }

        // ── Zoom with scroll wheel ───────────────────────────────────────────
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                self.is_view_changing = true;
                self.last_interaction_time = Instant::now();
                self.document.render_quality = RenderQuality::Preview;
                let factor = if scroll.y > 0.0 {
                    1.0 + scroll.y.abs() * 0.001
                } else {
                    1.0 / (1.0 + scroll.y.abs() * 0.001)
                };
                if let Some(pos) = response.hover_pos() {
                    let local = pos - canvas_rect.min;
                    self.document
                        .view_mut()
                        .zoom(factor.clamp(0.8, 1.25), GlamVec2::new(local.x, local.y));
                }
            }
        }

        // ── Tool ghost preview ───────────────────────────────────────────────
        if let Some(world) = world_at_pointer {
            let world = if self.snap_to_grid {
                snap_world_to_grid(world, self.document.view().scale)
            } else {
                world
            };
            self.update_tool_ghost(world);
        }

        // ── Cleanup drag state ───────────────────────────────────────────────
        if response.drag_stopped() {
            self.canvas_is_panning = false;
            self.canvas_drag_start = None;
        }

        // Keep last known position for external consumers (status bar, etc.)
        if let Some(pos) = current_pos {
            self.last_mouse_pos = Some(pos);
        }
    }

    pub(crate) fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if !self.show_grid {
            return;
        }
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let grid_color = if self.dark_mode {
            Color32::from_rgba_unmultiplied(255, 255, 255, 25)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 25)
        };
        let grid_stroke = Stroke::new(1.0, grid_color);
        let minor_stroke = Stroke::new(
            0.5,
            if self.dark_mode {
                Color32::from_rgba_unmultiplied(255, 255, 255, 12)
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 12)
            },
        );

        // Vertical grid lines
        if view.x_log {
            let min_pow = world_tl.x.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_br.x.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let x = 10_f64.powf(pow as f64);
                let a = view.world_to_screen(Point2::new(x, world_br.y));
                let b = view.world_to_screen(Point2::new(x, world_tl.y));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
                // Minor grid at 2..9 * 10^pow
                if pow < max_pow {
                    for k in 2..=9 {
                        let xm = k as f64 * 10_f64.powf(pow as f64);
                        let am = view.world_to_screen(Point2::new(xm, world_br.y));
                        let bm = view.world_to_screen(Point2::new(xm, world_tl.y));
                        painter.line_segment(
                            [
                                canvas_rect.min + Vec2::new(am.x, am.y),
                                canvas_rect.min + Vec2::new(bm.x, bm.y),
                            ],
                            minor_stroke,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
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
            let min_x = (world_tl.x / major_step).floor() as i32 - 1;
            let max_x = (world_br.x / major_step).ceil() as i32 + 1;
            for xi in min_x..=max_x {
                let x = xi as f64 * major_step;
                let a = view.world_to_screen(Point2::new(x, world_br.y.min(world_tl.y)));
                let b = view.world_to_screen(Point2::new(x, world_br.y.max(world_tl.y)));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
            }
        }

        // Horizontal grid lines
        if view.y_log {
            let min_pow = world_br.y.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_tl.y.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let y = 10_f64.powf(pow as f64);
                let a = view.world_to_screen(Point2::new(world_tl.x, y));
                let b = view.world_to_screen(Point2::new(world_br.x, y));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
                if pow < max_pow {
                    for k in 2..=9 {
                        let ym = k as f64 * 10_f64.powf(pow as f64);
                        let am = view.world_to_screen(Point2::new(world_tl.x, ym));
                        let bm = view.world_to_screen(Point2::new(world_br.x, ym));
                        painter.line_segment(
                            [
                                canvas_rect.min + Vec2::new(am.x, am.y),
                                canvas_rect.min + Vec2::new(bm.x, bm.y),
                            ],
                            minor_stroke,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
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
            let min_y = (world_br.y / major_step).floor() as i32 - 1;
            let max_y = (world_tl.y / major_step).ceil() as i32 + 1;
            for yi in min_y..=max_y {
                let y = yi as f64 * major_step;
                let a = view.world_to_screen(Point2::new(world_tl.x, y));
                let b = view.world_to_screen(Point2::new(world_br.x, y));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
            }
        }
    }

    pub(crate) fn draw_axes(&self, painter: &egui::Painter, canvas_rect: Rect) {
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let stroke = if self.dark_mode {
            Stroke::new(1.0, Color32::from_gray(180))
        } else {
            Stroke::new(1.0, Color32::from_gray(80))
        };

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        painter.line_segment(
            [
                canvas_rect.min + Vec2::new(x_axis_a.x, x_axis_a.y),
                canvas_rect.min + Vec2::new(x_axis_b.x, x_axis_b.y),
            ],
            stroke,
        );

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        painter.line_segment(
            [
                canvas_rect.min + Vec2::new(y_axis_a.x, y_axis_a.y),
                canvas_rect.min + Vec2::new(y_axis_b.x, y_axis_b.y),
            ],
            stroke,
        );
        // Tick marks and labels — log-appropriate or linear
        let text_color = if self.dark_mode {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(80)
        };
        let font = egui::FontId::proportional(12.0);
        let minor_tick = Stroke::new(0.5, text_color);

        // X-axis ticks
        if view.x_log {
            let min_pow = world_tl.x.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_br.x.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let x = 10_f64.powf(pow as f64);
                let s = view.world_to_screen(Point2::new(x, x_axis_y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(0.0, -4.0), pos + Vec2::new(0.0, 4.0)],
                    stroke,
                );
                let label = if pow == 0 {
                    "1".into()
                } else if pow == 1 {
                    "10".into()
                } else if pow == -1 {
                    "10⁻¹".into()
                } else {
                    format!("10{}", superscript(pow))
                };
                painter.text(
                    pos + Vec2::new(0.0, 6.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    font.clone(),
                    text_color,
                );
                // Minor ticks at 2..9 * 10^pow
                if pow < max_pow {
                    for k in 2..=9 {
                        let xm = k as f64 * 10_f64.powf(pow as f64);
                        let sm = view.world_to_screen(Point2::new(xm, x_axis_y));
                        let posm = canvas_rect.min + Vec2::new(sm.x, sm.y);
                        painter.line_segment(
                            [posm + Vec2::new(0.0, -2.0), posm + Vec2::new(0.0, 2.0)],
                            minor_tick,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
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
            let min_x = (world_tl.x / major_step).floor() as i32 - 1;
            let max_x = (world_br.x / major_step).ceil() as i32 + 1;
            for xi in min_x..=max_x {
                let x = xi as f64 * major_step;
                if x.abs() < 1e-9 {
                    continue;
                }
                let s = view.world_to_screen(Point2::new(x, x_axis_y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(0.0, -3.0), pos + Vec2::new(0.0, 3.0)],
                    stroke,
                );
                // Format nicely
                let label = if (x.fract()).abs() < 1e-9 {
                    format!("{}", x as i64)
                } else {
                    format!("{:.2}", x)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                painter.text(
                    pos + Vec2::new(0.0, 6.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    font.clone(),
                    text_color,
                );
            }
        }

        // Y-axis ticks
        if view.y_log {
            let min_pow = world_br.y.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_tl.y.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let y = 10_f64.powf(pow as f64);
                let s = view.world_to_screen(Point2::new(y_axis_x, y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(-4.0, 0.0), pos + Vec2::new(4.0, 0.0)],
                    stroke,
                );
                let label = if pow == 0 {
                    "1".into()
                } else if pow == 1 {
                    "10".into()
                } else if pow == -1 {
                    "10⁻¹".into()
                } else {
                    format!("10{}", superscript(pow))
                };
                painter.text(
                    pos + Vec2::new(-6.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    font.clone(),
                    text_color,
                );
                if pow < max_pow {
                    for k in 2..=9 {
                        let ym = k as f64 * 10_f64.powf(pow as f64);
                        let sm = view.world_to_screen(Point2::new(y_axis_x, ym));
                        let posm = canvas_rect.min + Vec2::new(sm.x, sm.y);
                        painter.line_segment(
                            [posm + Vec2::new(-2.0, 0.0), posm + Vec2::new(2.0, 0.0)],
                            minor_tick,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
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
            let min_y = (world_br.y / major_step).floor() as i32 - 1;
            let max_y = (world_tl.y / major_step).ceil() as i32 + 1;
            for yi in min_y..=max_y {
                let y = yi as f64 * major_step;
                if y.abs() < 1e-9 {
                    continue;
                }
                let s = view.world_to_screen(Point2::new(y_axis_x, y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(-3.0, 0.0), pos + Vec2::new(3.0, 0.0)],
                    stroke,
                );
                let label = if (y.fract()).abs() < 1e-9 {
                    format!("{}", y as i64)
                } else {
                    format!("{:.2}", y)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                painter.text(
                    pos + Vec2::new(-6.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    font.clone(),
                    text_color,
                );
            }
        }

        let origin = view.world_to_screen(Point2::new(0.0, 0.0));
        let origin_pos = canvas_rect.min + Vec2::new(origin.x, origin.y);
        painter.text(
            origin_pos + Vec2::new(-6.0, 6.0),
            egui::Align2::RIGHT_TOP,
            "0",
            font,
            text_color,
        );
    }

    pub(crate) fn draw_objects(&mut self, painter: &egui::Painter, canvas_rect: Rect) {
        // Pre-compute (or reuse cached) implicit-curve geometry before the
        // immutable draw pass. The cache lives inside each ImplicitCurveObj and
        // is invalidated only when expression, view bounds or variables change.
        {
            let view = *self.document.view();
            let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
            let world_br =
                view.screen_to_world(glam::Vec2::new(canvas_rect.width(), canvas_rect.height()));
            let view_bounds = (world_tl.x, world_br.x, world_tl.y, world_br.y);
            let quality = self.document.render_quality;
            let grid_size = grafito_core::implicit_curve::recommended_grid_size_for_quality(
                canvas_rect.width(),
                canvas_rect.height(),
                quality,
            );
            let variables = self.document.variables.clone();
            for (_, obj) in self.document.objects_iter_mut() {
                if let GeoObject::ImplicitCurve(ic) = obj {
                    let _unused = grafito_core::implicit_curve::segments_or_compute(
                        ic,
                        view_bounds,
                        grid_size,
                        &variables,
                        quality,
                    );
                }
            }
        }

        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            // Skip 3D objects in 2D view — they can't be rendered here
            if matches!(
                obj,
                GeoObject::Point3D(_)
                    | GeoObject::Segment3D(_)
                    | GeoObject::Sphere3D(_)
                    | GeoObject::Cube3D(_)
                    | GeoObject::Pyramid3D(_)
                    | GeoObject::Cone3D(_)
                    | GeoObject::Cylinder3D(_)
                    | GeoObject::Surface3D(_)
                    | GeoObject::ParametricCurve3D(_)
                    | GeoObject::Attractor3D(_)
                    | GeoObject::HyperSurface4D(_)
                    | GeoObject::VectorField3D(_)
            ) {
                continue;
            }
            self.draw_object(painter, canvas_rect, obj);
        }

        if let Some(preview) = &self.preview_object {
            let mut preview_clone = preview.clone();
            match &mut preview_clone {
                GeoObject::Function(f) => {
                    f.color = Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    };
                    f.width = 2.5;
                    f.label = String::new();
                }
                GeoObject::Point(p) => {
                    p.color = Color {
                        r: 0.4,
                        g: 0.4,
                        b: 0.4,
                        a: 0.8,
                    };
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

    fn draw_arrowhead(painter: &egui::Painter, from: Pos2, to: Pos2, width: f32, color: Color32) {
        let dir = to - from;
        let len = dir.length();
        if len < 1e-3 {
            return;
        }
        let dir = dir / len;
        let normal = Vec2::new(-dir.y, dir.x);
        let arrow_len = (width * 4.0).max(6.0).min(len * 0.5);
        let arrow_width = arrow_len * 0.5;

        let tip_back = to - dir * arrow_len;
        let left = tip_back + normal * arrow_width;
        let right = tip_back - normal * arrow_width;

        painter.line_segment([to, left], Stroke::new(width, color));
        painter.line_segment([to, right], Stroke::new(width, color));
    }

    pub(crate) fn draw_tool_ghost(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if let Some(ghost) = &self.tool_ghost {
            let mut g = ghost.clone();
            // Reduce opacity to create ghost/shadow effect
            let ghost_alpha = 0.3;
            match &mut g {
                GeoObject::Point(p) => {
                    p.color.a = (p.color.a * ghost_alpha).max(0.05);
                    p.size *= 1.3;
                }
                GeoObject::Line(l) => {
                    l.color.a = (l.color.a * ghost_alpha).max(0.05);
                    l.width *= 0.7;
                }
                GeoObject::Circle(c) => {
                    c.color.a = (c.color.a * ghost_alpha).max(0.05);
                    c.width *= 0.7;
                    c.fill_color = None;
                }
                _ => {}
            }
            self.draw_object(painter, canvas_rect, &g);
        }
    }

    pub(crate) fn draw_object(&self, painter: &egui::Painter, canvas_rect: Rect, obj: &GeoObject) {
        let view = self.document.view();
        let label_color = if self.dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
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
                let start = Point2::new(
                    self.document.resolve_expr(&l.start_x_expr, l.start.x),
                    self.document.resolve_expr(&l.start_y_expr, l.start.y),
                );
                let end = Point2::new(
                    self.document.resolve_expr(&l.end_x_expr, l.end.x),
                    self.document.resolve_expr(&l.end_y_expr, l.end.y),
                );

                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );

                let stroke = Stroke::new(l.width, to_color32(l.color));
                let clipped = match l.kind {
                    grafito_core::LineKind::Segment => {
                        grafito_geometry::clip_segment_to_rect(start, end, view_bounds)
                    }
                    grafito_core::LineKind::Ray => {
                        grafito_geometry::clip_ray_to_rect(start, end, view_bounds)
                    }
                    grafito_core::LineKind::Line => {
                        grafito_geometry::clip_line_to_rect(start, end, view_bounds)
                    }
                };
                if let Some((clip_start, clip_end)) = clipped {
                    let a = view.world_to_screen(clip_start);
                    let b = view.world_to_screen(clip_end);
                    let pa = canvas_rect.min + Vec2::new(a.x, a.y);
                    let pb = canvas_rect.min + Vec2::new(b.x, b.y);
                    painter.line_segment([pa, pb], stroke);

                    // Arrowhead for vectors at the forward (t=1) end.
                    let is_vector = l.label == "v";
                    if is_vector {
                        Self::draw_arrowhead(painter, pa, pb, l.width, to_color32(l.color));
                    }
                }
                if !l.label.is_empty() {
                    let mid = if l.kind == grafito_core::LineKind::Segment {
                        let a = view.world_to_screen(start);
                        let b = view.world_to_screen(end);
                        (a + b) * 0.5
                    } else {
                        // Place label near the start for rays/lines.
                        view.world_to_screen(start)
                    };
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
                let radius = (c.radius * view.scale) as f32;
                let radius = radius.clamp(0.5, 50000.0);
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
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let points: Vec<_> = poly
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = self
                            .document
                            .resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = self
                            .document
                            .resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        let s = view.world_to_screen(Point2::new(x, y));
                        canvas_rect.min + Vec2::new(s.x, s.y)
                    })
                    .collect();
                let stroke = Stroke::new(poly.width, to_color32(poly.color));
                if let Some(fill) = poly.fill_color {
                    painter.add(Shape::convex_polygon(
                        points.clone(),
                        to_color32(fill),
                        stroke,
                    ));
                } else {
                    painter.add(Shape::convex_polygon(
                        points.clone(),
                        Color32::TRANSPARENT,
                        stroke,
                    ));
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
            GeoObject::Function(fun) => {
                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let min_x = self
                    .document
                    .resolve_expr(&fun.domain_min_expr, fun.domain_min.unwrap_or(world_tl.x));
                let max_x = self
                    .document
                    .resolve_expr(&fun.domain_max_expr, fun.domain_max.unwrap_or(world_br.x));

                let variables = &self.document.variables;
                let samples: Vec<(f64, Option<f64>)> = if fun.is_integral {
                    let screen_width = canvas_rect.width() as f64;
                    let world_width = max_x - min_x;

                    let mut steps = screen_width.clamp(400.0, 4000.0) as usize;
                    let max_world_step = 0.01;
                    let mut step = world_width / steps as f64;
                    if step > max_world_step {
                        steps = (world_width / max_world_step).ceil() as usize;
                    }
                    steps = steps.min(500_000);
                    step = world_width / steps as f64;

                    let xs = (0..=steps).map(|i| min_x + i as f64 * step);

                    let mut s: Vec<(f64, Option<f64>)> = Vec::with_capacity(steps + 1);
                    let batch_results = eval_integral_batch(
                        &fun.expr,
                        &fun.integral_var,
                        fun.integral_lower,
                        xs.clone(),
                        variables,
                    );

                    for (x, y_opt) in
                        xs.zip(batch_results.into_iter().chain(std::iter::repeat(None)))
                    {
                        if let Some(y) = y_opt {
                            if y.is_finite() && y.abs() < 1e50 {
                                s.push((x, Some(y)));
                                continue;
                            }
                        }
                        let eps = 1e-9;
                        if let (Ok(y1), Ok(y2)) = (
                            eval_function_with_vars(&fun.expr, x - eps, variables),
                            eval_function_with_vars(&fun.expr, x + eps, variables),
                        ) {
                            if y1.is_finite() && y2.is_finite() && (y1 - y2).abs() < 1.0 {
                                s.push((x, Some((y1 + y2) * 0.5)));
                                continue;
                            }
                        }
                        if let Ok(y) = eval_function_with_vars(&fun.expr, x, variables) {
                            if y.is_finite() && y.abs() < 1e50 {
                                s.push((x, Some(y)));
                                continue;
                            }
                        }
                        s.push((x, None));
                    }
                    s
                } else {
                    let domain = (min_x, max_x);
                    let grid_size =
                        grafito_core::function_sampling::recommended_grid_size_for_quality(
                            canvas_rect.width(),
                            self.document.render_quality,
                        );
                    grafito_core::function_sampling::samples_or_compute(
                        fun, domain, grid_size, variables,
                    )
                    .clone()
                };

                let mut refined_samples = Vec::with_capacity(samples.len() + 100);
                for i in 0..samples.len() {
                    refined_samples.push(samples[i]);
                    if i + 1 < samples.len() {
                        let (x1, y1_opt) = samples[i];
                        let (x2, y2_opt) = samples[i + 1];
                        if y1_opt.is_some() != y2_opt.is_some() {
                            let mut good_x = if y1_opt.is_some() { x1 } else { x2 };
                            let mut bad_x = if y1_opt.is_some() { x2 } else { x1 };
                            let mut best_y = if let Some(y1) = y1_opt {
                                y1
                            } else {
                                y2_opt.unwrap_or(0.0)
                            };

                            for _ in 0..24 {
                                let mid = (good_x + bad_x) * 0.5;
                                if let Ok(y) = eval_function_with_vars(&fun.expr, mid, variables) {
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

                            refined_samples.push((good_x, Some(best_y)));
                        }
                    }
                }
                let samples = refined_samples;

                // Fill area under curve if fill_color is set
                if let Some(fill) = fun.fill_color {
                    let mut fill_pts: Vec<Pos2> = Vec::new();
                    // Top edge: left to right along the curve
                    for &(x, y_opt) in &samples {
                        if let Some(y) = y_opt {
                            if y.is_finite() {
                                let s = view.world_to_screen(Point2::new(x, y));
                                fill_pts.push(canvas_rect.min + Vec2::new(s.x, s.y));
                            }
                        }
                    }
                    // Bottom edge: right to left along y=0 (or min_y)
                    if !fill_pts.is_empty() {
                        let mut bottom_pts: Vec<Pos2> = Vec::new();
                        for &(x, _) in samples.iter().rev() {
                            let s = view.world_to_screen(Point2::new(x, 0.0));
                            bottom_pts.push(canvas_rect.min + Vec2::new(s.x, s.y));
                        }
                        fill_pts.append(&mut bottom_pts);
                        // Close polygon on the left
                        if let Some(first) = fill_pts.first() {
                            fill_pts.push(*first);
                        }
                        if fill_pts.len() >= 3 {
                            let fill_rgba = to_color32(fill);
                            let fill_stroke = Stroke::new(0.5, fill_rgba);
                            painter.add(Shape::line(fill_pts.clone(), fill_stroke));
                            painter.add(Shape::convex_polygon(fill_pts, fill_rgba, Stroke::NONE));
                        }
                    }
                }

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
                                    if p2.y < min_y {
                                        min_y = p2.y;
                                        min_j = j;
                                    }
                                    if p2.y > max_y {
                                        max_y = p2.y;
                                        max_j = j;
                                    }
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
                                optimized_points
                                    .push(canvas_rect.min + Vec2::new(px as f32, min_y));
                                optimized_points
                                    .push(canvas_rect.min + Vec2::new(px as f32, max_y));
                            } else {
                                optimized_points
                                    .push(canvas_rect.min + Vec2::new(px as f32, max_y));
                                optimized_points
                                    .push(canvas_rect.min + Vec2::new(px as f32, min_y));
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
                    if let Ok(y) = grafito_geometry::expr::eval_function_with_vars(
                        &fun.expr,
                        mid_x,
                        &self.document.variables,
                    ) {
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
                    let x = el.center.x + el.rx * t.cos() * el.angle.cos()
                        - el.ry * t.sin() * el.angle.sin();
                    let y = el.center.y
                        + el.rx * t.cos() * el.angle.sin()
                        + el.ry * t.sin() * el.angle.cos();
                    let s = view.world_to_screen(Point2::new(x, y));
                    pts.push(canvas_rect.min + Vec2::new(s.x, s.y));
                }
                if let Some(fill) = el.fill_color {
                    painter.add(Shape::convex_polygon(
                        pts.clone(),
                        to_color32(fill),
                        Stroke::NONE,
                    ));
                }
                for i in 0..n {
                    let j = (i + 1) % n;
                    painter.line_segment([pts[i], pts[j]], stroke);
                }
                if !el.label.is_empty() {
                    let s = view.world_to_screen(el.center);
                    painter.text(
                        canvas_rect.min
                            + Vec2::new(s.x, s.y + el.ry as f32 * view.scale as f32 + 14.0),
                        egui::Align2::CENTER_TOP,
                        &el.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Parabola(pb) => {
                let _v = view.world_to_screen(pb.vertex);
                let stroke = Stroke::new(pb.width, to_color32(pb.color));
                let steps = 64;
                let x_range = 10.0 / view.scale;
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
                        (
                            pb.vertex.x + t,
                            pb.vertex.y + t * t / (4.0 * pb.p.max(0.001)),
                        )
                    } else {
                        (
                            pb.vertex.x + t * t / (4.0 * pb.p.max(0.001)),
                            pb.vertex.y + t,
                        )
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
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y - 8.0),
                        egui::Align2::CENTER_BOTTOM,
                        &pb.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Hyperbola(hb) => {
                let stroke = Stroke::new(hb.width, to_color32(hb.color));
                let range = 8.0 / view.scale;
                let n = 64;
                // Right branch
                let mut prev: Option<Pos2> = None;
                for i in 0..=n {
                    let x = hb.center.x + hb.a + range * i as f64 / n as f64;
                    let dx = x - hb.center.x;
                    if dx > hb.a {
                        let y_off = hb.b * ((dx / hb.a).powi(2) - 1.0).sqrt();
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
                        let y_off = hb.b * ((dx / hb.a).powi(2) - 1.0).sqrt();
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
                if !hb.label.is_empty() {
                    let s =
                        view.world_to_screen(Point2::new(hb.center.x, hb.center.y + hb.b + 0.5));
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y),
                        egui::Align2::CENTER_BOTTOM,
                        &hb.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Text(txt) => {
                let s = view.world_to_screen(txt.position);
                painter.text(
                    canvas_rect.min + Vec2::new(s.x, s.y),
                    egui::Align2::LEFT_CENTER,
                    &txt.content,
                    egui::FontId::proportional(txt.font_size.max(8.0)),
                    to_color32(txt.color),
                );
            }
            GeoObject::Histogram(h) => {
                let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                if max_count <= 0.0 {
                    return;
                }
                let stroke = Stroke::new(h.width, to_color32(h.color));
                let fill = h
                    .fill_color
                    .map(to_color32)
                    .unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 100));
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
                if let Some((wl, q1, med, q3, wh, outliers)) =
                    grafito_geometry::statistics::boxplot_stats(&bp.data)
                {
                    let stroke = Stroke::new(bp.width, to_color32(bp.color));
                    let fill = bp
                        .fill_color
                        .map(to_color32)
                        .unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 80));
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
                        [
                            canvas_rect.min + Vec2::new(s_med_l.x, s_med_l.y),
                            canvas_rect.min + Vec2::new(s_med_r.x, s_med_r.y),
                        ],
                        Stroke::new(bp.width * 2.0, to_color32(bp.color)),
                    );
                    let s_wl = view.world_to_screen(Point2::new(bp.position, wl));
                    let s_q1c = view.world_to_screen(Point2::new(bp.position, q1));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wl.x, s_wl.y),
                            canvas_rect.min + Vec2::new(s_q1c.x, s_q1c.y),
                        ],
                        stroke,
                    );
                    let s_wh = view.world_to_screen(Point2::new(bp.position, wh));
                    let s_q3c = view.world_to_screen(Point2::new(bp.position, q3));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wh.x, s_wh.y),
                            canvas_rect.min + Vec2::new(s_q3c.x, s_q3c.y),
                        ],
                        stroke,
                    );
                    let wl_half = half_w * 0.4;
                    let s_wl_l = view.world_to_screen(Point2::new(bp.position - wl_half, wl));
                    let s_wl_r = view.world_to_screen(Point2::new(bp.position + wl_half, wl));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wl_l.x, s_wl_l.y),
                            canvas_rect.min + Vec2::new(s_wl_r.x, s_wl_r.y),
                        ],
                        stroke,
                    );
                    let s_wh_l = view.world_to_screen(Point2::new(bp.position - wl_half, wh));
                    let s_wh_r = view.world_to_screen(Point2::new(bp.position + wl_half, wh));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wh_l.x, s_wh_l.y),
                            canvas_rect.min + Vec2::new(s_wh_r.x, s_wh_r.y),
                        ],
                        stroke,
                    );
                    for &o in &outliers {
                        let s_o = view.world_to_screen(Point2::new(bp.position, o));
                        painter.circle_stroke(
                            canvas_rect.min + Vec2::new(s_o.x, s_o.y),
                            3.0,
                            stroke,
                        );
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
                    [
                        canvas_rect.min + Vec2::new(s0.x, s0.y),
                        canvas_rect.min + Vec2::new(s1.x, s1.y),
                    ],
                    stroke,
                );
                let pt_color = to_color32(rl.color);
                for (x, y) in rl.xs.iter().zip(rl.ys.iter()) {
                    let s = view.world_to_screen(Point2::new(*x, *y));
                    painter.circle_filled(canvas_rect.min + Vec2::new(s.x, s.y), 4.0, pt_color);
                }
                if !rl.label.is_empty() {
                    let s = view.world_to_screen(Point2::new(
                        (x0 + x1) * 0.5,
                        rl.slope * (x0 + x1) * 0.5 + rl.intercept,
                    ));
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y - 12.0),
                        egui::Align2::CENTER_BOTTOM,
                        &rl.label,
                        egui::FontId::proportional(11.0),
                        to_color32(rl.color),
                    );
                }
            }
            GeoObject::Fractal2D(fr) => {
                use grafito_geometry::fractals::{compute_fractal, fractal_color_hsv, FractalType};
                let fractal_type = match fr.fractal_type.as_str() {
                    "julia" if fr.params.len() >= 2 => FractalType::Julia {
                        cr: fr.params[0],
                        ci: fr.params[1],
                        max_iter: fr.max_iter,
                    },
                    "burning_ship" => FractalType::BurningShip {
                        max_iter: fr.max_iter,
                    },
                    "tricorn" => FractalType::Tricorn {
                        max_iter: fr.max_iter,
                    },
                    _ => FractalType::Mandelbrot {
                        max_iter: fr.max_iter,
                    },
                };
                let res = fr.resolution.clamp(20, 400);
                let pixels = compute_fractal(
                    &fractal_type,
                    fr.x_min,
                    fr.x_max,
                    fr.y_min,
                    fr.y_max,
                    res,
                    res,
                );
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
                    painter.rect_filled(
                        rect,
                        0.0,
                        Color32::from_rgba_premultiplied(
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8,
                            (a * 255.0) as u8,
                        ),
                    );
                }
            }
            GeoObject::ParametricCurve2D(pc) => {
                let steps = 4000;
                let samples = parametric_sampling::samples_or_compute_curve_2d(
                    pc,
                    steps,
                    &self.document.variables,
                );
                let mut prev: Option<Pos2> = None;
                for &(x, y) in samples.iter() {
                    if x.is_finite() && y.is_finite() {
                        let screen = view.world_to_screen(Point2::new(x, y));
                        let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                        if let Some(prev_pos) = prev {
                            painter.line_segment(
                                [prev_pos, pos],
                                Stroke::new(pc.width, to_color32(pc.color)),
                            );
                        }
                        prev = Some(pos);
                    } else {
                        prev = None;
                    }
                }
            }
            GeoObject::PolarCurve(pol) => {
                let steps = 4000;
                let samples = parametric_sampling::samples_or_compute_polar(
                    pol,
                    steps,
                    &self.document.variables,
                );
                let mut prev: Option<Pos2> = None;
                let mut all_pts: Vec<Pos2> = Vec::new();
                for &(x, y) in samples.iter() {
                    if x.is_finite() && y.is_finite() {
                        let screen = view.world_to_screen(Point2::new(x, y));
                        let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                        if let Some(prev_pos) = prev {
                            painter.line_segment(
                                [prev_pos, pos],
                                Stroke::new(pol.width, to_color32(pol.color)),
                            );
                        }
                        all_pts.push(pos);
                        prev = Some(pos);
                    } else {
                        prev = None;
                    }
                }
                // Fill from origin
                if let Some(fill) = pol.fill_color {
                    if all_pts.len() >= 3 {
                        let origin = view.world_to_screen(Point2::new(0.0, 0.0));
                        let origin_pos = canvas_rect.min + Vec2::new(origin.x, origin.y);
                        let mut fill_pts = all_pts.clone();
                        fill_pts.push(origin_pos);
                        if let Some(&first) = all_pts.first() {
                            fill_pts.push(first);
                        }
                        painter.add(Shape::convex_polygon(
                            fill_pts,
                            to_color32(fill),
                            Stroke::NONE,
                        ));
                    }
                }
            }
            GeoObject::VectorField2D(vf) => {
                let grid_size = 20;
                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
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

                                    painter.line_segment(
                                        [start_pos, end_pos],
                                        Stroke::new(1.5, to_color32(vf.color)),
                                    );

                                    // Arrow head
                                    let angle = (nv as f32).atan2(nu as f32);
                                    let head_len = arrow_length as f32 * 0.3;
                                    let head1 = end_pos
                                        + Vec2::new(
                                            -head_len * (angle - 0.4).cos(),
                                            -head_len * (angle - 0.4).sin(),
                                        );
                                    let head2 = end_pos
                                        + Vec2::new(
                                            -head_len * (angle + 0.4).cos(),
                                            -head_len * (angle + 0.4).sin(),
                                        );
                                    painter.line_segment(
                                        [end_pos, head1],
                                        Stroke::new(1.5, to_color32(vf.color)),
                                    );
                                    painter.line_segment(
                                        [end_pos, head2],
                                        Stroke::new(1.5, to_color32(vf.color)),
                                    );
                                }
                            }
                        }
                    }
                }
                // Streamlines: trace from seed points using RK4
                let sl_steps = 200;
                let sl_dt = 0.05;
                let sl_color = Color32::from_rgba_unmultiplied(180, 100, 200, 180);
                let sl_stroke = Stroke::new(1.2, sl_color);
                // Distribute seeds uniformly
                let seeds_x = 5;
                let seeds_y = 5;
                let sx = (world_br.x - world_tl.x) / (seeds_x + 1) as f64;
                let sy = (world_br.y - world_tl.y) / (seeds_y + 1) as f64;
                for si in 1..=seeds_x {
                    for sj in 1..=seeds_y {
                        let mut x = world_tl.x + si as f64 * sx;
                        let mut y = world_tl.y + sj as f64 * sy;
                        let mut prev: Option<Pos2> = None;
                        for _ in 0..sl_steps {
                            let vars_eval = vec![("x".to_string(), x), ("y".to_string(), y)];
                            if let (Ok(u), Ok(v)) = (
                                grafito_geometry::expr::evaluate(&vf.expr_u, &vars_eval),
                                grafito_geometry::expr::evaluate(&vf.expr_v, &vars_eval),
                            ) {
                                if !u.is_finite() || !v.is_finite() {
                                    break;
                                }
                                let k1x = u;
                                let k1y = v;
                                let half_dt = sl_dt * 0.5;
                                // k2
                                let (k2x, k2y) = match (
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_u,
                                        &[
                                            ("x".into(), x + half_dt * k1x),
                                            ("y".into(), y + half_dt * k1y),
                                        ],
                                    ),
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_v,
                                        &[
                                            ("x".into(), x + half_dt * k1x),
                                            ("y".into(), y + half_dt * k1y),
                                        ],
                                    ),
                                ) {
                                    (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() => (a, b),
                                    _ => {
                                        break;
                                    }
                                };
                                // k3
                                let (k3x, k3y) = match (
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_u,
                                        &[
                                            ("x".into(), x + half_dt * k2x),
                                            ("y".into(), y + half_dt * k2y),
                                        ],
                                    ),
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_v,
                                        &[
                                            ("x".into(), x + half_dt * k2x),
                                            ("y".into(), y + half_dt * k2y),
                                        ],
                                    ),
                                ) {
                                    (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() => (a, b),
                                    _ => {
                                        break;
                                    }
                                };
                                // k4
                                let (k4x, k4y) = match (
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_u,
                                        &[
                                            ("x".into(), x + sl_dt * k3x),
                                            ("y".into(), y + sl_dt * k3y),
                                        ],
                                    ),
                                    grafito_geometry::expr::evaluate(
                                        &vf.expr_v,
                                        &[
                                            ("x".into(), x + sl_dt * k3x),
                                            ("y".into(), y + sl_dt * k3y),
                                        ],
                                    ),
                                ) {
                                    (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() => (a, b),
                                    _ => {
                                        break;
                                    }
                                };
                                x += sl_dt / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
                                y += sl_dt / 6.0 * (k1y + 2.0 * k2y + 2.0 * k3y + k4y);
                                let screen = view.world_to_screen(Point2::new(x, y));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if x < world_tl.x - dx
                                    || x > world_br.x + dx
                                    || y < world_tl.y - dy
                                    || y > world_br.y + dy
                                {
                                    break;
                                }
                                if let Some(prev_pos) = prev {
                                    painter.line_segment([prev_pos, pos], sl_stroke);
                                }
                                prev = Some(pos);
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            GeoObject::ImplicitCurve(ic) => {
                let levels = ic.cached_segments.read().unwrap_or_else(|p| p.into_inner());
                if !levels.is_empty() {
                    let use_contour_colors = ic.contour_levels.is_some();
                    let contour_count = levels.len();
                    let palette = ic.contour_colors.as_deref().unwrap_or(&[]);
                    for (idx, (_level, segs)) in levels.iter().enumerate() {
                        let color = if use_contour_colors {
                            palette.get(idx).cloned().unwrap_or_else(|| {
                                let t = idx as f64 / contour_count.max(1) as f64;
                                Color::new(
                                    (0.5 + t * 0.5) as f32,
                                    (0.2 + (1.0 - t) * 0.6) as f32,
                                    0.2,
                                    1.0,
                                )
                            })
                        } else {
                            ic.color
                        };
                        let stroke = Stroke::new(ic.width, to_color32(color));
                        for (a, b) in segs {
                            let p1 = view.world_to_screen(*a);
                            let p2 = view.world_to_screen(*b);
                            let pos1 = canvas_rect.min + Vec2::new(p1.x, p1.y);
                            let pos2 = canvas_rect.min + Vec2::new(p2.x, p2.y);
                            painter.line_segment([pos1, pos2], stroke);
                        }
                    }
                }
            }
            GeoObject::ComplexGrid(cg) => {
                use num_complex::Complex64;
                use std::collections::HashMap;

                if cg.render_mode == 1 || cg.render_mode == 2 {
                    // Domain coloring (complex f(z)) or Heat map (real f(x,y))
                    let res = cg.density.clamp(50, 500);
                    let dx = (cg.x_max - cg.x_min) / res as f64;
                    let dy = (cg.y_max - cg.y_min) / res as f64;

                    let is_heatmap = cg.render_mode == 2;

                    if is_heatmap {
                        // Heat map: evaluate f(x,y) using real AST
                        let vars_map: std::collections::HashMap<String, f64> =
                            self.document.variables.clone();
                        let prepared = prepare_function_ast(&cg.expr, &vars_map, &["x", "y"]);

                        if let Ok(ast) = prepared {
                            for j in 0..res {
                                let y = cg.y_min + (res - 1 - j) as f64 * dy;
                                for i in 0..res {
                                    let x = cg.x_min + i as f64 * dx;
                                    let val = ast.eval_2d("x", x, "y", y);
                                    if val.is_finite() {
                                        // Thermal colormap: blue(cold) through green to red(hot)
                                        let t = (val.atan() / std::f64::consts::FRAC_PI_2)
                                            .clamp(-1.0, 1.0);
                                        let t = (t + 1.0) * 0.5; // [0, 1]
                                        let (r, g, b) = thermal_colormap(t);
                                        let sp1 = view.world_to_screen(Point2::new(
                                            cg.x_min + i as f64 * dx,
                                            cg.y_min + (res - 1 - j) as f64 * dy,
                                        ));
                                        let sp2 = view.world_to_screen(Point2::new(
                                            cg.x_min + (i + 1) as f64 * dx,
                                            cg.y_min + (res - j) as f64 * dy,
                                        ));
                                        let min = canvas_rect.min + Vec2::new(sp1.x, sp2.y);
                                        let max = canvas_rect.min + Vec2::new(sp2.x, sp1.y);
                                        let c = Color32::from_rgb(
                                            (r * 255.0) as u8,
                                            (g * 255.0) as u8,
                                            (b * 255.0) as u8,
                                        );
                                        painter.rect_filled(Rect::from_min_max(min, max), 0.0, c);
                                    }
                                }
                            }
                        }
                    } else {
                        // Domain coloring: evaluate complex f(z)
                        let expr = match grafito_geometry::complex_expr::parse(&cg.expr) {
                            Ok(e) => e,
                            Err(_) => return,
                        };
                        let mut vars: HashMap<String, Complex64> = HashMap::new();
                        for (name, val) in &self.document.variables {
                            vars.insert(name.clone(), Complex64::new(*val, 0.0));
                        }
                        for j in 0..res {
                            let y = cg.y_min + (res - 1 - j) as f64 * dy;
                            for i in 0..res {
                                let x = cg.x_min + i as f64 * dx;
                                vars.insert("z".to_string(), Complex64::new(x, y));
                                if let Ok(fz) = expr.eval(&vars) {
                                    if fz.re.is_finite() && fz.im.is_finite() {
                                        let arg = fz.arg();
                                        let mag = fz.norm();
                                        let hue = (arg + std::f64::consts::PI)
                                            / (2.0 * std::f64::consts::PI);
                                        let lightness = (mag.max(1e-10).ln().atan()
                                            / std::f64::consts::FRAC_PI_2)
                                            * 0.5
                                            + 0.5;
                                        let (r, g, b) =
                                            hsl_to_rgb(hue, 0.85, lightness.clamp(0.0, 1.0));
                                        let sp1 = view.world_to_screen(Point2::new(
                                            cg.x_min + i as f64 * dx,
                                            cg.y_min + (res - 1 - j) as f64 * dy,
                                        ));
                                        let sp2 = view.world_to_screen(Point2::new(
                                            cg.x_min + (i + 1) as f64 * dx,
                                            cg.y_min + (res - j) as f64 * dy,
                                        ));
                                        let min = canvas_rect.min + Vec2::new(sp1.x, sp2.y);
                                        let max = canvas_rect.min + Vec2::new(sp2.x, sp1.y);
                                        let c = Color32::from_rgb(
                                            (r * 255.0) as u8,
                                            (g * 255.0) as u8,
                                            (b * 255.0) as u8,
                                        );
                                        painter.rect_filled(Rect::from_min_max(min, max), 0.0, c);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }

                // Original: Draw deformed grid under complex mapping
                let grid_lines = cg.density;
                let dx = (cg.x_max - cg.x_min) / grid_lines as f64;
                let dy = (cg.y_max - cg.y_min) / grid_lines as f64;

                let expr = match grafito_geometry::complex_expr::parse(&cg.expr) {
                    Ok(e) => e,
                    Err(_) => return,
                };

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
                            if result.re.is_finite()
                                && result.im.is_finite()
                                && result.re.abs() < 1e6
                                && result.im.abs() < 1e6
                            {
                                let screen =
                                    view.world_to_screen(Point2::new(result.re, result.im));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment(
                                        [prev_pos, pos],
                                        Stroke::new(1.0, to_color32(cg.color)),
                                    );
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
                            if result.re.is_finite()
                                && result.im.is_finite()
                                && result.re.abs() < 1e6
                                && result.im.abs() < 1e6
                            {
                                let screen =
                                    view.world_to_screen(Point2::new(result.re, result.im));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment(
                                        [prev_pos, pos],
                                        Stroke::new(1.0, to_color32(cg.color)),
                                    );
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
                            for (i, vert) in poly.vertices.iter().enumerate() {
                                let x = self
                                    .document
                                    .resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), vert.x);
                                let y = self
                                    .document
                                    .resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), vert.y);
                                let z = Complex64::new(x, y);
                                vars.insert("z".to_string(), z);

                                if let Ok(result) = expr.eval(&vars) {
                                    if result.re.is_finite() && result.im.is_finite() {
                                        transformed_verts.push(Point2::new(result.re, result.im));
                                    }
                                }
                            }

                            if transformed_verts.len() >= 3 {
                                let points: Vec<Pos2> = transformed_verts
                                    .iter()
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
                            let start = Point2::new(
                                self.document.resolve_expr(&line.start_x_expr, line.start.x),
                                self.document.resolve_expr(&line.start_y_expr, line.start.y),
                            );
                            let end = Point2::new(
                                self.document.resolve_expr(&line.end_x_expr, line.end.x),
                                self.document.resolve_expr(&line.end_y_expr, line.end.y),
                            );
                            let steps = 50;
                            let dx = (end.x - start.x) / steps as f64;
                            let dy = (end.y - start.y) / steps as f64;
                            let mut prev: Option<Pos2> = None;

                            for i in 0..=steps {
                                let x = start.x + i as f64 * dx;
                                let y = start.y + i as f64 * dy;
                                let z = Complex64::new(x, y);
                                vars.insert("z".to_string(), z);

                                if let Ok(result) = expr.eval(&vars) {
                                    if result.re.is_finite() && result.im.is_finite() {
                                        let screen =
                                            view.world_to_screen(Point2::new(result.re, result.im));
                                        let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                        if let Some(prev_pos) = prev {
                                            painter.line_segment(
                                                [prev_pos, pos],
                                                Stroke::new(2.0, to_color32(cm.color)),
                                            );
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
