//! Grafito Desktop Application — Main entry point and eframe UI orchestrator.

pub use grafito_command as commands;
#[cfg(not(target_os = "android"))]
pub mod export;
pub mod render_2d;
pub mod render_3d;
pub mod gpu_canvas;
pub mod tool_dispatcher;

#[cfg(target_os = "android")]
pub mod android;

use egui::{Color32, Key, Pos2, Sense, Vec2};
use grafito_core::{
    CircleObj, Cube3DObj, Document, EllipseObj, FunctionObj, GeoObject, LineObj, ObjectId,
    Point3DObj, PointObj, PolygonObj, Sphere3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use grafito_ui::theme::{DARK as THEME_DARK, LIGHT as THEME_LIGHT};
use grafito_ui::Tool;

const MAX_UNDO: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    D2,
    D3,
}

#[allow(dead_code)]
fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
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
    pub keyboard_visible: bool,
    pub table_func_idx: usize,
    pub table_x_min: String,
    pub table_x_max: String,
    pub table_step: String,
    pub cas_history: Vec<String>,
    pub sidebar_tab: usize,
    pub recent_files: Vec<String>,
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<Document>,
    pub attractor_cache: std::collections::HashMap<ObjectId, (u64, Vec<Point3D>)>,
    pub active_color_picker: Option<(ObjectId, grafito_ui::color_picker::HsvColorPicker)>,
    pub color_favorites: [grafito_geometry::Color; 5],
    pub tool_ghost: Option<GeoObject>,
    pub tool_state: crate::tool_dispatcher::ToolState,
    pub gpu_resources: Option<std::sync::Arc<std::sync::RwLock<crate::gpu_canvas::GpuCanvasResources>>>,
    pub use_gpu: bool,
}

impl GrafitoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gpu_resources = cc.wgpu_render_state.as_ref().map(|rs| {
            let renderer = grafito_render::Renderer::new(&rs.device, rs.target_format, false);
            std::sync::Arc::new(std::sync::RwLock::new(crate::gpu_canvas::GpuCanvasResources {
                renderer: std::sync::Arc::new(std::sync::RwLock::new(renderer)),
                buffers_2d: None,
                buffers_3d: None,
            }))
        });
        let mut document = Document::new();
        document.set_view(ViewTransform::new(1280.0, 720.0));
        document.view_mut().scale = 50.0; // ~13 units each side — matches GeoGebra default zoom
        document.add_object(GeoObject::Point(
            PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
        ));
        document.add_object(GeoObject::Point(
            PointObj::new(Point2::new(3.0, 2.0)).with_label("B"),
        ));
        document.add_object(GeoObject::Line(
            LineObj::new(Point2::new(-2.0, -1.0), Point2::new(4.0, 3.0)).with_label("l"),
        ));
        document.add_object(GeoObject::Circle(
            CircleObj::new(Point2::new(1.0, 1.0), 2.0).with_label("c"),
        ));
        document.add_object(GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(-3.0, -2.0),
            Point2::new(-1.0, -3.0),
            Point2::new(-2.0, -1.0),
        ])));
        document.add_object(GeoObject::Function(
            FunctionObj::new("sin(x)").with_label("f(x)"),
        ));
        document.set_variable("a".into(), 2.0);
        document.add_object(GeoObject::Function(
            FunctionObj::new("a*sin(x)").with_label("g(x)"),
        ));
        document.add_object(GeoObject::Cube3D(
            Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0).with_label("C1"),
        ));
        document.add_object(GeoObject::Sphere3D(
            Sphere3DObj::new(Point3D::new(2.0, 1.0, 0.0), 1.0).with_label("S1"),
        ));
        document.add_object(GeoObject::Ellipse(
            EllipseObj::new(Point2::new(-1.0, -2.0), 2.0, 1.0).with_label("E1"),
        ));

        let config = load_config();
        let dark_mode = config.dark_mode;
        if dark_mode {
            THEME_DARK.apply(&cc.egui_ctx);
        } else {
            THEME_LIGHT.apply(&cc.egui_ctx);
        }

        Self {
            document,
            current_tool: Tool::default(),
            current_view: ViewMode::D2,
            camera: Camera3D::new(1280.0 / 720.0),
            animation_running: false,
            show_grid: config.show_grid,
            snap_to_grid: config.snap_to_grid,
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
            keyboard_visible: true,
            table_func_idx: 0,
            table_x_min: "-5".to_string(),
            table_x_max: "5".to_string(),
            table_step: "1.0".to_string(),
            cas_history: Vec::new(),
            sidebar_tab: 0,
            recent_files: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            attractor_cache: std::collections::HashMap::new(),
            active_color_picker: None,
            tool_ghost: None,
            tool_state: crate::tool_dispatcher::ToolState::default(),
            gpu_resources,
            use_gpu: false,
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

    #[cfg(not(target_os = "android"))]
    fn save_to_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Grafito Document", &["json"])
            .save_file()
        {
            let path_str = path.to_string_lossy().to_string();
            if let Err(e) = crate::export::save_document(&self.document, &path_str) {
                log::error!("Save failed: {}", e);
            }
            if self.recent_files.len() >= 10 {
                self.recent_files.remove(0);
            }
            self.recent_files.push(path_str);
        }
    }

    #[cfg(not(target_os = "android"))]
    fn load_from_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Grafito Document", &["json"])
            .pick_file()
        {
            let path_str = path.to_string_lossy().to_string();
            match crate::export::load_document(&path_str) {
                Ok(doc) => {
                    self.document = doc;
                    self.undo_stack.clear();
                    self.redo_stack.clear();
                    self.selected_object = None;
                    if self.recent_files.len() >= 10 {
                        self.recent_files.remove(0);
                    }
                    self.recent_files.push(path_str);
                }
                Err(e) => log::error!("Load failed: {}", e),
            }
        }
    }

    #[cfg(target_os = "android")]
    fn save_to_file(&mut self) {}

    #[cfg(target_os = "android")]
    fn load_from_file(&mut self) {}

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
            if !obj.is_visible() {
                continue;
            }
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
                        min.x = min.x.min(pt.x);
                        min.y = min.y.min(pt.y);
                        max.x = max.x.max(pt.x);
                        max.y = max.y.max(pt.y);
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
            self.document.view_mut().scale = scale;
            self.document.view_mut().offset =
                grafito_geometry::Point2::new(-cx * scale, cy * scale);
        }
    }
}

