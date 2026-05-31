//! Grafito Desktop Application — Main entry point and eframe UI orchestrator.

mod commands;
mod export;
mod render_2d;
mod render_3d;

use grafito_core::{Document, GeoObject, ObjectId,
    PointObj, LineObj, CircleObj, PolygonObj, FunctionObj, EllipseObj, Sphere3DObj, Cube3DObj, Point3DObj,
};
use grafito_geometry::{Point2, Point3D, ViewTransform, Camera3D, Color};
use grafito_ui::Tool;
use grafito_ui::theme::{DARK as THEME_DARK, LIGHT as THEME_LIGHT};
use egui::{Pos2, Vec2, Color32, Sense, Key};


const MAX_UNDO: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode { D2, D3 }

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
    pub pending_points: Vec<Point2>,
    pub pending_points_3d: Vec<Point3D>,
    pub last_mouse_pos: Option<Pos2>,
    pub selected_object: Option<ObjectId>,
    pub preview_object: Option<GeoObject>,
    pub input_text: String,
    pub cas_result: String,
    pub show_spreadsheet: bool,
    pub keyboard_tab: usize,
    pub sidebar_tab: usize,
    pub recent_files: Vec<String>,
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<Document>,
    pub attractor_cache: std::collections::HashMap<ObjectId, (u64, Vec<Point3D>)>,
    pub active_color_picker: Option<(ObjectId, grafito_ui::color_picker::HsvColorPicker)>,
    pub color_favorites: [grafito_geometry::Color; 5],
    pub tool_ghost: Option<GeoObject>,
}

