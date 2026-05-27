//! Grafito Desktop Application — Main entry point and eframe UI orchestrator.

mod commands;
mod export;
mod render_2d;
mod render_3d;

use grafito_core::{Document, GeoObject, ObjectId,
    PointObj, LineObj, CircleObj, PolygonObj, FunctionObj, EllipseObj, Sphere3DObj, Cube3DObj,
};
use grafito_geometry::{Point2, Point3D, ViewTransform, Camera3D, Color};
use grafito_ui::{Tool, algebra_view, properties_panel, toolbar};
use grafito_ui::theme::{DARK as THEME_DARK, LIGHT as THEME_LIGHT};
use grafito_ui::animation::RippleManager;
use grafito_ui::toast::ToastManager;
use egui::{Pos2, Color32, Sense, Key};
use glam::Vec2 as GlamVec2;
use std::fs;

const MAX_UNDO: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode { D2, D3 }

#[allow(dead_code)]
fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (c.r * 255.0) as u8, (c.g * 255.0) as u8,
        (c.b * 255.0) as u8, (c.a * 255.0) as u8,
    )
}

pub struct GrafitoApp {
    pub document: Document,
    pub current_tool: Tool,
    pub current_view: ViewMode,
    pub camera: Camera3D,
    pub animation_running: bool,
    pub show_grid: bool,
    pub snap_to_grid: bool,
    pub exam_mode: bool,
    pub dark_mode: bool,
    pub ripple_manager: RippleManager,
    pub toast_manager: ToastManager,
    pub pending_points: Vec<Point2>,
    pub pending_points_3d: Vec<Point3D>,
    pub last_mouse_pos: Option<Pos2>,
    pub selected_object: Option<ObjectId>,
    pub input_text: String,
    pub cas_result: String,
    pub recent_files: Vec<String>,
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<Document>,
}

