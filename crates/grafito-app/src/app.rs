//! Main application state and eframe orchestration.
//!
//! Holds `GrafitoApp`, its constructor, file/undo helpers, and the top-level
//! `eframe::App::update` loop that dispatches rendering to focused UI modules.

use crate::utils::{configure_modern_style, load_config, save_config, AppConfig};
use crate::{Perspective, ViewMode};
use egui::{Color32, Key, Pos2};
use grafito_core::{
    CircleObj, Cube3DObj, Document, EllipseObj, FunctionObj, GeoObject, LineObj, ObjectId,
    PointObj, PolygonObj, RenderQuality, Sphere3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::Tool;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use grafito_command::commands::{register_gpu_function_evaluator, GpuFunctionEvaluator};

const MAX_UNDO: usize = 50;

/// Evaluador GPU para la ruta híbrida de integrales definidas.
struct AppGpuFunctionEvaluator {
    renderer: Arc<RwLock<Option<grafito_render::Renderer>>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl GpuFunctionEvaluator for AppGpuFunctionEvaluator {
    fn evaluate_function_batch(
        &self,
        expr: &str,
        a: f64,
        b: f64,
        samples: usize,
        variables: &std::collections::HashMap<String, f64>,
    ) -> Option<Vec<f64>> {
        let renderer_lock = self.renderer.read().ok()?;
        let renderer = renderer_lock.as_ref()?;
        let pipeline = renderer.function_compute.as_ref()?;
        let grid_size = samples.saturating_sub(1).max(1);
        pipeline.evaluate_expr(
            &self.device,
            &self.queue,
            expr,
            (a, b),
            grid_size,
            variables,
        )
    }
}

/// Pending interactive action that requires selecting objects on the canvas.
#[derive(Debug, Clone, Default)]
pub enum PendingAction {
    #[default]
    None,
    Distance {
        first: Option<ObjectId>,
    },
    Angle {
        first: Option<ObjectId>,
    },
    Tangent {
        first: Option<ObjectId>,
    },
    Coincident {
        first: Option<ObjectId>,
    },
    Horizontal {
        line: Option<ObjectId>,
    },
    Vertical {
        line: Option<ObjectId>,
    },
    EqualLength {
        first: Option<ObjectId>,
    },
    Symmetry {
        point: Option<ObjectId>,
        mirror_point: Option<ObjectId>,
        line: Option<ObjectId>,
    },
    EllipseByFoci {
        f1: Option<ObjectId>,
        f2: Option<ObjectId>,
    },
    ParabolaByFocusDirectrix {
        focus: Option<ObjectId>,
        directrix: Option<ObjectId>,
    },
    HyperbolaByFoci {
        f1: Option<ObjectId>,
        f2: Option<ObjectId>,
    },
    ConicByFivePoints {
        points: Vec<ObjectId>,
    },
    BooleanUnion {
        first: Option<ObjectId>,
    },
    BooleanIntersection {
        first: Option<ObjectId>,
    },
    BooleanDifference {
        first: Option<ObjectId>,
    },
    BooleanXor {
        first: Option<ObjectId>,
    },
}

impl PendingAction {
    fn boolean_cmd_name(&self) -> Option<&'static str> {
        match self {
            PendingAction::BooleanUnion { .. } => Some("PolygonUnion"),
            PendingAction::BooleanIntersection { .. } => Some("PolygonIntersection"),
            PendingAction::BooleanDifference { .. } => Some("PolygonDifference"),
            PendingAction::BooleanXor { .. } => Some("PolygonXor"),
            _ => None,
        }
    }

    fn with_boolean_first(self, id: ObjectId) -> Self {
        match self {
            PendingAction::BooleanUnion { .. } => PendingAction::BooleanUnion { first: Some(id) },
            PendingAction::BooleanIntersection { .. } => {
                PendingAction::BooleanIntersection { first: Some(id) }
            }
            PendingAction::BooleanDifference { .. } => {
                PendingAction::BooleanDifference { first: Some(id) }
            }
            PendingAction::BooleanXor { .. } => PendingAction::BooleanXor { first: Some(id) },
            other => other,
        }
    }
}

pub struct GrafitoApp {
    pub document: Document,
    pub current_tool: Tool,
    pub previous_tool: Tool,
    pub current_view: ViewMode,
    /// Perspectiva activa (estilo GeoGebra). `current_view` se deriva de ésta.
    pub perspective: Perspective,
    pub camera: Camera3D,
    pub show_grid: bool,
    pub snap_to_grid: bool,
    pub snap_config: crate::snap::SnapConfig,
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
    /// Timestamp de inicio de la app (splash screen). None = ya pasó.
    pub splash_start: Option<Instant>,
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<Document>,
    pub attractor_cache: std::collections::HashMap<ObjectId, (u64, Vec<Point3D>)>,
    /// Caché de texturas de relleno para curvas implícitas. Usa `RwLock`
    /// para permitir mutación desde `draw_implicit_curve_fill` (que recibe
    /// `&self`). La clave es el `ObjectId` de la `ImplicitCurveObj`.
    pub fill_textures:
        std::sync::RwLock<std::collections::HashMap<ObjectId, crate::render_2d::FillTextureCache>>,
    pub active_color_picker: Option<(ObjectId, grafito_ui::color_picker::HsvColorPicker)>,
    pub color_favorites: [grafito_geometry::Color; 5],
    pub tool_ghost: Option<GeoObject>,
    pub tool_state: crate::tool_dispatcher::ToolState,
    pub use_gpu: bool,
    pub last_interaction_time: Instant,
    pub is_view_changing: bool,
    pub pending_action: PendingAction,
    pub toasts: grafito_ui::toast::ToastManager,
    pub hovered_analysis: Option<HoveredAnalysis>,
    pub hover_candidate_pos: Option<Point2>,
    pub hover_candidate_time: f64,
    pub hover_cached_analysis: Option<Option<HoveredAnalysis>>,
    pub document_snapshot: std::sync::Arc<Document>,
    pub snapshot_version: u64,
    pub style_applied: Option<bool>,
    pub command_palette: grafito_ui::command_palette::CommandPaletteState,
}

#[derive(Debug, Clone)]
pub struct HoveredAnalysis {
    pub point: Point2,
    pub label: String,
    pub is_snap: bool,
    pub feature: Option<grafito_geometry::analysis::AnalysisFeature>,
    pub snap_kind: Option<crate::snap::SnapKind>,
}

impl GrafitoApp {
    /// Limpia todo el estado transitorio de herramientas (puntos pendientes,
    /// objetos driver/driven, rectángulos de selección y outcome de comandos).
    pub fn reset_tool_input(&mut self) {
        self.pending_points.clear();
        self.pending_points_3d.clear();
        self.tool_state.pending.clear();
        self.tool_state.driver = None;
        self.tool_state.driven = None;
        self.tool_state.measure_src = None;
        self.tool_state.selection_rect = None;
        self.tool_state.last_outcome = None;
    }
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(render_state) = &cc.wgpu_render_state {
            let renderer: Arc<RwLock<Option<grafito_render::Renderer>>> =
                Arc::new(RwLock::new(None));
            let renderer_clone = Arc::clone(&renderer);
            let device_clone = Arc::clone(&render_state.device);
            let queue_clone = Arc::clone(&render_state.queue);
            let target_format = render_state.target_format;
            let egui_ctx = cc.egui_ctx.clone();

            std::thread::spawn(move || {
                let new_renderer = grafito_render::Renderer::new(
                    &device_clone,
                    &queue_clone,
                    target_format,
                    crate::MSAA_SAMPLES as u32,
                );
                if let Ok(mut lock) = renderer_clone.write() {
                    *lock = Some(new_renderer);
                }
                egui_ctx.request_repaint();
                log::info!("Background shader compilation finished.");
            });

            let resources = crate::canvas::GpuCanvasResources {
                renderer: Arc::clone(&renderer),
                buffers_2d: None,
                buffers_3d: None,
                cache_2d: None,
                cache_3d: None,
            };
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
            register_gpu_function_evaluator(Box::new(AppGpuFunctionEvaluator {
                renderer,
                device: Arc::clone(&render_state.device),
                queue: Arc::clone(&render_state.queue),
            }));
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
            DARK.apply(&cc.egui_ctx);
        } else {
            LIGHT.apply(&cc.egui_ctx);
        }

        let snapshot_version = document.version;
        let document_snapshot = std::sync::Arc::new(document.clone());

        Self {
            document,
            current_tool: Tool::default(),
            previous_tool: Tool::default(),
            current_view: ViewMode::D2,
            perspective: Perspective::Geometry2D,
            camera: Camera3D::new(1280.0 / 720.0),
            show_grid: config.show_grid,
            snap_to_grid: config.snap_to_grid,
            snap_config: config.snap,
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
            splash_start: Some(Instant::now()),
            recent_files: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            attractor_cache: std::collections::HashMap::new(),
            fill_textures: std::sync::RwLock::new(std::collections::HashMap::new()),
            active_color_picker: None,
            tool_ghost: None,
            tool_state: crate::tool_dispatcher::ToolState::default(),
            use_gpu: true,
            last_interaction_time: Instant::now(),
            is_view_changing: false,
            pending_action: PendingAction::None,
            toasts: grafito_ui::toast::ToastManager::default(),
            hovered_analysis: None,
            hover_candidate_pos: None,
            hover_candidate_time: 0.0,
            hover_cached_analysis: None,
            color_favorites: [
                grafito_geometry::Color::new(0.9, 0.1, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.1, 0.3, 0.9, 1.0),
                grafito_geometry::Color::new(0.9, 0.6, 0.1, 1.0),
                grafito_geometry::Color::new(0.5, 0.1, 0.9, 1.0),
            ],
            document_snapshot,
            snapshot_version,
            style_applied: None,
            command_palette: grafito_ui::command_palette::CommandPaletteState::default(),
        }
    }

    pub(crate) fn re_evaluate_constraints(&mut self, order: &[usize]) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("constraints");
        self.document.re_evaluate_constraints(order);
    }

    /// Devuelve un `Arc<Document>` para el callback GPU.
    /// Solo clona el documento cuando el `version` cambia (contenido modificado).
    /// Para cambios de view (pan/zoom), actualiza el view in-place vía `make_mut`.
    fn document_for_callback(&mut self) -> std::sync::Arc<Document> {
        if self.document.version != self.snapshot_version {
            self.document_snapshot = std::sync::Arc::new(self.document.clone());
            self.snapshot_version = self.document.version;
        } else {
            let snap = std::sync::Arc::make_mut(&mut self.document_snapshot);
            snap.set_view(*self.document.view());
        }
        self.document_snapshot.clone()
    }

    pub(crate) fn save_state(&mut self) {
        self.undo_stack.push(self.document.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
    }

    pub(crate) fn handle_command_outcome(
        &mut self,
        outcome: grafito_command::commands::CommandOutcome,
        time: f64,
        input_was: &str,
    ) {
        match outcome {
            grafito_command::commands::CommandOutcome::Ok => {}
            grafito_command::commands::CommandOutcome::Message(msg) => {
                self.cas_result = msg.clone();
                if !msg.is_empty() {
                    if self.cas_history.len() > 20 {
                        self.cas_history.remove(0);
                    }
                    self.cas_history.push(format!("> {}\n  {}", input_was, msg));
                }
            }
            grafito_command::commands::CommandOutcome::Error(msg) => {
                self.cas_result = msg.clone();
                self.toasts.push(
                    format!("Error: {}", msg),
                    grafito_ui::toast::ToastKind::Error,
                    time,
                );
            }
        }
    }

    /// Ejecuta la acción elegida desde la paleta de comandos (Ctrl+K).
    ///
    /// Los comandos de tipo herramienta seleccionan el `Tool` correspondiente;
    /// las acciones inmediatas (vista/archivo/exportación) se ejecutan directo;
    /// el resto se inserta en la barra de entrada como `Nombre[` para que el
    /// usuario complete los argumentos y se procese vía `process_input`.
    pub(crate) fn apply_palette_command(&mut self, name: &str, ctx: &egui::Context) {
        // 1) Selección de herramienta.
        let tool = match name {
            "Point Tool" => Some(Tool::Point),
            "Line Tool" => Some(Tool::Line),
            "Circle Tool" => Some(Tool::Circle),
            "Polygon Tool" => Some(Tool::Polygon),
            "Function Tool" => Some(Tool::Function),
            "Pencil" => Some(Tool::Pencil),
            "Eraser" => Some(Tool::Eraser),
            _ => None,
        };
        if let Some(tool) = tool {
            self.current_tool = tool;
            self.previous_tool = tool;
            self.tool_ghost = None;
            self.reset_tool_input();
            return;
        }

        // 2) Acciones inmediatas de vista y archivo.
        match name {
            "Zoom to Fit" => {
                self.zoom_to_fit();
                return;
            }
            "Toggle Grid" => {
                self.show_grid = !self.show_grid;
                return;
            }
            "Toggle Dark Mode" => {
                self.dark_mode = !self.dark_mode;
                if self.dark_mode {
                    DARK.apply(ctx);
                } else {
                    LIGHT.apply(ctx);
                }
                return;
            }
            "Save" => {
                self.save_to_file();
                return;
            }
            "Export SVG" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("SVG", &["svg"])
                    .save_file()
                {
                    let svg = crate::export::export_svg(&self.document, 1280.0, 720.0);
                    if let Err(e) = std::fs::write(&path, svg) {
                        self.toasts.push(
                            format!("Error SVG: {}", e),
                            grafito_ui::toast::ToastKind::Error,
                            5.0,
                        );
                    }
                }
                return;
            }
            "Export PNG" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .save_file()
                {
                    if let Err(e) = crate::export::export_png(
                        &self.document,
                        1280,
                        720,
                        &path.to_string_lossy(),
                    ) {
                        self.toasts.push(
                            format!("Error PNG: {}", e),
                            grafito_ui::toast::ToastKind::Error,
                            5.0,
                        );
                    }
                }
                return;
            }
            "Export TikZ" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("TeX", &["tex"])
                    .save_file()
                {
                    let tex = crate::export::export_latex(&self.document);
                    if let Err(e) = std::fs::write(&path, tex) {
                        self.toasts.push(
                            format!("Error TikZ: {}", e),
                            grafito_ui::toast::ToastKind::Error,
                            5.0,
                        );
                    }
                }
                return;
            }
            _ => {}
        }

        // 3) Resto: insertar `Nombre[` en la barra de entrada para que el
        //    usuario complete los argumentos y se procese vía `process_input`.
        self.input_text = format!("{}[", name);
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
                self.toasts.push(
                    format!("Error al guardar: {}", e),
                    grafito_ui::toast::ToastKind::Error,
                    5.0,
                );
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
                Err(e) => {
                    log::error!("Load failed: {}", e);
                    self.toasts.push(
                        format!("Error al cargar: {}", e),
                        grafito_ui::toast::ToastKind::Error,
                        5.0,
                    );
                }
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

    pub(crate) fn start_pending_action(&mut self, tool: Tool) {
        self.pending_action = match tool {
            Tool::DistanceConstraint => PendingAction::Distance { first: None },
            Tool::AngleConstraint => PendingAction::Angle { first: None },
            Tool::Tangent => PendingAction::Tangent { first: None },
            Tool::Coincident => PendingAction::Coincident { first: None },
            Tool::Horizontal => PendingAction::Horizontal { line: None },
            Tool::Vertical => PendingAction::Vertical { line: None },
            Tool::EqualLength => PendingAction::EqualLength { first: None },
            Tool::Symmetry => PendingAction::Symmetry {
                point: None,
                mirror_point: None,
                line: None,
            },
            Tool::EllipseByFoci => PendingAction::EllipseByFoci { f1: None, f2: None },
            Tool::ParabolaByFocusDirectrix => PendingAction::ParabolaByFocusDirectrix {
                focus: None,
                directrix: None,
            },
            Tool::HyperbolaByFoci => PendingAction::HyperbolaByFoci { f1: None, f2: None },
            Tool::ConicByFivePoints => PendingAction::ConicByFivePoints { points: Vec::new() },
            Tool::PolygonUnion => PendingAction::BooleanUnion { first: None },
            Tool::PolygonIntersection => PendingAction::BooleanIntersection { first: None },
            Tool::PolygonDifference => PendingAction::BooleanDifference { first: None },
            Tool::PolygonXor => PendingAction::BooleanXor { first: None },
            _ => PendingAction::None,
        };
    }

    pub(crate) fn clear_pending_action(&mut self) {
        self.pending_action = PendingAction::None;
    }

    pub(crate) fn is_constraint_tool(tool: Tool) -> bool {
        matches!(
            tool,
            Tool::DistanceConstraint
                | Tool::AngleConstraint
                | Tool::Tangent
                | Tool::Coincident
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
                | Tool::PolygonXor
        )
    }

    pub(crate) fn sync_pending_action_with_tool(&mut self) {
        if self.current_tool != self.previous_tool {
            if Self::is_constraint_tool(self.current_tool) {
                self.start_pending_action(self.current_tool);
            } else {
                self.clear_pending_action();
            }
            // Limpiar marcas del Eraser y del Pencil en curso al cambiar
            // de herramienta, para no dejar un PencilObj "huérfano".
            self.tool_state.last_erased = None;
            self.tool_state.drawing_pencil = None;
            self.previous_tool = self.current_tool;
        }
    }

    /// Cambia la perspectiva activa, sincroniza `current_view` y la herramienta
    /// por defecto, y carga objetos de ejemplo si el documento está vacío.
    pub(crate) fn set_perspective(&mut self, p: Perspective) {
        if self.perspective == p {
            return;
        }
        self.perspective = p;
        let layout = p.layout();
        let target_view = p.view_mode();
        let view_changed = self.current_view != target_view;
        self.current_view = target_view;
        self.current_tool = layout.default_tool;
        self.previous_tool = layout.default_tool;
        self.tool_ghost = None;
        self.reset_tool_input();
        self.clear_pending_action();
        // Visibilidad del teclado matemático según la perspectiva.
        self.keyboard_visible = layout.show_math_keyboard;
        // Ajuste del panel izquierdo: mapea el contenido declarado al tab
        // existente más cercano del sidebar.
        self.sidebar_tab = layout.left_panel.default_sidebar_tab();
        // Panel derecho: la hoja de cálculo lateral se muestra sólo cuando la
        // perspectiva la solicita explícitamente.
        self.show_spreadsheet = matches!(
            layout.right_panel,
            Some(crate::RightPanelContent::Spreadsheet)
                | Some(crate::RightPanelContent::Data)
                | Some(crate::RightPanelContent::Regression)
        );
        // Cargar ejemplos si el documento está vacío.
        if self.document.object_count() == 0 {
            self.load_perspective_examples(p);
        }
        if view_changed {
            self.document.bump_version();
        }
    }

    /// Carga objetos de ejemplo apropiados para la perspectiva dada.
    ///
    /// Se invoca al cambiar de perspectiva cuando el documento está vacío,
    /// ofreciendo un punto de partida similar a GeoGebra.
    pub(crate) fn load_perspective_examples(&mut self, p: Perspective) {
        let time = 0.0;
        let run = |app: &mut Self, cmd: &str| {
            let mut buf = cmd.to_string();
            let outcome = crate::commands::process_input(&mut app.document, &mut buf);
            app.handle_command_outcome(outcome, time, cmd);
        };
        match p {
            Perspective::Geometry2D => {
                self.document.add_object(GeoObject::Point(
                    PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
                ));
                self.document.add_object(GeoObject::Point(
                    PointObj::new(Point2::new(3.0, 2.0)).with_label("B"),
                ));
                self.document.add_object(GeoObject::Line(
                    LineObj::new(Point2::new(-2.0, -1.0), Point2::new(4.0, 3.0)).with_label("l"),
                ));
                self.document.add_object(GeoObject::Circle(
                    CircleObj::new(Point2::new(1.0, 1.0), 2.0).with_label("c"),
                ));
                self.document.add_object(GeoObject::Function(
                    FunctionObj::new("sin(x)").with_label("f(x)"),
                ));
            }
            Perspective::Geometry3D => {
                self.document.add_object(GeoObject::Cube3D(
                    Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0).with_label("C1"),
                ));
                self.document.add_object(GeoObject::Sphere3D(
                    Sphere3DObj::new(Point3D::new(2.0, 1.0, 0.0), 1.0).with_label("S1"),
                ));
            }
            Perspective::AlgebraCas => {
                self.document.add_object(GeoObject::Function(
                    FunctionObj::new("x^2").with_label("f(x)"),
                ));
                self.document.add_object(GeoObject::Function(
                    FunctionObj::new("sin(x)").with_label("g(x)"),
                ));
            }
            Perspective::Calculus => {
                self.document.add_object(GeoObject::Function(
                    FunctionObj::new("x^3").with_label("f(x)"),
                ));
                run(self, "Integral[x^3, x, 0, x]");
            }
            Perspective::Probability => {
                run(self, "Normal[0, 1]");
            }
            Perspective::Statistics => {
                run(self, "Scatter[(1,2),(2,3),(3,5),(4,4),(5,6)]");
            }
            Perspective::Complex => {
                // ComplexMapping[1/z, I] requiere un target etiquetado "I".
                run(self, "x^2 + y^2 = 1");
                run(self, "ComplexMapping[1/z, I]");
            }
            Perspective::Dynamics => {
                run(self, "Lorenz[]");
            }
            Perspective::DataAnalysis => {
                run(self, "Scatter[(1,2),(2,3),(3,5),(4,4),(5,6)]");
            }
            Perspective::Exam => {
                // Modo examen: documento vacío intencionalmente.
            }
        }
    }

    pub(crate) fn pending_action_hint(&self) -> Option<String> {
        Some(match &self.pending_action {
            PendingAction::None => return None,
            PendingAction::Distance { first } if first.is_none() => {
                "Distancia: selecciona el primer punto".to_string()
            }
            PendingAction::Distance { .. } => "Distancia: selecciona el segundo punto".to_string(),
            PendingAction::Angle { first } if first.is_none() => {
                "Ángulo: selecciona la primera recta".to_string()
            }
            PendingAction::Angle { .. } => "Ángulo: selecciona la segunda recta".to_string(),
            PendingAction::Tangent { first } if first.is_none() => {
                "Tangente: selecciona la circunferencia".to_string()
            }
            PendingAction::Tangent { .. } => "Tangente: selecciona la recta".to_string(),
            PendingAction::Coincident { first } if first.is_none() => {
                "Coincidente: selecciona el primer punto".to_string()
            }
            PendingAction::Coincident { .. } => {
                "Coincidente: selecciona el segundo punto".to_string()
            }
            PendingAction::Horizontal { .. } => "Horizontal: selecciona una recta".to_string(),
            PendingAction::Vertical { .. } => "Vertical: selecciona una recta".to_string(),
            PendingAction::EqualLength { first } if first.is_none() => {
                "Longitud igual: selecciona el primer segmento".to_string()
            }
            PendingAction::EqualLength { .. } => {
                "Longitud igual: selecciona el segundo segmento".to_string()
            }
            PendingAction::Symmetry { point, .. } if point.is_none() => {
                "Simetría: selecciona el punto original".to_string()
            }
            PendingAction::Symmetry { mirror_point, .. } if mirror_point.is_none() => {
                "Simetría: selecciona el punto imagen".to_string()
            }
            PendingAction::Symmetry { line, .. } if line.is_none() => {
                "Simetría: selecciona el eje".to_string()
            }
            PendingAction::Symmetry { .. } => "Simetría: confirma la restricción".to_string(),
            PendingAction::EllipseByFoci { f1, .. } if f1.is_none() => {
                "Elipse: selecciona el primer foco".to_string()
            }
            PendingAction::EllipseByFoci { f2, .. } if f2.is_none() => {
                "Elipse: selecciona el segundo foco".to_string()
            }
            PendingAction::EllipseByFoci { .. } => "Elipse: selecciona un punto".to_string(),
            PendingAction::ParabolaByFocusDirectrix { focus, .. } if focus.is_none() => {
                "Parábola: selecciona el foco".to_string()
            }
            PendingAction::ParabolaByFocusDirectrix { directrix, .. } if directrix.is_none() => {
                "Parábola: selecciona la directriz".to_string()
            }
            PendingAction::ParabolaByFocusDirectrix { .. } => "Parábola: confirma".to_string(),
            PendingAction::HyperbolaByFoci { f1, .. } if f1.is_none() => {
                "Hipérbola: selecciona el primer foco".to_string()
            }
            PendingAction::HyperbolaByFoci { f2, .. } if f2.is_none() => {
                "Hipérbola: selecciona el segundo foco".to_string()
            }
            PendingAction::HyperbolaByFoci { .. } => "Hipérbola: selecciona un punto".to_string(),
            PendingAction::ConicByFivePoints { points } => {
                format!("Cónica: selecciona el punto {} de 5", points.len() + 1)
            }
            PendingAction::BooleanUnion { first } if first.is_none() => {
                "Unión: selecciona el primer polígono".to_string()
            }
            PendingAction::BooleanUnion { .. } => {
                "Unión: selecciona el segundo polígono".to_string()
            }
            PendingAction::BooleanIntersection { first } if first.is_none() => {
                "Intersección: selecciona el primer polígono".to_string()
            }
            PendingAction::BooleanIntersection { .. } => {
                "Intersección: selecciona el segundo polígono".to_string()
            }
            PendingAction::BooleanDifference { first } if first.is_none() => {
                "Diferencia: selecciona el primer polígono".to_string()
            }
            PendingAction::BooleanDifference { .. } => {
                "Diferencia: selecciona el segundo polígono".to_string()
            }
            PendingAction::BooleanXor { first } if first.is_none() => {
                "XOR: selecciona el primer polígono".to_string()
            }
            PendingAction::BooleanXor { .. } => "XOR: selecciona el segundo polígono".to_string(),
        })
    }

    fn is_point(&self, id: ObjectId) -> bool {
        matches!(self.document.get_object(id), Some(GeoObject::Point(_)))
    }

    fn is_line(&self, id: ObjectId) -> bool {
        matches!(self.document.get_object(id), Some(GeoObject::Line(_)))
    }

    fn is_circle(&self, id: ObjectId) -> bool {
        matches!(self.document.get_object(id), Some(GeoObject::Circle(_)))
    }

    fn is_polygon(&self, id: ObjectId) -> bool {
        matches!(self.document.get_object(id), Some(GeoObject::Polygon(_)))
    }

    fn line_direction(&self, id: ObjectId) -> Option<Point2> {
        if let Some(GeoObject::Line(l)) = self.document.get_object(id) {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 1e-12 {
                return Some(Point2::new(dx / len, dy / len));
            }
        }
        None
    }

    fn angle_between_lines(&self, a: ObjectId, b: ObjectId) -> Option<f64> {
        let d1 = self.line_direction(a)?;
        let d2 = self.line_direction(b)?;
        let dot = d1.x * d2.x + d1.y * d2.y;
        let angle = dot.clamp(-1.0, 1.0).acos().to_degrees();
        Some(angle)
    }

    pub(crate) fn handle_pending_object_click(&mut self, id: ObjectId, time: f64) {
        use std::mem;
        let action = mem::take(&mut self.pending_action);
        match action {
            PendingAction::None => {
                self.pending_action = PendingAction::None;
                return;
            }
            PendingAction::Distance { first } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::Distance { first };
                    return;
                }
                if let Some(first) = first {
                    if let (Some(p1), Some(p2)) = (
                        self.document.point_position(first),
                        self.document.point_position(id),
                    ) {
                        self.save_state();
                        let target = p1.distance(&p2);
                        self.document.add_distance_constraint(first, id, target);
                        self.re_evaluate_constraints(&[]);
                    }
                } else {
                    self.pending_action = PendingAction::Distance { first: Some(id) };
                    return;
                }
            }
            PendingAction::Angle { first } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Angle { first };
                    return;
                }
                if let Some(first) = first {
                    if let Some(target) = self.angle_between_lines(first, id) {
                        self.save_state();
                        self.document.add_angle_constraint(first, id, target);
                        self.re_evaluate_constraints(&[]);
                    }
                } else {
                    self.pending_action = PendingAction::Angle { first: Some(id) };
                    return;
                }
            }
            PendingAction::Tangent { first } => {
                let valid = if first.is_none() {
                    self.is_circle(id)
                } else {
                    self.is_line(id)
                };
                if !valid {
                    self.pending_action = PendingAction::Tangent { first };
                    return;
                }
                if let Some(first) = first {
                    self.save_state();
                    self.document.add_tangent_constraint(first, id);
                    self.re_evaluate_constraints(&[]);
                } else {
                    self.pending_action = PendingAction::Tangent { first: Some(id) };
                    return;
                }
            }
            PendingAction::Coincident { first } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::Coincident { first };
                    return;
                }
                if let Some(first) = first {
                    self.save_state();
                    self.document.add_coincident_constraint(first, id);
                    self.re_evaluate_constraints(&[]);
                } else {
                    self.pending_action = PendingAction::Coincident { first: Some(id) };
                    return;
                }
            }
            PendingAction::Horizontal { line } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Horizontal { line };
                    return;
                }
                self.save_state();
                self.document.add_horizontal_constraint(id);
                self.re_evaluate_constraints(&[]);
            }
            PendingAction::Vertical { line: _ } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Vertical { line: None };
                    return;
                }
                self.save_state();
                self.document.add_vertical_constraint(id);
                self.re_evaluate_constraints(&[]);
            }
            PendingAction::EqualLength { first } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::EqualLength { first };
                    return;
                }
                if let Some(first) = first {
                    self.save_state();
                    self.document.add_equal_length_constraint(first, id);
                    self.re_evaluate_constraints(&[]);
                } else {
                    self.pending_action = PendingAction::EqualLength { first: Some(id) };
                    return;
                }
            }
            PendingAction::Symmetry {
                point,
                mirror_point,
                line,
            } => {
                let expected_point = point.is_none();
                let expected_mirror = point.is_some() && mirror_point.is_none();
                let valid = if expected_point || expected_mirror {
                    self.is_point(id)
                } else {
                    self.is_line(id)
                };
                if !valid {
                    self.pending_action = PendingAction::Symmetry {
                        point,
                        mirror_point,
                        line,
                    };
                    return;
                }
                if expected_point {
                    self.pending_action = PendingAction::Symmetry {
                        point: Some(id),
                        mirror_point,
                        line,
                    };
                    return;
                } else if expected_mirror {
                    self.pending_action = PendingAction::Symmetry {
                        point,
                        mirror_point: Some(id),
                        line,
                    };
                    return;
                } else if let (Some(p), Some(m)) = (point, mirror_point) {
                    self.save_state();
                    self.document.add_symmetry_constraint(p, m, id);
                    self.re_evaluate_constraints(&[]);
                } else {
                    // Estado inconsistente: devolver la acción para reintentar
                    self.pending_action = PendingAction::Symmetry {
                        point,
                        mirror_point,
                        line,
                    };
                    return;
                }
            }
            PendingAction::EllipseByFoci { f1, f2 } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::EllipseByFoci { f1, f2 };
                    return;
                }
                if f1.is_none() {
                    self.pending_action = PendingAction::EllipseByFoci { f1: Some(id), f2 };
                    return;
                } else if f2.is_none() {
                    self.pending_action = PendingAction::EllipseByFoci { f1, f2: Some(id) };
                    return;
                } else if let (Some(f1_id), Some(f2_id)) = (f1, f2) {
                    self.save_state();
                    let inputs = [f1_id, f2_id, id];
                    self.document
                        .add_ellipse_by_foci_constraint(inputs[0], inputs[1], inputs[2]);
                    let order = self.document.propagation_order(&inputs);
                    self.re_evaluate_constraints(&order);
                }
            }
            PendingAction::ParabolaByFocusDirectrix { focus, directrix } => {
                let expected_focus = focus.is_none();
                let valid = if expected_focus {
                    self.is_point(id)
                } else {
                    self.is_line(id)
                };
                if !valid {
                    self.pending_action =
                        PendingAction::ParabolaByFocusDirectrix { focus, directrix };
                    return;
                }
                if expected_focus {
                    self.pending_action = PendingAction::ParabolaByFocusDirectrix {
                        focus: Some(id),
                        directrix,
                    };
                    return;
                } else if let Some(focus_id) = focus {
                    self.save_state();
                    let inputs = [focus_id, id];
                    self.document
                        .add_parabola_by_focus_directrix_constraint(inputs[0], inputs[1]);
                    let order = self.document.propagation_order(&inputs);
                    self.re_evaluate_constraints(&order);
                }
            }
            PendingAction::HyperbolaByFoci { f1, f2 } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::HyperbolaByFoci { f1, f2 };
                    return;
                }
                if f1.is_none() {
                    self.pending_action = PendingAction::HyperbolaByFoci { f1: Some(id), f2 };
                    return;
                } else if f2.is_none() {
                    self.pending_action = PendingAction::HyperbolaByFoci { f1, f2: Some(id) };
                    return;
                } else if let (Some(f1_id), Some(f2_id)) = (f1, f2) {
                    self.save_state();
                    let inputs = [f1_id, f2_id, id];
                    self.document
                        .add_hyperbola_by_foci_constraint(inputs[0], inputs[1], inputs[2]);
                    let order = self.document.propagation_order(&inputs);
                    self.re_evaluate_constraints(&order);
                }
            }
            PendingAction::ConicByFivePoints { mut points } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::ConicByFivePoints { points };
                    return;
                }
                points.push(id);
                if points.len() < 5 {
                    self.pending_action = PendingAction::ConicByFivePoints { points };
                    return;
                }
                self.save_state();
                let cons = self.document.add_conic_by_five_points_constraint(&points);
                let order = self.document.propagation_order(&points);
                self.re_evaluate_constraints(&order);
                let _ = cons;
            }
            PendingAction::BooleanUnion { .. }
            | PendingAction::BooleanIntersection { .. }
            | PendingAction::BooleanDifference { .. }
            | PendingAction::BooleanXor { .. } => {
                if !self.is_polygon(id) {
                    self.pending_action = action;
                    return;
                }
                if let Some(first) = match &action {
                    PendingAction::BooleanUnion { first }
                    | PendingAction::BooleanIntersection { first }
                    | PendingAction::BooleanDifference { first }
                    | PendingAction::BooleanXor { first } => *first,
                    _ => None,
                } {
                    let first_label = self
                        .document
                        .get_object(first)
                        .map(|o| o.label().to_string())
                        .unwrap_or_default();
                    let second_label = self
                        .document
                        .get_object(id)
                        .map(|o| o.label().to_string())
                        .unwrap_or_default();
                    let cmd_name = action.boolean_cmd_name().unwrap_or("PolygonUnion");
                    let mut cmd = format!("{}[{}, {}]", cmd_name, first_label, second_label);
                    self.save_state();
                    let outcome = crate::commands::process_input(&mut self.document, &mut cmd);
                    self.handle_command_outcome(outcome, time, &cmd);
                } else {
                    self.pending_action = action.with_boolean_first(id);
                    return;
                }
            }
        }
        self.current_tool = Tool::Select;
        self.tool_ghost = None;
        self.pending_action = PendingAction::None;
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
        #[cfg(feature = "profile")]
        puffin::profile_scope!("app_update");

        if self.style_applied != Some(self.dark_mode) {
            configure_modern_style(ctx);
            self.style_applied = Some(self.dark_mode);
        }
        if self.is_view_changing
            && self.last_interaction_time.elapsed() > Duration::from_millis(150)
        {
            self.is_view_changing = false;
            self.document.render_quality = RenderQuality::High;
        }

        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        let mut any_animating = false;
        let mut changes = Vec::new();

        for (name, meta) in &self.document.variable_meta {
            if meta.animating && meta.animation_speed != 0.0 {
                any_animating = true;
                if let Some(&current_val) = self.document.variables.get(name) {
                    let mut next_val = current_val + meta.animation_speed * dt;
                    let mut next_speed = meta.animation_speed;
                    if next_val > meta.max {
                        next_val = meta.max;
                        next_speed = -meta.animation_speed;
                    } else if next_val < meta.min {
                        next_val = meta.min;
                        next_speed = -meta.animation_speed;
                    }
                    changes.push((name.clone(), next_val, next_speed));
                }
            }
        }

        for (name, new_val, new_speed) in changes {
            self.document.variables.insert(name.clone(), new_val);
            if let Some(meta) = self.document.variable_meta.get_mut(&name) {
                meta.animation_speed = new_speed;
            }
        }

        if any_animating {
            self.document.bump_version();
            ctx.request_repaint();
        }

        // Keyboard shortcuts
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.undo();
        }
        if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
            || ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl && !i.modifiers.shift)
        {
            self.redo();
        }
        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.delete_selected();
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.current_tool = Tool::Select;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::F2)) {
            self.current_tool = Tool::Point;
            self.tool_ghost = None;
        }
        if ctx.input(|i| i.key_pressed(Key::F3)) {
            self.current_tool = Tool::Line;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::F4)) {
            self.current_tool = Tool::Circle;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::F5)) {
            self.current_tool = Tool::Polygon;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::F6)) {
            self.current_tool = Tool::Function;
            self.tool_ghost = None;
        }
        if ctx.input(|i| i.key_pressed(Key::R) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Root;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::E) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Extremum;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::I) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::XIntercept;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::X) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Intersect;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::N) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Inflection;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::S) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Segment;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::Y) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Ray;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::V) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Vector;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::M) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.current_tool = Tool::Midpoint;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl && i.modifiers.shift) {
            self.current_tool = Tool::YIntercept;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::A) && i.modifiers.ctrl) {
            self.current_tool = Tool::Analyze;
            self.tool_ghost = None;
            self.reset_tool_input();
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.current_tool = Tool::Select;
            self.tool_ghost = None;
            self.reset_tool_input();
            self.clear_pending_action();
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
        // G: toggle snap-to-grid (sin modificadores).
        if ctx.input(|i| i.key_pressed(Key::G) && !i.modifiers.ctrl && !i.modifiers.alt) {
            self.snap_to_grid = !self.snap_to_grid;
            self.snap_config.snap_to_grid = self.snap_to_grid;
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
                snap: self.snap_config.clone(),
            });
        }
        // Ctrl+Shift+1..9,0: cambiar de perspectiva (1=Geometry2D … 9=DataAnalysis, 0=Exam).
        {
            const NUM_KEYS: [(Key, Perspective); 10] = [
                (Key::Num1, Perspective::Geometry2D),
                (Key::Num2, Perspective::Geometry3D),
                (Key::Num3, Perspective::AlgebraCas),
                (Key::Num4, Perspective::Calculus),
                (Key::Num5, Perspective::Probability),
                (Key::Num6, Perspective::Statistics),
                (Key::Num7, Perspective::Complex),
                (Key::Num8, Perspective::Dynamics),
                (Key::Num9, Perspective::DataAnalysis),
                (Key::Num0, Perspective::Exam),
            ];
            for (key, p) in NUM_KEYS {
                if ctx.input(|i| i.key_pressed(key) && i.modifiers.ctrl && i.modifiers.shift) {
                    self.set_perspective(p);
                    break;
                }
            }
        }
        // Ctrl+K: abrir la paleta de comandos.
        if ctx.input(|i| i.key_pressed(Key::K) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.command_palette.open = true;
            self.command_palette.search.clear();
            self.command_palette.selected_index = 0;
        }

        let is_dark = self.dark_mode;
        {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("ui");

            crate::ui::draw_top_bar(self, ctx);
            self.sync_pending_action_with_tool();
            match self.sidebar_tab {
                0 => crate::algebra::draw_algebra_panel(self, ctx),
                1 => crate::tools_panel::draw_tools_panel(self, ctx),
                2 => crate::panels::draw_cas_panel(self, ctx),
                3 => crate::panels::draw_table_panel(self, ctx),
                4 => crate::panels::draw_spreadsheet_panel(self, ctx),
                5 => crate::panels::draw_view_panel(self, ctx),
                _ => crate::panels::draw_empty_panel(self, ctx),
            }

            crate::ui::draw_bottom_bar(self, ctx);

            if self.keyboard_visible {
                crate::keyboard::draw_math_keyboard(self, ctx);
            }

            if self.show_spreadsheet {
                crate::panels::draw_right_spreadsheet(self, ctx);
            }
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
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope!("input");
                            self.handle_canvas_input(ui, canvas_rect);
                        }

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
                            // Draw grid and axes BEFORE the GPU callback so they are underneath
                            let mut painter = ui.painter().clone();
                            painter.set_clip_rect(canvas_rect);
                            self.draw_grid(&painter, canvas_rect);
                            self.draw_axes(&painter, canvas_rect);

                            let callback = egui_wgpu::Callback::new_paint_callback(
                                canvas_rect,
                                crate::canvas::CanvasCallback {
                                    document: self.document_for_callback(),
                                    dark_mode: self.dark_mode,
                                },
                            );
                            ui.painter().add(egui::epaint::Shape::Callback(callback));

                            // Overlay only: text, points drawn by CPU on top of GPU
                            self.draw_objects(&painter, canvas_rect, true);
                        } else {
                            let mut painter = ui.painter().clone();
                            painter.set_clip_rect(canvas_rect);
                            self.draw_grid(&painter, canvas_rect);
                            self.draw_axes(&painter, canvas_rect);
                            self.draw_objects(&painter, canvas_rect, false);
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

                    {
                        #[cfg(feature = "profile")]
                        puffin::profile_scope!("input");
                        self.handle_canvas_3d_input(ui, canvas_rect);
                    }

                    if self.use_gpu {
                        // Draw 3D grid BEFORE the GPU callback
                        self.draw_3d_grid(ui.painter(), canvas_rect, w, h, false);

                        let callback = egui_wgpu::Callback::new_paint_callback(
                            canvas_rect,
                            crate::canvas::Canvas3DCallback {
                                document: self.document_for_callback(),
                                camera: self.camera,
                                dark_mode: self.dark_mode,
                                screen_w: w,
                                screen_h: h,
                            },
                        );
                        ui.painter().add(egui::epaint::Shape::Callback(callback));

                        // Overlay only: text, points, and labels drawn by CPU on top of GPU
                        self.draw_3d_objects(ui.painter(), canvas_rect, w, h, true);
                    } else {
                        self.draw_3d_grid(ui.painter(), canvas_rect, w, h, false);
                        self.draw_3d_objects(ui.painter(), canvas_rect, w, h, false);
                    }

                    // Draw 3D tool ghost on top with CPU painter
                    if let Some(GeoObject::Point3D(ghost)) = &self.tool_ghost {
                        let painter = ui.painter();
                        let origin = canvas_rect.min;
                        if let Some(pt) = self.camera.project(&ghost.position, w, h) {
                            let pos = origin + egui::Vec2::new(pt.0, pt.1);
                            // Render ghost with reduced opacity
                            let ghost_color = egui::Color32::from_rgba_premultiplied(
                                (ghost.color.r * 255.0).clamp(0.0, 255.0) as u8,
                                (ghost.color.g * 255.0).clamp(0.0, 255.0) as u8,
                                (ghost.color.b * 255.0).clamp(0.0, 255.0) as u8,
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

        // Splash screen overlay (PR 6 polish): aparece por 1.5s al inicio
        // con el logo, nombre y versión. Se desvanece con un fade-out.
        if let Some(start) = self.splash_start {
            let elapsed = start.elapsed();
            let total_ms = 1500_u128;
            let fade_out_start_ms = 1000_u128;
            let elapsed_ms = elapsed.as_millis();
            if elapsed_ms < total_ms {
                let _theme = grafito_ui::theme::current_theme(ctx);
                let alpha = if elapsed_ms < fade_out_start_ms {
                    1.0
                } else {
                    let t = (elapsed_ms - fade_out_start_ms) as f32
                        / (total_ms - fade_out_start_ms) as f32;
                    1.0 - t
                };
                egui::Area::new(egui::Id::new("splash_overlay"))
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        let screen = ui.ctx().screen_rect();
                        ui.painter().rect_filled(
                            screen,
                            0.0,
                            egui::Color32::from_black_alpha((220.0 * alpha) as u8),
                        );
                        // Logo + nombre centrados
                        ui.vertical_centered(|ui| {
                            let (logo_rect, _) = ui.allocate_exact_size(
                                egui::vec2(128.0, 128.0),
                                egui::Sense::hover(),
                            );
                            if ui.is_rect_visible(logo_rect) {
                                if let Ok(img) = image::open("assets/grafito-icon-256x256.png") {
                                    let rgba = img.to_rgba8();
                                    let (w, h) = (rgba.width() as f32, rgba.height() as f32);
                                    let size = logo_rect.width().min(logo_rect.height());
                                    let tex = ctx.load_texture(
                                        "splash_logo",
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [w as usize, h as usize],
                                            rgba.as_raw(),
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    );
                                    let rect = egui::Rect::from_center_size(
                                        logo_rect.center(),
                                        egui::vec2(size, size),
                                    );
                                    ui.painter().image(
                                        tex.id(),
                                        rect,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::from_white_alpha((255.0 * alpha) as u8),
                                    );
                                }
                            }
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new("Grafito")
                                    .size(36.0)
                                    .strong()
                                    .color(egui::Color32::from_white_alpha((255.0 * alpha) as u8)),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                    .size(14.0)
                                    .color(egui::Color32::from_white_alpha((180.0 * alpha) as u8)),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Geometría interactiva · Álgebra · Cálculo")
                                    .size(13.0)
                                    .color(egui::Color32::from_white_alpha((150.0 * alpha) as u8)),
                            );
                        });
                    });
                ctx.request_repaint();
            } else {
                self.splash_start = None;
            }
        }

        // Paleta de comandos (Ctrl+K): ventana flotante de búsqueda rápida.
        if let Some(name) = self.command_palette.show(ctx) {
            self.apply_palette_command(&name, ctx);
        }

        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-12.0, -12.0))
            .show(ctx, |ui| {
                let time = ui.ctx().input(|i| i.time);
                self.toasts.draw(ui, time);
            });
    }
}

