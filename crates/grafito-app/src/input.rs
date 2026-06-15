//! Mouse, keyboard, and gesture input handling for the central canvas.
//!
//! Covers 2D/3D drag, pan, zoom, selection, tool clicks, and the transient
//! tool-ghost preview that follows the pointer.

use crate::{commands, GrafitoApp, PendingAction};
use egui::{PointerButton, Rect, Sense, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_core::{
    CircleObj, FunctionObj, GeoObject, ImplicitCurveObj, LineObj, ParametricCurve2DObj, Point3DObj,
    PointObj, PolarCurveObj, PolygonObj, RelationOperator, RenderQuality, VectorField2DObj,
};
use grafito_geometry::analysis::AnalysisResult;
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
    fn handle_canvas_primary_click(&mut self, world: Point2, time: f64) {
        if !matches!(self.pending_action, PendingAction::None) {
            let tolerance = 10.0 / self.document.view().scale;
            if let Some(id) = self.document.pick_object(world, tolerance) {
                self.document.clear_selection();
                self.document.select(id);
                self.selected_object = Some(id);
                self.handle_pending_object_click(id, time);
            }
            return;
        }

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
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let a = self.tool_state.pending[0];
                    let b = self.tool_state.pending[1];
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Line(LineObj::new(a, b)));
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Circle => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let center = self.tool_state.pending[0];
                    let edge = self.tool_state.pending[1];
                    let radius = center.distance(&edge);
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Circle(CircleObj::new(center, radius)));
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Polygon => {
                self.tool_state.pending.push(world);
            }
            Tool::Function => {
                let mut cmd = "y = x^2".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.current_tool = Tool::Select;
            }
            Tool::Point3D => {
                let p3 = Point3D::new(world.x, world.y, 0.0);
                self.save_state();
                self.document
                    .add_object(GeoObject::Point3D(grafito_core::Point3DObj::new(p3)));
                self.tool_ghost = None;
            }
            Tool::Sphere3D => {
                let p3 = Point3D::new(world.x, world.y, 0.0);
                self.pending_points_3d.push(p3);
                if self.pending_points_3d.len() == 2 {
                    let center = self.pending_points_3d[0];
                    let edge = self.pending_points_3d[1];
                    let radius = center.distance(&edge);
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Sphere3D(grafito_core::Sphere3DObj::new(
                            center, radius,
                        )));
                    self.pending_points_3d.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Cube3D => {
                let p3 = Point3D::new(world.x, world.y, 0.0);
                self.pending_points_3d.push(p3);
                if self.pending_points_3d.len() == 2 {
                    let p1 = self.pending_points_3d[0];
                    let p2 = self.pending_points_3d[1];
                    let size = p1.distance(&p2);
                    self.save_state();
                    self.document
                        .add_object(GeoObject::Cube3D(grafito_core::Cube3DObj::new(p1, size)));
                    self.pending_points_3d.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Attractor => {
                let mut cmd = "Lorenz[]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Fractal => {
                let mut cmd = "Mandelbrot[]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Histogram => {
                let mut cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ScatterPlot => {
                let mut cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Tangent => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let p1 = self.tool_state.pending[0];
                    let p2 = self.tool_state.pending[1];
                    let mut cmd = format!(
                        "Tangent[({:.2}, {:.2}), 1, ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    let outcome = commands::process_input(&mut self.document, &mut cmd);
                    self.handle_command_outcome(outcome, time, &cmd);
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::Perpendicular => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let p1 = self.tool_state.pending[0];
                    let p2 = self.tool_state.pending[1];
                    let mut cmd = format!(
                        "PerpendicularBisector[({:.2}, {:.2}), ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    let outcome = commands::process_input(&mut self.document, &mut cmd);
                    self.handle_command_outcome(outcome, time, &cmd);
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::DomainColoring => {
                let mut cmd = "DomainColoring[z^2 + 1, -3, 3, -3, 3, 200]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::HeatMap => {
                let mut cmd = "HeatMap[x^2 + y^2, -5, 5, -5, 5, 150]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ComplexGrid => {
                let mut cmd = "ComplexGrid[sin(z), -3, 3, -3, 3]".to_string();
                let outcome = commands::process_input(&mut self.document, &mut cmd);
                self.handle_command_outcome(outcome, time, &cmd);
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
            | Tool::Root
            | Tool::Extremum
            | Tool::Inflection
            | Tool::YIntercept
            | Tool::XIntercept
            | Tool::Analyze
            | Tool::Intersect
            | Tool::RegularPolygon
            | Tool::ParametricCurve2D
            | Tool::PolarCurve
            | Tool::ImplicitCurve
            | Tool::VectorField2D => {
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
                if let Some(outcome) = self.tool_state.last_outcome.take() {
                    self.handle_command_outcome(outcome, time, self.current_tool.name());
                }
                for obj in result.objects {
                    self.document.add_object(obj);
                }
            }
            Tool::Coincident
            | Tool::Horizontal
            | Tool::Vertical
            | Tool::EqualLength
            | Tool::Symmetry
            | Tool::EllipseByFoci
            | Tool::ParabolaByFocusDirectrix
            | Tool::HyperbolaByFoci
            | Tool::ConicByFivePoints
            | Tool::PolygonUnion
            | Tool::PolygonIntersection
            | Tool::PolygonDifference
            | Tool::PolygonXor => {
                // These tools are driven by the pending_action state machine in app.rs.
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
            Tool::Segment => {
                if let Some(first) = pts.first() {
                    self.tool_ghost = Some(GeoObject::Line(grafito_core::LineObj::new_with_kind(
                        *first,
                        world,
                        grafito_core::LineKind::Segment,
                    )));
                }
            }
            Tool::Ray => {
                if let Some(first) = pts.first() {
                    self.tool_ghost = Some(GeoObject::Line(grafito_core::LineObj::new_with_kind(
                        *first,
                        world,
                        grafito_core::LineKind::Ray,
                    )));
                }
            }
            Tool::Vector => {
                if let Some(first) = pts.first() {
                    self.tool_ghost = Some(GeoObject::Line(
                        grafito_core::LineObj::new_with_kind(
                            *first,
                            world,
                            grafito_core::LineKind::Segment,
                        )
                        .with_label("v"),
                    ));
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
            Tool::RegularPolygon => {
                if let Some(center) = pts.first() {
                    let radius = center.distance(&world);
                    let start_angle = (world.y - center.y).atan2(world.x - center.x);
                    let n = 5;
                    let verts: Vec<Point2> = (0..n)
                        .map(|i| {
                            let angle = start_angle + i as f64 / n as f64 * std::f64::consts::TAU;
                            Point2::new(
                                center.x + radius * angle.cos(),
                                center.y + radius * angle.sin(),
                            )
                        })
                        .collect();
                    self.tool_ghost = Some(GeoObject::Polygon(PolygonObj::new(verts)));
                }
            }
            Tool::Sphere3D => {
                if let Some(center) = self.pending_points_3d.first() {
                    let c2 = Point2::new(center.x, center.y);
                    let radius = c2.distance(&world);
                    self.tool_ghost = Some(GeoObject::Circle(CircleObj::new(c2, radius)));
                    // Draw 2D circle as ghost proxy
                }
            }
            Tool::Cube3D => {
                self.tool_ghost = Some(GeoObject::Point(PointObj::new(world)));
            }
            Tool::Angle if pts.len() == 1 => {
                self.tool_ghost = Some(GeoObject::Line(LineObj::new(pts[0], world)));
            }
            Tool::Angle if pts.len() == 2 => {
                // Show the two lines of the angle
                self.tool_ghost = Some(GeoObject::Line(LineObj::new(pts[1], world)));
            }
            Tool::Area
            | Tool::Slope
            | Tool::Root
            | Tool::Extremum
            | Tool::Inflection
            | Tool::YIntercept
            | Tool::Analyze => {
                // These tools highlight hovered items via hovered_analysis.
            }
            Tool::Slider => {
                let bar = LineObj::new(
                    Point2::new(world.x - 1.5, world.y),
                    Point2::new(world.x + 1.5, world.y),
                );
                self.tool_ghost = Some(GeoObject::Line(bar));
            }
            Tool::Function => {
                self.tool_ghost = Some(GeoObject::Function(FunctionObj::new("x^2")));
            }
            Tool::ParametricCurve2D => {
                self.tool_ghost = Some(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                    "cos(t)",
                    "sin(t)",
                    0.0,
                    std::f64::consts::TAU,
                )));
            }
            Tool::PolarCurve => {
                self.tool_ghost = Some(GeoObject::PolarCurve(PolarCurveObj::new(
                    "1 - cos(t)",
                    0.0,
                    std::f64::consts::TAU,
                )));
            }
            Tool::ImplicitCurve => {
                self.tool_ghost = Some(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                    "x^2 + y^2",
                    "4",
                    RelationOperator::Eq,
                )));
            }
            Tool::VectorField2D => {
                self.tool_ghost = Some(GeoObject::VectorField2D(VectorField2DObj::new("x", "y")));
            }
            Tool::Locus => {}
            _ => {}
        }
    }

    pub(crate) fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("input_canvas");

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
            #[cfg(feature = "profile")]
            puffin::profile_scope!("input_drag_start");
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
            self.current_tool != Tool::Polygon || self.tool_state.pending.is_empty();
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
            #[cfg(feature = "profile")]
            puffin::profile_scope!("input_pan");
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
                    self.re_evaluate_constraints(&order);
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
        } else {
            ui.ctx().set_cursor_icon(self.current_tool.cursor_icon());
        }

        // ── Hover Analytics (Dynamic Inspector) ───────────────────────────────
        if !panning
            && !self.is_view_changing
            && response.hover_pos().is_some()
            && self.current_view != crate::ViewMode::D3
        {
            if let Some(world) = world_at_pointer {
                let current_time = ui.ctx().input(|i| i.time);
                let pixel_tolerance = 15.0 / self.document.view().scale;
                
                let mut reset = true;
                if let Some(cand_pos) = self.hover_candidate_pos {
                    if cand_pos.distance(&world) <= (2.0 / self.document.view().scale) {
                        reset = false;
                    }
                }
                
                if reset {
                    self.hover_candidate_pos = Some(world);
                    self.hover_candidate_time = current_time;
                    self.hover_cached_analysis = None;
                    self.hovered_analysis = None;
                } else if current_time - self.hover_candidate_time >= 0.15 {
                    if self.hover_cached_analysis.is_none() {
                        self.hovered_analysis = None;
                        self.update_hover_analysis(world, pixel_tolerance);
                        self.hover_cached_analysis = Some(self.hovered_analysis.clone());
                    } else {
                        self.hovered_analysis = self.hover_cached_analysis.as_ref().unwrap().clone();
                    }
                } else {
                    self.hovered_analysis = None;
                }
                
                if self.hover_cached_analysis.is_none() {
                    let time_left = 0.15 - (current_time - self.hover_candidate_time);
                    if time_left > 0.0 {
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(time_left));
                    }
                }
            }
        } else {
            self.hovered_analysis = None;
            self.hover_candidate_pos = None;
            self.hover_cached_analysis = None;
        }

        // ── Clicks (ignore if this was a pan gesture) ────────────────────────
        let is_click = !self.canvas_is_panning && drag_distance <= CLICK_THRESHOLD;

        if response.clicked_by(PointerButton::Primary) && is_click {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("input_click");
            if let Some(world) = world_at_pointer {
                let world = if self.snap_to_grid {
                    snap_world_to_grid(world, self.document.view().scale)
                } else {
                    world
                };
                self.handle_canvas_primary_click(world, ui.ctx().input(|i| i.time));
            }
        }

        // Right-click: close polygon / cancel pending point (only if not a pan)
        if response.clicked_by(PointerButton::Secondary) && is_click {
            if !matches!(self.pending_action, PendingAction::None) {
                self.clear_pending_action();
                self.current_tool = Tool::Select;
                return;
            }
            if self.current_tool == Tool::Polygon && self.tool_state.pending.len() >= 3 {
                self.save_state();
                let vertices = self.tool_state.pending.clone();
                self.document
                    .add_object(GeoObject::Polygon(PolygonObj::new(vertices)));
                self.tool_state.pending.clear();
                self.tool_ghost = None;
            } else if !self.tool_state.pending.is_empty() {
                // Cancel single pending point (Line/Circle first point)
                self.tool_state.pending.clear();
                self.tool_ghost = None;
            }
        }

        // ── Zoom with scroll wheel ───────────────────────────────────────────
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("input_zoom");
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

    fn update_hover_analysis(&mut self, world: Point2, pixel_tolerance: f64) {
        use crate::snap::{snap_point, SnapOverrides};
        use grafito_core::analyzable::evaluate_curve_at;

        // 1) Snap jerárquico: característica > curva > objeto > eje > cuadrícula.
        let snap = snap_point(
            world,
            &self.document,
            self.document.view().scale,
            &self.snap_config,
            SnapOverrides::default(),
        );
        match snap.kind {
            crate::snap::SnapKind::Free => {
                // Sin snap: medir la distancia a la curva del primer objeto bajo
                // el cursor, si está cerca.
                let mut handled = false;
                for (_, obj) in self.document.objects_iter() {
                    if !obj.is_visible() {
                        continue;
                    }
                    let vars = self.document.variables.clone();
                    if let Some(y_curve) = evaluate_curve_at(obj, world, &vars) {
                        let y_match = match obj {
                            GeoObject::Function(_) => (y_curve - world.y).abs() <= pixel_tolerance,
                            _ => y_curve.abs() <= pixel_tolerance,
                        };
                        if y_match {
                            self.hovered_analysis = Some(crate::app::HoveredAnalysis {
                                point: world,
                                label: format!("({:.2}, {:.2})", world.x, world.y),
                                is_snap: false,
                                feature: None,
                                snap_kind: Some(snap.kind),
                            });
                            handled = true;
                            break;
                        }
                    }
                }
                if !handled {
                    self.hovered_analysis = Some(crate::app::HoveredAnalysis {
                        point: world,
                        label: format!("({:.2}, {:.2})", world.x, world.y),
                        is_snap: false,
                        feature: None,
                        snap_kind: Some(snap.kind),
                    });
                }
            }
            _ => {
                let is_snap = matches!(
                    snap.kind,
                    crate::snap::SnapKind::Feature | crate::snap::SnapKind::Axis
                );
                self.hovered_analysis = Some(crate::app::HoveredAnalysis {
                    point: snap.point,
                    label: snap.label.clone(),
                    is_snap,
                    feature: snap.feature,
                    snap_kind: Some(snap.kind),
                });
            }
        }

        // 2) Objetos geométricos simples: medidas al hover (longitud de
        // segmento, radio, conteo de vértices) — solo si el snap no produjo
        // un resultado más específico.
        if matches!(
            self.hovered_analysis,
            None | Some(crate::app::HoveredAnalysis {
                snap_kind: None | Some(crate::snap::SnapKind::Free),
                ..
            })
        ) {
            for (_, obj) in self.document.objects_iter() {
                let label = match obj {
                    GeoObject::Point(p) if p.position.distance(&world) <= pixel_tolerance => {
                        Some(format!("Punto: ({:.2}, {:.2})", p.position.x, p.position.y))
                    }
                    GeoObject::Line(l) => {
                        let d = point_to_line_distance(world, l.start, l.end);
                        if d <= pixel_tolerance {
                            let len = l.start.distance(&l.end);
                            Some(format!("Longitud: {:.2}", len))
                        } else {
                            None
                        }
                    }
                    GeoObject::Circle(c) => {
                        let d = world.distance(&c.center);
                        if (d - c.radius).abs() <= pixel_tolerance {
                            Some(format!("Radio: {:.2}", c.radius))
                        } else {
                            None
                        }
                    }
                    GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                        if point_inside_polygon(world, &poly.vertices) {
                            Some(format!("Vértices: {}", poly.vertices.len()))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(text) = label {
                    self.hovered_analysis = Some(crate::app::HoveredAnalysis {
                        point: world,
                        label: text,
                        is_snap: false,
                        feature: None,
                        snap_kind: Some(crate::snap::SnapKind::Object),
                    });
                    return;
                }
            }
        }
        let _ = pixel_tolerance;
    }
}

#[allow(dead_code)]
fn find_nearest_feature(
    results: &[AnalysisResult],
    world: Point2,
    tolerance: f64,
) -> Option<AnalysisResult> {
    let mut best: Option<AnalysisResult> = None;
    let mut best_dist = tolerance;
    for r in results {
        let dist = r.point.distance(&world);
        if dist < best_dist {
            best_dist = dist;
            best = Some(r.clone());
        }
    }
    best
}

fn point_to_line_distance(p: Point2, a: Point2, b: Point2) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let len2 = abx * abx + aby * aby;
    if len2 == 0.0 {
        return (apx * apx + apy * apy).sqrt();
    }
    let t = ((apx * abx + apy * aby) / len2).clamp(0.0, 1.0);
    let closest_x = a.x + t * abx;
    let closest_y = a.y + t * aby;
    let dx = p.x - closest_x;
    let dy = p.y - closest_y;
    (dx * dx + dy * dy).sqrt()
}

fn point_inside_polygon(p: Point2, vertices: &[Point2]) -> bool {
    let mut inside = false;
    let mut j = vertices.len() - 1;
    for i in 0..vertices.len() {
        let pi = vertices[i];
        let pj = vertices[j];
        if ((pi.y > p.y) != (pj.y > p.y))
            && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl GrafitoApp {
    pub(crate) fn handle_canvas_3d_input(&mut self, ui: &mut egui::Ui, canvas_rect: egui::Rect) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("input_canvas_3d");

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
            #[cfg(feature = "profile")]
            puffin::profile_scope!("input_orbit");
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let delta = response.drag_delta();
            self.camera.orbit(delta.x * 0.005, delta.y * 0.005);
        }
        // Pan with Space + primary, middle button, or primary drag in any tool
        else if (space_pressed && response.dragged_by(egui::PointerButton::Primary))
            || (pointer_in_canvas && pointer.button_down(egui::PointerButton::Middle))
            || response.dragged_by(egui::PointerButton::Primary)
        {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("input_pan");
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
                #[cfg(feature = "profile")]
                puffin::profile_scope!("input_zoom");
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
