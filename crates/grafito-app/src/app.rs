//! Main application state and eframe orchestration.
//!
//! Holds `GrafitoApp`, its constructor, file/undo helpers, and the top-level
//! `eframe::App::update` loop that dispatches rendering to focused UI modules.

use crate::utils::{configure_modern_style, load_config, save_config, AppConfig};
use crate::{ViewMode, MSAA_SAMPLES};
use egui::{Color32, Key, Pos2};
use grafito_core::{
    CircleObj, Cube3DObj, Document, EllipseObj, FunctionObj, GeoObject, LineObj, ObjectId,
    PointObj, PolygonObj, RenderQuality, Sphere3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use grafito_ui::theme::{DARK as THEME_DARK, LIGHT as THEME_LIGHT};
use grafito_ui::Tool;
use std::time::{Duration, Instant};

const MAX_UNDO: usize = 50;

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
    pub canvas_drag_start: Option<Pos2>,
    pub canvas_is_panning: bool,
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
    pub use_gpu: bool,
    pub last_interaction_time: Instant,
    pub is_view_changing: bool,
}

impl GrafitoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(render_state) = &cc.wgpu_render_state {
            let renderer = grafito_render::Renderer::new(
                &render_state.device,
                render_state.target_format,
                MSAA_SAMPLES as u32,
            );
            let resources = crate::canvas::GpuCanvasResources {
                renderer: std::sync::Arc::new(std::sync::RwLock::new(renderer)),
                buffers_2d: None,
                buffers_3d: None,
            };
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }
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
        document.render_quality = RenderQuality::Normal;

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
            canvas_drag_start: None,
            canvas_is_panning: false,
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
            use_gpu: true,
            last_interaction_time: Instant::now(),
            is_view_changing: false,
            color_favorites: [
                grafito_geometry::Color::new(0.9, 0.1, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.3, 0.9, 1.0),
                grafito_geometry::Color::new(0.9, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.5, 0.1, 0.9, 1.0),
            ],
        }
    }

    pub(crate) fn save_state(&mut self) {
        self.undo_stack.push(self.document.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
    }

    pub(crate) fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.document.clone());
            self.document = prev;
            self.selected_object = None;
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.document.clone());
            self.document = next;
            self.selected_object = None;
        }
    }

    pub(crate) fn save_to_file(&mut self) {
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

    pub(crate) fn load_from_file(&mut self) {
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

    pub(crate) fn delete_selected(&mut self) {
        if let Some(id) = self.selected_object {
            self.save_state();
            self.document.remove_object(id);
            self.selected_object = None;
        }
    }

    pub(crate) fn zoom_to_fit(&mut self) {
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

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        configure_modern_style(ctx);
        if self.is_view_changing
            && self.last_interaction_time.elapsed() > Duration::from_millis(150)
        {
            self.is_view_changing = false;
            self.document.render_quality = RenderQuality::High;
        }

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

        crate::ui::draw_top_bar(self, ctx);

        let is_dark = self.dark_mode;
        match self.sidebar_tab {
            0 => crate::algebra::draw_algebra_panel(self, ctx),
            1 => crate::panels::draw_cas_panel(self, ctx),
            2 => crate::panels::draw_table_panel(self, ctx),
            3 => crate::panels::draw_spreadsheet_panel(self, ctx),
            4 => crate::panels::draw_view_panel(self, ctx),
            _ => crate::panels::draw_empty_panel(self, ctx),
        }

        crate::ui::draw_bottom_bar(self, ctx);

        if self.keyboard_visible {
            crate::keyboard::draw_math_keyboard(self, ctx);
        }

        if self.show_spreadsheet {
            crate::panels::draw_right_spreadsheet(self, ctx);
        }

        // Central canvas: 2D or 3D view.
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
                                .frame(
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(220, 53, 69))
                                        .inner_margin(8.0),
                                )
                                .show_inside(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("⚠ MODO EXAMEN ACTIVO")
                                                .color(Color32::WHITE)
                                                .size(18.0)
                                                .strong(),
                                        );
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

                        // Keep the view's screen size in sync with the actual canvas rect
                        // so that both CPU and GPU renderers project correctly.
                        let canvas_size = canvas_rect.size();
                        self.document.view_mut().screen_size =
                            glam::Vec2::new(canvas_size.x, canvas_size.y);

                        if self.use_gpu && canvas_size.x > 0.0 && canvas_size.y > 0.0 {
                            let callback = egui_wgpu::Callback::new_paint_callback(
                                canvas_rect,
                                crate::canvas::CanvasCallback {
                                    document: std::sync::Arc::new(self.document.clone()),
                                    dark_mode: self.dark_mode,
                                },
                            );
                            ui.painter().add(egui::epaint::Shape::Callback(callback));
                        } else {
                            let mut painter = ui.painter().clone();
                            painter.set_clip_rect(canvas_rect);
                            self.draw_grid(&painter, canvas_rect);
                            self.draw_axes(&painter, canvas_rect);
                            self.draw_objects(&painter, canvas_rect);
                        }

                        // Tool ghost and preview are transient overlays, render with CPU on top.
                        let mut overlay_painter = ui.painter().clone();
                        overlay_painter.set_clip_rect(canvas_rect);
                        self.draw_tool_ghost(&overlay_painter, canvas_rect);

                        if let Some(preview) = &self.preview_object {
                            match preview {
                                GeoObject::Function(fun) => {
                                    let mut f = fun.clone();
                                    f.color = Color::new(0.5, 0.5, 0.5, 0.6);
                                    self.draw_object(
                                        &overlay_painter,
                                        canvas_rect,
                                        &GeoObject::Function(f),
                                    );
                                }
                                GeoObject::Point(p) => {
                                    let mut pt = p.clone();
                                    pt.color = Color::new(0.5, 0.5, 0.5, 0.6);
                                    self.draw_object(
                                        &overlay_painter,
                                        canvas_rect,
                                        &GeoObject::Point(pt),
                                    );
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

                    self.handle_canvas_3d_input(ui, canvas_rect);

                    if self.use_gpu {
                        let callback = egui_wgpu::Callback::new_paint_callback(
                            canvas_rect,
                            crate::canvas::Canvas3DCallback {
                                document: std::sync::Arc::new(self.document.clone()),
                                camera: self.camera,
                                dark_mode: self.dark_mode,
                                screen_w: w,
                                screen_h: h,
                            },
                        );
                        ui.painter().add(egui::epaint::Shape::Callback(callback));
                    } else {
                        self.draw_3d_grid(ui.painter(), canvas_rect, w, h);
                        self.draw_3d_objects(ui.painter(), canvas_rect, w, h);
                    }

                    // Draw 3D tool ghost on top with CPU painter
                    if let Some(GeoObject::Point3D(ghost)) = &self.tool_ghost {
                        let painter = ui.painter();
                        let origin = canvas_rect.min;
                        if let Some(pt) = self.camera.project(&ghost.position, w, h) {
                            let pos = origin + egui::Vec2::new(pt.0, pt.1);
                            // Render ghost with reduced opacity
                            let ghost_color = egui::Color32::from_rgba_premultiplied(
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
                                    egui::Color32::from_rgba_premultiplied(100, 150, 255, 120),
                                ),
                            );
                        }
                    }
                });
            }
        }

        crate::ui::draw_color_picker(self, ctx);
    }
}

/// Run the native Grafito desktop application.
pub fn run_app() -> Result<(), eframe::Error> {
    env_logger::init();

    for arg in std::env::args().skip(1) {
        if arg == "--help" || arg == "-h" {
            println!("Grafito v0.9.0-alpha");
            println!("Usage: grafito [OPTIONS]");
            println!("Options:");
            println!("  -h, --help    Print help information");
            return Ok(());
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_decorations(true)
            .with_transparent(false),
        multisampling: crate::MSAA_SAMPLES,
        ..Default::default()
    };
    eframe::run_native(
        "Grafito",
        options,
        Box::new(|cc| Ok(Box::new(GrafitoApp::new(cc)))),
    )
}