fn configure_modern_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Smooth corners everywhere
    style.visuals.window_rounding = 8.0.into();
    style.visuals.menu_rounding = 8.0.into();
    style.visuals.widgets.noninteractive.rounding = 6.0.into();
    style.visuals.widgets.inactive.rounding = 6.0.into();
    style.visuals.widgets.hovered.rounding = 6.0.into();
    style.visuals.widgets.active.rounding = 6.0.into();

    // Spacing so it doesn't look cramped
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 16.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(40),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(40),
    };

    ctx.set_style(style);
}

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        configure_modern_style(ctx);
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
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.current_tool = Tool::Select;
            self.tool_ghost = None;
            self.pending_points.clear();
        }
        if ctx.input(|i| i.key_pressed(Key::F2)) {
            self.current_tool = Tool::Point;
            self.tool_ghost = None;
        }
        if ctx.input(|i| i.key_pressed(Key::F3)) {
            self.current_tool = Tool::Line;
            self.tool_ghost = None;
            self.pending_points.clear();
        }
        if ctx.input(|i| i.key_pressed(Key::F4)) {
            self.current_tool = Tool::Circle;
            self.tool_ghost = None;
            self.pending_points.clear();
        }
        if ctx.input(|i| i.key_pressed(Key::F5)) {
            self.current_tool = Tool::Polygon;
            self.tool_ghost = None;
            self.pending_points.clear();
        }
        if ctx.input(|i| i.key_pressed(Key::F6)) {
            self.current_tool = Tool::Function;
            self.tool_ghost = None;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.current_tool = Tool::Select;
            self.tool_ghost = None;
            self.pending_points.clear();
        }
        // Log axis toggles: Shift+L = X, Shift+K = Y, Shift+J = both
        if ctx.input(|i| i.key_pressed(Key::L) && i.modifiers.shift) {
            self.document.view_mut().x_log = !self.document.view().x_log;
        }
        if ctx.input(|i| i.key_pressed(Key::K) && i.modifiers.shift) {
            self.document.view_mut().y_log = !self.document.view().y_log;
        }
        if ctx.input(|i| i.key_pressed(Key::J) && i.modifiers.shift) {
            let v = self.document.view_mut();
            let both = !v.x_log || !v.y_log;
            v.x_log = both;
            v.y_log = both;
        }
        // Ctrl+S: save, Ctrl+O: load
        if ctx.input(|i| i.key_pressed(Key::S) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.save_to_file();
        }
        if ctx.input(|i| i.key_pressed(Key::O) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.load_from_file();
        }
        // Ctrl+Shift+S: save app config
        if ctx.input(|i| i.key_pressed(Key::S) && i.modifiers.ctrl && i.modifiers.shift) {
            save_config(&AppConfig {
                dark_mode: self.dark_mode,
                show_grid: self.show_grid,
                snap_to_grid: self.snap_to_grid,
            });
        }

