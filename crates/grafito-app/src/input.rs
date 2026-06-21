//! Mouse, keyboard, and gesture input handling for the central canvas.
//!
//! Covers 2D/3D drag, pan, zoom, selection, tool clicks, and the transient
//! tool-ghost preview that follows the pointer.

use crate::{GrafitoApp, PendingAction};
use egui::{PointerButton, Rect, Sense, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_core::{
    CircleObj, FunctionObj, GeoObject, ImplicitCurveObj, LineObj, ParametricCurve2DObj, PencilObj,
    Point3DObj, PointObj, PolarCurveObj, PolygonObj, RelationOperator, RenderQuality,
    VectorField2DObj,
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
            Tool::Pencil => {
                // El Pencil se construye en `response.drag_stopped`, no con un
                // clic simple. Aquí no hacemos nada.
            }
            Tool::Eraser => {
                // Clic simple: borrar el objeto bajo el cursor (si hay).
                let tolerance = 10.0 / self.document.view().scale;
                if let Some(id) = self.document.pick_object(world, tolerance) {
                    self.save_state();
                    self.document.remove_object(id);
                    if self.selected_object == Some(id) {
                        self.selected_object = None;
                    }
                }
            }
            Tool::Point => {
                self.save_state();
                self.add_object_logged(GeoObject::Point(PointObj::new(world)), "Point");
                self.tool_ghost = None;
            }
            Tool::Line => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let a = self.tool_state.pending[0];
                    let b = self.tool_state.pending[1];
                    self.save_state();
                    self.add_object_logged(GeoObject::Line(LineObj::new(a, b)), "Line");
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
                    self.add_object_logged(
                        GeoObject::Circle(CircleObj::new(center, radius)),
                        "Circle",
                    );
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Polygon => {
                self.tool_state.pending.push(world);
            }
            Tool::Function => {
                self.execute_command_and_record("y = x^2", time);
                self.current_tool = Tool::Select;
            }
            Tool::Point3D => {
                let p3 = Point3D::new(world.x, world.y, 0.0);
                self.save_state();
                self.add_object_logged(
                    GeoObject::Point3D(grafito_core::Point3DObj::new(p3)),
                    "Point3D",
                );
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
                    self.add_object_logged(
                        GeoObject::Sphere3D(grafito_core::Sphere3DObj::new(center, radius)),
                        "Sphere3D",
                    );
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
                    self.add_object_logged(
                        GeoObject::Cube3D(grafito_core::Cube3DObj::new(p1, size)),
                        "Cube3D",
                    );
                    self.pending_points_3d.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Attractor => {
                self.execute_command_and_record("Lorenz[]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Fractal => {
                self.execute_command_and_record("Mandelbrot[]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Histogram => {
                self.execute_command_and_record("Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ScatterPlot => {
                self.execute_command_and_record("ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::Tangent => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let p1 = self.tool_state.pending[0];
                    let p2 = self.tool_state.pending[1];
                    let cmd = format!(
                        "Tangent[({:.2}, {:.2}), 1, ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    self.execute_command_and_record(&cmd, time);
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
                    let cmd = format!(
                        "PerpendicularBisector[({:.2}, {:.2}), ({:.2}, {:.2})]",
                        p1.x, p1.y, p2.x, p2.y
                    );
                    self.execute_command_and_record(&cmd, time);
                    self.tool_state.pending.clear();
                    self.tool_ghost = None;
                    self.current_tool = Tool::Select;
                }
            }
            Tool::DomainColoring => {
                self.execute_command_and_record("DomainColoring[z^2 + 1, -3, 3, -3, 3, 200]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::HeatMap => {
                self.execute_command_and_record("HeatMap[x^2 + y^2, -5, 5, -5, 5, 150]", time);
                self.selected_object = None;
                self.current_tool = Tool::Select;
            }
            Tool::ComplexGrid => {
                self.execute_command_and_record("ComplexGrid[sin(z), -3, 3, -3, 3]", time);
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
                let tool_name = self.current_tool.name();
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
                    self.handle_command_outcome(outcome, time, tool_name);
                }
                for obj in result.objects {
                    self.add_object_logged(obj, tool_name);
                }
            }
            Tool::Coincident
            | Tool::DistanceConstraint
            | Tool::AngleConstraint
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
            _ => {}
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
            Tool::Eraser => {
                // El borrador muestra un anillo de tamaño variable según la
                // tolerancia de selección; no dibuja objetos.
                self.tool_ghost = None;
            }
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

            // Inicio de Pencil: crear el PencilObj directamente en el
            // documento con el primer punto. De este modo el usuario ve
            // el trazo en tiempo real sin "ghost": cada tick del drag
            // añade un punto al PencilObj existente. Sin Space (que panea).
            // Solo creamos si no hay ya un PencilObj en curso (caso
            // touch/stylus que ya creó uno con `button_down`).
            if self.current_tool == Tool::Pencil
                && response.drag_started_by(PointerButton::Primary)
                && !space_pressed
                && self.tool_state.drawing_pencil.is_none()
            {
                self.save_state();
                if let Some(pos) = current_pos {
                    let local = pos - canvas_rect.min;
                    let world = self
                        .document
                        .view()
                        .screen_to_world(GlamVec2::new(local.x, local.y));
                    let mut pencil = PencilObj::new(vec![world]);
                    pencil.color = self.color_favorites[0];
                    pencil.width = 2.0;
                    let id = self.add_object_logged(GeoObject::Pencil(pencil), "Pencil");
                    self.tool_state.drawing_pencil = Some(id);
                }
            }
        }

        // ── Compatibilidad con tabletas gráficas (stylus) ────────────────
        // Las tabletas y pantallas táctiles emiten presión desde el primer
        // frame, sin movimiento significativo, por lo que egui no marca
        // `drag_started`/`dragged_by` con la suficiente rapidez. Para que
        // el Pencil funcione con stylus, detectamos el botón presionado
        // directamente con `pointer.button_down(...)` y creamos el
        // PencilObj en el frame actual. Space anula el comportamiento
        // (pan universal). Solo Primary, Secondary y Middle disparan el
        // Pencil para que la goma lateral del stylus (Secondary) también
        // dibuje.
        if !space_pressed
            && pointer_in_canvas
            && (pointer.button_down(PointerButton::Primary)
                || pointer.button_down(PointerButton::Secondary)
                || pointer.button_down(PointerButton::Middle))
            && self.tool_state.drawing_pencil.is_none()
            && self.current_tool == Tool::Pencil
        {
            self.save_state();
            if let Some(pos) = current_pos {
                let local = pos - canvas_rect.min;
                let world = self
                    .document
                    .view()
                    .screen_to_world(GlamVec2::new(local.x, local.y));
                let mut pencil = PencilObj::new(vec![world]);
                pencil.color = self.color_favorites[0];
                pencil.width = 2.0;
                let id = self.add_object_logged(GeoObject::Pencil(pencil), "Pencil");
                self.tool_state.drawing_pencil = Some(id);
                self.is_view_changing = true;
            }
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
        // Si la herramienta es Pencil o Eraser, **bloqueamos el pan con
        // Middle/Secondary** porque el botón lateral del stylus suele
        // emitir Secondary y queremos que sirva para borrar/dibujar, no
        // para mover la vista. Con Space+Primary sigue siendo pan.
        let drawing_tool = matches!(self.current_tool, Tool::Pencil | Tool::Eraser);
        let can_pan_with_right =
            self.current_tool != Tool::Polygon || self.tool_state.pending.is_empty();
        let pan_button_pressed = !drawing_tool
            && pointer_in_canvas
            && (pointer.button_down(PointerButton::Middle)
                || (pointer.button_down(PointerButton::Secondary) && can_pan_with_right));

        // 1. Space + primary drag: universal pan
        if space_pressed && response.dragged_by(PointerButton::Primary) {
            panning = true;
            pan_delta = response.drag_delta();
        }
        // 2. Middle/right button drag: universal pan (direct pointer reading).
        //    Bloqueado durante Pencil/Eraser (ver `pan_button_pressed`).
        else if pan_button_pressed {
            let delta = pointer.delta();
            if delta != Vec2::ZERO {
                panning = true;
                pan_delta = delta;
            }
        }
        // 3. Primary drag: pan unless we are moving a free point in Select mode
        //    o dibujando con Pencil/Eraser (donde el arrastre primario es
        //    para acumular puntos del trazo o borrar, no para mover la
        //    vista).
        else if response.dragged_by(PointerButton::Primary) {
            let moving_point = self.current_tool == Tool::Select
                && self
                    .selected_object
                    .map(|id| self.document.is_free_object(&id))
                    .unwrap_or(false);
            let drawing = drawing_tool;
            if !moving_point && !drawing {
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

        // ── Pencil: añadir puntos al PencilObj vivo durante el drag ───────
        // Pencil no usa pan durante el arrastre. Modificamos el PencilObj
        // directamente en el documento para que se vea en tiempo real.
        // Aceptamos Primary, Secondary y Middle como botones de dibujo
        // para máxima compatibilidad con stylus (botón lateral del
        // lápiz óptico).
        if !panning
            && self.current_tool == Tool::Pencil
            && (pointer.button_down(PointerButton::Primary)
                || pointer.button_down(PointerButton::Secondary)
                || pointer.button_down(PointerButton::Middle))
        {
            if let (Some(pencil_id), Some(pos)) = (self.tool_state.drawing_pencil, current_pos) {
                let local = pos - canvas_rect.min;
                let world = self
                    .document
                    .view()
                    .screen_to_world(GlamVec2::new(local.x, local.y));
                // Throttling: solo añadimos un punto si está al menos a
                // `min_step` unidades del último (en coords del mundo).
                let min_step = 0.01 / self.document.view().scale.max(1e-3);
                if let Some(GeoObject::Pencil(p)) = self.document.get_object_mut(pencil_id) {
                    let should_push = p
                        .points
                        .last()
                        .map(|last| last.distance(&world) >= min_step)
                        .unwrap_or(true);
                    if should_push {
                        p.push(world);
                    }
                }
                // Forzamos repintado para que el PencilObj actualizado se vea.
                self.is_view_changing = true;
            }
        }

        // ── Eraser: borrar el objeto bajo el cursor durante el arrastre ─────
        // Igual que Pencil, no debe paneo con arrastre primario. Borra cada
        // objeto que esté dentro de la tolerancia en cada tick del drag.
        // Aceptamos cualquier botón de dibujo (compatibilidad con stylus).
        if !panning
            && self.current_tool == Tool::Eraser
            && (pointer.button_down(PointerButton::Primary)
                || pointer.button_down(PointerButton::Secondary)
                || pointer.button_down(PointerButton::Middle))
        {
            if let Some(pos) = current_pos {
                let local = pos - canvas_rect.min;
                let world = self
                    .document
                    .view()
                    .screen_to_world(GlamVec2::new(local.x, local.y));
                let tolerance = 10.0 / self.document.view().scale;
                if let Some(id) = self.document.pick_object(world, tolerance) {
                    if self.tool_state.last_erased != Some(id) {
                        self.save_state();
                        self.document.remove_object(id);
                        if self.selected_object == Some(id) {
                            self.selected_object = None;
                        }
                        self.tool_state.last_erased = Some(id);
                    }
                }
                self.is_view_changing = true;
            }
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
            && !response.dragged()
            && self.current_view != crate::ViewMode::D3
        {
            if let Some(world) = world_at_pointer {
                let pixel_tolerance = 15.0 / self.document.view().scale;

                // A very short debounce (30ms) ensures 30fps for analysis while the app runs at 60fps+.
                // Or we can just run it if the mouse stopped moving.
                // Let's implement a spatial + temporal debounce
                let dist_moved = if let Some(last) = self.hover_candidate_pos {
                    world.distance(&last) * self.document.view().scale // pixels
                } else {
                    100.0
                };

                if dist_moved > 5.0 {
                    self.hover_candidate_pos = Some(world);
                    // Reset hovered_analysis so we don't show old ghosts while moving fast
                    self.hovered_analysis = None;
                } else {
                    self.update_hover_analysis(world, pixel_tolerance);
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
            if let Some(mut world) = world_at_pointer {
                // Ensure instant snap calculation on click to avoid missing snaps due to hover debounce
                use grafito_geometry::analysis::AnalysisFeature;
                let tool_filter = match self.current_tool {
                    grafito_ui::Tool::Root => {
                        Some(vec![AnalysisFeature::Root, AnalysisFeature::XIntercept])
                    }
                    grafito_ui::Tool::Extremum => Some(vec![
                        AnalysisFeature::LocalMaximum,
                        AnalysisFeature::LocalMinimum,
                    ]),
                    grafito_ui::Tool::Inflection => Some(vec![AnalysisFeature::Inflection]),
                    grafito_ui::Tool::YIntercept => Some(vec![AnalysisFeature::YIntercept]),
                    grafito_ui::Tool::XIntercept => Some(vec![AnalysisFeature::XIntercept]),
                    _ => None,
                };
                let snap = crate::snap::snap_point(
                    world,
                    &self.document,
                    self.document.view().scale,
                    &self.snap_config,
                    crate::snap::SnapOverrides::default(),
                    tool_filter,
                );

                if snap.kind != crate::snap::SnapKind::Free {
                    world = snap.point;
                } else if self.snap_to_grid {
                    world = snap_world_to_grid(world, self.document.view().scale);
                }
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
                self.add_object_logged(GeoObject::Polygon(PolygonObj::new(vertices)), "Polygon");
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
        if let Some(mut world) = world_at_pointer {
            if let Some(hover) = &self.hovered_analysis {
                if hover.is_snap {
                    world = hover.point;
                } else if self.snap_to_grid {
                    world = snap_world_to_grid(world, self.document.view().scale);
                }
            } else if self.snap_to_grid {
                world = snap_world_to_grid(world, self.document.view().scale);
            }
            self.update_tool_ghost(world);
        }

        // ── Cleanup drag state ───────────────────────────────────────────────
        if response.drag_stopped() {
            // Finalizar Pencil: el PencilObj ya está en el documento y
            // actualizado en cada tick del drag. Si solo tiene 1 punto
            // (clic simple sin arrastrar), lo eliminamos — no es un trazo
            // válido. El undo ya se guardó al inicio del drag con
            // `save_state`, así que un solo Ctrl+Z deshará el trazo entero.
            if self.current_tool == Tool::Pencil {
                if let Some(id) = self.tool_state.drawing_pencil.take() {
                    let too_short = self
                        .document
                        .get_object(id)
                        .map(|obj| {
                            if let GeoObject::Pencil(p) = obj {
                                p.points.len() < 2
                            } else {
                                true
                            }
                        })
                        .unwrap_or(true);
                    if too_short {
                        self.document.remove_object(id);
                    }
                }
            }
            self.canvas_is_panning = false;
            self.canvas_drag_start = None;
        }

        // ── Finalizar Pencil/Eraser cuando se suelta el botón (caso touch
        // y stylus): si ninguno de los botones de dibujo está presionado,
        // terminamos el trazo del mismo modo que `drag_stopped`. Esto
        // cubre el caso en que el driver de la tableta emite `button_down`
        // durante varios frames sin notificar `drag_stopped`.
        let any_draw_button = pointer.button_down(PointerButton::Primary)
            || pointer.button_down(PointerButton::Secondary)
            || pointer.button_down(PointerButton::Middle);
        if !any_draw_button {
            if let Some(id) = self.tool_state.drawing_pencil.take() {
                let too_short = self
                    .document
                    .get_object(id)
                    .map(|obj| {
                        if let GeoObject::Pencil(p) = obj {
                            p.points.len() < 2
                        } else {
                            true
                        }
                    })
                    .unwrap_or(true);
                if too_short {
                    self.document.remove_object(id);
                }
            }
            // Eraser: al soltar el botón, limpiamos `last_erased` para
            // permitir borrar el mismo objeto en un trazo posterior.
            self.tool_state.last_erased = None;
        }

        // Keep last known position for external consumers (status bar, etc.)
        if let Some(pos) = current_pos {
            self.last_mouse_pos = Some(pos);
        }
    }

    fn update_hover_analysis(&mut self, world: Point2, pixel_tolerance: f64) {
        use crate::snap::{snap_point, SnapOverrides};
        use grafito_core::analyzable::evaluate_curve_at;

        use grafito_geometry::analysis::AnalysisFeature;
        let tool_filter = match self.current_tool {
            grafito_ui::Tool::Root => {
                Some(vec![AnalysisFeature::Root, AnalysisFeature::XIntercept])
            }
            grafito_ui::Tool::Extremum => Some(vec![
                AnalysisFeature::LocalMaximum,
                AnalysisFeature::LocalMinimum,
            ]),
            grafito_ui::Tool::Inflection => Some(vec![AnalysisFeature::Inflection]),
            grafito_ui::Tool::YIntercept => Some(vec![AnalysisFeature::YIntercept]),
            grafito_ui::Tool::XIntercept => Some(vec![AnalysisFeature::XIntercept]),
            _ => None,
        };

        // 1) Snap jerárquico: característica > curva > objeto > eje > cuadrícula.
        let snap = snap_point(
            world,
            &self.document,
            self.document.view().scale,
            &self.snap_config,
            SnapOverrides::default(),
            tool_filter,
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