impl GrafitoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(1280.0, 720.0));
        document.add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("A")));
        document.add_object(GeoObject::Point(PointObj::new(Point2::new(3.0, 2.0)).with_label("B")));
        document.add_object(GeoObject::Line(LineObj::new(Point2::new(-2.0, -1.0), Point2::new(4.0, 3.0)).with_label("l")));
        document.add_object(GeoObject::Circle(CircleObj::new(Point2::new(1.0, 1.0), 2.0).with_label("c")));
        document.add_object(GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(-3.0, -2.0), Point2::new(-1.0, -3.0), Point2::new(-2.0, -1.0),
        ])));
        document.add_object(GeoObject::Function(FunctionObj::new("sin(x)").with_label("f(x)")));
        document.set_variable("a".into(), 2.0);
        document.add_object(GeoObject::Function(FunctionObj::new("a*sin(x)").with_label("g(x)")));
        document.add_object(GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0).with_label("C1")));
        document.add_object(GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(2.0, 1.0, 0.0), 1.0).with_label("S1")));
        document.add_object(GeoObject::Ellipse(EllipseObj::new(Point2::new(-1.0, -2.0), 2.0, 1.0).with_label("E1")));

        let dark_mode = true; // dark by default
        if dark_mode { THEME_DARK.apply(&_cc.egui_ctx); } else { THEME_LIGHT.apply(&_cc.egui_ctx); }

        Self {
            document,
            current_tool: Tool::default(),
            current_view: ViewMode::D2,
            camera: Camera3D::new(1280.0 / 720.0),
            animation_running: false,
            show_grid: true,
            snap_to_grid: false,
            exam_mode: false,
            dark_mode,
            ripple_manager: RippleManager::default(),
            toast_manager: ToastManager::default(),
            pending_points: Vec::new(),
            pending_points_3d: Vec::new(),
            last_mouse_pos: None,
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
        if self.undo_stack.len() > MAX_UNDO { self.undo_stack.remove(0); }
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
            self.document.view_mut().offset = GlamVec2::new(-cx as f32 * scale, cy as f32 * scale);
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
                            .add_filter("Grafito Document", &["grafito", "json", "toml"]).pick_file()
                        {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(doc) = serde_json::from_str::<Document>(&content) {
                                    self.document = doc; self.undo_stack.clear(); self.redo_stack.clear();
                                    let p = path.to_string_lossy().to_string();
                                    if !self.recent_files.contains(&p) { self.recent_files.insert(0, p); self.recent_files.truncate(8); }
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Grafito Document", &["grafito"]).set_file_name("grafito.grafito").save_file()
                        {
                            if let Ok(json) = serde_json::to_string_pretty(&self.document) { let _ = fs::write(path, json); }
                        }
                        ui.close_menu();
                    }
                    if !self.recent_files.is_empty() {
                        ui.separator(); ui.label("Recent:");
                        for f in self.recent_files.clone() {
                            let name = std::path::Path::new(&f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(f.clone());
                            if ui.button(&name).clicked() {
                                if let Ok(content) = fs::read_to_string(&f) {
                                    if let Ok(doc) = serde_json::from_str::<Document>(&content) {
                                        self.document = doc; self.undo_stack.clear(); self.redo_stack.clear();
                                    }
                                }
                                ui.close_menu();
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Export SVG...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("SVG Image", &["svg"]).set_file_name("grafito.svg").save_file()
                            { let svg = export::export_svg(&self.document); let _ = fs::write(path, svg); }
                        ui.close_menu();
                    }
                    if ui.button("Export TikZ...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("LaTeX TikZ", &["tex", "tikz"]).set_file_name("grafito.tex").save_file()
                            { let tikz = export::export_tikz(&self.document); let _ = fs::write(path, tikz); }
                        ui.close_menu();
                    }
                    if ui.button("Export PNG...").clicked() {
                        let w = self.document.view().screen_size.x as u32;
                        let h = self.document.view().screen_size.y as u32;
                        if let Some(path) = rfd::FileDialog::new().add_filter("PNG Image", &["png"]).set_file_name("grafito.png").save_file()
                            { let img = export::export_png(&self.document, w, h); let _ = img.save(path); }
                        ui.close_menu();
                    }
                });
                if !self.cas_result.is_empty() {
                    ui.separator(); ui.colored_label(Color32::from_rgb(40, 120, 40), &self.cas_result);
                }
            });
        });

        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) { self.undo(); }
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
            || ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl) { self.redo(); }
        if ctx.input(|i| i.key_pressed(Key::Delete)) { self.delete_selected(); }

        // Tool shortcuts (F1-F6)
        if ctx.input(|i| i.key_pressed(Key::F1)) { self.current_tool = Tool::Select; }
        if ctx.input(|i| i.key_pressed(Key::F2)) { self.current_tool = Tool::Point; }
        if ctx.input(|i| i.key_pressed(Key::F3)) { self.current_tool = Tool::Line; }
        if ctx.input(|i| i.key_pressed(Key::F4)) { self.current_tool = Tool::Circle; }
        if ctx.input(|i| i.key_pressed(Key::F5)) { self.current_tool = Tool::Polygon; }
        if ctx.input(|i| i.key_pressed(Key::F6)) { self.current_tool = Tool::Function; }
        if ctx.input(|i| i.key_pressed(Key::Escape)) { self.current_tool = Tool::Select; }

        // Top toolbar with view mode tabs
        egui::TopBottomPanel::top("toolbar").frame(
            egui::Frame::none().fill(if self.dark_mode { Color32::from_rgb(35, 37, 46) } else { Color32::from_rgb(240, 240, 245) }).inner_margin(8.0)
        ).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_view, ViewMode::D2, "📐 2D");
                ui.selectable_value(&mut self.current_view, ViewMode::D3, "🧊 3D");
                ui.separator();
                toolbar(ui, &mut self.current_tool);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(if self.dark_mode { "☀" } else { "🌙" }).clicked() {
                        self.dark_mode = !self.dark_mode;
                        if self.dark_mode { THEME_DARK.apply(ui.ctx()); }
                        else { THEME_LIGHT.apply(ui.ctx()); }
                    }
                    if ui.button("⛶").on_hover_text("Zoom to Fit").clicked() { self.zoom_to_fit(); }
                    ui.checkbox(&mut self.show_grid, "Grid");
                    if ui.checkbox(&mut self.exam_mode, "Exam").changed() && self.exam_mode {
                        self.cas_result = "EXAM MODE: CAS disabled".into();
                    }
                });
            });
        });

        // Left: Algebra View + Variables + Sliders
        egui::SidePanel::left("algebra").default_width(200.0).show(ctx, |ui| {
            algebra_view(ui, &self.document, &mut self.selected_object);
            if let Some(id) = self.selected_object {
                ui.separator();
                properties_panel(ui, &mut self.document, id);
            }
            if !self.document.variables.is_empty() {
                ui.separator(); ui.heading("Variables");
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
            ui.heading("Spreadsheet"); ui.separator();
            let (rows, cols) = self.document.spreadsheet_dim();
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("sp_grid").striped(true).show(ui, |ui| {
                    ui.label("");
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
                                if let Ok((x, y)) = commands::parse_point_str(&val) {
                                    self.document.add_object(GeoObject::Point(
                                        PointObj::new(Point2::new(x, y)).with_label(format!("{}{}", (b'A' + c as u8) as char, r + 1))
                                    ));
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
            if self.exam_mode { ui.label("EXAM MODE — input disabled"); return; }
            ui.horizontal(|ui| {
                ui.label("Input:");
                let response = ui.text_edit_singleline(&mut self.input_text);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.save_state();
                    self.cas_result = commands::process_input(&mut self.document, &mut self.input_text).unwrap_or_default();
                }
                if ui.button("Enter").clicked() {
                    self.save_state();
                    self.cas_result = commands::process_input(&mut self.document, &mut self.input_text).unwrap_or_default();
                }
            });
        });

        // Central canvas
        match self.current_view {
            ViewMode::D2 => {
                self.camera.aspect = 1.6;
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas_rect = ui.available_rect_before_wrap();
                    self.handle_canvas_input(ui, canvas_rect);
                    let response = ui.interact(canvas_rect, ui.id().with("ctx_menu"), Sense::click());
                    if response.clicked_by(egui::PointerButton::Secondary) {
                        response.context_menu(|ui| {
                            if ui.button("Delete selected").clicked() { self.delete_selected(); ui.close_menu(); }
                            if ui.button("Zoom to fit").clicked() { self.zoom_to_fit(); ui.close_menu(); }
                            ui.separator();
                            if ui.button("Reset view").clicked() {
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
                    let w = canvas_rect.width(); let h = canvas_rect.height();
                    self.camera.aspect = w / h.max(1.0);
                    let ctx_resp = ui.interact(canvas_rect, ui.id().with("ctx_menu_3d"), Sense::click());
                    if ctx_resp.clicked_by(egui::PointerButton::Secondary) {
                        ctx_resp.context_menu(|ui| {
                            if ui.button("Delete selected").clicked() { self.delete_selected(); ui.close_menu(); }
                            if ui.button("Reset view").clicked() { self.camera = Camera3D::new(w / h.max(1.0)); ui.close_menu(); }
                        });
                    }
                    let response = ui.interact(canvas_rect, ui.id().with("canvas3d"), Sense::click_and_drag());
                    if let Some(pos) = response.hover_pos() {
                        if response.dragged_by(egui::PointerButton::Secondary) {
                            if let Some(last) = self.last_mouse_pos { self.camera.orbit((pos.x - last.x) * 0.005, (pos.y - last.y) * 0.005); }
                        }
                        if response.dragged_by(egui::PointerButton::Primary) {
                            if let Some(last) = self.last_mouse_pos {
                                if self.current_tool == Tool::Select { self.camera.pan(pos.x - last.x, pos.y - last.y); }
                            }
                        }
                        if response.hovered() {
                            let scroll = ui.input(|i| i.smooth_scroll_delta);
                            if scroll.y != 0.0 { self.camera.zoom(1.0 + scroll.y * 0.005); }
                        }
                        self.last_mouse_pos = Some(pos);
                    }
                    if response.clicked_by(egui::PointerButton::Primary) && self.current_tool != Tool::Select {
                        self.handle_3d_click(ui, &response, canvas_rect, w, h);
                    }
                    let painter = ui.painter();
                    self.draw_3d_grid(painter, canvas_rect, w, h);
                    self.draw_3d_objects(painter, canvas_rect, w, h);
                });
            }
        }

        // Status bar
        egui::TopBottomPanel::bottom("status").min_height(20.0).frame(
            egui::Frame::none().fill(if self.dark_mode { Color32::from_rgb(28, 30, 36) } else { Color32::from_rgb(245, 245, 248) }).inner_margin(6.0)
        ).show(ctx, |ui| {
            let tool_hint = match self.current_tool {
                Tool::Select => "🖱 Select: Click to pick, drag to pan, scroll to zoom",
                Tool::Point => "⊙ Point: Click to place | (x,y) or A=(x,y) in input bar",
                Tool::Line => "╱ Line: Click two points | F3",
                Tool::Circle => "○ Circle: Click center then edge | F4",
                Tool::Polygon => "⬠ Polygon: Click vertices | F5",
                Tool::Function => "𝑓 Function: f(x)=expr in input bar | F6",
            };
            ui.horizontal(|ui| {
                ui.label(tool_hint);
                ui.add_space(20.0);
                ui.label(format!("Objs: {} | {}", self.document.object_count(),
                    if self.current_view == ViewMode::D2 { "2D" } else { "3D" }));
            });
        });

        // Ripples and toasts drawn last (on the 2D canvas painter)
        // They are drawn via the canvas painter in render_2d.
    }
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("Grafito", options, Box::new(|cc| Ok(Box::new(GrafitoApp::new(cc))))).unwrap();
}
