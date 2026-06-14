//! Mouse, keyboard, and gesture input handling for the central canvas.
//!
//! Covers 2D/3D drag, pan, zoom, selection, tool clicks, and the transient
//! tool-ghost preview that follows the pointer.

use crate::{commands, GrafitoApp};
use egui::{PointerButton, Rect, Sense, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_core::{
    CircleObj, GeoObject, LineObj, Point3DObj, PointObj, PolygonObj, RenderQuality,
};
use grafito_geometry::{Camera3D, Point2, Point3D};
use grafito_ui::Tool;
use std::time::Instant;

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
                commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Fractal => {
                let mut cmd = "Mandelbrot[]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Histogram => {
                let mut cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ScatterPlot => {
                let mut cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
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
                    commands::process_input(&mut self.document, &mut cmd);
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
                    commands::process_input(&mut self.document, &mut cmd);
                    self.pending_points.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::DomainColoring => {
                let mut cmd = "DomainColoring[z^2 + 1, -3, 3, -3, 3, 200]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::HeatMap => {
                let mut cmd = "HeatMap[x^2 + y^2, -5, 5, -5, 5, 150]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ComplexGrid => {
                let mut cmd = "ComplexGrid[sin(z), -3, 3, -3, 3]".to_string();
                commands::process_input(&mut self.document, &mut cmd);
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
            && (pointer.button_down(PointerButton::Middle)
                || (pointer.button_down(PointerButton::Secondary) && can_pan_with_right));

        // 1. Space + primary drag: universal pan
        if space_pressed && response.dragged_by(PointerButton::Primary) {
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
        else if response.dragged_by(PointerButton::Primary) {
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
            && response.dragged_by(PointerButton::Primary)
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
                    if response.drag_started_by(PointerButton::Primary) {
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

        if response.clicked_by(PointerButton::Primary) && is_click {
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
        if response.clicked_by(PointerButton::Secondary) && is_click {
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
}

impl GrafitoApp {
    pub(crate) fn handle_canvas_3d_input(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        let w = canvas_rect.width();
        let h = canvas_rect.height();
        self.camera.aspect = w / h.max(1.0);

        let ctx_resp = ui.interact(
            canvas_rect,
            ui.id().with("ctx3d"),
            egui::Sense::click_and_drag(),
        );
        if ctx_resp.clicked_by(egui::PointerButton::Secondary) {
            ctx_resp.context_menu(|ui| {
                if ui.button("Borrar selección").clicked() {
                    self.delete_selected();
                    ui.close_menu();
                }
                if ui.button("Reiniciar vista").clicked() {
                    self.camera = Camera3D::new(w / h.max(1.0));
                    ui.close_menu();
                }
            });
        }

        let response = ui.interact(
            canvas_rect,
            ui.id().with("canvas3d"),
            egui::Sense::click_and_drag(),
        );

        let space_pressed = ui.input(|i| i.key_down(egui::Key::Space));
        let pointer = ui.input(|i| i.pointer.clone());
        let current_pos = response
            .interact_pointer_pos()
            .or(response.hover_pos())
            .or(pointer.latest_pos());
        let pointer_in_canvas = current_pos
            .map(|p| canvas_rect.contains(p))
            .unwrap_or(false);

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
        if drag_distance > 3.0 {
            self.canvas_is_panning = true;
        }

        // Orbit with right drag
        if response.dragged_by(egui::PointerButton::Secondary) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let delta = response.drag_delta();
            self.camera.orbit(delta.x * 0.005, delta.y * 0.005);
        }
        // Pan with Space + primary, middle button, or primary drag in any tool
        else if (space_pressed && response.dragged_by(egui::PointerButton::Primary))
            || (pointer_in_canvas && pointer.button_down(egui::PointerButton::Middle))
            || response.dragged_by(egui::PointerButton::Primary)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let delta = if pointer.button_down(egui::PointerButton::Middle) {
                pointer.delta()
            } else {
                response.drag_delta()
            };
            if delta != egui::Vec2::ZERO {
                self.is_view_changing = true;
                self.last_interaction_time = Instant::now();
                self.document.render_quality = RenderQuality::Preview;
                self.camera.pan(delta.x, delta.y);
            }
        } else if space_pressed && pointer_in_canvas {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        if response.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta);
            if sc.y != 0.0 {
                self.is_view_changing = true;
                self.last_interaction_time = Instant::now();
                self.document.render_quality = RenderQuality::Preview;
                self.camera.zoom(1.0 + sc.y * 0.005);
            }
        }

        // Tool ghost for 3D mode
        self.tool_ghost = None;
        if matches!(
            self.current_tool,
            Tool::Point3D | Tool::Sphere3D | Tool::Cube3D
        ) {
            let t = self.camera.target;
            let ghost_pos = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
            self.tool_ghost = Some(GeoObject::Point3D(Point3DObj::new(ghost_pos)));
        }

        if let Some(pos) = current_pos {
            self.last_mouse_pos = Some(pos);
        }

        if response.drag_stopped() {
            self.canvas_is_panning = false;
            self.canvas_drag_start = None;
        }

        // 3D object placement: only on real clicks, not drags
        let is_click = !self.canvas_is_panning && drag_distance <= 3.0;
        if response.clicked_by(egui::PointerButton::Primary)
            && is_click
            && self.current_tool != Tool::Select
        {
            self.handle_3d_click(ui, &response, canvas_rect, w, h);
            self.tool_ghost = None;
        }
    }
}