/// Run the native Grafito desktop application.
pub fn run_app() -> Result<(), eframe::Error> {
    env_logger::init();

    #[cfg(feature = "profile")]
    let mut profile = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Grafito v1.0.0-beta");
                println!("Usage: grafito [OPTIONS]");
                println!("Options:");
                println!("  -h, --help       Print help information");
                #[cfg(feature = "profile")]
                println!(
                    "  --profile        Start a puffin_http profiler server on port {}",
                    puffin_http::DEFAULT_PORT
                );
                return Ok(());
            }
            #[cfg(feature = "profile")]
            "--profile" => profile = true,
            _ => {}
        }
    }

    #[cfg(feature = "profile")]
    if profile {
        let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
        match puffin_http::Server::new(&server_addr) {
            Ok(server) => {
                // Leak the server so its background thread lives for the app lifetime.
                Box::leak(Box::new(server));
                puffin::set_scopes_on(true);
                log::info!("Puffin profiling server started on {}", server_addr);
            }
            Err(e) => log::warn!("Failed to start puffin profiling server: {}", e),
        }
    }

    let icon = {
        let image_data = include_bytes!("../../../assets/grafito-icon-256x256.png");
        match image::load_from_memory(image_data) {
            Ok(img) => {
                let img = img.into_rgba8();
                let (width, height) = img.dimensions();
                egui::IconData {
                    rgba: img.into_raw(),
                    width,
                    height,
                }
            }
            Err(e) => {
                log::warn!("Failed to load icon: {}. Using fallback.", e);
                egui::IconData::default()
            }
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_decorations(true)
            .with_transparent(false)
            .with_app_id("grafito")
            .with_icon(std::sync::Arc::new(icon)),
        multisampling: crate::MSAA_SAMPLES,
        ..Default::default()
    };
    eframe::run_native(
        "Grafito",
        options,
        Box::new(|cc| Ok(Box::new(GrafitoApp::new(cc)))),
    )
}
