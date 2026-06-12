//! GrafitoEngine — API principal expuesta via UniFFI

use std::sync::{Arc, Mutex};
use grafito_core::{Document, GeoObject, ObjectId};
use grafito_geometry::{Camera3D, Color, Point2};
use glam::Vec2;
use crate::canvas::CanvasRenderer;
use crate::dto::*;
use crate::converters::{geo_object_to_dto, id_to_string};
use crate::persist;

const MAX_UNDO: usize = 50;

#[derive(uniffi::Object)]
pub struct GrafitoEngine {
    document: Arc<Mutex<Document>>,
    camera: Arc<Mutex<Camera3D>>,
    view_mode: Arc<Mutex<String>>,
    current_tool: Arc<Mutex<ToolDto>>,
    dark_mode: Arc<Mutex<bool>>,
    selected_id: Arc<Mutex<Option<ObjectId>>>,
    undo_stack: Arc<Mutex<Vec<Document>>>,
    redo_stack: Arc<Mutex<Vec<Document>>>,
    screen_width: Arc<Mutex<f32>>,
    screen_height: Arc<Mutex<f32>>,
    pending_points: Arc<Mutex<Vec<Point2>>>,
}

#[uniffi::export]
impl GrafitoEngine {
    #[uniffi::constructor]
    pub fn new(screen_width: f32, screen_height: f32) -> Arc<Self> {
        let mut doc = Document::new();
        doc.view_mut().screen_size = glam::Vec2::new(screen_width, screen_height);
        let aspect = screen_width / screen_height.max(1.0);
        let mut cam = Camera3D::new(aspect);
        cam.aspect = aspect;

        Arc::new(Self {
            document: Arc::new(Mutex::new(doc)),
            camera: Arc::new(Mutex::new(cam)),
            view_mode: Arc::new(Mutex::new("2D".to_string())),
            current_tool: Arc::new(Mutex::new(ToolDto::Select)),
            dark_mode: Arc::new(Mutex::new(true)),
            selected_id: Arc::new(Mutex::new(None)),
            undo_stack: Arc::new(Mutex::new(Vec::new())),
            redo_stack: Arc::new(Mutex::new(Vec::new())),
            screen_width: Arc::new(Mutex::new(screen_width)),
            screen_height: Arc::new(Mutex::new(screen_height)),
            pending_points: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn get_snapshot(self: &Arc<Self>) -> DocumentSnapshot {
        let doc = self.document.lock().unwrap();
        let selected = self.selected_id.lock().unwrap();
        let view_mode = self.view_mode.lock().unwrap();
        let undo_stack = self.undo_stack.lock().unwrap();
        let redo_stack = self.redo_stack.lock().unwrap();

        let objects: Vec<ObjectDto> = doc.objects_iter()
            .map(|(_, obj)| geo_object_to_dto(obj))
            .collect();

        let variables: Vec<VariableDto> = doc.variables()
            .iter()
            .map(|(name, value)| VariableDto {
                name: name.clone(),
                value: *value,
                min: -10.0,
                max: 10.0,
            })
            .collect();

        DocumentSnapshot {
            objects,
            variables,
            selected_id: selected.map(id_to_string),
            view_mode: view_mode.clone(),
            undo_available: !undo_stack.is_empty(),
            redo_available: !redo_stack.is_empty(),
        }
    }

    pub fn process_command(self: &Arc<Self>, input: String) -> CommandResult {
        self.save_state();
        let mut doc = self.document.lock().unwrap();
        let mut input_text = input;
        let outcome = crate::command_processor::process_input(&mut doc, &mut input_text);
        if outcome.success {
            CommandResult {
                success: true,
                message: outcome.message,
                new_object_id: outcome.new_object_id.map(id_to_string),
            }
        } else {
            CommandResult {
                success: false,
                message: Some("Invalid command".to_string()),
                new_object_id: None,
            }
        }
    }

    pub fn delete_object(self: &Arc<Self>, id: String) -> bool {
        if let Some(obj_id) = parse_object_id(&id) {
            self.save_state();
            let mut doc = self.document.lock().unwrap();
            doc.remove_object(obj_id).is_some()
        } else {
            false
        }
    }

    pub fn toggle_visibility(self: &Arc<Self>, id: String) -> bool {
        if let Some(obj_id) = parse_object_id(&id) {
            let mut doc = self.document.lock().unwrap();
            if let Some(obj) = doc.get_object_mut(obj_id) {
                let current = obj.is_visible();
                obj.set_visible(!current);
                return true;
            }
        }
        false
    }

    pub fn set_object_label(self: &Arc<Self>, id: String, label: String) -> bool {
        if let Some(obj_id) = parse_object_id(&id) {
            let mut doc = self.document.lock().unwrap();
            if let Some(obj) = doc.get_object_mut(obj_id) {
                obj.set_label(label);
                return true;
            }
        }
        false
    }

    pub fn set_object_color(self: &Arc<Self>, id: String, r: f32, g: f32, b: f32) -> bool {
        if let Some(obj_id) = parse_object_id(&id) {
            let mut doc = self.document.lock().unwrap();
            if let Some(obj) = doc.get_object_mut(obj_id) {
                obj.set_color(Color::new(r, g, b, 1.0));
                return true;
            }
        }
        false
    }

    pub fn set_variable(self: &Arc<Self>, name: String, value: f64) {
        let mut doc = self.document.lock().unwrap();
        doc.set_variable(name, value);
    }

    pub fn undo(self: &Arc<Self>) -> bool {
        let mut undo_stack = self.undo_stack.lock().unwrap();
        let mut redo_stack = self.redo_stack.lock().unwrap();
        let mut doc = self.document.lock().unwrap();
        if let Some(prev_doc) = undo_stack.pop() {
            redo_stack.push(doc.clone());
            *doc = prev_doc;
            true
        } else {
            false
        }
    }

    pub fn redo(self: &Arc<Self>) -> bool {
        let mut undo_stack = self.undo_stack.lock().unwrap();
        let mut redo_stack = self.redo_stack.lock().unwrap();
        let mut doc = self.document.lock().unwrap();
        if let Some(next_doc) = redo_stack.pop() {
            undo_stack.push(doc.clone());
            *doc = next_doc;
            true
        } else {
            false
        }
    }

    pub fn clear(self: &Arc<Self>) {
        self.save_state();
        let mut doc = self.document.lock().unwrap();
        doc.clear();
        self.selected_id.lock().unwrap().take();
    }

    pub fn select_object(self: &Arc<Self>, id: Option<String>) {
        let mut selected = self.selected_id.lock().unwrap();
        *selected = id.and_then(|s| parse_object_id(&s));
    }

    pub fn pick_object_at(self: &Arc<Self>, screen_x: f32, screen_y: f32) -> Option<String> {
        let mut doc = self.document.lock().unwrap();
        let screen = Vec2::new(screen_x, screen_y);
        let world = doc.view().screen_to_world(screen);
        let tolerance = 10.0 / doc.view().scale;
        doc.pick_object(world, tolerance).map(id_to_string)
    }

    pub fn create_canvas_renderer(self: &Arc<Self>) -> Arc<CanvasRenderer> {
        CanvasRenderer::new(
            self.clone(),
            *self.screen_width.lock().unwrap() as u32,
            *self.screen_height.lock().unwrap() as u32,
        )
    }

    pub fn update_screen_size(self: &Arc<Self>, width: f32, height: f32) {
        *self.screen_width.lock().unwrap() = width;
        *self.screen_height.lock().unwrap() = height;
        self.document.lock().unwrap().view_mut().screen_size = glam::Vec2::new(width, height);
        self.camera.lock().unwrap().aspect = width / height.max(1.0);
    }

    pub fn set_view_mode(self: &Arc<Self>, mode: String) {
        *self.view_mode.lock().unwrap() = mode;
    }

    pub fn set_tool(self: &Arc<Self>, tool: ToolDto) {
        *self.current_tool.lock().unwrap() = tool;
    }

    pub fn get_tool(self: &Arc<Self>) -> ToolDto {
        self.current_tool.lock().unwrap().clone()
    }

    pub fn set_dark_mode(self: &Arc<Self>, dark: bool) {
        *self.dark_mode.lock().unwrap() = dark;
    }

    pub fn is_dark_mode(self: &Arc<Self>) -> bool {
        *self.dark_mode.lock().unwrap()
    }

    pub fn canvas_pan(self: &Arc<Self>, dx: f32, dy: f32) {
        if self.get_view_mode() == "3D" {
            let mut cam = self.camera.lock().unwrap();
            cam.orbit(dx * 0.005, dy * 0.005);
        } else {
            let mut doc = self.document.lock().unwrap();
            doc.view_mut().pan(glam::Vec2::new(dx, dy));
        }
    }

    pub fn canvas_zoom(self: &Arc<Self>, factor: f32, center_x: f32, center_y: f32) {
        if self.get_view_mode() == "3D" {
            let mut cam = self.camera.lock().unwrap();
            cam.zoom(factor);
        } else {
            let mut doc = self.document.lock().unwrap();
            let anchor = Vec2::new(center_x, center_y);
            doc.view_mut().zoom(factor, anchor);
        }
    }

    pub fn canvas_tap(self: &Arc<Self>, x: f32, y: f32) -> CommandResult {
        let tool = self.current_tool.lock().unwrap().clone();
        
        // Save state BEFORE acquiring the document lock to prevent a Mutex deadlock
        match tool {
            ToolDto::Select => {} // Don't save state on select
            _ => self.save_state(),
        }

        let mut doc = self.document.lock().unwrap();
        let world = doc.view().screen_to_world(Vec2::new(x, y));
        
        match tool {
            ToolDto::Point => {
                let id = doc.add_object(GeoObject::Point(grafito_core::PointObj::new(world)));
                CommandResult { success: true, message: Some("Point created".to_string()), new_object_id: Some(id_to_string(id)) }
            }
            ToolDto::Select => {
                let tolerance = 10.0 / doc.view().scale;
                if let Some(obj_id) = doc.pick_object(world, tolerance) {
                    drop(doc);
                    self.select_object(Some(id_to_string(obj_id)));
                    CommandResult { success: true, message: Some("Object selected".to_string()), new_object_id: None }
                } else {
                    drop(doc);
                    self.select_object(None);
                    CommandResult { success: true, message: Some("Selection cleared".to_string()), new_object_id: None }
                }
            }
            ToolDto::Line => {
                let mut pts = self.pending_points.lock().unwrap();
                pts.push(world);
                if pts.len() == 2 {
                    let id = doc.add_object(GeoObject::Line(grafito_core::LineObj::new(pts[0], pts[1])));
                    pts.clear();
                    CommandResult { success: true, message: Some("Line created".to_string()), new_object_id: Some(id_to_string(id)) }
                } else {
                    CommandResult { success: true, message: Some("Select second point".to_string()), new_object_id: None }
                }
            }
            ToolDto::Circle => {
                let mut pts = self.pending_points.lock().unwrap();
                pts.push(world);
                if pts.len() == 2 {
                    let r = pts[0].distance(&pts[1]);
                    let id = doc.add_object(GeoObject::Circle(grafito_core::CircleObj::new(pts[0], r)));
                    pts.clear();
                    CommandResult { success: true, message: Some("Circle created".to_string()), new_object_id: Some(id_to_string(id)) }
                } else {
                    CommandResult { success: true, message: Some("Select edge point".to_string()), new_object_id: None }
                }
            }
            ToolDto::Polygon => {
                let mut pts = self.pending_points.lock().unwrap();
                pts.push(world);
                if pts.len() >= 3 && world.distance(&pts[0]) < (20.0 / doc.view().scale) {
                    let mut final_pts = pts.clone();
                    final_pts.pop(); // remove the last click that closed it
                    let id = doc.add_object(GeoObject::Polygon(grafito_core::PolygonObj::new(final_pts)));
                    pts.clear();
                    CommandResult { success: true, message: Some("Polygon created".to_string()), new_object_id: Some(id_to_string(id)) }
                } else {
                    CommandResult { success: true, message: Some(format!("Point {} added", pts.len())), new_object_id: None }
                }
            }
            ToolDto::Fractal => {
                let mut cmd = "Mandelbrot[]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Fractal created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::Attractor => {
                let mut cmd = "Lorenz[]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Attractor created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::Histogram => {
                let mut cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Histogram created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::ScatterPlot => {
                let mut cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Scatter Plot created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::Tangent => {
                let mut pts = self.pending_points.lock().unwrap();
                pts.push(world);
                if pts.len() == 3 {
                    let r = pts[0].distance(&pts[1]);
                    let mut cmd = format!("Tangent[({:.2},{:.2}), {:.3}, ({:.2},{:.2})]", pts[0].x, pts[0].y, r, pts[2].x, pts[2].y);
                    crate::command_processor::process_input(&mut doc, &mut cmd);
                    pts.clear();
                    CommandResult { success: true, message: Some("Tangents created".to_string()), new_object_id: None }
                } else {
                    CommandResult { success: true, message: Some(format!("{} point(s) selected", pts.len())), new_object_id: None }
                }
            }
            ToolDto::Perpendicular => {
                let mut pts = self.pending_points.lock().unwrap();
                pts.push(world);
                if pts.len() == 2 {
                    let mut cmd = format!("PerpendicularBisector[({:.2},{:.2}), ({:.2},{:.2})]", pts[0].x, pts[0].y, pts[1].x, pts[1].y);
                    let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                    pts.clear();
                    CommandResult { success: outcome.success, message: outcome.message.or(Some("Bisector created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
                } else {
                    CommandResult { success: true, message: Some("Select second point".to_string()), new_object_id: None }
                }
            }
            ToolDto::DomainColoring => {
                let mut cmd = "DomainColoring[z^2 + 1, -2, 2, -2, 2]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Domain coloring created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::HeatMap => {
                let mut cmd = "HeatMap[sin(x)*cos(y), -3, 3, -3, 3]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Heat map created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            ToolDto::ComplexGrid => {
                let mut cmd = "ComplexGrid[z^3 - 1, -2, 2, -2, 2]".to_string();
                let outcome = crate::command_processor::process_input(&mut doc, &mut cmd);
                *self.current_tool.lock().unwrap() = ToolDto::Select;
                CommandResult { success: outcome.success, message: outcome.message.or(Some("Complex grid created".to_string())), new_object_id: outcome.new_object_id.map(id_to_string) }
            }
            _ => CommandResult {
                success: false,
                message: Some("Tool handled via command or not implemented".to_string()),
                new_object_id: None,
            }
        }
    }

    pub fn canvas_drag_point(self: &Arc<Self>, object_id: String, x: f32, y: f32) -> bool {
        if let Some(id) = parse_object_id(&object_id) {
            let mut doc = self.document.lock().unwrap();
            let world = doc.view().screen_to_world(Vec2::new(x, y));
            if doc.is_free_object(&id) {
                doc.move_point(id, world);
                let order = doc.propagation_order(&[id]);
                doc.re_evaluate_constraints(&order);
                return true;
            }
        }
        false
    }

    pub fn zoom_to_fit(self: &Arc<Self>) {
        let mut doc = self.document.lock().unwrap();
        let view = doc.view_mut();
        view.offset = Point2::new(0.0, 0.0);
        view.scale = 1.0;
    }

    pub fn camera_orbit(self: &Arc<Self>, delta_azimuth: f32, delta_elevation: f32) {
        let mut camera = self.camera.lock().unwrap();
        camera.orbit(delta_azimuth, delta_elevation);
    }

    pub fn camera_dolly(self: &Arc<Self>, delta: f32) {
        let mut camera = self.camera.lock().unwrap();
        camera.distance = (camera.distance - delta).clamp(1.0, 100.0);
    }

    pub fn get_spreadsheet(self: &Arc<Self>) -> SpreadsheetDto {
        let doc = self.document.lock().unwrap();
        let (rows, cols) = doc.spreadsheet_dim();
        let mut cells = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let value = doc.get_spreadsheet_cell(r, c);
                let evaluated = doc.eval_spreadsheet_cell(r, c);
                cells.push(CellDto {
                    row: r as u32,
                    col: c as u32,
                    value,
                    evaluated,
                });
            }
        }
        SpreadsheetDto { rows: rows as u32, cols: cols as u32, cells }
    }

    pub fn set_cell(self: &Arc<Self>, row: u32, col: u32, value: String) {
        let mut doc = self.document.lock().unwrap();
        doc.set_spreadsheet_cell(row as usize, col as usize, value);
    }

    pub fn search_commands(self: &Arc<Self>, query: String) -> Vec<PaletteCommandDto> {
        let all = vec![
            ("Point", "2D", "Point[(x,y)]"),
            ("Line", "2D", "Line[(x1,y1), (x2,y2)]"),
            ("Circle", "2D", "Circle[(cx,cy), r]"),
            ("Ellipse", "2D", "Ellipse[(cx,cy), rx, ry]"),
            ("Parabola", "2D", "Parabola[(vx,vy), p]"),
            ("Hyperbola", "2D", "Hyperbola[(cx,cy), a, b]"),
            ("Function", "2D", "Function[expr]"),
            ("Polygon", "2D", "Polygon[(x1,y1), (x2,y2), ...]"),
            ("Point3D", "3D", "Point3D[(x,y,z)]"),
            ("Sphere3D", "3D", "Sphere3D[(cx,cy,cz), r]"),
            ("Cube3D", "3D", "Cube3D[(cx,cy,cz), size]"),
            ("Derivative", "CAS", "Derivative[expr, x]"),
            ("Integral", "CAS", "Integral[expr, x, a, b]"),
            ("Solve", "CAS", "Solve[equation, x]"),
            ("Taylor", "CAS", "Taylor[expr, x, a, n]"),
        ];
        let ql = query.to_lowercase();
        all.into_iter()
            .filter(|(n, _, _)| n.to_lowercase().contains(&ql))
            .map(|(n, c, s)| PaletteCommandDto {
                name: n.to_string(), category: c.to_string(), syntax_hint: s.to_string(),
            })
            .collect()
    }

    pub fn save_to_file(self: &Arc<Self>, path: String) -> bool {
        let doc = self.document.lock().unwrap();
        persist::save_document(&doc, &path)
    }

    pub fn load_from_file(self: &Arc<Self>, path: String) -> bool {
        if let Some(doc) = persist::load_document(&path) {
            *self.document.lock().unwrap() = doc;
            true
        } else {
            false
        }
    }

}

// ── Internal methods (not exposed to FFI) ──────────────────────

impl GrafitoEngine {
    pub fn get_view_mode(&self) -> String {
        self.view_mode.lock().unwrap().clone()
    }

    pub fn get_dark_mode(&self) -> bool {
        *self.dark_mode.lock().unwrap()
    }

    pub fn get_document(&self) -> Arc<Mutex<Document>> {
        self.document.clone()
    }

    pub fn get_camera(&self) -> Arc<Mutex<Camera3D>> {
        self.camera.clone()
    }

    fn save_state(&self) {
        let doc = self.document.lock().unwrap();
        let mut undo_stack = self.undo_stack.lock().unwrap();
        let mut redo_stack = self.redo_stack.lock().unwrap();
        undo_stack.push(doc.clone());
        if undo_stack.len() > MAX_UNDO { undo_stack.remove(0); }
        redo_stack.clear();
    }
}

fn parse_object_id(id_str: &str) -> Option<ObjectId> {
    uuid::Uuid::parse_str(id_str).ok().map(ObjectId)
}