impl GrafitoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(1280.0, 720.0));
        document.view_mut().scale = 50.0; // ~13 units each side — matches GeoGebra default zoom
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

        let dark_mode = false; // light mode by default, like GeoGebra
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
            pending_points: Vec::new(),
            pending_points_3d: Vec::new(),
            last_mouse_pos: None,
            selected_object: None,
            preview_object: None,
            input_text: String::new(),
            cas_result: String::new(),
            show_spreadsheet: false,
            keyboard_tab: 0,
            sidebar_tab: 0,
            recent_files: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            attractor_cache: std::collections::HashMap::new(),
            active_color_picker: None,
            tool_ghost: None,
            color_favorites: [
                grafito_geometry::Color::new(0.9, 0.1, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.3, 0.9, 1.0),
                grafito_geometry::Color::new(0.9, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.5, 0.1, 0.9, 1.0),
            ],
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
            let cx = (min.x + max.x) / 2.0;
            let cy = (min.y + max.y) / 2.0;
            let dx = (max.x - min.x).max(10.0);
            let dy = (max.y - min.y).max(10.0);
            let scale = (1000.0 / dx).min(600.0 / dy) * 0.8;
            self.document.view_mut().scale = scale as f64;
            self.document.view_mut().offset = grafito_geometry::Point2::new(-cx as f64 * scale as f64, cy as f64 * scale as f64);
        }
    }
}

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) { self.undo(); }
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
            || ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl) { self.redo(); }
        if ctx.input(|i| i.key_pressed(Key::Delete)) { self.delete_selected(); }
        if ctx.input(|i| i.key_pressed(Key::F1)) { self.current_tool = Tool::Select; self.tool_ghost = None; self.pending_points.clear(); }
        if ctx.input(|i| i.key_pressed(Key::F2)) { self.current_tool = Tool::Point; self.tool_ghost = None; }
        if ctx.input(|i| i.key_pressed(Key::F3)) { self.current_tool = Tool::Line; self.tool_ghost = None; self.pending_points.clear(); }
        if ctx.input(|i| i.key_pressed(Key::F4)) { self.current_tool = Tool::Circle; self.tool_ghost = None; self.pending_points.clear(); }
        if ctx.input(|i| i.key_pressed(Key::F5)) { self.current_tool = Tool::Polygon; self.tool_ghost = None; self.pending_points.clear(); }
        if ctx.input(|i| i.key_pressed(Key::F6)) { self.current_tool = Tool::Function; self.tool_ghost = None; }
        if ctx.input(|i| i.key_pressed(Key::Escape)) { self.current_tool = Tool::Select; self.tool_ghost = None; self.pending_points.clear(); }

        let is_dark = self.dark_mode;
        let accent   = Color32::from_rgb(100, 80, 200);
        let bar_fill  = if is_dark { Color32::from_rgb(40, 42, 54)  } else { Color32::WHITE };
        let side_fill = if is_dark { Color32::from_rgb(32, 34, 43)  } else { Color32::from_rgb(250,250,252) };
        let alg_fill  = if is_dark { Color32::from_rgb(25, 27, 36)  } else { Color32::WHITE };
        let sep_col   = if is_dark { Color32::from_gray(55)         } else { Color32::from_gray(225) };
        let txt_col   = if is_dark { Color32::WHITE                 } else { Color32::from_gray(30) };

        // ─── 1. TOP BAR (40px) ────────────────────────────────────────────────
        egui::TopBottomPanel::top("topbar")
            .exact_height(40.0)
            .frame(egui::Frame::none().fill(bar_fill)
                .stroke(egui::Stroke::new(1.0, sep_col))
                .inner_margin(egui::Margin::symmetric(12.0, 0.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let _ = ui.add(egui::Button::new(egui::RichText::new("☰").size(20.0).color(Color32::from_gray(150))).frame(false));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Grafito").color(accent).strong().size(16.0));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Suite Calculadora").color(Color32::from_gray(120)).size(13.0));
                    ui.add_space(10.0);
                    let pill_bg = Color32::from_gray(if is_dark { 55 } else { 238 });
                    egui::Frame::none().fill(pill_bg).rounding(16.0)
                        .inner_margin(egui::Margin::symmetric(10.0, 4.0)).show(ui, |ui| {
                        let is_3d = self.current_view == ViewMode::D3;
                        let text = if is_3d { "Gráficos 3D" } else { "Gráficos 2D" };
                        if ui.selectable_label(is_3d, egui::RichText::new(text).size(12.5)).clicked() {
                            self.current_view = if is_3d { ViewMode::D2 } else { ViewMode::D3 };
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        if ui.button("Pantalla").clicked() {
                            let is_fullscreen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                        }
                        ui.add_space(4.0);
                        if ui.button("Tema").clicked() {
                            self.dark_mode = !self.dark_mode;
                            if self.dark_mode { THEME_DARK.apply(ui.ctx()); } else { THEME_LIGHT.apply(ui.ctx()); }
                        }
                        ui.add_space(4.0);
                        ui.menu_button("Ajustes", |ui| {
                            ui.checkbox(&mut self.show_grid, "Mostrar Cuadrícula");
                            ui.checkbox(&mut self.snap_to_grid, "Ajustar a la Cuadrícula");
                            ui.checkbox(&mut self.exam_mode, "Modo Examen");
                        });
                        ui.add_space(4.0);
                        if ui.selectable_label(self.exam_mode, "Examen").clicked() {
                            self.exam_mode = !self.exam_mode;
                            if self.exam_mode { self.cas_result = "EXAM MODE: CAS disabled".into(); }
                        }
                    });
                });
            });

        // ─── 2. LEFT ICON SIDEBAR (44px, icons only with tooltips) ────────────
        egui::SidePanel::left("icon_bar")
            .exact_width(44.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(side_fill).stroke(egui::Stroke::new(1.0, sep_col)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    for (i, (icon, tip)) in [
                        ("A","Álgebra"),
                        ("T","Herramientas"),
                        ("#","Tabla"),
                        ("S","Hoja"),
                    ].iter().enumerate() {
                        let active = self.sidebar_tab == i;
                        let bg = if active { Color32::from_rgba_unmultiplied(100,80,200,35) } else { Color32::TRANSPARENT };
                        let ic = if active { accent } else { Color32::from_gray(130) };
                        let resp = ui.add_sized([44.0,44.0],
                            egui::Button::new(egui::RichText::new(*icon).size(20.0).color(ic))
                                .fill(bg).frame(false));
                        if resp.clicked() { 
                            if self.sidebar_tab == i && i != 0 {
                                self.sidebar_tab = 0;
                            } else {
                                self.sidebar_tab = i; 
                            }
                            // Sync spreadsheet visibility with tab 3
                            self.show_spreadsheet = self.sidebar_tab == 3;
                        }
                        resp.on_hover_text(*tip);
                        ui.add_space(2.0);
                    }
                });
            });

        // ─── 3. ALGEBRA PANEL ────────────────────────────────────────────────
        if self.sidebar_tab == 0 {
            egui::SidePanel::left("algebra_panel")
            .default_width(220.0)
            .min_width(160.0)
            .resizable(true)
            .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
            .show(ctx, |ui| {
                // Input row
                egui::Frame::none()
                    .fill(if is_dark { Color32::from_gray(33) } else { Color32::from_gray(248) })
                    .inner_margin(egui::Margin { left:8.0, right:8.0, top:6.0, bottom:6.0 })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                            ui.add_space(3.0);
                            let r = ui.add_sized(
                                [ui.available_width(), 22.0],
                                egui::TextEdit::singleline(&mut self.input_text)
                                    .hint_text("Entrada...")
                                    .frame(false)
                                    .text_color(txt_col));
                            self.preview_object = None;
                            if !self.input_text.is_empty() {
                                self.preview_object = commands::parse_preview(&self.input_text);
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                self.save_state();
                                self.cas_result = commands::process_input(&mut self.document, &mut self.input_text).unwrap_or_default();
                            }
                        });
                    });
                ui.add(egui::Separator::default().spacing(0.0));

                // ── Object list — compact, 1 line each ──────────────────────
                egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
                    let mut delete_id: Option<ObjectId> = None;
                    let ids: Vec<ObjectId> = self.document.objects_iter().map(|(id,_)| *id).collect();
                    for oid in ids {
                        let (obj_label, obj_name, obj_vis, obj_col, obj_expr) = {
                            let Some(obj) = self.document.get_object(oid) else { continue; };
                            
                            // Filter objects by current view mode
                            let is_3d_object = matches!(obj.name(),
                                "Point3D" | "Segment3D" | "Sphere3D" | "Cube3D" | "Pyramid3D" |
                                "Cone3D" | "Cylinder3D" | "Surface3D" | "ParametricCurve3D" |
                                "Attractor3D" | "HyperSurface4D" | "VectorField3D"
                            );
                            let is_3d_view = self.current_view == ViewMode::D3;
                            if is_3d_object != is_3d_view {
                                continue;
                            }
                            
                            let col = match obj.name() {
                                "Point"|"Point3D" => Color32::from_rgb(50,100,255),
                                "Line"            => Color32::from_rgb(90,110,130),
                                "Function"        => Color32::from_rgb(16,185,129),
                                _                 => Color32::from_rgb(239,68,68),
                            };
                            let expr = match obj {
                                grafito_core::GeoObject::Function(f) => f.expr.clone(),
                                grafito_core::GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
                                grafito_core::GeoObject::Line(l) => format!("({:.2}, {:.2}) ↔ ({:.2}, {:.2})", l.start.x, l.start.y, l.end.x, l.end.y),
                                grafito_core::GeoObject::Circle(c) => format!("(x - {:.2})² + (y - {:.2})² = {:.2}²", c.center.x, c.center.y, c.radius),
                                grafito_core::GeoObject::Ellipse(e) => format!("(x - {:.2})²/{:.2}² + (y - {:.2})²/{:.2}² = 1", e.center.x, e.center.y, e.rx, e.ry),
                                grafito_core::GeoObject::Polygon(p) => format!("{} vertices", p.vertices.len()),
                                grafito_core::GeoObject::Point3D(p) => format!("({:.2}, {:.2}, {:.2})", p.position.x, p.position.y, p.position.z),
                                grafito_core::GeoObject::Sphere3D(s) => format!("r={:.2} c=({:.2}, {:.2}, {:.2})", s.radius, s.center.x, s.center.y, s.center.z),
                                grafito_core::GeoObject::Cube3D(c) => format!("size={:.2}", c.size),
                                grafito_core::GeoObject::Segment3D(s) => format!("({:.2}, {:.2}, {:.2}) ↔ ({:.2}, {:.2}, {:.2})", s.a.x, s.a.y, s.a.z, s.b.x, s.b.y, s.b.z),
                                _ => String::new(),
                            };
                            (obj.label().to_string(), obj.name().to_string(), obj.is_visible(), col, expr)
                        };

                        let is_sel = self.selected_object == Some(oid);
                        let row_bg = if is_sel {
                            if is_dark { Color32::from_gray(42) } else { Color32::from_rgb(238,238,252) }
                        } else { Color32::TRANSPARENT };

                        let mut row_clicked = false;
                        egui::Frame::none()
                            .fill(row_bg)
                            .inner_margin(egui::Margin { left:8.0, right:4.0, top:5.0, bottom:5.0 })
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Right-side controls drawn first
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add_sized([24.0, 20.0], egui::Button::new("del").frame(false)).clicked() {
                                            delete_id = Some(oid);
                                        }
                                        if ui.add_sized([24.0, 20.0], egui::Button::new(if obj_vis { "ver" } else { "ocu" }).frame(false)).clicked() {
                                            if let Some(o) = self.document.get_object_mut(oid) {
                                                let v = o.is_visible(); o.set_visible(!v);
                                            }
                                        }
                                        
                                        // Left-side controls in remaining space
                                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            let dot_alpha = if obj_vis { 255u8 } else { 80u8 };
                                            let dot_col = Color32::from_rgba_unmultiplied(
                                                obj_col.r(), obj_col.g(), obj_col.b(), dot_alpha);
                                            let (dot_r, _) = ui.allocate_exact_size(egui::vec2(10.0,10.0), egui::Sense::hover());
                                            ui.painter().circle_filled(dot_r.center(), 5.0, dot_col);
                                            ui.add_space(5.0);
                                            
                                            let txt = if !obj_expr.is_empty() {
                                                format!("{}: {}", obj_label, obj_expr)
                                            } else {
                                                format!("{}: {}", obj_label, obj_name)
                                            };
                                            let lbl_resp = ui.add(egui::Label::new(
                                                egui::RichText::new(txt).size(13.0).color(txt_col)).sense(egui::Sense::click()).truncate());
                                            if lbl_resp.clicked() { row_clicked = true; }
                                            if lbl_resp.double_clicked() {
                                                if !obj_expr.is_empty() && obj_name == "Function" {
                                                    self.input_text = format!("{}={}", obj_label, obj_expr);
                                                } else if obj_name == "Point" {
                                                    self.input_text = format!("{}={}", obj_label, obj_expr);
                                                }
                                            }
                                        });
                                    });
                                });
                                
                                // Properties Panel (Inline)
                                if is_sel {
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.add_space(20.0); // indent
                                        ui.label("Color:");
                                        let obj_color = self.document.get_object(oid).map(|o| o.color()).unwrap_or_else(|| grafito_geometry::Color::new(1.0, 1.0, 1.0, 1.0));
                                        let color32 = Color32::from_rgba_unmultiplied(
                                            (obj_color.r * 255.0) as u8,
                                            (obj_color.g * 255.0) as u8,
                                            (obj_color.b * 255.0) as u8,
                                            (obj_color.a * 255.0) as u8,
                                        );
                                        let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(30.0, 20.0), Sense::click());
                                        let painter = ui.painter();
                                        painter.rect_filled(rect.translate(egui::Vec2::new(0.0, 1.0)), 4.0, Color32::from_black_alpha(40)); // shadow
                                        painter.rect_filled(rect, 4.0, color32);
                                        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, if resp.hovered() { Color32::WHITE } else { Color32::from_gray(120) }));
                                        
                                        if resp.clicked() {
                                            self.active_color_picker = Some((oid, grafito_ui::color_picker::HsvColorPicker::new(obj_color)));
                                        }
                                    });
                                }
                            });
                        
                        if row_clicked {
                            self.selected_object = if is_sel { None } else { Some(oid) };
                        }
                        
                        // Thin separator
                        let (sr, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                        ui.painter().hline(sr.x_range(), sr.center().y, egui::Stroke::new(0.5, sep_col));
                    }
                    if let Some(id) = delete_id {
                        self.document.remove_object(id);
                        if self.selected_object == Some(id) { self.selected_object = None; }
                    }

                    // Variables
                    if !self.document.variables.is_empty() {
                        ui.add_space(6.0);
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Variables").size(11.0).color(Color32::from_gray(130)));
                        ui.checkbox(&mut self.animation_running, egui::RichText::new("Animación").size(12.0));
                        let vars: Vec<(String,f64)> = self.document.variables.clone().into_iter().collect();
                        for (name, val) in &vars {
                            let mut v = *val;
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new(name).size(12.0));
                                let sl = egui::Slider::new(&mut v,-10.0..=10.0).step_by(0.1);
                                if ui.add_sized([100.0, 16.0], sl).changed() {
                                    self.document.set_variable(name.clone(), v);
                                }
                            });
                        }
                        if self.animation_running {
                            for (name,_) in &vars {
                                if let Some(v) = self.document.variables.get(name) {
                                    let nv = (v + 0.02) % 20.0 - 10.0;
                                    self.document.set_variable(name.clone(), nv);
                                }
                            }
                            ctx.request_repaint();
                        }
                    }
                });
            });
        } else if self.sidebar_tab == 1 {
            egui::SidePanel::left("tools_panel")
                .default_width(220.0)
                .min_width(160.0)
                .resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    grafito_ui::toolbar(ui, &mut self.current_tool, self.current_view == ViewMode::D3);
                });
        } else if self.sidebar_tab == 3 {
            egui::SidePanel::left("spreadsheet_panel")
                .default_width(220.0)
                .min_width(160.0)
                .resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(egui::RichText::new("La Hoja de Cálculo se muestra a la derecha.").color(Color32::from_gray(150)));
                    });
                });
        } else if self.sidebar_tab == 2 {
            egui::SidePanel::left("table_panel")
                .default_width(220.0)
                .min_width(160.0)
                .resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(egui::RichText::new("Tabla de Valores (Próximamente)\nUsa la Hoja de Cálculo (S) por ahora.").color(Color32::from_gray(150)));
                    });
                });
        } else {
            egui::SidePanel::left("empty_panel")
                .default_width(220.0)
                .min_width(160.0)
                .resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(egui::RichText::new("En construcción...").color(Color32::from_gray(150)));
                    });
                });
        }

        // ─── 4. MATH KEYBOARD — docked bottom panel (central area only) ──────────────
        egui::TopBottomPanel::bottom("math_keyboard")
            .min_height(180.0)
            .frame(egui::Frame::none()
                .fill(if is_dark { Color32::from_rgb(28,28,36) } else { Color32::from_rgb(244,245,250) })
                .stroke(egui::Stroke::new(1.0, sep_col)))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal_centered(|ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        // Tab bar
                        ui.horizontal(|ui| {
                            for (i, lbl) in ["123", "f(x)", "ABC", "3D"].iter().enumerate() {
                                let active = self.keyboard_tab == i;
                                let c = if active { accent } else { Color32::from_gray(110) };
                                let fbg = if active { Color32::from_rgba_unmultiplied(100,80,200,30) } else { Color32::TRANSPARENT };
                                let r = egui::Frame::none().fill(fbg).rounding(6.0)
                                    .inner_margin(egui::Margin::symmetric(8.0,3.0)).show(ui, |ui| {
                                    ui.label(egui::RichText::new(*lbl).size(12.0).color(c).strong());
                                }).response;
                                if ui.interact(r.rect, ui.id().with(i), egui::Sense::click()).clicked() {
                                    self.keyboard_tab = i;
                                }
                                ui.add_space(4.0);
                            }
                        });
                        ui.add_space(5.0);

                        let avail_w = ui.available_width();
                        let sp = 4.0_f32;
                        let btn_w = ((avail_w - (7.0 * sp) - 10.0) / 8.0).clamp(24.0, 65.0);
                        let total_w = (btn_w * 8.0) + (sp * 7.0);
                        let pad = ((avail_w - total_w) / 2.0).max(0.0);

                        macro_rules! kb {
                            ($ui:expr, $t:expr, $i:expr) => {{
                                let (r,resp) = $ui.allocate_exact_size(egui::vec2(btn_w, 32.0), egui::Sense::click());
                                if $ui.is_rect_visible(r) {
                                    let bg = if resp.hovered() {
                                        if is_dark { Color32::from_gray(70) } else { Color32::from_gray(215) }
                                    } else {
                                        if is_dark { Color32::from_gray(48) } else { Color32::WHITE }
                                    };
                                    $ui.painter().rect(r, 4.0, bg, egui::Stroke::new(1.0, Color32::from_gray(if is_dark {65} else {210})));
                                    $ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, $t,
                                        egui::FontId::proportional((btn_w * 0.4).clamp(10.0, 15.0)),
                                        if is_dark { Color32::WHITE } else { Color32::BLACK });
                                }
                                if resp.clicked() { self.input_text.push_str($i); }
                            }};
                        }

                        let key_rows: &[&[(&str,&str)]] = match self.keyboard_tab {
                            0 => &[
                                &[("x","x"),("y","y"),("π","π"),("e","e"),("7","7"),("8","8"),("9","9"),("/","/")],
                                &[("x²","^2"),("v/","sqrt("),("^","^"),("|","abs("),("4","4"),("5","5"),("6","6"),("*","*")],
                                &[("<","<"),(">",">"),("(", "("),(")",")"),("1","1"),("2","2"),("3","3"),("-","-")],
                            ],
                            1 => &[
                                &[("sin","sin("),("cos","cos("),("tan","tan("),("asin","asin("),("acos","acos("),("atan","atan("),("log","log("),("ln","ln(")],
                                &[("sec","sec("),("csc","csc("),("cot","cot("),("!","!"),("deg","deg"),("rad","rad"),("f","f"),("g","g")],
                                &[("<","<"),(">",">"),("(", "("),(")",")"),("1","1"),("2","2"),("3","3"),("-","-")],
                            ],
                            2 => &[
                                &[("q","q"),("w","w"),("e","e"),("r","r"),("t","t"),("y","y"),("u","u"),("i","i")],
                                &[("a","a"),("s","s"),("d","d"),("f","f"),("g","g"),("h","h"),("j","j"),("k","k")],
                                &[("z","z"),("x","x"),("c","c"),("v","v"),("b","b"),("n","n"),("m","m"),(",","")],
                            ],
                            _ => &[
                                &[("Lor","Lorenz[10, 28, 2.66]"),("Roe","Rossler[0.2, 0.2, 5.7]"),("Aiz","Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]"),("Rab","Dadras[3, 2.7, 1.7, 2, 9]"),("Sph","Sphere[0,0,0,5]"),("Cub","Cube[0,0,0,5]"),("P3D","Point3D[1,1,1]"),("S3D","Segment3D[0,0,0,1,1,1]")],
                                &[("Hal","Halvorsen[2.0]"),("Tho","Thomas[0.208186]"),("Che","Chen[35, 3, 28]"),("Spr","Chua[15.6, 28, -1.14, -0.71]"),("Cyl","Cylinder[0,0,0,2,5]"),("Con","Cone[0,0,0,3,5]"),("Tor","Torus[0,0,0,4,1]"),("Moe","Moebius[2,1]")],
                                &[("<","<"),(">",">"),("(", "("),(")",")"),("[","["),("]","]"),("{","{"),("}","}")],
                            ],
                        };
                        for row in key_rows {
                            ui.horizontal(|ui| {
                                ui.add_space(pad);
                                for (t, i) in *row { kb!(ui, *t, *i); ui.add_space(sp); }
                            });
                            ui.add_space(sp);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(pad);
                            kb!(ui,"ans","ans"); ui.add_space(sp);
                            kb!(ui,".",".");   ui.add_space(sp);
                            kb!(ui,"0","0");   ui.add_space(sp);
                            kb!(ui,"(","(");   ui.add_space(sp);
                            kb!(ui,")",")");   ui.add_space(sp);
                            kb!(ui,"=","=");   ui.add_space(sp);
                            // Backspace
                            { let (r,resp)=ui.allocate_exact_size(egui::vec2(btn_w, 32.0),egui::Sense::click());
                              let bg=if resp.hovered(){Color32::from_rgb(220,60,60)}else{Color32::from_gray(if is_dark{48}else{230})};
                              ui.painter().rect(r,4.0,bg,egui::Stroke::new(1.0,Color32::from_gray(if is_dark{65}else{210})));
                              ui.painter().text(r.center(),egui::Align2::CENTER_CENTER,"Del",egui::FontId::proportional(14.0),if is_dark{Color32::WHITE}else{Color32::BLACK});
                              if resp.clicked(){self.input_text.pop();} }
                            ui.add_space(sp);
                            // Enter
                            { let (r,resp)=ui.allocate_exact_size(egui::vec2(btn_w, 32.0),egui::Sense::click());
                              let bg=if resp.hovered(){Color32::from_rgb(120,100,240)}else{Color32::from_rgb(100,80,200)};
                              ui.painter().rect(r,4.0,bg,egui::Stroke::NONE);
                              ui.painter().text(r.center(),egui::Align2::CENTER_CENTER,"Enter",egui::FontId::proportional(13.0),Color32::WHITE);
                              if resp.clicked(){self.save_state();self.cas_result=commands::process_input(&mut self.document,&mut self.input_text).unwrap_or_default();} }
                        });
                        ui.add_space(12.0);
                    });
                });
            });

        // ─── 5. SPREADSHEET (optional right panel) ────────────────────────────
        if self.show_spreadsheet {
            egui::SidePanel::right("spreadsheet")
                .resizable(true).default_width(280.0)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.heading("Hoja de Cálculo");
                    ui.separator();
                    let (rows, cols) = self.document.spreadsheet_dim();
                    let text_col = if is_dark { Color32::WHITE } else { Color32::BLACK };
                    let hdr_col = if is_dark { Color32::from_gray(160) } else { Color32::from_gray(80) };

                    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                        egui::Grid::new("sp_grid")
                            .min_col_width(52.0)
                            .spacing(egui::vec2(1.0, 1.0))
                            .striped(true)
                            .show(ui, |ui| {
                            // Header row
                            ui.label(""); // corner
                            for c in 0..cols {
                                let letter = if c < 26 { format!("{}", (b'A' + c as u8) as char) } else { format!("{}", c + 1) };
                                ui.centered_and_justified(|ui| {
                                    ui.label(egui::RichText::new(letter).monospace().strong().color(hdr_col));
                                });
                            }
                            ui.end_row();

                            // Data rows
                            for r in 0..rows {
                                ui.label(egui::RichText::new(format!("{}", r + 1)).monospace().strong().color(hdr_col));
                                for c in 0..cols {
                                    let mut val = self.document.get_spreadsheet_cell(r, c);
                                    let resp = ui.add_sized([52.0, 18.0],
                                        egui::TextEdit::singleline(&mut val)
                                            .font(egui::TextStyle::Monospace)
                                            .text_color(text_col)
                                            .horizontal_align(egui::Align::Center)
                                    );
                                    if resp.changed() {
                                        self.save_state();
                                        self.document.set_spreadsheet_cell(r, c, val.clone());
                                        if let Ok((x, y)) = commands::parse_point_str(&val) {
                                            self.document.add_object(GeoObject::Point(
                                                PointObj::new(Point2::new(x, y)).with_label(
                                                    format!("{}{}", (b'A' + c as u8) as char, r + 1))));
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
        }

        // ─── 6. CENTRAL CANVAS ───────────────────────────────────────────────
        match self.current_view {
            ViewMode::D2 => {
                self.camera.aspect = 1.6;
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(if is_dark { Color32::from_gray(18) } else { Color32::WHITE }))
                    .show(ctx, |ui| {
                        let canvas_rect = ui.available_rect_before_wrap();
                        self.handle_canvas_input(ui, canvas_rect);

                        // Compact canvas controls — top-right corner, inside canvas
                        let ctrl_x = canvas_rect.right() - 44.0;
                        let ctrl_y = canvas_rect.top() + 8.0;
                        let painter = ui.painter();
                        // Zoom-fit button
                        let zf_rect = egui::Rect::from_min_size(
                            egui::pos2(ctrl_x, ctrl_y), egui::vec2(34.0, 28.0));
                        painter.rect(zf_rect, 4.0, Color32::from_rgba_unmultiplied(255,255,255,200),
                            egui::Stroke::new(1.0, Color32::from_gray(200)));
                        painter.text(zf_rect.center(), egui::Align2::CENTER_CENTER, "[ ]",
                            egui::FontId::proportional(16.0), Color32::from_gray(60));
                        if ui.interact(zf_rect, ui.id().with("zf"), egui::Sense::click())
                            .on_hover_text("Ajustar Vista")
                            .clicked() {
                            self.zoom_to_fit();
                        }

                        let mut painter = ui.painter().clone();
                        painter.set_clip_rect(canvas_rect);
                        self.draw_grid(&painter, canvas_rect);
                        self.draw_axes(&painter, canvas_rect);
                        self.draw_objects(&painter, canvas_rect);
                        self.draw_tool_ghost(&painter, canvas_rect);

                        if let Some(preview) = &self.preview_object {
                            match preview {
                                GeoObject::Function(fun) => {
                                    let mut f = fun.clone(); f.color = Color::new(0.5,0.5,0.5,0.6);
                                    self.draw_object(&painter, canvas_rect, &GeoObject::Function(f));
                                }
                                GeoObject::Point(p) => {
                                    let mut pt = p.clone(); pt.color = Color::new(0.5,0.5,0.5,0.6);
                                    self.draw_object(&painter, canvas_rect, &GeoObject::Point(pt));
                                }
                                _ => {}
                            }
                        }
                    });
            }
            ViewMode::D3 => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas_rect = ui.available_rect_before_wrap();
                    let w = canvas_rect.width(); let h = canvas_rect.height();
                    self.camera.aspect = w / h.max(1.0);
                    let ctx_resp = ui.interact(canvas_rect, ui.id().with("ctx3d"), Sense::click());
                    if ctx_resp.clicked_by(egui::PointerButton::Secondary) {
                        ctx_resp.context_menu(|ui| {
                            if ui.button("Borrar selección").clicked() { self.delete_selected(); ui.close_menu(); }
                            if ui.button("Reiniciar vista").clicked() { self.camera = Camera3D::new(w/h.max(1.0)); ui.close_menu(); }
                        });
                    }
                    let response = ui.interact(canvas_rect, ui.id().with("canvas3d"), Sense::click_and_drag());
                    if let Some(pos) = response.hover_pos() {
                        if response.dragged_by(egui::PointerButton::Secondary) {
                            if let Some(last) = self.last_mouse_pos {
                                self.camera.orbit((pos.x-last.x)*0.005, (pos.y-last.y)*0.005);
                            }
                        }
                        if response.dragged_by(egui::PointerButton::Primary) {
                            if let Some(last) = self.last_mouse_pos {
                                if self.current_tool == Tool::Select { self.camera.pan(pos.x-last.x, pos.y-last.y); }
                            }
                        }
                        if response.hovered() {
                            let sc = ui.input(|i| i.smooth_scroll_delta);
                            if sc.y != 0.0 { self.camera.zoom(1.0 + sc.y*0.005); }
                        }
                        
                        // Tool ghost for 3D mode
                        self.tool_ghost = None;
                        if matches!(self.current_tool, Tool::Point3D | Tool::Sphere3D | Tool::Cube3D) {
                            let t = self.camera.target;
                            let ghost_pos = Point3D::new(t.x as f64, t.y as f64, t.z as f64);
                            self.tool_ghost = Some(GeoObject::Point3D(Point3DObj::new(ghost_pos)));
                        }
                        
                        self.last_mouse_pos = Some(pos);
                    }
                    if (response.clicked_by(egui::PointerButton::Primary) 
                        || response.drag_stopped_by(egui::PointerButton::Primary))
                        && self.current_tool != Tool::Select {
                        self.handle_3d_click(ui, &response, canvas_rect, w, h);
                        self.tool_ghost = None;
                    }
                    self.draw_3d_grid(ui.painter(), canvas_rect, w, h);
                    self.draw_3d_objects(ui.painter(), canvas_rect, w, h);
                    
                    // Draw 3D tool ghost
                    if let Some(GeoObject::Point3D(ghost)) = &self.tool_ghost {
                        let painter = ui.painter();
                        let origin = canvas_rect.min;
                        if let Some(pt) = self.camera.project(&ghost.position, w, h) {
                            let pos = origin + Vec2::new(pt.0, pt.1);
                            // Render ghost with reduced opacity
                            let ghost_color = Color32::from_rgba_premultiplied(
                                (ghost.color.r * 255.0) as u8,
                                (ghost.color.g * 255.0) as u8,
                                (ghost.color.b * 255.0) as u8,
                                80, // ~30% opacity
                            );
                            painter.circle_filled(pos, ghost.size.min(8.0) * 1.3, ghost_color);
                            painter.circle_stroke(pos, ghost.size.min(8.0) * 1.3, 
                                egui::Stroke::new(1.5, Color32::from_rgba_premultiplied(100, 150, 255, 120)));
                        }
                    }
                });
            }
        }

        if let Some((oid, mut picker)) = self.active_color_picker.clone() {
            let mut keep_open = true;
            
            // Adjust the window design to be centered and not ugly
            egui::Window::new("🎨 Selector de Color")
                .collapsible(false)
                .resizable(false)
                .default_width(320.0)
                .fixed_size([320.0, 300.0])
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    if picker.show(ui, &mut self.color_favorites) {
                        if let Some(o) = self.document.get_object_mut(oid) {
                            o.set_color(picker.to_color());
                        }
                        ctx.request_repaint();
                    }
                });

            if keep_open {
                self.active_color_picker = Some((oid, picker));
            } else {
                self.active_color_picker = None;
            }
        }
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