type FuncInfo = (String, String, String, Option<f64>, Option<f64>);

        let is_dark = self.dark_mode;
        let accent = Color32::from_rgb(53, 132, 228);  // GNOME blue
        let bar_fill = if is_dark { Color32::from_rgb(36, 36, 36) } else { Color32::WHITE };
        let side_fill = if is_dark { Color32::from_rgb(30, 30, 38) } else { Color32::from_rgb(250, 250, 252) };
        let alg_fill = if is_dark { Color32::from_rgb(24, 26, 34) } else { Color32::from_rgb(248, 249, 252) };
        let sep_col = if is_dark { Color32::from_rgb(55, 55, 60) } else { Color32::from_rgb(175, 175, 180) };
        let txt_col = if is_dark { Color32::WHITE } else { Color32::from_rgb(26, 26, 26) };
        let txt_dim = if is_dark { Color32::from_gray(140) } else { Color32::from_gray(110) };

        // ── MENU BAR + QUICK CONTROLS ──
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(32.0)
            .frame(egui::Frame::none().fill(bar_fill).inner_margin(egui::Margin::symmetric(8.0, 4.0)))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Archivo", |ui| {
                        if ui.button("Nuevo").clicked() { self.document.clear(); }
                        if ui.button("Abrir (Ctrl+O)").clicked() { self.load_from_file(); }
                        if ui.button("Guardar (Ctrl+S)").clicked() { self.save_to_file(); }
                        ui.separator();
                        if ui.button("Salir").clicked() { std::process::exit(0); }
                    });
                    ui.menu_button("Editar", |ui| {
                        if ui.button("Deshacer (Ctrl+Z)").clicked() { self.undo(); }
                        if ui.button("Rehacer (Ctrl+Y)").clicked() { self.redo(); }
                        if ui.button("Eliminar (Supr)").clicked() { self.delete_selected(); }
                    });
                    ui.menu_button("Vista", |ui| {
                        ui.checkbox(&mut self.show_grid, "Mostrar cuadrícula");
                        ui.checkbox(&mut self.dark_mode, "Modo oscuro").clicked().then(|| {
                            if self.dark_mode { THEME_DARK.apply(ui.ctx()); } else { THEME_LIGHT.apply(ui.ctx()); }
                        });
                        ui.checkbox(&mut self.snap_to_grid, "Ajustar a cuadrícula").changed();
                        ui.separator();
                        let mut is_3d = self.current_view == ViewMode::D3;
                        if ui.checkbox(&mut is_3d, "Vista 3D").changed() {
                            self.current_view = if is_3d { ViewMode::D3 } else { ViewMode::D2 };
                        }
                        ui.checkbox(&mut self.exam_mode, "Modo examen");
                        ui.checkbox(&mut self.document.view_mut().x_log, "Eje X log");
                        ui.checkbox(&mut self.document.view_mut().y_log, "Eje Y log");
                    });
                    ui.menu_button("Herramientas", |ui| {
                        ui.checkbox(&mut self.keyboard_visible, "Teclado visible");
                    });
                    ui.menu_button("Ayuda", |ui| {
                        if ui.button("Acerca de Grafito v0.9.0-alpha").clicked() {}
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Grafito").color(accent).strong().size(14.0));
                        ui.add_space(4.0);
                        if ui.add(egui::Button::new(if self.dark_mode { "Tema Claro" } else { "Tema Oscuro" }).frame(false)).clicked() {
                            self.dark_mode = !self.dark_mode;
                            if self.dark_mode { THEME_DARK.apply(ui.ctx()); } else { THEME_LIGHT.apply(ui.ctx()); }
                        }
                    });
                });
            });

        // ── TOOLBAR (horizontal, with dropdown groups) ──
        egui::TopBottomPanel::top("toolbar_panel")
            .exact_height(38.0)
            .frame(egui::Frame::none().fill(side_fill))
            .show(ctx, |ui| {
                grafito_ui::toolbar::toolbar(ui, &mut self.current_tool, self.current_view == ViewMode::D3);
            });

        // ── LEFT SIDEBAR (56px, labeled tabs) ──
        let tabs: &[(&str, &str, &str)] = &[
            ("Álgebra", "∑", "Objetos, variables, comandos"),
            ("CAS", "⌨", "Cálculo simbólico paso a paso"),
            ("Tabla", "☰", "Valores numéricos x|f(x)"),
            ("Hoja", "⊞", "Hoja de cálculo"),
            ("Vista", "◎", "Cuadrícula, ejes, etiquetas"),
        ];
        egui::SidePanel::left("icon_bar")
            .exact_width(52.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(side_fill).stroke(egui::Stroke::new(1.0, sep_col)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    for (i, (label, icon, tip)) in tabs.iter().enumerate() {
                        let active = self.sidebar_tab == i;
                        let bg = if active { Color32::from_rgba_unmultiplied(53, 132, 228, 50) } else { Color32::TRANSPARENT };
                        let ic = if active { accent } else { Color32::from_gray(130) };
                        
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(46.0, 48.0), egui::Sense::click());
                        if ui.is_rect_visible(rect) {
                            ui.painter().rect_filled(rect, 6.0, bg);
                            // Draw the icon
                            ui.painter().text(
                                rect.center() - egui::vec2(0.0, 6.0),
                                egui::Align2::CENTER_CENTER,
                                *icon,
                                egui::FontId::proportional(16.0),
                                ic
                            );
                            // Draw the text
                            ui.painter().text(
                                rect.center() + egui::vec2(0.0, 12.0),
                                egui::Align2::CENTER_CENTER,
                                *label,
                                egui::FontId::proportional(9.0),
                                ic
                            );
                        }
                        
                        if resp.clicked() {
                            self.sidebar_tab = i;
                            if i == 3 {
                                self.show_spreadsheet = false; // Never auto-open right panel when switching to Hoja
                            }
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
                                if !self.cas_result.is_empty() {
                                    if self.cas_history.len() > 20 { self.cas_history.remove(0); }
                                    self.cas_history.push(format!("> {}\n  {}", self.input_text, self.cas_result));
                                }
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
                                "Cone3D" | "Cylinder3D" | "Torus3D" | "MoebiusStrip" |
                                "Surface3D" | "ParametricCurve3D" |
                                "Attractor3D" | "HyperSurface4D" | "VectorField3D"
                            );
                            let is_3d_view = self.current_view == ViewMode::D3;
                            if is_3d_object != is_3d_view {
                                continue;
                            }

                            let o_col = obj.color();
                            let col = Color32::from_rgba_unmultiplied(
                                (o_col.r * 255.0) as u8,
                                (o_col.g * 255.0) as u8,
                                (o_col.b * 255.0) as u8,
                                255,
                            );
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
                        let frame_fill = if is_sel {
                            if is_dark { Color32::from_rgba_unmultiplied(94, 139, 255, 40) } else { Color32::from_rgba_unmultiplied(38, 99, 255, 30) }
                        } else {
                            if is_dark { Color32::from_gray(30) } else { Color32::from_rgb(255, 255, 255) }
                        };
                        let border = if is_sel {
                            egui::Stroke::new(1.0, if is_dark { Color32::from_rgb(94, 139, 255) } else { Color32::from_rgb(38, 99, 255) })
                        } else {
                            egui::Stroke::new(1.0, if is_dark { Color32::from_gray(40) } else { Color32::from_rgb(230, 230, 235) })
                        };

                        let mut row_clicked = false;
                        ui.add_space(4.0);
                        egui::Frame::none()
                            .fill(frame_fill)
                            .rounding(8.0)
                            .stroke(border)
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Right-side controls drawn first
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add_sized([28.0, 24.0], egui::Button::new("🗑").frame(false)).on_hover_text("Eliminar").clicked() {
                                            delete_id = Some(oid);
                                        }
                                        let eye = if obj_vis { "👁" } else { "Ø" };
                                        if ui.add_sized([28.0, 24.0], egui::Button::new(eye).frame(false)).on_hover_text("Visibilidad").clicked() {
                                            if let Some(o) = self.document.get_object_mut(oid) {
                                                let v = o.is_visible(); o.set_visible(!v);
                                            }
                                        }

                                        // Left-side controls in remaining space
                                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            let dot_alpha = if obj_vis { 255u8 } else { 80u8 };
                                            let dot_col = Color32::from_rgba_unmultiplied(
                                                obj_col.r(), obj_col.g(), obj_col.b(), dot_alpha);
                                            let (dot_r, dot_resp) = ui.allocate_exact_size(egui::vec2(12.0,12.0), egui::Sense::click());
                                            ui.painter().circle_filled(dot_r.center(), 6.0, dot_col);
                                            if dot_resp.hovered() {
                                                ui.painter().circle_stroke(dot_r.center(), 6.0, egui::Stroke::new(1.0, Color32::WHITE));
                                            }
                                            let dot_resp = dot_resp.on_hover_text("Cambiar color");
                                            if dot_resp.clicked() {
                                                let obj_color = self.document.get_object(oid).map(|o| o.color()).unwrap_or_else(|| grafito_geometry::Color::new(1.0, 1.0, 1.0, 1.0));
                                                self.active_color_picker = Some((oid, grafito_ui::color_picker::HsvColorPicker::new(obj_color)));
                                                row_clicked = true;
                                            }
                                            ui.add_space(5.0);

                                            let txt = if !obj_expr.is_empty() {
                                                format!("{}: {}", obj_label, obj_expr)
                                            } else {
                                                format!("{}: {}", obj_label, obj_name)
                                            };
                                            let lbl_resp = ui.add(egui::Label::new(
                                                egui::RichText::new(txt).size(13.0).color(txt_col)).sense(egui::Sense::click()).truncate());
                                            if lbl_resp.clicked() { row_clicked = true; }
                                            if lbl_resp.double_clicked() && !obj_expr.is_empty() && (obj_name == "Function" || obj_name == "Point") {
                                                self.input_text = format!("{}={}", obj_label, obj_expr);
                                            }
                                        });
                                    });
                                });

                                // Properties Panel (Inline)
                                if is_sel {
                                    // Property sliders
                                    if let Some(obj) = self.document.get_object_mut(oid) {
                                        ui.add_space(2.0);
                                        match obj {
                                            GeoObject::Line(l) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut l.width, 0.5..=10.0).trailing_fill(true));
                                                });
                                            }
                                            GeoObject::Circle(c) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut c.width, 0.5..=10.0).trailing_fill(true));
                                                });
                                            }
                                            GeoObject::Function(f) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut f.width, 0.5..=10.0).trailing_fill(true));
                                                });
                                            }
                                            GeoObject::Point(p) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("●").size(10.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                                });
                                            }
                                            GeoObject::Point3D(p) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("●").size(10.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                                });
                                            }
                                            GeoObject::Polygon(poly) => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                    ui.add(egui::Slider::new(&mut poly.width, 0.5..=10.0).trailing_fill(true));
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            });

                        if row_clicked {
                            self.selected_object = if is_sel { None } else { Some(oid) };
                        }
                        ui.add_space(2.0);
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
            // ── CAS PANEL (tab 1) ──
            egui::SidePanel::left("cas_panel")
                .default_width(260.0).min_width(180.0).resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Cálculo Simbólico (CAS)").color(accent).strong().size(16.0));
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    
                    egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Derivar").clicked() { self.input_text = "Derivative[".to_string(); }
                            if ui.button("Integrar").clicked() { self.input_text = "Integral[".to_string(); }
                            if ui.button("Resolver").clicked() { self.input_text = "Solve[".to_string(); }
                            if ui.button("Límite").clicked() { self.input_text = "Limit[".to_string(); }
                        });
                    });
                    ui.separator();
                    // CAS Input
                    ui.label(egui::RichText::new("Entrada CAS:").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut execute_cas = false;
                        if ui.add_sized([28.0, 24.0], egui::Button::new("▶")).clicked() {
                            execute_cas = true;
                        }
                        
                        let r = ui.add_sized(
                            [ui.available_width(), 24.0],
                            egui::TextEdit::singleline(&mut self.input_text)
                                .hint_text("Comando CAS...")
                        );
                        
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            execute_cas = true;
                        }
                        
                        if execute_cas && !self.input_text.is_empty() {
                            self.save_state();
                            let mut cmd = self.input_text.clone();
                            self.cas_result = crate::commands::process_input(&mut self.document, &mut cmd).unwrap_or_default();
                            if !self.cas_result.is_empty() {
                                if self.cas_history.len() > 20 { self.cas_history.remove(0); }
                                self.cas_history.push(format!("> {}\n  {}", self.input_text, self.cas_result));
                            }
                            self.input_text.clear();
                        }
                    });
                    
                    // Show CAS history
                    egui::ScrollArea::vertical().max_height(ui.available_height() - 8.0).show(ui, |ui| {
                        egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                            if self.cas_history.is_empty() {
                                ui.label(egui::RichText::new("Escribe comandos CAS...\n\nEj: Derivative[x², x]\nEj: Integral[sin(x), x]\nEj: Solve[x²-4, x]\nEj: Limit[sin(x)/x, x, 0]").size(12.0).color(txt_dim));
                            } else {
                                for (i, entry) in self.cas_history.iter().enumerate() {
                                    egui::Frame::none()
                                        .fill(if self.dark_mode { Color32::from_rgb(45, 45, 50) } else { Color32::from_rgb(240, 240, 245) })
                                        .rounding(6.0)
                                        .inner_margin(8.0)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new(format!("{}", i+1)).color(accent).strong());
                                                ui.add_space(4.0);
                                                ui.label(egui::RichText::new(entry).size(13.0).color(txt_col));
                                            });
                                        });
                                    ui.add_space(6.0);
                                }
                            }
                        });
                    });
                });
        } else if self.sidebar_tab == 4 {
            // ── VIEW/SETTINGS PANEL (tab 4) ──
            egui::SidePanel::left("view_panel")
                .default_width(220.0).min_width(160.0).resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Vista").color(accent).strong().size(16.0));
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    
                    egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                        ui.label(egui::RichText::new("General").color(txt_dim).size(11.0).strong());
                        ui.add_space(4.0);
                        ui.checkbox(&mut self.show_grid, "Mostrar cuadrícula");
                        ui.checkbox(&mut self.dark_mode, "Modo oscuro").changed().then(|| {
                            if self.dark_mode { THEME_DARK.apply(ui.ctx()); } else { THEME_LIGHT.apply(ui.ctx()); }
                        });
                        ui.checkbox(&mut self.snap_to_grid, "Ajustar a cuadrícula");
                        ui.checkbox(&mut self.exam_mode, "Modo examen");
                        let mut is_3d = self.current_view == ViewMode::D3;
                        if ui.checkbox(&mut is_3d, "Vista 3D").changed() {
                            self.current_view = if is_3d { ViewMode::D3 } else { ViewMode::D2 };
                        }
                        
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Ejes").color(txt_dim).size(11.0).strong());
                        ui.add_space(4.0);
                        ui.checkbox(&mut self.document.view_mut().x_log, "Eje X logarítmico");
                        ui.checkbox(&mut self.document.view_mut().y_log, "Eje Y logarítmico");
                        
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Exportación").color(txt_dim).size(11.0).strong());
                        ui.add_space(4.0);
                        if ui.button("Exportar SVG").clicked() {
                            let svg = crate::export::export_svg(&self.document, 800.0, 600.0);
                            let path = "grafito_export.svg";
                            let _ = std::fs::write(path, svg);
                            self.cas_result = format!("SVG saved to {}", path);
                        }
                    });
                });
        } else if self.sidebar_tab == 3 {
            egui::SidePanel::left("spreadsheet_panel")
                .default_width(260.0).min_width(180.0).resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Hoja de Cálculo").color(accent).strong().size(16.0));
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    
                    let (mut rows, mut cols) = self.document.spreadsheet_dim();
                    // Assure at least 15 rows and 6 columns for nice UI, but expand infinitely if needed
                    rows = rows.max(15);
                    cols = cols.max(6);
                    
                    egui::ScrollArea::both().show(ui, |ui| {
                        egui::Frame::none()
                            .stroke(egui::Stroke::new(1.0, sep_col))
                            .show(ui, |ui| {
                                egui::Grid::new("mini_sheet").striped(true).min_col_width(60.0).spacing(egui::vec2(0.0, 0.0)).show(ui, |ui| {
                                    // Header row
                                    ui.add_sized([28.0, 28.0], egui::Label::new(""));
                                    for c in 0..cols {
                                        ui.horizontal_centered(|ui| {
                                            ui.add_space(8.0);
                                            let col_name = if c < 26 {
                                                format!("{}", (b'A' + c as u8) as char)
                                            } else {
                                                format!("{}{}", (b'A' + (c/26 - 1) as u8) as char, (b'A' + (c%26) as u8) as char)
                                            };
                                            ui.label(egui::RichText::new(col_name).size(12.0).strong().color(accent));
                                        });
                                    }
                                    ui.end_row();
                                    
                                    // Data rows
                                    for r in 0..rows {
                                        ui.horizontal_centered(|ui| {
                                            ui.add_space(8.0);
                                            ui.label(egui::RichText::new(format!("{}", r+1)).size(11.0).color(txt_dim));
                                        });
                                        for c in 0..cols {
                                            let mut val = self.document.get_spreadsheet_cell(r, c);
                                            
                                            let cell_frame = egui::Frame::none()
                                                .stroke(egui::Stroke::new(0.5, sep_col))
                                                .inner_margin(egui::Margin::symmetric(4.0, 4.0));
                                                
                                            cell_frame.show(ui, |ui| {
                                                let r2 = ui.add_sized([60.0, 20.0],
                                                    egui::TextEdit::singleline(&mut val)
                                                    .font(egui::FontId::proportional(12.0))
                                                    .frame(false)); // No pill frame!
                                                    
                                                if r2.changed() {
                                                    self.document.set_spreadsheet_cell(r, c, val);
                                                    if let Some(ev) = self.document.eval_spreadsheet_cell(r, c) {
                                                        self.document.set_variable(format!("{}{}", (b'A'+c as u8) as char, r+1), ev);
                                                    }
                                                }
                                            });
                                        }
                                        ui.end_row();
                                    }
                                });
                            });
                    });
                    
                    ui.add_space(8.0);
                    if ui.button("Abrir hoja completa →").clicked() {
                        self.show_spreadsheet = !self.show_spreadsheet;
                    }
                });
        } else if self.sidebar_tab == 2 {
            egui::SidePanel::left("table_panel")
                .default_width(240.0).min_width(180.0).resizable(true)
                .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
                .show(ctx, |ui| {
                    let functions: Vec<FuncInfo> = self
                        .document.objects_iter()
                        .filter_map(|(_, obj)| {
                            match obj {
                                GeoObject::Function(f) => Some((f.label.clone(), f.expr.clone(), "f(x)".to_string(), f.domain_min, f.domain_max)),
                                GeoObject::ParametricCurve2D(pc) => Some((pc.label.clone(), format!("x={}, y={}", pc.expr_x, pc.expr_y), "(x,y)".to_string(), Some(pc.t_min), Some(pc.t_max))),
                                GeoObject::PolarCurve(pc) => Some((pc.label.clone(), pc.expr_r.clone(), "r(θ)".to_string(), Some(pc.t_min), Some(pc.t_max))),
                                _ => None,
                            }
                        }).collect();

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Tabla de Valores").color(accent).strong().size(16.0));
                    });
                    ui.add_space(4.0);
                    ui.separator();

                    if functions.is_empty() {
                        egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                            ui.label(egui::RichText::new("Sin funciones\nEscribe f(x)=... en la entrada").size(12.0).color(txt_dim));
                        });
                    } else {
                        if self.table_func_idx >= functions.len() { self.table_func_idx = 0; }
                        let (_, expr, ftype, dmin, dmax) = &functions[self.table_func_idx];
                        let var = match ftype.as_str() { "(x,y)" | "r(θ)" => "t", _ => "x" };
                        let name_labels: Vec<String> = functions.iter().map(|(l,_,_,_,_)| l.clone()).collect();
                        
                        egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Función:").strong());
                                let selected = name_labels.get(self.table_func_idx).cloned().unwrap_or_default();
                                egui::ComboBox::from_id_salt("func_dropdown")
                                    .selected_text(&selected)
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        for (i, name) in name_labels.iter().enumerate() {
                                            if ui.selectable_label(self.table_func_idx == i, name).clicked() {
                                                self.table_func_idx = i;
                                            }
                                        }
                                    });
                            });
                            
                            ui.add_space(8.0);
                            egui::Grid::new("table_config_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                                ui.label("Desde:"); 
                                ui.add_sized([80.0, 18.0], egui::TextEdit::singleline(&mut self.table_x_min).font(egui::FontId::proportional(12.0)));
                                ui.end_row();
                                
                                ui.label("Hasta:"); 
                                ui.add_sized([80.0, 18.0], egui::TextEdit::singleline(&mut self.table_x_max).font(egui::FontId::proportional(12.0)));
                                ui.end_row();
                                
                                ui.label("Paso:"); 
                                ui.horizontal(|ui| {
                                    ui.add_sized([50.0, 18.0], egui::TextEdit::singleline(&mut self.table_step).font(egui::FontId::proportional(12.0)));
                                    if ui.button("📍").on_hover_text("Agregar puntos al canvas").clicked() {
                                        let x_min: f64 = self.table_x_min.parse().unwrap_or(-5.0);
                                        let x_max: f64 = self.table_x_max.parse().unwrap_or(5.0);
                                        let step: f64 = self.table_step.parse().unwrap_or(1.0);
                                        let is_polar = ftype == "r(θ)";
                                        let mut x = x_min;
                                        while x <= x_max + 1e-9 {
                                            let vars = vec![(var.to_string(), x)];
                                            if let Ok(y) = grafito_geometry::expr::evaluate(expr, &vars) {
                                                if y.is_finite() {
                                                    let pt = if is_polar { Point2::new(y * x.cos(), y * x.sin()) } else { Point2::new(x, y) };
                                                    self.document.add_object(GeoObject::Point(PointObj::new(pt)));
                                                }
                                            }
                                            x += step;
                                        }
                                    }
                                });
                                ui.end_row();
                            });
                        });
                        ui.separator();

                        // Table display
                        let x_min: f64 = dmin.unwrap_or(self.table_x_min.parse().unwrap_or(-5.0));
                        let x_max: f64 = dmax.unwrap_or(self.table_x_max.parse().unwrap_or(5.0));
                        let step: f64 = self.table_step.parse().unwrap_or(1.0);
                        let max_rows = 50;
                        egui::ScrollArea::vertical().max_height(ui.available_height() - 8.0).show(ui, |ui| {
                            egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                                egui::Grid::new("tbl_grid").striped(true).min_col_width(80.0).spacing([16.0, 8.0]).show(ui, |ui| {
                                    ui.label(egui::RichText::new(var).strong().color(accent));
                                    ui.label(egui::RichText::new(&functions[self.table_func_idx].0).strong().color(accent));
                                    ui.end_row();
                                    
                                    let mut x = x_min;
                                    let mut count = 0;
                                    while x <= x_max + 1e-9 && count < max_rows {
                                        let vars = vec![(var.to_string(), x)];
                                        if let Ok(y) = grafito_geometry::expr::evaluate(expr, &vars) {
                                            if y.is_finite() {
                                                ui.label(egui::RichText::new(format!("{:.3}", x)).size(12.0));
                                                let out = format!("{:.4}", y);
                                                ui.label(egui::RichText::new(out).size(12.0));
                                                ui.end_row();
                                            }
                                        }
                                        x += step;
                                        count += 1;
                                    }
                                });
                            });
                        });
                    }
                });
        } else {
            egui::SidePanel::left("empty_panel")
                .default_width(220.0)
                .min_width(160.0)
                .resizable(true)
                .frame(
                    egui::Frame::none()
                        .fill(alg_fill)
                        .stroke(egui::Stroke::new(1.0, sep_col)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(
                            egui::RichText::new("En construcción...")
                                .color(Color32::from_gray(150)),
                        );
                    });
                });
        }

        // ── INPUT BAR (always visible, like GeoGebra) ──
        {
            let mut should_exec = false;
            egui::TopBottomPanel::bottom("input_bar")
                .exact_height(32.0)
                .frame(egui::Frame::none()
                    .fill(if is_dark { Color32::from_rgb(32, 32, 40) } else { Color32::from_rgb(245, 246, 250) })
                    .stroke(egui::Stroke::new(1.0, sep_col))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                        let r = ui.add_sized([ui.available_width() - 40.0, 22.0],
                            egui::TextEdit::singleline(&mut self.input_text)
                                .hint_text("Entrada... (ej: sin(x), A=(1,2), Derivative[x^2,x])")
                                .frame(false)
                                .text_color(txt_col));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            should_exec = true;
                        }
                        if ui.add_sized([28.0, 22.0], egui::Button::new(egui::RichText::new("▶").color(accent))).clicked() {
                            should_exec = true;
                        }
                    });
                });
            if should_exec && !self.input_text.is_empty() {
                self.save_state();
                let mut cmd = self.input_text.clone();
                self.cas_result = commands::process_input(&mut self.document, &mut cmd).unwrap_or_default();
                if !self.cas_result.is_empty() {
                    if self.cas_history.len() > 20 { self.cas_history.remove(0); }
                    self.cas_history.push(format!("> {}\n  {}", self.input_text, self.cas_result));
                }
                self.input_text.clear();
            }
        }

        // ── STATUS BAR ──
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .frame(egui::Frame::none()
                .fill(if is_dark { Color32::from_rgb(28, 28, 34) } else { Color32::from_rgb(240, 241, 245) })
                .stroke(egui::Stroke::new(1.0, sep_col))
                .inner_margin(egui::Margin::symmetric(10.0, 1.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let coord_text = if let Some(pos) = ui.ctx().pointer_hover_pos() {
                        // Approximate world coords via last known view
                        format!("x: {:.2}, y: {:.2}", pos.x, pos.y)
                    } else {
                        "x: ---, y: ---".to_string()
                    };
                    ui.label(egui::RichText::new(coord_text).size(11.0).color(txt_dim));
                    ui.add_space(16.0);
                    let hint = match self.current_tool {
                        Tool::Select => "↖ Seleccionar: clic para elegir, arrastrar para mover punto",
                        Tool::Point => "· Punto: clic para crear",
                        Tool::Line => "╱ Recta: clic en dos puntos",
                        Tool::Circle => "○ Círculo: clic centro, clic borde",
                        Tool::Polygon => "△ Polígono: clic vértices, clic der para cerrar",
                        Tool::Function => "f(x) Función: escribe en la entrada",
                        Tool::Distance => "↔ Distancia: clic en dos puntos",
                        Tool::Angle => "∠ Ángulo: clic vértice, luego dos puntos",
                        Tool::Slider => "═ Deslizador: clic para crear variable",
                        Tool::Locus => "⌒ Locus: selecciona punto móvil, luego dependiente",
                        _ => "",
                    };
                    if !hint.is_empty() {
                        ui.label(egui::RichText::new(hint).size(11.0).color(txt_dim));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("{} objetos", self.document.object_count())).size(11.0).color(txt_dim));
                    });
                });
            });

        // ─── 4. MATH KEYBOARD — docked bottom panel (central area only) ──────────────
        if self.keyboard_visible {
            egui::TopBottomPanel::bottom("math_keyboard")
            .min_height(180.0)
            .frame(
                egui::Frame::none()
                    .fill(if is_dark {
                        Color32::from_rgb(28, 28, 36)
                    } else {
                        Color32::from_rgb(244, 245, 250)
                    })
                    .stroke(egui::Stroke::new(1.0, sep_col)),
            )
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal_centered(|ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        // Tab bar
                        ui.horizontal(|ui| {
                            for (i, lbl) in ["123", "f(x)", "ABC", "3D"].iter().enumerate() {
                                let active = self.keyboard_tab == i;
                                let c = if active {
                                    accent
                                } else {
                                    Color32::from_gray(110)
                                };
                                let fbg = if active {
                                    Color32::from_rgba_unmultiplied(100, 80, 200, 30)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let r = egui::Frame::none()
                                    .fill(fbg)
                                    .rounding(6.0)
                                    .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(*lbl).size(12.0).color(c).strong(),
                                        );
                                    })
                                    .response;
                                if ui
                                    .interact(r.rect, ui.id().with(i), egui::Sense::click())
                                    .clicked()
                                {
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
                                let (r, resp) = $ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                if $ui.is_rect_visible(r) {
                                    let bg = if resp.hovered() {
                                        if is_dark {
                                            Color32::from_gray(70)
                                        } else {
                                            Color32::from_gray(215)
                                        }
                                    } else {
                                        if is_dark {
                                            Color32::from_gray(48)
                                        } else {
                                            Color32::WHITE
                                        }
                                    };
                                    $ui.painter().rect(
                                        r,
                                        4.0,
                                        bg,
                                        egui::Stroke::new(
                                            1.0,
                                            Color32::from_gray(if is_dark { 65 } else { 210 }),
                                        ),
                                    );
                                    $ui.painter().text(
                                        r.center(),
                                        egui::Align2::CENTER_CENTER,
                                        $t,
                                        egui::FontId::proportional((btn_w * 0.4).clamp(10.0, 15.0)),
                                        if is_dark {
                                            Color32::WHITE
                                        } else {
                                            Color32::BLACK
                                        },
                                    );
                                }
                                if resp.clicked() {
                                    self.input_text.push_str($i);
                                }
                            }};
                        }

                        let key_rows: &[&[(&str, &str)]] = match self.keyboard_tab {
                            0 => &[
                                &[
                                    ("x", "x"),
                                    ("y", "y"),
                                    ("π", "π"),
                                    ("e", "e"),
                                    ("7", "7"),
                                    ("8", "8"),
                                    ("9", "9"),
                                    ("/", "/"),
                                ],
                                &[
                                    ("x²", "^2"),
                                    ("v/", "sqrt("),
                                    ("^", "^"),
                                    ("|", "abs("),
                                    ("4", "4"),
                                    ("5", "5"),
                                    ("6", "6"),
                                    ("*", "*"),
                                ],
                                &[
                                    ("<", "<"),
                                    (">", ">"),
                                    ("(", "("),
                                    (")", ")"),
                                    ("1", "1"),
                                    ("2", "2"),
                                    ("3", "3"),
                                    ("-", "-"),
                                ],
                            ],
                            1 => &[
                                &[
                                    ("sin", "sin("),
                                    ("cos", "cos("),
                                    ("tan", "tan("),
                                    ("asin", "asin("),
                                    ("acos", "acos("),
                                    ("atan", "atan("),
                                    ("log", "log("),
                                    ("ln", "ln("),
                                ],
                                &[
                                    ("sec", "sec("),
                                    ("csc", "csc("),
                                    ("cot", "cot("),
                                    ("!", "!"),
                                    ("deg", "deg"),
                                    ("rad", "rad"),
                                    ("f", "f"),
                                    ("g", "g"),
                                ],
                                &[
                                    ("<", "<"),
                                    (">", ">"),
                                    ("(", "("),
                                    (")", ")"),
                                    ("1", "1"),
                                    ("2", "2"),
                                    ("3", "3"),
                                    ("-", "-"),
                                ],
                            ],
                            2 => &[
                                &[
                                    ("q", "q"),
                                    ("w", "w"),
                                    ("e", "e"),
                                    ("r", "r"),
                                    ("t", "t"),
                                    ("y", "y"),
                                    ("u", "u"),
                                    ("i", "i"),
                                ],
                                &[
                                    ("a", "a"),
                                    ("s", "s"),
                                    ("d", "d"),
                                    ("f", "f"),
                                    ("g", "g"),
                                    ("h", "h"),
                                    ("j", "j"),
                                    ("k", "k"),
                                ],
                                &[
                                    ("z", "z"),
                                    ("x", "x"),
                                    ("c", "c"),
                                    ("v", "v"),
                                    ("b", "b"),
                                    ("n", "n"),
                                    ("m", "m"),
                                    (",", ""),
                                ],
                            ],
                            _ => &[
                                &[
                                    ("Lor", "Lorenz[10, 28, 2.66]"),
                                    ("Roe", "Rossler[0.2, 0.2, 5.7]"),
                                    ("Aiz", "Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]"),
                                    ("Rab", "Dadras[3, 2.7, 1.7, 2, 9]"),
                                    ("Sph", "Sphere[0,0,0,5]"),
                                    ("Cub", "Cube[0,0,0,5]"),
                                    ("P3D", "Point3D[1,1,1]"),
                                    ("S3D", "Segment3D[0,0,0,1,1,1]"),
                                ],
                                &[
                                    ("Hal", "Halvorsen[2.0]"),
                                    ("Tho", "Thomas[0.208186]"),
                                    ("Che", "Chen[35, 3, 28]"),
                                    ("Spr", "Chua[15.6, 28, -1.14, -0.71]"),
                                    ("Cyl", "Cylinder[0,0,0,2,5]"),
                                    ("Con", "Cone[0,0,0,3,5]"),
                                    ("Tor", "Torus[0,0,0,4,1]"),
                                    ("Moe", "Moebius[2,1]"),
                                ],
                                &[
                                    ("<", "<"),
                                    (">", ">"),
                                    ("(", "("),
                                    (")", ")"),
                                    ("[", "["),
                                    ("]", "]"),
                                    ("{", "{"),
                                    ("}", "}"),
                                ],
                            ],
                        };
                        for row in key_rows {
                            ui.horizontal(|ui| {
                                ui.add_space(pad);
                                for (t, i) in *row {
                                    kb!(ui, *t, *i);
                                    ui.add_space(sp);
                                }
                            });
                            ui.add_space(sp);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(pad);
                            kb!(ui, "ans", "ans");
                            ui.add_space(sp);
                            kb!(ui, ".", ".");
                            ui.add_space(sp);
                            kb!(ui, "0", "0");
                            ui.add_space(sp);
                            kb!(ui, "(", "(");
                            ui.add_space(sp);
                            kb!(ui, ")", ")");
                            ui.add_space(sp);
                            kb!(ui, "=", "=");
                            ui.add_space(sp);
                            // Backspace
                            {
                                let (r, resp) = ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                let bg = if resp.hovered() {
                                    Color32::from_rgb(220, 60, 60)
                                } else {
                                    Color32::from_gray(if is_dark { 48 } else { 230 })
                                };
                                ui.painter().rect(
                                    r,
                                    4.0,
                                    bg,
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_gray(if is_dark { 65 } else { 210 }),
                                    ),
                                );
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Del",
                                    egui::FontId::proportional(14.0),
                                    if is_dark {
                                        Color32::WHITE
                                    } else {
                                        Color32::BLACK
                                    },
                                );
                                if resp.clicked() {
                                    self.input_text.pop();
                                }
                            }
                            ui.add_space(sp);
                            // Enter
                            {
                                let (r, resp) = ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                let bg = if resp.hovered() {
                                    Color32::from_rgb(120, 100, 240)
                                } else {
                                    Color32::from_rgb(100, 80, 200)
                                };
                                ui.painter().rect(r, 4.0, bg, egui::Stroke::NONE);
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Enter",
                                    egui::FontId::proportional(13.0),
                                    Color32::WHITE,
                                );
                                if resp.clicked() {
                                    self.save_state();
                                    self.cas_result = commands::process_input(
                                        &mut self.document,
                                        &mut self.input_text,
                                    )
                                    .unwrap_or_default();
                                }
                            }
                        });
                        ui.add_space(12.0);
                    });
                });
            });
        }

        // ─── 5. SPREADSHEET (optional right panel) ────────────────────────────
        if self.show_spreadsheet {
            egui::SidePanel::right("spreadsheet")
                .resizable(true)
                .default_width(280.0)
                .frame(
                    egui::Frame::none()
                        .fill(alg_fill)
                        .stroke(egui::Stroke::new(1.0, sep_col)),
                )
                .show(ctx, |ui| {
                    ui.heading("Hoja de Cálculo");
                    ui.separator();
                    let (rows, cols) = self.document.spreadsheet_dim();
                    let text_col = if is_dark {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    };
                    let hdr_col = if is_dark {
                        Color32::from_gray(160)
                    } else {
                        Color32::from_gray(80)
                    };

                    egui::ScrollArea::both()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            egui::Grid::new("sp_grid")
                                .min_col_width(52.0)
                                .spacing(egui::vec2(1.0, 1.0))
                                .striped(true)
                                .show(ui, |ui| {
                                    // Header row
                                    ui.label(""); // corner
                                    for c in 0..cols {
                                        let letter = if c < 26 {
                                            format!("{}", (b'A' + c as u8) as char)
                                        } else {
                                            format!("{}", c + 1)
                                        };
                                        ui.centered_and_justified(|ui| {
                                            ui.label(
                                                egui::RichText::new(letter)
                                                    .monospace()
                                                    .strong()
                                                    .color(hdr_col),
                                            );
                                        });
                                    }
                                    ui.end_row();

                                    // Data rows
                                    for r in 0..rows {
                                        ui.label(
                                            egui::RichText::new(format!("{}", r + 1))
                                                .monospace()
                                                .strong()
                                                .color(hdr_col),
                                        );
                                        for c in 0..cols {
                                            let mut val = self.document.get_spreadsheet_cell(r, c);
                                            let resp = ui.add_sized(
                                                [52.0, 18.0],
                                                egui::TextEdit::singleline(&mut val)
                                                    .font(egui::TextStyle::Monospace)
                                                    .text_color(text_col)
                                                    .horizontal_align(egui::Align::Center),
                                            );
                                            if resp.changed() {
                                                self.save_state();
                                                self.document.set_spreadsheet_cell(
                                                    r,
                                                    c,
                                                    val.clone(),
                                                );
                                                if let Ok((x, y)) = commands::parse_point_str(&val)
                                                {
                                                    self.document.add_object(GeoObject::Point(
                                                        PointObj::new(Point2::new(x, y))
                                                            .with_label(format!(
                                                                "{}{}",
                                                                (b'A' + c as u8) as char,
                                                                r + 1
                                                            )),
                                                    ));
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
                    .frame(egui::Frame::none().fill(if is_dark {
                        Color32::from_gray(18)
                    } else {
                        Color32::WHITE
                    }))
                    .show(ctx, |ui| {
                        if self.exam_mode {
                            egui::TopBottomPanel::top("exam_banner")
                                .frame(egui::Frame::none().fill(Color32::from_rgb(220, 53, 69)).inner_margin(8.0))
                                .show_inside(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(egui::RichText::new("⚠ MODO EXAMEN ACTIVO")
                                            .color(Color32::WHITE)
                                            .size(18.0)
                                            .strong());
                                    });
                                });
                        }

                        let canvas_rect = ui.available_rect_before_wrap();
                        self.handle_canvas_input(ui, canvas_rect);

                        // Compact canvas controls — top-right corner, inside canvas
                        let ctrl_x = canvas_rect.right() - 44.0;
                        let ctrl_y = canvas_rect.top() + 8.0;
                        let painter = ui.painter();
                        // Zoom-fit button
                        let zf_rect = egui::Rect::from_min_size(
                            egui::pos2(ctrl_x, ctrl_y),
                            egui::vec2(34.0, 28.0),
                        );
                        painter.rect(
                            zf_rect,
                            4.0,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                            egui::Stroke::new(1.0, Color32::from_gray(200)),
                        );
                        painter.text(
                            zf_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "[ ]",
                            egui::FontId::proportional(16.0),
                            Color32::from_gray(60),
                        );
                        if ui
                            .interact(zf_rect, ui.id().with("zf"), egui::Sense::click())
                            .on_hover_text("Ajustar Vista")
                            .clicked()
                        {
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
                                    let mut f = fun.clone();
                                    f.color = Color::new(0.5, 0.5, 0.5, 0.6);
                                    self.draw_object(
                                        &painter,
                                        canvas_rect,
                                        &GeoObject::Function(f),
                                    );
                                }
                                GeoObject::Point(p) => {
                                    let mut pt = p.clone();
                                    pt.color = Color::new(0.5, 0.5, 0.5, 0.6);
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
                    let w = canvas_rect.width();
                    let h = canvas_rect.height();
                    self.camera.aspect = w / h.max(1.0);
                    let ctx_resp = ui.interact(canvas_rect, ui.id().with("ctx3d"), Sense::click_and_drag());
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
                        Sense::click_and_drag(),
                    );
                    if let Some(pos) = response.hover_pos() {
                        if response.dragged_by(egui::PointerButton::Secondary) {
                            if let Some(last) = self.last_mouse_pos {
                                self.camera
                                    .orbit((pos.x - last.x) * 0.005, (pos.y - last.y) * 0.005);
                            }
                        }
                        if response.dragged_by(egui::PointerButton::Primary) {
                            if let Some(last) = self.last_mouse_pos {
                                if self.current_tool == Tool::Select {
                                    self.camera.pan(pos.x - last.x, pos.y - last.y);
                                }
                            }
                        }
                        if response.hovered() {
                            let sc = ui.input(|i| i.smooth_scroll_delta);
                            if sc.y != 0.0 {
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

                        self.last_mouse_pos = Some(pos);
                    }
                    if (response.clicked_by(egui::PointerButton::Primary)
                        || response.drag_stopped_by(egui::PointerButton::Primary))
                        && self.current_tool != Tool::Select
                    {
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
                            painter.circle_stroke(
                                pos,
                                ghost.size.min(8.0) * 1.3,
                                egui::Stroke::new(
                                    1.5,
                                    Color32::from_rgba_premultiplied(100, 150, 255, 120),
                                ),
                            );
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

#[derive(serde::Serialize, serde::Deserialize)]
struct AppConfig {
    dark_mode: bool,
    show_grid: bool,
    snap_to_grid: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { dark_mode: false, show_grid: true, snap_to_grid: false }
    }
}

fn config_path() -> std::path::PathBuf {
    std::path::PathBuf::from("grafito_config.json")
}

fn load_config() -> AppConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(config: &AppConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(config_path(), json);
    }
}
