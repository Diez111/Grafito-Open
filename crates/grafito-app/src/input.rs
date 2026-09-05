//! Mouse, keyboard, and gesture input handling for the central canvas.
//!
//! Covers 2D/3D drag, pan, zoom, selection, tool clicks, and the transient
//! tool-ghost preview that follows the pointer.

use crate::{tool_dispatcher::ToolState, GrafitoApp, PendingAction};
use egui::{PointerButton, Rect, Sense, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_core::{
    CircleObj, Document, FunctionObj, GeoObject, ImplicitCurveObj, LineObj, ParametricCurve2DObj,
    PencilObj, Point3DObj, PointObj, PolarCurveObj, PolygonObj, RelationOperator, RenderQuality,
    VectorField2DObj,
};
use grafito_geometry::{Point2, Point3D};
use grafito_ui::tokens::{SPACE_SM, SPACE_XS, SPACE_XXL, TYPE_XS};
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

/// Los objetos que se construyen sobre el plano de construcción muestran una
/// previsualización en la posición del puntero. Los politopos proyectados se
/// crean centrados, por lo que deliberadamente no usan este fantasma.
pub(crate) const fn uses_3d_position_ghost(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Point3D
            | Tool::Segment3D
            | Tool::Line3D
            | Tool::Plane3D
            | Tool::Sphere3D
            | Tool::Cube3D
            | Tool::Cylinder3D
            | Tool::Cone3D
            | Tool::Torus3D
            | Tool::MoebiusStrip
            | Tool::Surface3D
            | Tool::ParametricCurve3D
            | Tool::VectorField3D
            | Tool::HyperSurface4D
    )
}

pub(crate) fn cancel_locus_selection(state: &mut ToolState) {
    state.driver = None;
}

pub(crate) fn canvas_local_pointer(canvas_rect: Rect, pointer: egui::Pos2) -> Option<Vec2> {
    let size = canvas_rect.size();
    if !pointer.x.is_finite()
        || !pointer.y.is_finite()
        || !size.is_finite()
        || size.x <= 0.0
        || size.y <= 0.0
        || !canvas_rect.contains(pointer)
    {
        return None;
    }
    let local = pointer - canvas_rect.min;
    local.is_finite().then_some(local)
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
                    let mut stroke_has_mutated = false;
                    if crate::app::erase_object_for_stroke(
                        &mut self.document,
                        id,
                        &mut stroke_has_mutated,
                        &mut self.undo_stack,
                        &mut self.redo_stack,
                    ) && self.selected_object == Some(id)
                    {
                        self.selected_object = None;
                    }
                }
            }
            Tool::Point => {
                self.insert_object_from_tool(GeoObject::Point(PointObj::new(world)), "Point", time);
                self.tool_ghost = None;
            }
            Tool::Line => {
                self.tool_state.pending.push(world);
                if self.tool_state.pending.len() == 2 {
                    let a = self.tool_state.pending[0];
                    let b = self.tool_state.pending[1];
                    self.insert_object_from_tool(GeoObject::Line(LineObj::new(a, b)), "Line", time);
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
                    self.insert_object_from_tool(
                        GeoObject::Circle(CircleObj::new(center, radius)),
                        "Circle",
                        time,
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
                self.insert_object_from_tool(
                    GeoObject::Point3D(grafito_core::Point3DObj::new(p3)),
                    "Point3D",
                    time,
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
                    self.insert_object_from_tool(
                        GeoObject::Sphere3D(grafito_core::Sphere3DObj::new(center, radius)),
                        "Sphere3D",
                        time,
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
                    self.insert_object_from_tool(
                        GeoObject::Cube3D(grafito_core::Cube3DObj::new(p1, size)),
                        "Cube3D",
                        time,
                    );
                    self.pending_points_3d.clear();
                    self.tool_ghost = None;
                }
            }
            Tool::Tesseract4D | Tool::Hypercube5D => {
                // Los politopos tipados solo se crean desde `handle_3d_click`.
                self.tool_ghost = None;
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
                    // GeoGebra: perpendicular = punto + recta existentes por etiqueta.
                    // Solo si no hay ese par, mediatriz honesta de dos puntos libres.
                    let tol = 10.0 / self.document.view().scale;
                    let classify = |obj: &GeoObject| {
                        (
                            obj.label().to_owned(),
                            matches!(obj, GeoObject::Point(_)),
                            matches!(obj, GeoObject::Line(_)),
                        )
                    };
                    let r1 = self
                        .document
                        .pick_object(p1, tol)
                        .and_then(|id| self.document.get_object(id).map(classify));
                    let r2 = self
                        .document
                        .pick_object(p2, tol)
                        .and_then(|id| self.document.get_object(id).map(classify));
                    let cmd = perpendicular_command(r1, r2, p1, p2);
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
            | Tool::Parallel
            | Tool::Arc
            | Tool::Sector
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
                let before = self.document.clone();
                let mut staged = self.document.detached_clone_for_staging();
                let result = crate::tool_dispatcher::dispatch_tool(
                    self.current_tool,
                    &mut state,
                    &mut staged,
                    world,
                );
                let outcome = state.last_outcome.take();
                let inserted = result
                    .objects
                    .into_iter()
                    .map(|object| staged.try_add_object(object))
                    .collect::<Result<Vec<_>, _>>();
                self.tool_state = state;
                if result.reset_tool {
                    self.current_tool = Tool::Select;
                }
                if let Some(msg) = result.message {
                    self.cas_result = msg;
                }
                let command_failed = matches!(
                    outcome.as_ref(),
                    Some(grafito_command::commands::CommandOutcome::Error(_))
                );
                let insertion_error = match inserted {
                    Ok(ids) => {
                        if !command_failed
                            && crate::app::documents_semantically_differ(&before, &staged)
                        {
                            staged.version = before.version.wrapping_add(1);
                            self.document = staged;
                            self.save_snapshot(before);
                            let outputs = ids
                                .iter()
                                .filter_map(|id| {
                                    self.document
                                        .get_object(*id)
                                        .map(|object| object.label().to_string())
                                })
                                .collect::<Vec<_>>();
                            for output in outputs {
                                self.record_construction_step(tool_name, Vec::new(), &output);
                            }
                        }
                        None
                    }
                    Err(error) => Some(error),
                };
                if let Some(outcome) = outcome {
                    self.handle_command_outcome(outcome, time, tool_name);
                }
                if let Some(error) = insertion_error {
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(error),
                        time,
                        tool_name,
                    );
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

    fn update_tool_ghost(&mut self, world: Point2, painter: &egui::Painter, canvas_rect: Rect) {
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
                    // n paramétrico (variable `n` > memoria sesión > 5), igual que el commit.
                    let n = crate::tool_dispatcher::resolve_polygon_sides(
                        &self.tool_state,
                        &self.document,
                    );
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
            Tool::Tesseract4D | Tool::Hypercube5D => {
                // La creación no depende de una posición 2D.
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
            Tool::Locus => {
                if let Some(driver) = self.tool_state.driver {
                    if let Some(GeoObject::Point(point)) = self.document.get_object(driver) {
                        self.tool_ghost =
                            Some(GeoObject::Line(LineObj::new(point.position, world)));
                    }
                }
            }
            Tool::Eraser => {
                // El borrador muestra un anillo de tamaño variable según la
                // tolerancia de selección; no dibuja objetos.
                self.tool_ghost = None;
            }
            _ => {}
        }
        // Overlay fantasma tangente/normal: no pisa el ghost propio de la
        // herramienta activa (se dibuja solo si `tool_ghost` sigue vacío y
        // el hover está sobre una función).
        self.update_tangent_normal_ghost(world, painter, canvas_rect);
    }

    /// Overlay fantasma de tangente + normal al hover sobre una `Function`.
    ///
    /// Algoritmo (toda la matemática en `crate::snap`, pura y testeada):
    /// - `tangent_ghost_at_hover` localiza la función bajo el cursor
    ///   (distancia vertical ≤ 12 px) y su punto base `(x, f(x))`.
    /// - `tangent_slope_central` aproxima f'(x) por diferencia central con
    ///   h = 1e-6 (igual que `intersections.rs::newton`); la curvatura κ sale
    ///   de `analysis::curvature_at` cuando no hay variables extra.
    /// - Los segmentos se acotan a ±40 px en unidades de mundo y se pintan
    ///   con el color de acento translúcido.
    ///
    /// No toca `tool_ghost`: si la herramienta activa ya tiene su propio
    /// fantasma, no se dibuja nada. Al salir del hover no hay función cercana
    /// y el overlay desaparece solo (se recalcula cada frame, sin estado).
    fn update_tangent_normal_ghost(
        &mut self,
        world: Point2,
        painter: &egui::Painter,
        canvas_rect: Rect,
    ) {
        if self.tool_ghost.is_some() {
            return;
        }
        let scale = self.document.view().scale;
        let Some(ghost) = crate::snap::tangent_ghost_at_hover(world, &self.document, scale) else {
            return;
        };
        let view = *self.document.view();
        let to_screen = |p: Point2| -> Option<egui::Pos2> {
            if !p.x.is_finite() || !p.y.is_finite() {
                return None;
            }
            let s = view.world_to_screen(p);
            if !s.is_finite() {
                return None;
            }
            Some(canvas_rect.min + egui::Vec2::new(s.x, s.y))
        };
        let (Some(ta), Some(tb), Some(na), Some(nb), Some(base)) = (
            to_screen(ghost.tangent_a),
            to_screen(ghost.tangent_b),
            to_screen(ghost.normal_a),
            to_screen(ghost.normal_b),
            to_screen(ghost.base),
        ) else {
            return;
        };
        // Se oculta al salir del hover: base fuera del lienzo → nada.
        if !canvas_rect.contains(base) {
            return;
        }
        // Acento translúcido: tangente celeste, normal naranja.
        let tangent_color = egui::Color32::from_rgba_unmultiplied(56, 189, 248, 150);
        let normal_color = egui::Color32::from_rgba_unmultiplied(251, 146, 60, 150);
        painter.line_segment([ta, tb], egui::Stroke::new(1.5, tangent_color));
        painter.line_segment([na, nb], egui::Stroke::new(1.5, normal_color));
        painter.circle_filled(base, 2.5, tangent_color);
        // Etiqueta en español con pendiente y curvatura (se reconstruye cada
        // frame desde el hover base, así que el sufijo no se acumula; al salir
        // del hover el fantasma es `None` y la etiqueta vuelve a la base).
        if let Some(hover) = self.hovered_analysis.as_mut() {
            if !hover.label.contains("tangente") {
                hover.label = match ghost.curvature {
                    Some(k) => {
                        format!(
                            "{} · tangente f'≈{:.3}, κ≈{:.3}",
                            hover.label, ghost.slope, k
                        )
                    }
                    None => format!("{} · tangente f'≈{:.3}", hover.label, ghost.slope),
                };
            }
        }
    }

    pub(crate) fn handle_canvas_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("input_canvas");

        const CLICK_THRESHOLD: f32 = 3.0;

        // Sliders sobre el lienzo: prioridad sobre pan / arrastre / selección.
        // Si el gesto cae sobre un slider, el resto del input se suprime.
        if self.handle_canvas_sliders(ui, canvas_rect) {
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos().or(i.pointer.hover_pos())) {
                self.last_mouse_pos = Some(pos);
            }
            return;
        }

        let response = ui.interact(canvas_rect, ui.id().with("canvas"), Sense::click_and_drag());

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
            self.point_drag_has_mutated = false;
            self.eraser_stroke_has_mutated = false;
            if self.current_tool == Tool::Eraser {
                self.tool_state.last_erased = None;
            }
            self.point_drag_error_reported = false;
            self.select_drag_object = None;
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            self.document.render_quality = RenderQuality::Preview;

            if self.current_tool == Tool::Select
                && response.drag_started_by(PointerButton::Primary)
                && !space_pressed
            {
                if let Some(pos) = current_pos {
                    let local = pos - canvas_rect.min;
                    let world = self
                        .document
                        .view()
                        .screen_to_world(GlamVec2::new(local.x, local.y));
                    let tolerance = 10.0 / self.document.view().scale;
                    self.select_drag_object = crate::app::captured_select_drag_object(
                        &mut self.document,
                        world,
                        tolerance,
                    );
                    if let Some(id) = self.select_drag_object {
                        self.document.clear_selection();
                        self.document.select(id);
                        self.selected_object = Some(id);
                    }
                }
            }

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
                if let Some(pos) = current_pos {
                    let local = pos - canvas_rect.min;
                    let world = self
                        .document
                        .view()
                        .screen_to_world(GlamVec2::new(local.x, local.y));
                    let mut pencil = PencilObj::new(vec![world]);
                    pencil.color = self.color_favorites[0];
                    pencil.width = 2.0;
                    let id = self.insert_object_from_tool(
                        GeoObject::Pencil(pencil),
                        "Pencil",
                        ui.ctx().input(|input| input.time),
                    );
                    self.tool_state.drawing_pencil = id;
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
            if let Some(pos) = current_pos {
                let local = pos - canvas_rect.min;
                let world = self
                    .document
                    .view()
                    .screen_to_world(GlamVec2::new(local.x, local.y));
                let mut pencil = PencilObj::new(vec![world]);
                pencil.color = self.color_favorites[0];
                pencil.width = 2.0;
                let id = self.insert_object_from_tool(
                    GeoObject::Pencil(pencil),
                    "Pencil",
                    ui.ctx().input(|input| input.time),
                );
                self.tool_state.drawing_pencil = id;
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
                    .select_drag_object
                    .map(|id| crate::app::is_free_point(&self.document, id))
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
                    if self.tool_state.last_erased != Some(id)
                        && crate::app::erase_object_for_stroke(
                            &mut self.document,
                            id,
                            &mut self.eraser_stroke_has_mutated,
                            &mut self.undo_stack,
                            &mut self.redo_stack,
                        )
                    {
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
            if let (Some(sel_id), Some(pos)) = (self.select_drag_object, current_pos) {
                if self.document.is_free_object(&sel_id) {
                    let local = pos - canvas_rect.min;
                    let mut world = self
                        .document
                        .view()
                        .screen_to_world(GlamVec2::new(local.x, local.y));
                    if self.snap_to_grid {
                        world = snap_world_to_grid(world, self.document.view().scale);
                    }
                    let before = (!self.point_drag_has_mutated
                        && crate::app::free_point_position_differs(&self.document, sel_id, world))
                    .then(|| self.document.clone());
                    let version_before = self.document.version;
                    match self.document.try_move_point_and_re_evaluate(sel_id, world) {
                        Ok(true) => {
                            if let Some(before) = before {
                                self.save_snapshot(before);
                            }
                            self.point_drag_has_mutated = true;
                            crate::app::refresh_direct_document_change(
                                &mut self.document,
                                version_before,
                            );
                        }
                        Ok(false) => {}
                        Err(error) if !self.point_drag_error_reported => {
                            self.point_drag_error_reported = true;
                            self.handle_command_outcome(
                                grafito_command::commands::CommandOutcome::Error(error),
                                ui.ctx().input(|input| input.time),
                                "Mover punto",
                            );
                        }
                        Err(_) => {}
                    }
                }
            }
        }

        // ── Cursor feedback ──────────────────────────────────────────────────
        // Flag de hover sobre slider (lo publica `handle_canvas_sliders` en
        // memoria temporal egui para no añadir campos a `GrafitoApp`).
        let slider_hovered = ui.ctx().memory(|mem| {
            mem.data
                .get_temp::<bool>(egui::Id::new(CANVAS_SLIDER_HOVER_KEY))
                .unwrap_or(false)
        });
        if panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if (space_pressed && pointer_in_canvas) || slider_hovered {
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
            if self.current_tool == Tool::Locus && self.tool_state.driver.is_some() {
                cancel_locus_selection(&mut self.tool_state);
                self.tool_ghost = None;
            } else if self.current_tool == Tool::Polygon && self.tool_state.pending.len() >= 3 {
                let vertices = self.tool_state.pending.clone();
                self.insert_object_from_tool(
                    GeoObject::Polygon(PolygonObj::new(vertices)),
                    "Polygon",
                    ui.ctx().input(|input| input.time),
                );
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
            // El overlay tangente/normal pinta con acento translúcido sobre
            // el lienzo; el pintor se recorta al canvas para no invadir paneles.
            let overlay_painter = ui.painter().with_clip_rect(canvas_rect);
            self.update_tool_ghost(world, &overlay_painter, canvas_rect);
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
            self.point_drag_has_mutated = false;
            self.select_drag_object = None;
            self.point_drag_error_reported = false;
            self.eraser_stroke_has_mutated = false;
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
            self.eraser_stroke_has_mutated = false;
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

// ═══════════════════════════════════════════════════════════════
// Sliders arrastrables sobre el lienzo (Piel pura)
// ═══════════════════════════════════════════════════════════════
//
// El Cerebro expone cada slider como `VariableMeta { position, min, max,
// step, visible, animating }` más su valor en `Document::variables`.
// Este widget dibuja pista + relleno + thumb + etiqueta `nombre = valor`
// (prefijo `▶` si anima) con el painter del overlay recortado al canvas,
// y traduce el arrastre horizontal a `X → [min, max]` vía
// `try_set_variable` (clamp + snap continuo por `step`).
// Piel pura: sin I/O ni spawn; colores de `ui.visuals()` y tamaños de
// `grafito_ui::tokens` (cero literales de tamaño/fuente).

/// Ancho de la pista del slider (4 × SPACE_XXL = 160 px, sin literales).
const CANVAS_SLIDER_TRACK_WIDTH: f32 = SPACE_XXL + SPACE_XXL + SPACE_XXL + SPACE_XXL;
/// Clave temporal (memoria egui) con el flag de hover para el cursor Grab.
const CANVAS_SLIDER_HOVER_KEY: &str = "canvas_slider_hover";
/// Clave temporal (memoria egui) con el nombre del slider en arrastre.
const CANVAS_SLIDER_ACTIVE_KEY: &str = "canvas_slider_active";
/// Prefijo de clave temporal (memoria egui) para el snapshot de undo.
const CANVAS_SLIDER_UNDO_KEY: &str = "canvas_slider_undo";
/// Guarda de dominio para `sanitize_animation_speed` (no es tamaño UI).
const CANVAS_SLIDER_MAX_ANIMATION_SPEED: f64 = 100.0;
/// Velocidad de repuesto cuando la persistida no es finita.
const CANVAS_SLIDER_DEFAULT_ANIMATION_SPEED: f64 = 1.0;

/// Nombres internos que nunca se dibujan como slider (espejo de la regla
/// privada de `algebra.rs`: el lienzo no duplica variables trigonométricas
/// sintéticas; las de spreadsheet se filtran vía `is_spreadsheet_owned_variable`).
fn canvas_slider_is_internal_name(name: &str) -> bool {
    name == "TrigGraph" || name == "TrigValue" || name == "trig_angle" || name.starts_with("trig_")
}

/// Sanea una velocidad de animación persistida: no-finitos → 1.0 (defecto
/// del Cerebro), resto con clamp a ±100 para evitar recorridos desbocados
/// por dato corrupto. Pura, sin I/O.
pub(crate) fn sanitize_animation_speed(raw: f64) -> f64 {
    if !raw.is_finite() {
        return CANVAS_SLIDER_DEFAULT_ANIMATION_SPEED;
    }
    raw.clamp(
        -CANVAS_SLIDER_MAX_ANIMATION_SPEED,
        CANVAS_SLIDER_MAX_ANIMATION_SPEED,
    )
}

/// Traduce X del puntero a valor `[min, max]` con clamp + snap continuo por
/// `step` (redondeo al múltiplo más cercano; `step` no-finito o ≤ 0 =
/// continuo sin snap). Rango inválido → `min` (o 0.0 si ni `min` es finito).
/// Pura, sin I/O: la mutación real la hace `try_set_variable`.
pub(crate) fn canvas_slider_apply_drag(
    pointer_x: f32,
    track_min_x: f32,
    track_max_x: f32,
    min: f64,
    max: f64,
    step: f64,
) -> f64 {
    if !min.is_finite() {
        return 0.0;
    }
    if !max.is_finite() || min >= max {
        return min;
    }
    let span_px = f64::from(track_max_x - track_min_x);
    if !span_px.is_finite() || span_px <= 0.0 {
        return min;
    }
    let ratio = f64::from(pointer_x - track_min_x) / span_px;
    let mut value = min + ratio.clamp(0.0, 1.0) * (max - min);
    if step.is_finite() && step > 0.0 {
        value = min + ((value - min) / step).round() * step;
    }
    if !value.is_finite() {
        return min;
    }
    value.clamp(min, max)
}

/// Fracción 0..=1 del valor dentro de `[min, max]` para colocar el thumb.
/// Rango o valor inválido → 0.0. Pura, sin I/O.
pub(crate) fn canvas_slider_value_to_t(value: f64, min: f64, max: f64) -> f32 {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || min >= max {
        return 0.0;
    }
    let ratio = (value.clamp(min, max) - min) / (max - min);
    ratio.clamp(0.0, 1.0) as f32
}

/// Hit-test del widget: pista expandida por `tolerance` o cercanía al thumb
/// (`thumb_radius + tolerance`). El llamador pasa `SPACE_SM` (= 8 px).
/// Pura, sin I/O.
pub(crate) fn canvas_slider_hit(
    pointer: egui::Pos2,
    track: Rect,
    thumb_center: egui::Pos2,
    thumb_radius: f32,
    tolerance: f32,
) -> bool {
    if track.expand(tolerance).contains(pointer) {
        return true;
    }
    pointer.distance(thumb_center) <= thumb_radius + tolerance
}

/// Texto del valor con el formato del panel Álgebra (entero sin decimales).
fn canvas_slider_value_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

/// Geometría en pantalla de un slider ya filtrado y con culling superado.
struct CanvasSliderGeom {
    name: String,
    track: Rect,
    thumb: egui::Pos2,
    min: f64,
    max: f64,
    step: f64,
}

impl GrafitoApp {
    /// Dibuja los sliders visibles sobre el lienzo y gestiona su gesto.
    ///
    /// Llamar al inicio de `handle_canvas_input`, antes de pan / arrastre /
    /// selección. Retorna `true` cuando el puntero opera sobre un slider y el
    /// resto del input (pan, creación, mover-punto, clics) debe suprimirse.
    ///
    /// - Dibuja pista + relleno + thumb + etiqueta `nombre = valor` (prefijo
    ///   `▶` si anima) con painter recortado al canvas, colores de
    ///   `ui.visuals()` y tokens `SPACE_*` / `TYPE_*`.
    /// - Hit-test con pista expandida + thumb (`SPACE_SM` = 8 px de tolerancia).
    /// - Drag horizontal `X → [min, max]` vía `try_set_variable`.
    /// - Doble-clic alterna `animating` vía
    ///   `try_replace_variable_meta_with_previous`.
    /// - `visible = false` no se dibuja; nombres internos trig y variables
    ///   spreadsheet se filtran; culling fuera de pantalla; orden por nombre.
    /// - Undo: una entrada por gesto (snapshot pre-mutación en el primer
    ///   frame con movimiento, en memoria temporal egui, sin campos nuevos).
    pub(crate) fn handle_canvas_sliders(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) -> bool {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("input_canvas_sliders");

        // Snapshot determinista (orden por nombre) de sliders dibujables.
        let mut entries: Vec<(String, f64, grafito_core::VariableMeta)> = self
            .document
            .variables()
            .iter()
            .filter_map(|(name, value)| {
                if canvas_slider_is_internal_name(name)
                    || self.document.is_spreadsheet_owned_variable(name)
                {
                    return None;
                }
                let meta = self.document.variable_meta(name)?.clone();
                if !meta.visible
                    || !meta.min.is_finite()
                    || !meta.max.is_finite()
                    || meta.min >= meta.max
                    || !value.is_finite()
                {
                    return None;
                }
                Some((name.clone(), *value, meta))
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Colores vivos del tema (copias; no retienen préstamo de `ui`).
        let track_color = ui.visuals().widgets.inactive.bg_fill;
        let fill_color = ui.visuals().selection.bg_fill;
        let thumb_fill = ui.visuals().selection.bg_fill;
        let label_color = ui.visuals().text_color();
        // El overlay se recorta al canvas para no invadir paneles.
        let painter = ui.painter().with_clip_rect(canvas_rect);
        let view = *self.document.view();

        let mut geoms: Vec<CanvasSliderGeom> = Vec::new();
        for (name, value, meta) in &entries {
            let screen = view.world_to_screen(meta.position);
            if !screen.x.is_finite() || !screen.y.is_finite() {
                continue;
            }
            let anchor = canvas_rect.min + egui::Vec2::new(screen.x, screen.y);
            let track_min = egui::pos2(anchor.x, anchor.y + TYPE_XS + SPACE_XS);
            let track =
                Rect::from_min_size(track_min, egui::vec2(CANVAS_SLIDER_TRACK_WIDTH, SPACE_XS));
            // Culling: etiqueta + pista expandidos por el thumb.
            if !Rect::from_min_max(anchor, track.max)
                .expand(SPACE_SM)
                .intersects(canvas_rect)
            {
                continue;
            }
            let ratio = canvas_slider_value_to_t(*value, meta.min, meta.max);
            let thumb = egui::pos2(
                track.min.x + ratio * CANVAS_SLIDER_TRACK_WIDTH,
                track.center().y,
            );
            let mut label = format!("{name} = {}", canvas_slider_value_text(*value));
            if meta.animating {
                label = format!("▶ {label}");
            }
            painter.rect_filled(track, SPACE_XS, track_color);
            let fill = Rect::from_min_max(track.min, egui::pos2(thumb.x, track.max.y));
            if fill.width() > 0.0 {
                painter.rect_filled(fill, SPACE_XS, fill_color);
            }
            painter.circle_filled(thumb, SPACE_SM, thumb_fill);
            painter.circle_stroke(thumb, SPACE_SM, ui.visuals().widgets.active.fg_stroke);
            painter.text(
                anchor,
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(TYPE_XS),
                label_color,
            );
            geoms.push(CanvasSliderGeom {
                name: name.clone(),
                track,
                thumb,
                min: meta.min,
                max: meta.max,
                step: meta.step,
            });
        }

        let pointer_pos = ui.input(|i| i.pointer.latest_pos().or(i.pointer.hover_pos()));
        let primary_down = ui.input(|i| i.pointer.button_down(PointerButton::Primary));
        let primary_pressed = ui.input(|i| i.pointer.button_pressed(PointerButton::Primary));
        let double_clicked = ui.input(|i| i.pointer.button_double_clicked(PointerButton::Primary));

        let hovered: Option<String> = pointer_pos.and_then(|pos| {
            geoms
                .iter()
                .find(|geom| canvas_slider_hit(pos, geom.track, geom.thumb, SPACE_SM, SPACE_SM))
                .map(|geom| geom.name.clone())
        });
        ui.ctx().memory_mut(|mem| {
            mem.data
                .insert_temp(egui::Id::new(CANVAS_SLIDER_HOVER_KEY), hovered.is_some());
        });
        if hovered.is_some() && !primary_down {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let active_id = egui::Id::new(CANVAS_SLIDER_ACTIVE_KEY);
        let active: Option<String> = ui.ctx().memory(|mem| mem.data.get_temp(active_id));

        // Doble-clic sobre el widget: alterna `animating` (cierra antes un
        // drag en curso y reclama el gesto para que no cree objetos).
        if double_clicked {
            if let Some(pos) = pointer_pos {
                if let Some(hit) = geoms
                    .iter()
                    .find(|geom| canvas_slider_hit(pos, geom.track, geom.thumb, SPACE_SM, SPACE_SM))
                {
                    self.finish_canvas_slider_gesture(ui, &hit.name);
                    if let Some(meta) = self.document.variable_meta(&hit.name).cloned() {
                        let mut candidate = meta;
                        candidate.animating = !candidate.animating;
                        candidate.animation_speed =
                            sanitize_animation_speed(candidate.animation_speed);
                        match self
                            .document
                            .try_replace_variable_meta_with_previous(&hit.name, candidate)
                        {
                            Ok(Some(previous)) => {
                                self.save_snapshot(previous);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                let time = ui.ctx().input(|input| input.time);
                                self.handle_command_outcome(
                                    grafito_command::commands::CommandOutcome::Error(error),
                                    time,
                                    "Slider",
                                );
                            }
                        }
                    }
                    // Reclama también el release: evita un clic fantasma.
                    ui.ctx()
                        .memory_mut(|mem| mem.data.insert_temp(active_id, hit.name.clone()));
                    return true;
                }
            }
        }

        // Gesto en curso: arrastre (con mutación) o release (confirma undo).
        if let Some(active_name) = active {
            if primary_down {
                if let Some(geom) = geoms.iter().find(|geom| geom.name == active_name) {
                    if let Some(pos) = pointer_pos {
                        let next = canvas_slider_apply_drag(
                            pos.x,
                            geom.track.min.x,
                            geom.track.max.x,
                            geom.min,
                            geom.max,
                            geom.step,
                        );
                        let current = self.document.get_variable(&active_name).unwrap_or(next);
                        // Comparación exacta por bits: ambos valores son
                        // finitos y el snap es determinista; evita lint float.
                        if next.to_bits() != current.to_bits() {
                            // Snapshot pre-mutación solo en el primer frame
                            // con movimiento: una entrada de undo por gesto.
                            let undo_id =
                                egui::Id::new((CANVAS_SLIDER_UNDO_KEY, active_name.clone()));
                            let has_snapshot: bool = ui
                                .ctx()
                                .memory(|mem| mem.data.get_temp::<Document>(undo_id).is_some());
                            if !has_snapshot {
                                let snapshot = self.document.clone();
                                ui.ctx()
                                    .memory_mut(|mem| mem.data.insert_temp(undo_id, snapshot));
                            }
                            let version_before = self.document.version;
                            if self
                                .document
                                .try_set_variable(active_name.clone(), next)
                                .is_ok()
                            {
                                crate::app::refresh_direct_document_change(
                                    &mut self.document,
                                    version_before,
                                );
                                self.is_view_changing = true;
                                self.last_interaction_time = Instant::now();
                            }
                        }
                    }
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                return true;
            }
            // Release: confirma el undo si hubo movimiento y consume el frame
            // para que el canvas no lo lea como clic de creación.
            self.finish_canvas_slider_gesture(ui, &active_name);
            return true;
        }

        // Inicio de gesto: el press debe nacer sobre el widget (no se roban
        // arrastres ajenos) y dentro del lienzo.
        if primary_pressed {
            if let Some(pos) = pointer_pos {
                if canvas_rect.contains(pos) {
                    if let Some(name) = hovered {
                        ui.ctx()
                            .memory_mut(|mem| mem.data.insert_temp(active_id, name));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Confirma el gesto de un slider: si el snapshot temporal muestra un
    /// valor distinto al actual (hubo movimiento), guarda una única entrada
    /// de undo; siempre limpia el estado temporal egui.
    fn finish_canvas_slider_gesture(&mut self, ui: &mut egui::Ui, name: &str) {
        let undo_id = egui::Id::new((CANVAS_SLIDER_UNDO_KEY, name.to_owned()));
        let snapshot: Option<Document> = ui.ctx().memory_mut(|mem| mem.data.remove_temp(undo_id));
        ui.ctx().memory_mut(|mem| {
            mem.data
                .remove_temp::<String>(egui::Id::new(CANVAS_SLIDER_ACTIVE_KEY))
        });
        if let Some(before) = snapshot {
            if before.get_variable(name) != self.document.get_variable(name) {
                self.save_snapshot(before);
            }
        }
    }
}

/// Comando para la herramienta Perpendicular dados dos picks del lienzo.
///
/// `r` = `Some((etiqueta, es_punto, es_recta))` si el clic cayó sobre un
/// objeto existente. Con par (punto, recta) en cualquier orden emite
/// `Perpendicular[punto, recta]` (GeoGebra); en otro caso, mediatriz honesta
/// de dos puntos libres (única perpendicular definible sin objetos).
pub(crate) fn perpendicular_command(
    r1: Option<(String, bool, bool)>,
    r2: Option<(String, bool, bool)>,
    p1: Point2,
    p2: Point2,
) -> String {
    let mut point_label: Option<String> = None;
    let mut line_label: Option<String> = None;
    for r in [r1, r2].into_iter().flatten() {
        if r.1 && point_label.is_none() {
            point_label = Some(r.0.clone());
        }
        if r.2 && line_label.is_none() {
            line_label = Some(r.0);
        }
    }
    match (point_label, line_label) {
        (Some(p), Some(l)) => format!("Perpendicular[{p}, {l}]"),
        _ => format!(
            "PerpendicularBisector[({:.2}, {:.2}), ({:.2}, {:.2})]",
            p1.x, p1.y, p2.x, p2.y
        ),
    }
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
    pub(crate) fn handle_canvas_3d_input(
        &mut self,
        ui: &mut egui::Ui,
        canvas_rect: egui::Rect,
        typed_four_d_phase: Option<f64>,
    ) {
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
                    crate::app::reset_3d_view_and_pause_motion(
                        &mut self.camera,
                        w,
                        h,
                        &mut self.multidimensional_motion_enabled,
                    );
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
            self.pause_multidimensional_motion();
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
                self.pause_multidimensional_motion();
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
            // Filtra scroll NaN/Inf y limita delta para no saltar de 0.01 a 50k en un frame.
            if sc.y.is_finite() && sc.y.abs() > f32::EPSILON && sc.y.abs() < 1e4 {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("input_zoom");
                self.pause_multidimensional_motion();
                self.is_view_changing = true;
                self.last_interaction_time = Instant::now();
                self.document.render_quality = RenderQuality::Preview;
                let factor = (1.0 + sc.y * 0.005).clamp(0.8, 1.25);
                self.camera.zoom(factor);
            }
        }

        // Tool ghost for 3D mode
        self.tool_ghost = None;
        if uses_3d_position_ghost(self.current_tool) {
            if let Some(local) = current_pos.and_then(|pos| canvas_local_pointer(canvas_rect, pos))
            {
                if let Some(ghost_pos) = crate::render_3d::construction_point_from_canvas(
                    &self.camera,
                    local,
                    canvas_rect.size(),
                ) {
                    self.tool_ghost = Some(GeoObject::Point3D(Point3DObj::new(ghost_pos)));
                }
            }
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
        if response.clicked_by(egui::PointerButton::Primary) && is_click {
            if let Some(local) = current_pos.and_then(|pos| canvas_local_pointer(canvas_rect, pos))
            {
                if self.current_tool == Tool::Select {
                    match typed_four_d_phase {
                        Some(phase) => {
                            crate::render_3d::select_3d_object_at_pointer_with_typed_four_d_phase(
                                &mut self.document,
                                &mut self.selected_object,
                                &self.camera,
                                local,
                                canvas_rect.size(),
                                Some(phase),
                            );
                        }
                        None => {
                            crate::render_3d::select_3d_object_at_pointer(
                                &mut self.document,
                                &mut self.selected_object,
                                &self.camera,
                                local,
                                canvas_rect.size(),
                            );
                        }
                    }
                } else {
                    self.handle_3d_click(ui, local, canvas_rect.size());
                    self.tool_ghost = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod perpendicular_command_tests {
    use super::*;
    use grafito_geometry::Point2;

    fn pt(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn point_line_pair_in_any_order_emits_perpendicular() {
        let p = |s: &str| Some((s.to_owned(), true, false));
        let l = |s: &str| Some((s.to_owned(), false, true));
        assert_eq!(
            perpendicular_command(p("A"), l("r"), pt(0.0, 0.0), pt(1.0, 1.0)),
            "Perpendicular[A, r]"
        );
        assert_eq!(
            perpendicular_command(l("r"), p("A"), pt(0.0, 0.0), pt(1.0, 1.0)),
            "Perpendicular[A, r]"
        );
    }

    #[test]
    fn missing_pair_falls_back_to_honest_bisector() {
        let p = |s: &str| Some((s.to_owned(), true, false));
        let (p1, p2) = (pt(0.0, 0.0), pt(2.0, 0.0));
        assert_eq!(
            perpendicular_command(None, None, p1, p2),
            "PerpendicularBisector[(0.00, 0.00), (2.00, 0.00)]"
        );
        assert!(perpendicular_command(p("A"), p("B"), p1, p2).starts_with("PerpendicularBisector"));
        assert!(perpendicular_command(None, p("A"), p1, p2).starts_with("PerpendicularBisector"));
    }
}

#[cfg(test)]
mod canvas_slider_widget_tests {
    use super::{
        canvas_slider_apply_drag, canvas_slider_hit, canvas_slider_is_internal_name,
        canvas_slider_value_to_t, sanitize_animation_speed,
    };

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn drag_maps_track_edges_and_center_without_step() {
        // Sin paso (step <= 0): continuo puro.
        assert!(approx_eq(
            canvas_slider_apply_drag(0.0, 0.0, 160.0, -5.0, 5.0, 0.0),
            -5.0
        ));
        assert!(approx_eq(
            canvas_slider_apply_drag(160.0, 0.0, 160.0, -5.0, 5.0, 0.0),
            5.0
        ));
        assert!(approx_eq(
            canvas_slider_apply_drag(80.0, 0.0, 160.0, -5.0, 5.0, 0.0),
            0.0
        ));
    }

    #[test]
    fn drag_clamps_outside_and_snaps_to_step() {
        // Fuera de pista: clamp a extremos.
        assert!(approx_eq(
            canvas_slider_apply_drag(-40.0, 0.0, 100.0, 0.0, 10.0, 0.0),
            0.0
        ));
        assert!(approx_eq(
            canvas_slider_apply_drag(400.0, 0.0, 100.0, 0.0, 10.0, 0.0),
            10.0
        ));
        // 33 % de [0, 10] = 3.3 → snap a múltiplo de 2 = 4.0.
        assert!(approx_eq(
            canvas_slider_apply_drag(33.0, 0.0, 100.0, 0.0, 10.0, 2.0),
            4.0
        ));
        // Paso no-finito o <= 0 = continuo sin snap.
        assert!(approx_eq(
            canvas_slider_apply_drag(33.0, 0.0, 100.0, 0.0, 10.0, f64::NAN),
            3.3
        ));
    }

    #[test]
    fn drag_rejects_degenerate_ranges_without_panic() {
        assert!(approx_eq(
            canvas_slider_apply_drag(50.0, 0.0, 100.0, 5.0, 5.0, 0.1),
            5.0
        ));
        assert!(approx_eq(
            canvas_slider_apply_drag(50.0, 0.0, 100.0, 7.0, -7.0, 0.1),
            7.0
        ));
        assert!(approx_eq(
            canvas_slider_apply_drag(50.0, 0.0, 100.0, f64::NAN, 5.0, 0.1),
            0.0
        ));
        // Pista de ancho cero: no hay mapeo, conserva el mínimo.
        assert!(approx_eq(
            canvas_slider_apply_drag(50.0, 80.0, 80.0, -5.0, 5.0, 0.1),
            -5.0
        ));
    }

    #[test]
    fn sanitize_speed_keeps_finite_and_repairs_the_rest() {
        assert!(approx_eq(sanitize_animation_speed(2.5), 2.5));
        assert!(approx_eq(sanitize_animation_speed(0.0), 0.0));
        assert!(approx_eq(sanitize_animation_speed(-3.0), -3.0));
        assert!(approx_eq(sanitize_animation_speed(250.0), 100.0));
        assert!(approx_eq(sanitize_animation_speed(-250.0), -100.0));
        assert!(approx_eq(sanitize_animation_speed(f64::NAN), 1.0));
        assert!(approx_eq(sanitize_animation_speed(f64::INFINITY), 1.0));
    }

    #[test]
    fn value_to_t_covers_range_and_clamps() {
        assert!((canvas_slider_value_to_t(0.0, -5.0, 5.0) - 0.5).abs() < 1e-6);
        assert!((canvas_slider_value_to_t(5.0, -5.0, 5.0) - 1.0).abs() < 1e-6);
        assert!((canvas_slider_value_to_t(-5.0, -5.0, 5.0) - 0.0).abs() < 1e-6);
        assert!((canvas_slider_value_to_t(99.0, -5.0, 5.0) - 1.0).abs() < 1e-6);
        assert!((canvas_slider_value_to_t(f64::NAN, -5.0, 5.0) - 0.0).abs() < 1e-6);
        assert!((canvas_slider_value_to_t(1.0, 5.0, 5.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn hit_covers_expanded_track_and_thumb_with_8px_tolerance() {
        use egui::{pos2, Rect, Vec2};
        let track = Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(160.0, 4.0));
        let thumb = pos2(80.0, 2.0);
        // Sobre pista y thumb.
        assert!(canvas_slider_hit(pos2(80.0, 2.0), track, thumb, 8.0, 8.0));
        // Borde expandido: 5 px más allá del final sigue dentro (160 + 8).
        assert!(canvas_slider_hit(pos2(165.0, 2.0), track, thumb, 8.0, 8.0));
        // 15 px bajo el thumb: fuera de pista expandida pero dentro del
        // radio del thumb (8 + 8 = 16).
        assert!(canvas_slider_hit(pos2(80.0, 17.0), track, thumb, 8.0, 8.0));
        // Lejos de ambos: sin hit.
        assert!(!canvas_slider_hit(
            pos2(200.0, 60.0),
            track,
            thumb,
            8.0,
            8.0
        ));
    }

    #[test]
    fn internal_trig_names_are_filtered_but_user_names_pass() {
        assert!(canvas_slider_is_internal_name("trig_angle"));
        assert!(canvas_slider_is_internal_name("trig_aux_1"));
        assert!(canvas_slider_is_internal_name("TrigGraph"));
        assert!(canvas_slider_is_internal_name("TrigValue"));
        assert!(!canvas_slider_is_internal_name("alpha"));
        assert!(!canvas_slider_is_internal_name("v0"));
        assert!(!canvas_slider_is_internal_name("trigger"));
    }
}
