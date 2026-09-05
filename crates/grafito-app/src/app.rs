//! Main application state and eframe orchestration.
//!
//! Holds `GrafitoApp`, its constructor, file/undo helpers, and the top-level
//! `eframe::App::update` loop that dispatches rendering to focused UI modules.
//!
//! # Presupuestos de rendimiento (Piel)
//! - Tessellation egui: 1-2 ms/frame con 10K vértices (feature `egui/rayon`,
//!   epaint/rayon, ver `Cargo.toml` workspace). Medir con `--features profile`
//!   + `puffin_viewer 127.0.0.1:8585` antes de optimizar.
//! - GPU compute: `domain_coloring_compute` 500×500 = 250k cells en un único
//!   dispatch wgpu (grafito-render). CPU submit << GPU time ⇒ GPU-bound.

use crate::utils::{load_config, save_config, AppConfig, AppLocale, AutosaveDebouncer};
use crate::{Perspective, ViewMode};
use egui::{Key, Pos2};
use grafito_core::{
    ChangeSet, CircleObj, Cube3DObj, Document, EllipseObj, FunctionObj, GeoObject, HyperbolaObj,
    LineObj, ObjectId, ParabolaObj, PointObj, RenderQuality, Sphere3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::Tool;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use grafito_command::commands::{register_gpu_function_evaluator, GpuFunctionEvaluator};

/// Máximo de entradas de undo. Usa `VecDeque<Document>` con `pop_front` O(1)
/// (antes `Vec` con `remove(0)` O(n) shift). Ver `push_history_snapshot` y
/// `crate::controllers::DocumentController` para la evolución con contador.
pub(crate) const MAX_UNDO: usize = 50;
/// Presupuesto global de memoria para undo: 50 MB. Aunque `MAX_UNDO=50` ya es
/// `VecDeque` O(1) `pop_front`, cada `Document` clonado puede pesar hasta
/// ~10 MB (5000 objetos × 200 KB ⇒ `Document::estimated_bytes()`), por lo que
/// 50 entradas sin cota = 500 MB. Este presupuesto corta la cola cuando el
/// total estimado supera 50 MB. Ver `Document::estimated_bytes()` para la
/// estimación `max(object_count*200KiB, json_len, 8KiB)`.
pub(crate) const MAX_UNDO_BYTES: usize = 50 * 1024 * 1024;

/// Cota del protocolo de construcción: 500 entradas cronológicas máximo.
/// Tras cada `push` se trunca manteniendo las más recientes (`drain 0..excess`)
/// para presupuesto O(n) acotado (500) y orden cronológico.
pub(crate) const MAX_CONSTRUCTION_LOG: usize = 500;

/// Job de guardado en background — evita bloquear UI thread (60fps) en `save_document`.
/// Pattern `spawn_profile_save` (assistant.rs:41-51) con `sync_channel(1)` + `request_repaint`.
#[allow(dead_code)]
pub(crate) struct PendingSaveJob {
    pub receiver: Receiver<Result<PathBuf, String>>,
    pub path: PathBuf,
}
/// Job de apertura en background — evita bloquear UI thread en `choose_and_open_document`.
#[allow(dead_code)]
pub(crate) struct PendingOpenJob {
    pub receiver: Receiver<Result<(PathBuf, Document), String>>,
}
/// Job de export en background — evita bloquear UI thread en `export_with_dialog`.
#[allow(dead_code)]
pub(crate) struct PendingExportJob {
    pub receiver: Receiver<Result<PathBuf, String>>,
    pub format: crate::export::ExportFormat,
}

/// Presupuestos espejo de `grafito-ggb` (sin añadir dependencia para no tocar
/// `Cargo.toml`): `MAX_GGB_BYTES` 64MiB, `MAX_GGB_XML_BYTES` 10MiB,
/// `MAX_ELEMS` 5000, `MAX_ZIP_ENTRIES` 4096. Ver
/// `crates/grafito-ggb/src/lib.rs:12-16`.
pub(crate) const GGB_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const GGB_MAX_XML_BYTES: u64 = 10 * 1024 * 1024;
pub(crate) const GGB_MAX_ELEMS: usize = 5000;
pub(crate) const GGB_MAX_ZIP_ENTRIES: usize = 4096;
pub(crate) const GGB_XML_NAME: &str = "geogebra.xml";
/// Cota de expresión espejo de `grafito-ggb::MAX_EXPR_CHARS` (2000) y de
/// `grafito-core::validation::MAX_EXPR_LENGTH` (2000).
pub(crate) const GGB_MAX_EXPR_CHARS: usize = 2000;

/// Comando Grafito mapeado desde `.ggb` — espejo de `grafito-ggb::MappedObject`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GgbMappedCommand {
    pub kind: String,
    pub command: String,
}

/// Elemento omitido con razón honesta — espejo de `grafito-ggb::OmittedObject`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GgbOmitted {
    pub kind: String,
    pub label: String,
    pub reason: String,
}

/// Reporte de importación — espejo de `grafito-ggb::ImportReport` con
/// `.commands()` y `.summary()`. Nunca fallo silencioso: `omitted` siempre
/// explica cada salto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GgbImportReport {
    pub mapped: Vec<GgbMappedCommand>,
    pub omitted: Vec<GgbOmitted>,
}

impl GgbImportReport {
    pub(crate) fn commands(&self) -> Vec<String> {
        self.mapped.iter().map(|m| m.command.clone()).collect()
    }

    pub(crate) fn summary(&self) -> String {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for m in &self.mapped {
            *counts.entry(m.kind.as_str()).or_insert(0) += 1;
        }
        let tipos = counts
            .iter()
            .map(|(tipo, n)| format!("{tipo} x{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let detalle = if tipos.is_empty() {
            "sin objetos"
        } else {
            &tipos
        };
        format!(
            "ggb importado: {} objetos ({}) + {} omitidos",
            self.mapped.len(),
            detalle,
            self.omitted.len()
        )
    }

    /// Detalle honesto de omitidos para el toast (máx. 3 + contador resto).
    pub(crate) fn omitted_detail(&self) -> String {
        if self.omitted.is_empty() {
            return "sin omitidos".to_string();
        }
        let primeros = self
            .omitted
            .iter()
            .take(3)
            .map(|o| {
                if o.label.is_empty() {
                    format!("{}: {}", o.kind, o.reason)
                } else {
                    format!("{} '{}': {}", o.kind, o.label, o.reason)
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        if self.omitted.len() > 3 {
            format!(
                "{} omitidos: {}… y {} más",
                self.omitted.len(),
                primeros,
                self.omitted.len() - 3
            )
        } else {
            format!("{} omitidos: {primeros}", self.omitted.len())
        }
    }
}

/// Job de importación `.ggb` en background — lectura + parse fuera del UI thread.
/// Pattern `spawn_profile_save` con `sync_channel(1)` + `request_repaint`.
pub(crate) struct PendingGgbImportJob {
    pub receiver: Receiver<Result<GgbImportReport, String>>,
    pub path: PathBuf,
}

/// Spawns document save in background — evita bloquear UI thread (60fps).
/// Pattern `spawn_profile_save` (assistant.rs:41-51) con `sync_channel(1)` + `request_repaint`.
#[allow(dead_code)]
pub(crate) fn spawn_document_save(
    document: Document,
    path: PathBuf,
    ctx: egui::Context,
) -> Receiver<Result<PathBuf, String>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("document-save".into())
        .spawn(move || {
            let res = write_document_to_path(&document, &path)
                .map(|_| path.clone())
                .map_err(|e| e.to_string());
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    rx
}

/// Spawns document open in background — evita bloquear UI thread.
#[allow(dead_code)]
pub(crate) fn spawn_document_open(
    path: PathBuf,
    ctx: egui::Context,
) -> Receiver<Result<(PathBuf, Document), String>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("document-open".into())
        .spawn(move || {
            let res = load_document_candidate(&path)
                .map(|doc| (path.clone(), doc))
                .map_err(|e| e.to_string());
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    rx
}

/// Spawns export en background — evita bloquear UI thread en `export_with_dialog`.
#[allow(dead_code)]
pub(crate) fn spawn_export(
    document: Document,
    path: PathBuf,
    format: crate::export::ExportFormat,
    ctx: egui::Context,
) -> Receiver<Result<PathBuf, String>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            let res: Result<crate::export::ExportReport, String> = match format {
                crate::export::ExportFormat::Svg => {
                    crate::export::export_svg(&document, &path).map_err(|e| e.to_string())
                }
                crate::export::ExportFormat::Png => {
                    crate::export::export_png(&document, &path).map_err(|e| e.to_string())
                }
                crate::export::ExportFormat::Tikz => {
                    crate::export::export_tikz(&document, &path).map_err(|e| e.to_string())
                }
            };
            let _ = tx.send(res.map(|_| path.clone()));
            ctx.request_repaint();
        });
    rx
}
const TRIG_GRAPH_LABEL: &str = "TrigGraph";
const TRIG_VALUE_LABEL: &str = "TrigValue";
const VIEW_SETTLE_DURATION: Duration = Duration::from_millis(150);
const MULTIDIMENSIONAL_MOTION_REPAINT_INTERVAL: Duration = Duration::from_millis(33);
pub(crate) const DEFAULT_3D_ORBIT_RADIANS_PER_SECOND: f32 = 0.3;
pub(crate) const DEFAULT_4D_ROTATION_RADIANS_PER_SECOND: f64 = 0.55;
pub(crate) const MIN_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 0.25;
pub(crate) const DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 1.0;
pub(crate) const MAX_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 2.0;

// ── F17 Repaint coalesce ─────────────────────────────────────────────────────
// El scheduler unificado de `GrafitoApp::update` (ver `update`) es la única
// fuente de `ctx.request_repaint_after` para el estado global (animating /
// warmup / busy). Los widgets periféricos NO deben llamar
// `ctx.request_repaint_after` directamente: deben pedir vía
// `GrafitoApp::request_repaint_budget` para que `RepaintBudget` acumule la
// necesidad mínima del frame y el scheduler la aplique una sola vez al final.
//
// Inventario F17 — fuentes de wake extra (todas coalescidas por el presupuesto):
//   1. whiteboard_ui.rs:482   — 16ms  pointer/touch down (canvas pizarra)
//   2. whiteboard_ui.rs:1188  — 16ms busy / 100ms idle (scheduler pizarra)
//   3. teaching_ui.rs:1021    — 80ms  playback animación nativa (12fps)
//   4. teaching_ui.rs:1057    — 48ms  pulso "Generando animación"
//   5. grafito-ui/animation.rs:232  — 50ms orb de pensamiento (fallback standalone)
//   6. grafito-ui/assistant.rs:3748 — 48ms pulso "Generando animación…"
//   7. grafito-ui/assistant.rs:3800 — 40ms playback media card (GIF-like)
//   8. app.rs:4176            — 33ms motion multidimensional (redundante con scheduler)
// Las fuentes 5-7 viven en `grafito-ui` (capa Piel, DAG `ui → app`) y no pueden
// alcanzar `GrafitoApp`; se mantienen como fallback local con constantes
// nombradas y quedan subsumidas por el scheduler cuando `assistant.is_pending`
// (el scheduler ya repinta a 16ms en ese caso).
// TODO(F17): si `whiteboard` idle 100ms solo alimenta `ctx.animate_bool` de la
// paleta, egui ya repinta automáticamente durante animaciones → se puede quitar.

/// Presupuesto de repintado coalescido por frame (F17).
///
/// Acumula la necesidad mínima de repintado de los widgets del frame y la
/// aplica una sola vez al final (`apply`). El intervalo más corto gana; el
/// resto se descarta. egui ya coalesce múltiples `request_repaint_after` al
/// mínimo, pero centralizar la política evita intervalos arbitrarios
/// dispersos (16/40/48/50/80/100ms) y deja una única ruta documentada:
/// `GrafitoApp::request_repaint_budget`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RepaintBudget {
    needed: Option<Duration>,
}

impl RepaintBudget {
    /// Pide repintado periódico con `delay`; conserva el mínimo acumulado.
    pub(crate) fn request(&mut self, delay: Duration) {
        self.needed = Some(match self.needed {
            Some(current) => current.min(delay),
            None => delay,
        });
    }

    /// Aplica el presupuesto acumulado a `ctx` (una sola llamada por frame).
    pub(crate) fn apply(self, ctx: &egui::Context) {
        if let Some(delay) = self.needed {
            if delay.is_zero() {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(delay);
            }
        }
    }
}

// ── F5 Scandinavian progressive disclosure — toolbar filtrado por nivel ──
// Sin laberinto: Primary (level_value `0..=PRIMARY_MAX`=`4`) → 5 grupos compactos
// [Move, Point, Line, Circle, Polygon]; Secondary (`5..=10`) → 8 (añade Pencil, Measure, Analysis);
// University (`11+`) → todos (Constraint, Boolean, Advanced, Dynamics, ThreeD…).
// Helper ligero vía `grafito_ui::toolbar::toolbar_groups_for_level_value(level_value)` o
// tipado `toolbar_groups_for_level(PedagogicalLevel)` sin romper API existente.
// Usa constantes `TOOLBAR_LEVEL_PRIMARY_MAX`/`SECONDARY_MAX` para evitar magia 4/10.
// Uso: `toolbar_groups_for_level_filtered(perspective.layout().visible_tool_groups, self.profile.level)`
// Si `level_value <= PRIMARY_MAX` fuerza compact set aunque la perspectiva pida más.

/// Filtra los grupos visibles de la perspectiva por nivel pedagógico (progressive disclosure).
/// Usa `TOOLBAR_LEVEL_PRIMARY_MAX` (4) y `SECONDARY_MAX` (10) vía `filter_groups_by_level`.
/// Intersecta con `perspective_groups` para respetar la perspectiva y no añadir grupos extra.
pub(crate) fn toolbar_groups_for_level_filtered(
    perspective_groups: &[grafito_ui::toolbar::ToolGroupId],
    level_value: u32,
) -> Vec<grafito_ui::toolbar::ToolGroupId> {
    grafito_ui::toolbar::filter_groups_by_level(perspective_groups, level_value)
}

/// Variante tipada que acepta `PedagogicalLevel` directamente (convierte vía `level_value()`).
pub(crate) fn toolbar_groups_for_pedagogical_level(
    perspective_groups: &[grafito_ui::toolbar::ToolGroupId],
    level: grafito_pedagogy::PedagogicalLevel,
) -> Vec<grafito_ui::toolbar::ToolGroupId> {
    grafito_ui::toolbar::filter_groups_by_pedagogical_level(perspective_groups, level)
}

/// The trigonometric explorer draws a 2D canvas overlay, so it is unavailable in 3D.
pub(crate) fn trig_animation_supported(view: ViewMode) -> bool {
    view != ViewMode::D3
}

fn canvas_resize_preview_active(resized_at: Option<Instant>, now: Instant) -> bool {
    resized_at
        .is_some_and(|resized_at| now.saturating_duration_since(resized_at) <= VIEW_SETTLE_DURATION)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtrlYShortcut {
    Redo,
    YIntercept,
}

pub(crate) const fn ctrl_y_shortcut(shift: bool) -> CtrlYShortcut {
    if shift {
        CtrlYShortcut::YIntercept
    } else {
        CtrlYShortcut::Redo
    }
}

// Re-export lifecycle canonical types to avoid duplication. `app.rs` retains
// `pub(crate)` visibility for `crate::app::DocumentLifecycle` etc. so tests
// and input modules keep working, but the single source of truth lives in
// `crate::lifecycle`. Dirty-check semantics ignore `view.screen_size`.
pub(crate) use crate::lifecycle::{
    command_mutated_document, documents_semantically_differ, file_shortcut,
    load_document_candidate, try_stage_numeric_constraint, write_document_to_path,
    DeferredFileActions, DeferredFileIntent, DocumentAction, DocumentActionRequest,
    DocumentLifecycle, FileCommand, SaveMode, UnsavedDecision, UnsavedResolution,
};
// `SaveAttempt` is an internal helper owned by `lifecycle`.
use crate::lifecycle::SaveAttempt;

pub(crate) fn apply_command_outcome(
    outcome: &grafito_command::commands::CommandOutcome,
    cas_result: &mut String,
    cas_history: &mut VecDeque<String>,
    toasts: &mut grafito_ui::toast::ToastManager,
    time: f64,
    input_was: &str,
) {
    match outcome {
        grafito_command::commands::CommandOutcome::Ok => {}
        grafito_command::commands::CommandOutcome::Message(message) => {
            let feedback = if message.is_empty() {
                "Comando completado"
            } else {
                message
            };
            *cas_result = feedback.to_string();
            if cas_history.len() > 20 {
                cas_history.pop_front();
            }
            cas_history.push_back(format!("> {}\n  {}", input_was, feedback));
            toasts.push(
                wrap_toast_message(feedback, 52),
                grafito_ui::toast::ToastKind::Info,
                time,
            );
        }
        grafito_command::commands::CommandOutcome::Error(message) => {
            *cas_result = message.clone();
            if cas_history.len() > 20 {
                cas_history.pop_front();
            }
            cas_history.push_back(format!("> {}\n  Error: {}", input_was, message));
            toasts.push(
                wrap_toast_message(&format!("Error: {}", message), 52),
                grafito_ui::toast::ToastKind::Error,
                time,
            );
        }
    }
}

pub(crate) fn apply_export_outcome(
    outcome: Result<crate::export::ExportReport, crate::export::ExportError>,
    cas_result: &mut String,
    toasts: &mut grafito_ui::toast::ToastManager,
    time: f64,
) {
    let (message, kind) = match outcome {
        Ok(report) => (report.summary(), grafito_ui::toast::ToastKind::Success),
        Err(error) => (error.to_string(), grafito_ui::toast::ToastKind::Error),
    };
    *cas_result = message.clone();
    toasts.push(wrap_toast_message(&message, 52), kind, time);
}

pub(crate) fn clear_submitted_input_on_success(
    input_text: &mut String,
    outcome: &grafito_command::commands::CommandOutcome,
) -> bool {
    if matches!(outcome, grafito_command::commands::CommandOutcome::Error(_)) {
        return false;
    }
    input_text.clear();
    true
}

pub(crate) fn sidebar_uses_cas_worksheet(sidebar_tab: usize) -> bool {
    sidebar_tab == crate::LeftPanelContent::Cas.default_sidebar_tab()
}

pub(crate) fn renderer_is_ready<T>(renderer: Option<&Arc<RwLock<Option<T>>>>) -> bool {
    renderer
        .and_then(|renderer| renderer.try_write().ok().map(|lock| lock.is_some()))
        .unwrap_or(false)
}

#[cfg(test)]
pub(crate) fn should_use_gpu_2d(
    use_gpu: bool,
    renderer_ready: bool,
    canvas_size: egui::Vec2,
) -> bool {
    use_gpu && renderer_ready && canvas_size.x > 0.0 && canvas_size.y > 0.0
}

pub(crate) const fn should_use_gpu_3d(
    use_gpu: bool,
    renderer_ready: bool,
    view_is_changing: bool,
) -> bool {
    use_gpu && renderer_ready && !view_is_changing
}

pub(crate) const fn should_prepare_gpu_3d(
    use_gpu: bool,
    renderer_ready: bool,
    view_is_changing: bool,
    automatic_motion_active: bool,
    retained_typed_four_d_phase: bool,
    readiness: crate::canvas::Scene3DReadiness,
) -> bool {
    if !should_use_gpu_3d(use_gpu, renderer_ready, view_is_changing)
        || automatic_motion_active
        || retained_typed_four_d_phase
    {
        return false;
    }
    match readiness {
        crate::canvas::Scene3DReadiness::CpuOnly => false,
        crate::canvas::Scene3DReadiness::Pending | crate::canvas::Scene3DReadiness::GpuReady => {
            true
        }
    }
}

pub(crate) const fn should_repaint_3d_warmup(readiness: crate::canvas::Scene3DReadiness) -> bool {
    matches!(readiness, crate::canvas::Scene3DReadiness::Pending)
}

pub(crate) fn should_animate_multidimensional_scene(
    view: ViewMode,
    motion_enabled: bool,
    has_visible_3d_object: bool,
) -> bool {
    view == ViewMode::D3 && motion_enabled && has_visible_3d_object
}

pub(crate) fn is_typed_four_d_projection(object: &GeoObject) -> bool {
    match object {
        GeoObject::RegularPolychoron4D(_) => true,
        GeoObject::RegularPolytopeND(polytope) => polytope.dimension == 4,
        _ => false,
    }
}

pub(crate) fn typed_four_d_motion_phase(four_d_phase: f64) -> Option<f64> {
    four_d_phase.is_finite().then_some(four_d_phase)
}

pub(crate) fn pause_default_multidimensional_motion(motion_enabled: &mut bool) -> bool {
    let changed = *motion_enabled;
    *motion_enabled = false;
    changed
}

pub(crate) fn reset_3d_view_and_pause_motion(
    camera: &mut Camera3D,
    canvas_width: f32,
    canvas_height: f32,
    motion_enabled: &mut bool,
) {
    pause_default_multidimensional_motion(motion_enabled);
    *camera = Camera3D::new(canvas_width / canvas_height.max(1.0));
}

pub(crate) fn toggle_default_multidimensional_motion(motion_enabled: &mut bool) -> bool {
    *motion_enabled = !*motion_enabled;
    *motion_enabled
}

pub(crate) fn normalize_multidimensional_motion_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(
            MIN_MULTIDIMENSIONAL_MOTION_SPEED,
            MAX_MULTIDIMENSIONAL_MOTION_SPEED,
        )
    } else {
        DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED
    }
}

pub(crate) fn advance_default_camera_orbit_at_speed(
    camera: &mut Camera3D,
    dt: f64,
    speed: f32,
) -> bool {
    if !dt.is_finite() || dt <= 0.0 {
        return false;
    }
    let speed = normalize_multidimensional_motion_speed(speed);
    camera.orbit(
        DEFAULT_3D_ORBIT_RADIANS_PER_SECOND * speed * dt.min(0.1) as f32,
        0.0,
    );
    camera.theta = camera.theta.rem_euclid(std::f32::consts::TAU);
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TransientRenderState {
    homotopy_time: f64,
    four_d_phase: f64,
    revision: u64,
}

impl TransientRenderState {
    pub(crate) fn advance_homotopy(&mut self, dt: f64) -> bool {
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        let next = self.homotopy_time + dt;
        if !next.is_finite() {
            return false;
        }
        self.homotopy_time = next;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn homotopy_time(self) -> f64 {
        self.homotopy_time
    }

    pub(crate) fn advance_four_d_phase(&mut self, delta: f64) -> bool {
        if !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        self.four_d_phase = (self.four_d_phase + delta).rem_euclid(std::f64::consts::TAU);
        true
    }

    pub(crate) fn four_d_phase(self) -> f64 {
        self.four_d_phase
    }

    pub(crate) fn revision(self) -> u64 {
        self.revision
    }
}

/// Refreshes render state after direct panel edits that bypass Document setters.
pub(crate) fn refresh_unversioned_document_change(
    before: &Document,
    document: &mut Document,
) -> bool {
    refresh_direct_document_change(document, before.version)
}

/// Actualiza el estado de render después de una mutación directa que pudo no
/// haber actualizado la versión del documento.
pub(crate) fn refresh_direct_document_change(document: &mut Document, version_before: u64) -> bool {
    if document.version == version_before {
        document.bump_version();
    }

    document.invalidate_all_caches();
    true
}

pub(crate) fn is_free_point(document: &Document, id: ObjectId) -> bool {
    document.is_free_object(&id) && matches!(document.get_object(id), Some(GeoObject::Point(_)))
}

/// Captures the object under the pointer at Select-drag start. The input loop
/// keeps this result for the whole gesture instead of consulting selection
/// again after the pointer has moved.
pub(crate) fn captured_select_drag_object(
    document: &mut Document,
    world: Point2,
    tolerance: f64,
) -> Option<ObjectId> {
    document.pick_object(world, tolerance)
}

pub(crate) fn free_point_position_differs(
    document: &Document,
    id: ObjectId,
    new_pos: Point2,
) -> bool {
    is_free_point(document, id)
        && matches!(
            document.get_object(id),
            Some(GeoObject::Point(point))
                if point.position.x != new_pos.x || point.position.y != new_pos.y
        )
}

/// Delegado fino a `Document::estimated_bytes()` (`max(object_count*200KiB, json_len, 8KiB)`).
/// Mantiene compatibilidad con call-sites heredados; el presupuesto real vive en `Document`.
/// `DocumentController` (controllers.rs) mantiene `undo_total_bytes` como running
/// counter O(1) — este shim sólo se usa en `push_history_snapshot` (GrafitoApp).
fn document_bytes_approx(doc: &Document) -> usize {
    doc.estimated_bytes()
}

/// Enforce budgets O(1) con running counter — espejo de `DocumentController::enforce_budgets` (controllers.rs:173-201).
/// Evicción por `MAX_UNDO` (50) y `MAX_UNDO_BYTES` (50 MiB) con `pop_front` O(1) y `saturating_sub`.
fn enforce_undo_budgets(undo_stack: &mut VecDeque<Document>, total_bytes: &mut usize) {
    while undo_stack.len() > MAX_UNDO {
        if let Some(front) = undo_stack.pop_front() {
            *total_bytes = total_bytes.saturating_sub(front.estimated_bytes());
        } else {
            break;
        }
    }
    while *total_bytes > MAX_UNDO_BYTES && undo_stack.len() > 1 {
        if let Some(front) = undo_stack.pop_front() {
            *total_bytes = total_bytes.saturating_sub(front.estimated_bytes());
        } else {
            break;
        }
    }
    debug_assert!(
        *total_bytes
            <= undo_stack
                .iter()
                .map(|d| d.estimated_bytes())
                .fold(0usize, |a, b| a.saturating_add(b)),
        "running counter must not exceed recomputed sum"
    );
}

/// Inserta snapshot con contador running O(1) si se provee, o scan O(n≤50) fallback para call-sites sin contador.
/// `GrafitoApp` usa `Some(&mut self.undo_total_bytes)` para O(1); tests y free functions legacy usan `None` y escanean.
fn push_history_snapshot_with_counter(
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    snapshot: Document,
    undo_total_bytes: Option<&mut usize>,
) {
    if let Some(total) = undo_total_bytes {
        let bytes = snapshot.estimated_bytes();
        undo_stack.push_back(snapshot);
        redo_stack.clear();
        *total = total.saturating_add(bytes);
        enforce_undo_budgets(undo_stack, total);
    } else {
        // Fallback O(n≤50) para call-sites sin contador (tests, legacy)
        undo_stack.push_back(snapshot);
        redo_stack.clear();
        while undo_stack.len() > MAX_UNDO {
            undo_stack.pop_front();
        }
        let mut total_bytes: usize = undo_stack
            .iter()
            .map(document_bytes_approx)
            .fold(0usize, |acc, bytes| acc.saturating_add(bytes));
        while total_bytes > MAX_UNDO_BYTES && undo_stack.len() > 1 {
            if let Some(front) = undo_stack.pop_front() {
                total_bytes = total_bytes.saturating_sub(document_bytes_approx(&front));
            } else {
                break;
            }
        }
    }
}

/// Wrapper legacy — mantiene compatibilidad con call-sites externos sin contador (input.rs, panels.rs, tests).
/// Internamente delega a `push_history_snapshot_with_counter` con `None` (scan O(n≤50)).
fn push_history_snapshot(
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    snapshot: Document,
) {
    push_history_snapshot_with_counter(undo_stack, redo_stack, snapshot, None);
}

/// Captura el estado previo de un panel sólo cuando una interacción puede
/// modificar el documento, evitando clonar y serializar durante repaints idle.
pub(crate) struct DeferredPanelSnapshot {
    before: Option<Document>,
    undo_depth: usize,
    requires_semantic_comparison: bool,
}

impl DeferredPanelSnapshot {
    pub(crate) const fn new(undo_depth: usize) -> Self {
        Self {
            before: None,
            undo_depth,
            requires_semantic_comparison: false,
        }
    }

    pub(crate) fn capture(&mut self, document: &Document) {
        if self.before.is_none() {
            self.before = Some(document.clone());
        }
        self.requires_semantic_comparison = true;
    }

    /// Conserva el estado anterior que un reemplazo ya validó y comprometió.
    /// No requiere la comparación JSON usada por mutaciones directas heredadas.
    pub(crate) fn capture_successful_replacement(&mut self, before: Document) {
        if self.before.is_none() {
            self.before = Some(before);
        }
    }

    #[cfg(test)]
    pub(crate) const fn is_captured(&self) -> bool {
        self.before.is_some()
    }

    #[cfg(test)]
    pub(crate) const fn requires_semantic_comparison(&self) -> bool {
        self.requires_semantic_comparison
    }

    pub(crate) fn save_if_semantically_changed(
        &mut self,
        document: &mut Document,
        undo_stack: &mut VecDeque<Document>,
        redo_stack: &mut VecDeque<ChangeSet>,
    ) -> bool {
        let requires_semantic_comparison = std::mem::take(&mut self.requires_semantic_comparison);
        let Some(before) = self.before.take() else {
            return false;
        };
        if undo_stack.len() != self.undo_depth {
            return false;
        }
        if requires_semantic_comparison {
            if !documents_semantically_differ(&before, document) {
                return false;
            }
            refresh_unversioned_document_change(&before, document);
        }
        push_history_snapshot(undo_stack, redo_stack, before);
        true
    }
}

/// Inserts a complete object batch on detached state and records one undo
/// snapshot only after every object has passed validation.
pub(crate) fn commit_object_insertions(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    objects: Vec<GeoObject>,
) -> Result<Vec<ObjectId>, String> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }

    let before = document.clone();
    let mut staged = document.detached_clone_for_staging();
    let ids = objects
        .into_iter()
        .map(|object| staged.try_add_object(object))
        .collect::<Result<Vec<_>, _>>()?;
    staged.version = before.version.wrapping_add(1);
    *document = staged;
    push_history_snapshot(undo_stack, redo_stack, before);
    Ok(ids)
}

/// Removes an object as part of one eraser gesture, snapshotting only the
/// first actual deletion so no-op strokes leave redo history intact.
pub(crate) fn erase_object_for_stroke(
    document: &mut Document,
    id: ObjectId,
    stroke_has_mutated: &mut bool,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
) -> bool {
    if document.get_object(id).is_none() {
        return false;
    }
    if !*stroke_has_mutated {
        push_history_snapshot(undo_stack, redo_stack, document.clone());
        *stroke_has_mutated = true;
    }
    document.remove_object(id).is_some()
}

/// Stages a conic construction and its propagation before replacing the live
/// document, so an invalid point set cannot leave a placeholder output behind.
pub(crate) fn try_stage_conic_by_five_points(
    document: &mut Document,
    points: &[ObjectId],
) -> Result<(), String> {
    let mut staged = document.detached_clone_for_staging();
    staged.try_add_conic_by_five_points_constraint(points)?;
    let order = staged.propagation_order(points);
    staged.try_re_evaluate_constraints(&order)?;
    *document = staged;
    Ok(())
}

/// Commits a complete conic operation as one undoable action only after its
/// staged fit and propagation have succeeded.
pub(crate) fn commit_conic_by_five_points(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    points: &[ObjectId],
) -> Result<(), String> {
    let before = document.clone();
    try_stage_conic_by_five_points(document, points)?;
    push_history_snapshot(undo_stack, redo_stack, before);
    Ok(())
}

/// Adds a constructive conic and evaluates its complete dependency path on
/// detached state. The undo entry is created only after that state is valid.
fn commit_constructive_conic<F>(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    inputs: &[ObjectId],
    add_constraint: F,
) -> Result<(), String>
where
    F: FnOnce(&mut Document) -> Result<(), String>,
{
    let before = document.clone();
    let mut staged = document.detached_clone_for_staging();
    add_constraint(&mut staged)?;
    let order = staged.propagation_order(inputs);
    staged.try_re_evaluate_constraints(&order)?;
    *document = staged;
    push_history_snapshot(undo_stack, redo_stack, before);
    Ok(())
}

pub(crate) fn commit_ellipse_by_foci(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    first_focus: ObjectId,
    second_focus: ObjectId,
    point_on_ellipse: ObjectId,
) -> Result<(), String> {
    let inputs = [first_focus, second_focus, point_on_ellipse];
    commit_constructive_conic(document, undo_stack, redo_stack, &inputs, |staged| {
        staged
            .try_add_constructed_object(
                GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
                "EllipseByFoci",
                &inputs,
            )
            .map(|_| ())
    })
}

pub(crate) fn commit_parabola_by_focus_directrix(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    focus: ObjectId,
    directrix: ObjectId,
) -> Result<(), String> {
    let inputs = [focus, directrix];
    commit_constructive_conic(document, undo_stack, redo_stack, &inputs, |staged| {
        staged
            .try_add_constructed_object(
                GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 0.0), 1.0)),
                "ParabolaByFocusDirectrix",
                &inputs,
            )
            .map(|_| ())
    })
}

pub(crate) fn commit_hyperbola_by_foci(
    document: &mut Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
    first_focus: ObjectId,
    second_focus: ObjectId,
    point_on_hyperbola: ObjectId,
) -> Result<(), String> {
    let inputs = [first_focus, second_focus, point_on_hyperbola];
    commit_constructive_conic(document, undo_stack, redo_stack, &inputs, |staged| {
        staged
            .try_add_constructed_object(
                GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 1.0)),
                "HyperbolaByFoci",
                &inputs,
            )
            .map(|_| ())
    })
}

pub(crate) fn wrap_toast_message(message: &str, max_columns: usize) -> String {
    if max_columns == 0 || message.chars().count() <= max_columns {
        return message.to_string();
    }

    let mut wrapped = String::new();
    let mut line_len = 0usize;
    for word in message.split_whitespace() {
        let word_len = word.chars().count();
        if line_len > 0 {
            if line_len + 1 + word_len > max_columns {
                wrapped.push('\n');
                line_len = 0;
            } else {
                wrapped.push(' ');
                line_len += 1;
            }
        }
        for character in word.chars() {
            if line_len == max_columns {
                wrapped.push('\n');
                line_len = 0;
            }
            wrapped.push(character);
            line_len += 1;
        }
    }
    wrapped
}

pub(crate) fn save_command_snapshot_if_mutated(
    outcome: &grafito_command::commands::CommandOutcome,
    before: Document,
    after: &Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
) {
    if command_mutated_document(outcome, &before, after) {
        push_history_snapshot(undo_stack, redo_stack, before);
    }
}

/// Las celdas CAS de error mutan el documento de forma intencional aunque el
/// comando subyacente haya fallado, por lo que no pueden usar la regla genérica
/// que descarta snapshots para `CommandOutcome::Error`.
pub(crate) fn save_cas_worksheet_snapshot_if_mutated(
    before: Document,
    after: &Document,
    undo_stack: &mut VecDeque<Document>,
    redo_stack: &mut VecDeque<ChangeSet>,
) {
    if documents_semantically_differ(&before, after) {
        push_history_snapshot(undo_stack, redo_stack, before);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TrigFunctionSpec {
    pub name: &'static str,
    pub color: Color,
}

pub(crate) const TRIG_FUNCTIONS: [TrigFunctionSpec; 6] = [
    TrigFunctionSpec {
        name: "sin",
        color: Color::new(0.15, 0.38, 0.95, 1.0),
    },
    TrigFunctionSpec {
        name: "cos",
        color: Color::new(0.90, 0.35, 0.10, 1.0),
    },
    TrigFunctionSpec {
        name: "tan",
        color: Color::new(0.55, 0.20, 0.90, 1.0),
    },
    TrigFunctionSpec {
        name: "cot",
        color: Color::new(0.05, 0.55, 0.45, 1.0),
    },
    TrigFunctionSpec {
        name: "sec",
        color: Color::new(0.90, 0.65, 0.05, 1.0),
    },
    TrigFunctionSpec {
        name: "csc",
        color: Color::new(0.85, 0.10, 0.35, 1.0),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrigViewMode {
    Didactic,
    Grid,
}

#[derive(Debug, Clone)]
pub(crate) struct TrigGraphCache {
    pub function: u8,
    pub x_min_bits: u64,
    pub x_max_bits: u64,
    pub y_min_bits: u64,
    pub y_max_bits: u64,
    pub width_px: u32,
    pub quality: RenderQuality,
    pub segments: Vec<(Point2, Point2)>,
    pub asymptotes: Vec<f64>,
}

// Pou eliminado — ver avatar.rs

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
        let renderer_lock = self.renderer.write().ok()?;
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

pub(crate) fn pending_action_needs_reinitialization(
    current_tool: Tool,
    previous_tool: Tool,
    pending_action: &PendingAction,
) -> bool {
    current_tool != previous_tool
        || (GrafitoApp::is_constraint_tool(current_tool)
            && matches!(pending_action, PendingAction::None))
}

/// Campo persistido que modifica el selector de color compartido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorPickerTarget {
    /// El color expuesto por [`GeoObject::color`].
    ObjectColor,
    /// El relleno opcional de un [`grafito_core::RegularPolychoron4DObj`].
    RegularPolychoronFill,
}

/// Estado transitorio del único diálogo de color de Grafito.
#[derive(Debug, Clone)]
pub(crate) struct ActiveColorPicker {
    pub(crate) object_id: ObjectId,
    pub(crate) target: ColorPickerTarget,
    pub(crate) picker: grafito_ui::color_picker::HsvColorPicker,
}

/// `GrafitoApp` es el compositor (God Object en migración): orquesta Piel ↔ Cerebro.
///
/// Descomposición incremental hacia `crate::controllers`:
///
/// - `DocumentController` — document + undo/redo + lifecycle (ya con `Document::estimated_bytes()` y running counter)
/// - `ViewController` — camera/perspective/view (cache derivado)
/// - `AssistantController` — assistant state + runtime
///
/// `GrafitoApp` conserva campos directos para compatibilidad pero delega lógica
/// nueva a `controllers` cuando existe.
pub struct GrafitoApp {
    pub document: Document,
    pub current_tool: Tool,
    pub previous_tool: Tool,
    /// Cache derivado de `perspective.view_mode()` — **no es fuente independiente**.
    ///
    /// Fuente canónica: [`Perspective`]. Este campo es un cache para evitar
    /// recomputar `view_mode()` en cada frame y se mantiene sincronizado
    /// exclusivamente vía [`GrafitoApp::set_perspective`] /
    /// [`GrafitoApp::sync_current_view`]. Invariante:
    /// `debug_assert_eq!(current_view, perspective.view_mode())`.
    /// No asignar `current_view` directamente fuera de `set_perspective` /
    /// `sync_current_view`.
    pub current_view: ViewMode,
    /// Perspectiva activa (estilo GeoGebra) — **fuente canónica única**.
    ///
    /// [`Perspective::canvas_mode`] y [`Perspective::view_mode`] derivan
    /// [`crate::CanvasMode`] y [`crate::ViewMode`]. `current_view` y cualquier
    /// uso de `CanvasMode` deben derivarse de aquí. Ver `set_perspective`.
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
    /// Origen del canvas que recibió el último puntero, para convertir las
    /// coordenadas globales de egui a coordenadas locales del canvas.
    pub canvas_origin: Option<Pos2>,
    pub canvas_drag_start: Option<Pos2>,
    pub canvas_is_panning: bool,
    /// Indica si el arrastre Select actual ya produjo su primera mutación de punto.
    pub point_drag_has_mutated: bool,
    /// Indica si el gesto Eraser actual ya registró su único snapshot de undo.
    pub eraser_stroke_has_mutated: bool,
    /// Object captured under the pointer at the start of the active Select drag.
    pub select_drag_object: Option<ObjectId>,
    /// Avoids repeating the same numeric-solver toast on every drag frame.
    pub point_drag_error_reported: bool,
    pub selected_object: Option<ObjectId>,
    pub preview_object: Option<GeoObject>,
    pub input_text: String,
    /// Texto de celdas aún no confirmado; nunca se reconcilia con geometría.

    /// Solicita foco para la primera entrada de comandos visible del próximo frame.
    pub(crate) command_input_focus_requested: bool,
    pub cas_result: String,

    pub keyboard_tab: usize,
    pub keyboard_visible: bool,
    /// Fuerza temporalmente el teclado completo en una ventana corta.
    pub keyboard_expanded: bool,
    pub table_func_idx: usize,
    pub table_x_min: String,
    pub table_x_max: String,
    pub table_step: String,
    pub cas_history: VecDeque<String>,
    pub sidebar_tab: usize,
    pub recent_files: VecDeque<String>,
    document_lifecycle: DocumentLifecycle,
    deferred_file_actions: DeferredFileActions,
    /// Timestamp de inicio de la app (splash screen). None = ya pasó.
    pub splash_start: Option<Instant>,
    /// Textura retenida mientras el splash la referencia en sus primitivas egui.
    /// LRU: tamaño 1, se libera con `ctx.forget_image("splash_logo")` al cerrar splash para no retener GPU.
    splash_logo: Option<egui::TextureHandle>,
    /// Avatar local de Mora, cargado una vez cuando el asistente se vuelve visible.
    /// LRU: tamaño 1, se libera con `ctx.forget_image("mora_avatar")` en `on_close`/`Drop` para evitar fuga GPU.
    pub(crate) mora_texture: Option<egui::TextureHandle>,
    /// Evita volver a decodificar el recurso embebido si el fallback ya fue necesario.
    pub(crate) mora_texture_load_attempted: bool,
    /// Registry de plugins del asistente, cargado una sola vez.
    pub(crate) plugin_registry: Option<grafito_plugins::PluginRegistry>,
    /// Guarda si ya se intentó cargar los plugins una vez.
    pub(crate) plugins_loaded: bool,
    /// Cache de bloques del transcript del asistente (persistente entre frames).
    pub(crate) assistant_blocks_cache: grafito_ui::assistant::AssistantBlocksCache,
    /// Overlay de pizarra nativa (estilo macOS).
    pub whiteboard_open: bool,
    pub whiteboard: crate::whiteboard_ui::WhiteboardSession,
    /// Libro de hojas de la pizarra (tipo Notepad — múltiples pizarras una por una).
    pub whiteboard_book: crate::whiteboard_ui::WhiteboardBook,
    /// Drawer izquierdo de hojas anclado (si false, auto-hide al borde izquierdo).
    #[allow(dead_code)]
    pub whiteboard_left_pinned: bool,
    /// Asistente visible (y ocultable) dentro de la pizarra.
    #[allow(dead_code)]
    pub show_whiteboard_assistant: bool,
    /// Memoria pedagógica del usuario (nivel, ramas, exámenes) del tutor.
    pub profile: grafito_profile::StudentProfile,
    /// Historial de undo acotado `MAX_UNDO=50` y `MAX_UNDO_BYTES=50MiB`.
    /// Usa `VecDeque<Document>` con `pop_front` O(1) (corrige `Vec` O(n) shift).
    /// Presupuesto vía `Document::estimated_bytes()`; ver `push_history_snapshot`.
    /// Evolución running counter O(1) en `crate::controllers::DocumentController`.
    pub undo_stack: VecDeque<Document>,
    /// Historial de redo; limpiado en cada `push_history_snapshot`.
    pub redo_stack: VecDeque<ChangeSet>,
    /// Contador running O(1) de bytes en undo_stack — evita scan O(n) por push.
    /// Se actualiza con saturating_add/sub en push/pop y se enforce con MAX_UNDO/BYTES.
    /// Ver `crate::controllers::DocumentController::undo_total_bytes`.
    pub undo_total_bytes: usize,
    /// Ventana onboarding Scandinavian 30s — true si `config.onboarding_completed` es false.
    /// Se muestra una vez con 3 bullets + [Probar ejemplo][Empezar vacío][No mostrar].
    pub show_onboarding: bool,
    /// Jobs de I/O en background para no bloquear UI thread (60fps) — save/open/export.
    /// Pattern `spawn_profile_save` (assistant.rs:41-51) con `sync_channel(1)` + `request_repaint`.
    /// `None` = idle; `Some(receiver)` = polling con `try_recv` en `update`.
    pub(crate) pending_save_job: Option<PendingSaveJob>,
    pub(crate) pending_open_job: Option<PendingOpenJob>,
    pub(crate) pending_export_job: Option<PendingExportJob>,
    /// Job de importación `.ggb` en background (F1-1): lectura + parse fuera del
    /// UI thread, resultado aplicado con undo único vía `process_input`.
    pub(crate) pending_ggb_import_job: Option<PendingGgbImportJob>,
    pub attractor_cache: std::collections::HashMap<ObjectId, (u64, Vec<Point3D>)>,
    /// Caché de texturas de relleno para curvas implícitas. Usa `RwLock`
    /// para permitir mutación desde `draw_implicit_curve_fill` (que recibe
    /// `&self`). La clave es el `ObjectId` de la `ImplicitCurveObj`.
    pub fill_textures: std::sync::RwLock<crate::render_2d::FillTextureCacheStore>,
    pub(crate) active_color_picker: Option<ActiveColorPicker>,
    pub color_favorites: [grafito_geometry::Color; 5],
    pub tool_ghost: Option<GeoObject>,
    pub tool_state: crate::tool_dispatcher::ToolState,
    pub gpu_renderer: Option<Arc<RwLock<Option<grafito_render::Renderer>>>>,
    pub gpu_scene_readiness: Option<crate::canvas::GpuSceneReadiness>,
    /// Estado visual por frame; no se serializa ni se inserta en `Document::variables`.
    pub(crate) transient_render_state: TransientRenderState,
    /// Activa una orbita de camara 3D y una rotacion de proyecciones 4D por defecto.
    pub(crate) multidimensional_motion_enabled: bool,
    /// Multiplicador transitorio compartido por la órbita 3D y la fase 4D.
    pub(crate) multidimensional_motion_speed: f32,
    pub use_gpu: bool,
    pub last_interaction_time: Instant,
    pub is_view_changing: bool,
    last_canvas_resize_at: Option<Instant>,
    pub pending_action: PendingAction,
    pub toasts: grafito_ui::toast::ToastManager,
    pub hovered_analysis: Option<HoveredAnalysis>,
    pub hover_candidate_pos: Option<Point2>,
    pub hover_candidate_time: f64,
    pub hover_cached_analysis: Option<Option<HoveredAnalysis>>,
    pub document_snapshot: std::sync::Arc<Document>,
    pub snapshot_version: u64,
    pub snapshot_render_quality: RenderQuality,
    pub command_palette: grafito_ui::command_palette::CommandPaletteState,
    /// Estado sin I/O del asistente matemático.
    pub assistant: grafito_ui::assistant::AssistantPanelState,
    /// Trabajos remotos y claves de sesión que nunca se serializan.
    pub(crate) assistant_runtime: crate::assistant::AssistantRuntime,
    /// El drawer derecho contextual puede cerrarse sin perder su estado.
    pub right_drawer_open: bool,
    /// Pestaña visible del dock único de Geometry 3D.
    pub(crate) workspace_dock_tab: crate::WorkspaceDockTab,
    /// En modo compacto, el dock de Geometry 3D se abre bajo demanda.
    pub(crate) compact_geometry_utility_open: bool,
    /// El drawer izquierdo puede cerrarse para devolver espacio al canvas.
    /// El rail y el menú Paneles siguen permitiendo restaurarlo.
    pub left_drawer_open: bool,
    /// En modo compacto, el menú Paneles abre temporalmente el drawer izquierdo.
    pub compact_drawer_open: bool,
    /// El asistente puede ocultarse sin detener sus trabajos remotos pendientes.
    pub assistant_visible: bool,
    /// Tiempo del frame actual, usado por todas las notificaciones nuevas.
    pub ui_time: f64,
    /// Protocolo de construcción: historial de pasos que crean objetos o
    /// restricciones. Se muestra en el panel derecho de la perspectiva
    /// Geometry2D.
    pub construction_log: Vec<ConstructionStep>,
    /// Visibilidad del panel derecho "Protocolo de Construcción".
    pub show_construction_protocol: bool,
    /// Datos numéricos para el panel de Estadística (ingresados por el usuario).
    pub statistics_data: Vec<f64>,
    /// Buffer de texto crudo para el `TextEdit` del panel de Estadística.
    /// Se parsea a `statistics_data` sólo al perder foco, para no destruir
    /// la entrada del usuario frame a frame.
    pub statistics_input_buf: String,
    /// Error persistente del último intento de parsear datos estadísticos.
    pub statistics_input_error: Option<String>,
    /// Estado del popup de autocompletado de la barra de entrada.
    pub autocomplete: InputAutocomplete,
    /// Visibilidad de la ventana modal "Acerca de Grafito".
    pub show_about: bool,
    /// Visibilidad del panel de animación trigonométrica (círculo unitario).
    pub show_trig_animation: bool,
    /// Ángulo actual para la animación trigonométrica (en radianes).
    pub trig_angle: f64,
    /// Si la animación trigonométrica está corriendo.
    pub trig_animating: bool,
    /// Velocidad de la animación trigonométrica (rad/seg).
    pub trig_speed: f64,
    /// Función a visualizar: 0=sin, 1=cos, 2=tan, 3=cot, 4=sec, 5=csc.
    pub trig_function: u8,
    /// Presentación visual de la animación trigonométrica.
    pub(crate) trig_view_mode: TrigViewMode,
    /// Cache de la curva trigonométrica visible; el marcador se anima aparte.
    pub(crate) trig_graph_cache: std::sync::RwLock<Option<TrigGraphCache>>,
    /// Configuración — avatar super configurable.
    pub show_mascot_config: bool,
    pub avatar_draft: grafito_profile::AvatarConfig,
    pub config_name_error: Option<String>,
    /// Enseñanza paso a paso — burbujas, pizarra y manim.
    pub teaching_ui: crate::teaching_ui::TeachingUiState,
    /// Presupuesto de repintado coalescido del frame (F17). Los widgets piden
    /// vía [`GrafitoApp::request_repaint_budget`]; el scheduler unificado
    /// aplica el mínimo al final del frame (`apply`). Se resetea al inicio de
    /// cada `update`.
    pub(crate) repaint_budget: RepaintBudget,
    /// Autosave debouncer — tick en `update` (nunca en `Ui::`), escribe sidecar
    /// en background thread si `should_autosave(now)`.
    pub(crate) autosave: AutosaveDebouncer,
    /// Versión del documento vista en el último tick de autosave (para detectar
    /// mutaciones que no pasaron por `save_snapshot` y marcar dirty fallback).
    pub(crate) autosave_last_version: u64,
    /// Opt-in explícito para Aula/red avanzada (loopback sin red).
    pub advanced_red_opt_in: bool,
    /// Idioma de la UI (ES/EN) — O2 i18n: persiste en `AppConfig.locale`,
    /// se edita desde Ayuda con `locale_selector` (Piel pura, sin I/O en Ui).
    pub(crate) locale: AppLocale,
    /// Panel de Aula (F0 sin red) — loopback con ShareCode + QR dibujado con líneas egui.
    pub classroom: crate::classroom::ClassroomPanel,
}

pub(crate) const DEFAULT_KEYBOARD_VISIBLE: bool = false;
pub(crate) const DEFAULT_CONSTRUCTION_PROTOCOL_VISIBLE: bool = false;

/// Construye un documento inicial vacío: los ejemplos se cargan sólo a petición.
pub(crate) fn initial_document() -> Document {
    let mut document = Document::new();
    document.set_view(ViewTransform::new(1280.0, 720.0));
    document.view_mut().scale = 50.0;
    document.render_quality = RenderQuality::Normal;
    document
}

/// Los atajos globales no deben competir con un editor de texto enfocado.
pub(crate) const fn global_shortcuts_allowed(wants_keyboard_input: bool) -> bool {
    !wants_keyboard_input
}

#[derive(Debug, Clone)]
pub struct HoveredAnalysis {
    pub point: Point2,
    pub label: String,
    pub is_snap: bool,
    pub feature: Option<grafito_geometry::analysis::AnalysisFeature>,
    pub snap_kind: Option<crate::snap::SnapKind>,
}

/// Entrada del protocolo de construcción (estilo GeoGebra Construction Protocol).
///
/// Cada vez que se añade un objeto o restricción al documento se registra un
/// paso con la acción que lo originó, las etiquetas de los objetos de entrada
/// y la etiqueta del objeto resultante.
#[derive(Debug, Clone)]
pub struct ConstructionStep {
    pub n: usize,
    pub action: String,
    pub inputs: Vec<String>,
    pub output: String,
    pub disabled: bool,
    pub timestamp: f64,
}

/// Estado del autocompletado de la barra de entrada.
#[derive(Debug, Clone, Default)]
pub struct InputAutocomplete {
    pub open: bool,
    pub selected: usize,
}

/// Item sugerido por el autocompletado.
#[derive(Debug, Clone)]
pub struct AutocompleteItem {
    pub text: String,
    pub detail: String,
    pub bracket: bool,
}

impl GrafitoApp {
    /// Abre el selector para el color general de un objeto existente.
    pub(crate) fn open_object_color_picker(&mut self, object_id: ObjectId) -> bool {
        let Some(object) = self.document.get_object(object_id) else {
            return false;
        };
        self.active_color_picker = Some(ActiveColorPicker {
            object_id,
            target: ColorPickerTarget::ObjectColor,
            picker: grafito_ui::color_picker::HsvColorPicker::new(object.color()),
        });
        true
    }

    /// Abre el selector para el relleno habilitado de un politopo regular 4D.
    pub(crate) fn open_regular_polychoron_fill_color_picker(
        &mut self,
        object_id: ObjectId,
    ) -> bool {
        let Some(GeoObject::RegularPolychoron4D(polychoron)) = self.document.get_object(object_id)
        else {
            return false;
        };
        let Some(fill_color) = polychoron.fill_color else {
            return false;
        };
        self.active_color_picker = Some(ActiveColorPicker {
            object_id,
            target: ColorPickerTarget::RegularPolychoronFill,
            picker: grafito_ui::color_picker::HsvColorPicker::new(fill_color),
        });
        true
    }

    pub(crate) fn trig_spec(index: u8) -> TrigFunctionSpec {
        TRIG_FUNCTIONS
            .get(index as usize)
            .copied()
            .unwrap_or(TRIG_FUNCTIONS[0])
    }

    pub(crate) fn trig_value(index: u8, t: f64) -> f64 {
        match index as usize {
            0 => t.sin(),
            1 => t.cos(),
            2 => t.tan(),
            3 => 1.0 / t.tan(),
            4 => 1.0 / t.cos(),
            5 => 1.0 / t.sin(),
            _ => t.sin(),
        }
    }

    pub(crate) fn trig_identity(index: u8) -> &'static str {
        match index as usize {
            0 => "sin θ es la altura del punto sobre el círculo unitario.",
            1 => "cos θ es la distancia horizontal al eje vertical.",
            2 => "tan θ = sin θ / cos θ; crece mucho cerca de cos θ = 0.",
            3 => "cot θ = cos θ / sin θ; no está definida cuando sin θ = 0.",
            4 => "sec θ = 1 / cos θ; mide el inverso de la proyección horizontal.",
            5 => "csc θ = 1 / sin θ; mide el inverso de la altura.",
            _ => "sin θ es la altura del punto sobre el círculo unitario.",
        }
    }

    pub(crate) fn set_trig_animation_visible(&mut self, visible: bool) {
        self.assert_view_invariant();
        self.show_trig_animation = visible && trig_animation_supported(self.current_view);
        if self.show_trig_animation {
            self.right_drawer_open = true;
        }
        if !self.show_trig_animation {
            self.trig_animating = false;
        }
        self.cleanup_trig_document_artifacts();
    }

    pub(crate) fn set_trig_function(&mut self, index: u8) {
        let clamped = (index as usize).min(TRIG_FUNCTIONS.len() - 1) as u8;
        if self.trig_function != clamped {
            self.trig_function = clamped;
            if let Ok(mut cache) = self.trig_graph_cache.write() {
                *cache = None;
            }
        }
        self.cleanup_trig_document_artifacts();
    }

    pub(crate) fn cleanup_trig_document_artifacts(&mut self) {
        let index = (self.trig_function as usize).min(TRIG_FUNCTIONS.len() - 1) as u8;
        if self.trig_function != index {
            self.trig_function = index;
        }

        let legacy_ids = self
            .document
            .objects_iter()
            .filter_map(|(id, obj)| {
                let label = obj.label();
                (label == TRIG_GRAPH_LABEL || label == TRIG_VALUE_LABEL).then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in legacy_ids {
            self.document.remove_object(id);
        }

        let legacy_vars = self
            .document
            .variables
            .keys()
            .filter(|name| name.as_str() == "trig_angle" || name.starts_with("trig_"))
            .cloned()
            .collect::<Vec<_>>();
        for name in legacy_vars {
            self.document.remove_variable(&name);
        }
    }

    fn gpu_renderer_ready(&self) -> bool {
        renderer_is_ready(self.gpu_renderer.as_ref())
    }

    /// Mantiene la proyección ligada al rectángulo real del canvas sin
    /// invalidar el índice espacial ni preparar GPU cuando el tamaño no cambió.
    fn sync_canvas_screen_size(&mut self, canvas_size: egui::Vec2) -> bool {
        if !self.document.set_screen_size(canvas_size.x, canvas_size.y) {
            return false;
        }
        let now = Instant::now();
        self.document.render_quality = RenderQuality::Preview;
        self.is_view_changing = true;
        self.last_interaction_time = now;
        self.last_canvas_resize_at = Some(now);
        true
    }

    fn gpu_scene_2d_readiness(&self) -> crate::canvas::Scene2DReadiness {
        let key = crate::canvas::Cache2DKey {
            version: self.document.version,
            view: *self.document.view(),
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            transient_revision: self.transient_render_state.revision(),
        };
        self.gpu_scene_readiness
            .as_ref()
            .map_or(crate::canvas::Scene2DReadiness::Pending, |readiness| {
                readiness.status_2d(&key)
            })
    }

    fn gpu_scene_3d_readiness(
        &self,
        screen_w: f32,
        screen_h: f32,
    ) -> crate::canvas::Scene3DReadiness {
        let key = crate::canvas::Cache3DKey {
            version: self.document.version,
            camera: self.camera,
            render_quality: self.document.render_quality,
            dark_mode: self.dark_mode,
            screen_w,
            screen_h,
        };
        self.gpu_scene_readiness
            .as_ref()
            .map_or(crate::canvas::Scene3DReadiness::Pending, |readiness| {
                readiness.status_3d(&key)
            })
    }

    fn replace_document(&mut self, document: Document, path: Option<PathBuf>) {
        self.document = document;
        if let Some(path) = path {
            self.document_lifecycle
                .establish_opened_document(path, &self.document);
        } else {
            self.document_lifecycle
                .establish_new_document(&self.document);
        }
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.undo_total_bytes = 0;
        self.autosave.mark_saved();
        self.autosave_last_version = self.document.version;
        self.clear_document_bound_transient_state();
        self.document_snapshot = std::sync::Arc::new(self.document.clone());
        self.snapshot_version = self.document.version;
        self.snapshot_render_quality = self.document.render_quality;
        self.attractor_cache.clear();
        if let Ok(mut fill_textures) = self.fill_textures.write() {
            fill_textures.clear();
        }
        if let Some(readiness) = &self.gpu_scene_readiness {
            readiness.clear();
        }
    }

    /// Limpia el estado de UI cuyos valores pertenecen al documento reemplazado.
    fn clear_document_bound_transient_state(&mut self) {
        self.pending_points.clear();
        self.pending_points_3d.clear();
        self.selected_object = None;
        self.preview_object = None;
        self.input_text.clear();
        self.command_input_focus_requested = false;
        self.cas_result.clear();
        self.cas_history.clear();
        self.statistics_input_error = None;
        self.statistics_data.clear();
        self.statistics_input_buf.clear();
        self.table_func_idx = 0;
        self.active_color_picker = None;
        self.tool_ghost = None;
        self.pending_action = PendingAction::None;
        self.canvas_drag_start = None;
        self.canvas_is_panning = false;
        self.point_drag_has_mutated = false;
        self.eraser_stroke_has_mutated = false;
        self.select_drag_object = None;
        self.point_drag_error_reported = false;
        self.hovered_analysis = None;
        self.hover_candidate_pos = None;
        self.hover_cached_analysis = None;
        self.autocomplete = InputAutocomplete::default();
        self.assistant.focus = None;
        self.assistant.verified_proposals.clear();
        self.assistant.invalidate_proposal_correction();
        self.construction_log.clear();
        self.transient_render_state = TransientRenderState::default();
        self.reset_tool_input();
    }

    /// Limpia todo el estado transitorio de herramientas y sus referencias a objetos.
    pub fn reset_tool_input(&mut self) {
        self.pending_points.clear();
        self.pending_points_3d.clear();
        self.tool_state.clear();
    }
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Scandinavian: Instala Inter Variable como fuente principal (calm, 500/600)
        {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "Inter".to_owned(),
                egui::FontData::from_static(include_bytes!("../../../assets/InterVariable.ttf")),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Inter".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "Inter".to_owned());
            cc.egui_ctx.set_fonts(fonts);
        }
        let (gpu_renderer, gpu_scene_readiness) = if let Some(render_state) = &cc.wgpu_render_state
        {
            let renderer: Arc<RwLock<Option<grafito_render::Renderer>>> =
                Arc::new(RwLock::new(None));
            let scene_readiness = crate::canvas::GpuSceneReadiness::default();
            let renderer_clone = Arc::clone(&renderer);
            let device_clone = Arc::clone(&render_state.device);
            let queue_clone = Arc::clone(&render_state.queue);
            let target_format = render_state.target_format;
            let egui_ctx = cc.egui_ctx.clone();
            // Usa Weak para no retener el renderer si la app se cerró antes de compilar.
            let weak_renderer = std::sync::Arc::downgrade(&renderer_clone);

            std::thread::spawn(move || {
                let new_renderer = grafito_render::Renderer::new(
                    &device_clone,
                    &queue_clone,
                    target_format,
                    crate::MSAA_SAMPLES as u32,
                );
                if let Some(renderer) = weak_renderer.upgrade() {
                    if let Ok(mut lock) = renderer.write() {
                        *lock = Some(new_renderer);
                    }
                }
                // Solo repinta si el viewport sigue válido — evita request_repaint
                // póstumo si la ventana ya se cerró. Equivalente a `if ctx.viewport_id().is_valid()`.
                if egui_ctx.viewport_id() == egui::ViewportId::ROOT {
                    egui_ctx.request_repaint();
                }
                log::info!("Background shader compilation finished.");
            });

            let resources = crate::canvas::GpuCanvasResources {
                renderer: Arc::clone(&renderer),
                buffers_2d: None,
                buffers_3d: None,
                cache_2d: None,
                cache_3d: None,
                scene_readiness: scene_readiness.clone(),
            };
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
            register_gpu_function_evaluator(Box::new(AppGpuFunctionEvaluator {
                renderer: Arc::clone(&renderer),
                device: Arc::clone(&render_state.device),
                queue: Arc::clone(&render_state.queue),
            }));
            (Some(renderer), Some(scene_readiness))
        } else {
            (None, None)
        };
        let gpu_available = gpu_renderer.is_some();
        let document = initial_document();
        let document_lifecycle = DocumentLifecycle::new(&document);

        let config = load_config();
        let dark_mode = config.dark_mode;
        if dark_mode {
            DARK.apply(&cc.egui_ctx);
        } else {
            LIGHT.apply(&cc.egui_ctx);
        }
        let mut assistant = grafito_ui::assistant::AssistantPanelState::default();
        assistant.apply_preferences(config.assistant_provider, config.assistant_model.clone());
        assistant.allow_fusion_fallback = config.allow_fusion_fallback;
        assistant.full_permission = config.assistant_full_permission;
        assistant.agent_mode = config.assistant_agent_mode;

        let snapshot_version = document.version;
        let snapshot_render_quality = document.render_quality;
        let document_snapshot = std::sync::Arc::new(document.clone());
        let profile = crate::utils::load_profile();
        let avatar_draft = profile.avatar.clone();
        let advanced_red_opt_in = config.advanced_red_opt_in;
        let locale = config.locale;
        let mut classroom = crate::classroom::ClassroomPanel::new();
        classroom.set_opt_in(advanced_red_opt_in);
        let autosave_last_version = document.version;

        // `current_view` es cache derivado de `perspective` — fuente canónica
        // `Perspective::view_mode()` / `Perspective::canvas_mode()`.
        let perspective = Perspective::Geometry2D;
        let current_view = perspective.view_mode();
        debug_assert_eq!(current_view, ViewMode::D2);
        debug_assert_eq!(perspective.canvas_mode(), crate::CanvasMode::D2);
        Self {
            document,
            current_tool: Tool::default(),
            previous_tool: Tool::default(),
            current_view,
            perspective,
            camera: Camera3D::new(1280.0 / 720.0),
            show_grid: config.show_grid,
            snap_to_grid: config.snap_to_grid,
            snap_config: config.snap,
            exam_mode: false,
            dark_mode,
            pending_points: Vec::new(),
            pending_points_3d: Vec::new(),
            last_mouse_pos: None,
            canvas_origin: None,
            canvas_drag_start: None,
            canvas_is_panning: false,
            point_drag_has_mutated: false,
            eraser_stroke_has_mutated: false,
            select_drag_object: None,
            point_drag_error_reported: false,
            selected_object: None,
            preview_object: None,
            input_text: String::new(),
            command_input_focus_requested: false,
            cas_result: String::new(),
            keyboard_tab: 0,
            keyboard_visible: DEFAULT_KEYBOARD_VISIBLE,
            keyboard_expanded: false,
            table_func_idx: 0,
            table_x_min: "-5".to_string(),
            table_x_max: "5".to_string(),
            table_step: "1.0".to_string(),
            cas_history: VecDeque::new(),
            sidebar_tab: 0,
            splash_start: Some(Instant::now()),
            splash_logo: None,
            mora_texture: None,
            mora_texture_load_attempted: false,
            plugin_registry: None,
            plugins_loaded: false,
            assistant_blocks_cache: grafito_ui::assistant::AssistantBlocksCache::default(),
            whiteboard_open: false,
            whiteboard: crate::whiteboard_ui::WhiteboardSession::default(),
            whiteboard_book: crate::whiteboard_ui::WhiteboardBook::default(),
            whiteboard_left_pinned: false,
            show_whiteboard_assistant: true,
            profile,
            recent_files: VecDeque::new(),
            document_lifecycle,
            deferred_file_actions: DeferredFileActions::default(),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            undo_total_bytes: 0,
            show_onboarding: !config.onboarding_completed,
            pending_save_job: None,
            pending_open_job: None,
            pending_export_job: None,
            pending_ggb_import_job: None,
            attractor_cache: std::collections::HashMap::new(),
            fill_textures: std::sync::RwLock::new(
                crate::render_2d::FillTextureCacheStore::default(),
            ),
            active_color_picker: None,
            tool_ghost: None,
            tool_state: crate::tool_dispatcher::ToolState::default(),
            gpu_renderer,
            gpu_scene_readiness,
            transient_render_state: TransientRenderState::default(),
            multidimensional_motion_enabled: true,
            multidimensional_motion_speed: DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
            use_gpu: gpu_available,
            last_interaction_time: Instant::now(),
            is_view_changing: false,
            last_canvas_resize_at: None,
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
            snapshot_render_quality,
            command_palette: grafito_ui::command_palette::CommandPaletteState::default(),
            assistant,
            assistant_runtime: crate::assistant::AssistantRuntime::default(),
            right_drawer_open: true,
            workspace_dock_tab: crate::WorkspaceDockTab::Inspector,
            compact_geometry_utility_open: false,
            left_drawer_open: true,
            compact_drawer_open: false,
            assistant_visible: true,
            ui_time: 0.0,
            construction_log: Vec::new(),
            show_construction_protocol: DEFAULT_CONSTRUCTION_PROTOCOL_VISIBLE,
            statistics_data: Vec::new(),
            statistics_input_buf: String::new(),
            statistics_input_error: None,
            autocomplete: InputAutocomplete::default(),
            show_about: false,
            show_trig_animation: false,
            trig_angle: 0.0,
            trig_animating: false,
            trig_speed: 0.5,
            trig_function: 0,
            trig_view_mode: TrigViewMode::Didactic,
            trig_graph_cache: std::sync::RwLock::new(None),
            show_mascot_config: false,
            avatar_draft,
            config_name_error: None,
            teaching_ui: crate::teaching_ui::TeachingUiState::default(),
            repaint_budget: RepaintBudget::default(),
            autosave: AutosaveDebouncer::new(),
            autosave_last_version,
            advanced_red_opt_in,
            locale,
            classroom,
        }
    }

    /// Ruta única para que los widgets pidan repintado periódico coalescido (F17).
    ///
    /// Los widgets de la Piel NO deben llamar `ctx.request_repaint_after`
    /// directamente con intervalos arbitrarios: deben pasar por aquí para que
    /// [`RepaintBudget`] acumule el mínimo del frame y el scheduler unificado
    /// lo aplique una sola vez al final de `update`.
    pub(crate) fn request_repaint_budget(&mut self, delay: Duration) {
        self.repaint_budget.request(delay);
    }

    /// Aplica el presupuesto de repintado acumulado del frame (F17).
    /// Lo invoca el scheduler unificado al final de `update` (y
    /// `draw_whiteboard_overlay`, que hace early-return antes de ese punto).
    pub(crate) fn apply_repaint_budget(&mut self, ctx: &egui::Context) {
        self.repaint_budget.apply(ctx);
    }

    /// Epoch segundos para autosave (SystemTime -> secs, 0 si falla).
    pub(crate) fn autosave_now_epoch() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Marca el documento como sucio para autosave (debounce).
    pub(crate) fn mark_autosave_dirty(&mut self) {
        let now = Self::autosave_now_epoch();
        self.autosave.mark_dirty(now);
    }

    /// Tick de autosave en `update` (nunca en `Ui::`): si `should_autosave(now)`
    /// escribe sidecar en background thread + `mark_saved()`. Reutiliza patrón
    /// `spawn_profile_save` (thread Builder + request_repaint), nunca bloquea UI.
    /// También detecta cambios de `document.version` no capturados por
    /// `mark_autosave_dirty()` (fallback para mutaciones via free functions).
    pub(crate) fn tick_autosave(&mut self, ctx: &egui::Context) {
        // Fallback: si el documento cambió desde el último tick y aún no está dirty,
        // marcarlo. Esto cubre mutaciones que no pasaron por `save_snapshot`.
        if self.document.version != self.autosave_last_version && !self.autosave.is_dirty() {
            self.mark_autosave_dirty();
        }
        self.autosave_last_version = self.document.version;
        let now = Self::autosave_now_epoch();
        if !self.autosave.should_autosave(now) {
            return;
        }
        let Some(main_path) = self
            .document_lifecycle
            .current_path()
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        let doc = self.document.clone();
        let egui_ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                if let Err(err) =
                    grafito_core::persistence::write_autosave_sidecar(&doc, &main_path)
                {
                    log::warn!("autosave sidecar failed for {}: {err}", main_path.display());
                }
                egui_ctx.request_repaint();
            });
        if spawned.is_ok() {
            self.autosave.mark_saved();
        } else {
            log::warn!("autosave: spawn failed");
        }
    }

    /// Persiste preferencias de interfaz; las claves del asistente viven sólo en el llavero.
    /// Idioma actual de la UI para los helpers localizados (toolbar/paleta).
    pub(crate) fn config_locale(&self) -> grafito_ui::i18n::Locale {
        self.locale.as_ui_locale()
    }

    /// Cambia el idioma en vivo y lo persiste (Piel pura: el selector solo
    /// setea el flag, la escritura ocurre aquí fuera del closure de Ui).
    pub(crate) fn set_locale(&mut self, locale: grafito_ui::i18n::Locale) {
        self.locale = AppLocale::from_ui_locale(locale);
        self.save_app_config();
    }

    pub(crate) fn save_app_config(&self) {
        // Se conservan los toggles de plugins ya persistidos; los cambios de
        // plugins se guardan en su propia ruta en assistant.rs.
        let existing = load_config();
        save_config(&AppConfig {
            dark_mode: self.dark_mode,
            show_grid: self.show_grid,
            snap_to_grid: self.snap_to_grid,
            snap: self.snap_config.clone(),
            assistant_provider: self.assistant.provider,
            assistant_model: self.assistant.model.clone(),
            allow_fusion_fallback: self.assistant.allow_fusion_fallback,
            assistant_full_permission: self.assistant.full_permission,
            assistant_agent_mode: self.assistant.agent_mode,
            onboarding_completed: existing.onboarding_completed,
            enabled_plugins: existing.enabled_plugins,
            disabled_plugins: existing.disabled_plugins,
            advanced_red_opt_in: self.advanced_red_opt_in,
            // O2 i18n: el idioma se edita en vivo y persiste aquí.
            locale: self.locale,
        });
    }

    pub(crate) fn re_evaluate_constraints(&mut self, order: &[usize]) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("constraints");
        self.document.re_evaluate_constraints(order);
    }

    pub(crate) fn has_visible_multidimensional_object(&self) -> bool {
        self.document
            .objects_iter()
            .any(|(_, object)| object.is_visible() && object.is_3d())
    }

    pub(crate) fn has_visible_four_d_projection(&self) -> bool {
        self.document.objects_iter().any(|(_, object)| {
            object.is_visible()
                && (matches!(object, GeoObject::HyperSurface4D(_))
                    || is_typed_four_d_projection(object))
        })
    }

    pub(crate) fn has_visible_typed_four_d_projection(&self) -> bool {
        self.document
            .objects_iter()
            .any(|(_, object)| object.is_visible() && is_typed_four_d_projection(object))
    }

    pub(crate) fn pause_multidimensional_motion(&mut self) {
        pause_default_multidimensional_motion(&mut self.multidimensional_motion_enabled);
    }

    pub(crate) fn set_multidimensional_motion_speed(&mut self, speed: f32) -> bool {
        let speed = normalize_multidimensional_motion_speed(speed);
        if self.multidimensional_motion_speed == speed {
            return false;
        }
        self.multidimensional_motion_speed = speed;
        true
    }

    fn advance_multidimensional_motion(&mut self, dt: f64) -> bool {
        self.assert_view_invariant();
        if !should_animate_multidimensional_scene(
            self.current_view,
            self.multidimensional_motion_enabled,
            self.has_visible_multidimensional_object(),
        ) {
            return false;
        }

        let speed = normalize_multidimensional_motion_speed(self.multidimensional_motion_speed);
        self.multidimensional_motion_speed = speed;
        let camera_advanced = advance_default_camera_orbit_at_speed(&mut self.camera, dt, speed);
        let four_d_advanced = self.has_visible_four_d_projection()
            && self.transient_render_state.advance_four_d_phase(
                DEFAULT_4D_ROTATION_RADIANS_PER_SECOND * speed as f64 * dt.min(0.1),
            );
        if !camera_advanced && !four_d_advanced {
            return false;
        }

        true
    }

    fn commit_numeric_constraint<F>(&mut self, add_constraint: F) -> Result<(), String>
    where
        F: FnOnce(&mut Document) -> Result<(), String>,
    {
        let before = self.document.clone();
        try_stage_numeric_constraint(&mut self.document, add_constraint)?;
        self.save_snapshot(before);
        Ok(())
    }

    /// Devuelve un `Arc<Document>` para el callback GPU.
    /// Solo clona el documento cuando el `version` cambia (contenido modificado).
    /// Para cambios de view (pan/zoom), actualiza el view in-place vía `make_mut`.
    fn document_for_callback(&mut self) -> std::sync::Arc<Document> {
        if self.document.version != self.snapshot_version
            || self.document.render_quality != self.snapshot_render_quality
        {
            self.document_snapshot = std::sync::Arc::new(self.document.clone());
            self.snapshot_version = self.document.version;
            self.snapshot_render_quality = self.document.render_quality;
        } else {
            let snap = std::sync::Arc::make_mut(&mut self.document_snapshot);
            snap.set_view(*self.document.view());
        }
        self.document_snapshot.clone()
    }

    pub(crate) fn save_state(&mut self) {
        self.save_snapshot(self.document.clone());
    }

    /// Sincroniza `undo_total_bytes` si hubo pushes legacy sin contador (panels.rs, tests).
    fn sync_undo_total_bytes(&mut self) {
        let recomputed = self
            .undo_stack
            .iter()
            .map(|d| d.estimated_bytes())
            .fold(0usize, |a, b| a.saturating_add(b));
        if recomputed != self.undo_total_bytes {
            self.undo_total_bytes = recomputed;
        }
    }

    pub(crate) fn save_snapshot(&mut self, snapshot: Document) {
        self.sync_undo_total_bytes();
        push_history_snapshot_with_counter(
            &mut self.undo_stack,
            &mut self.redo_stack,
            snapshot,
            Some(&mut self.undo_total_bytes),
        );
        self.mark_autosave_dirty();
    }

    /// Guarda un único estado previo sólo cuando una interacción de panel
    /// alteró el contenido persistible del documento. Esto evita que los
    /// repaints y los `bump_version` internos generen entradas de undo.
    #[allow(dead_code)] // TODO P2: remover cuando se migre a ValidatedDocument wrapper (usado en tests de snapshot semántico)
    pub(crate) fn save_snapshot_if_semantically_changed(
        &mut self,
        before: Document,
        undo_depth: usize,
    ) {
        if self.undo_stack.len() != undo_depth {
            return;
        }
        if documents_semantically_differ(&before, &self.document) {
            refresh_unversioned_document_change(&before, &mut self.document);
            self.save_snapshot(before);
        }
    }

    pub(crate) fn handle_command_outcome(
        &mut self,
        outcome: grafito_command::commands::CommandOutcome,
        time: f64,
        input_was: &str,
    ) {
        apply_command_outcome(
            &outcome,
            &mut self.cas_result,
            &mut self.cas_history,
            &mut self.toasts,
            time,
            input_was,
        );
    }

    pub(crate) fn notify_at(
        &mut self,
        message: impl Into<String>,
        kind: grafito_ui::toast::ToastKind,
        time: f64,
    ) {
        self.toasts
            .push(wrap_toast_message(&message.into(), 52), kind, time);
    }

    pub(crate) fn notify(
        &mut self,
        message: impl Into<String>,
        kind: grafito_ui::toast::ToastKind,
    ) {
        self.notify_at(message, kind, self.ui_time);
    }

    pub(crate) fn pending_document_action(&self) -> Option<DocumentAction> {
        self.document_lifecycle.pending_action()
    }

    pub(crate) fn document_save_error(&self) -> Option<&str> {
        self.document_lifecycle.save_error()
    }

    pub(crate) fn current_document_path(&self) -> Option<&Path> {
        self.document_lifecycle.current_path()
    }

    pub(crate) fn handle_file_command(&mut self, command: FileCommand) {
        let _ = self.deferred_file_actions.queue_command(command);
    }

    pub(crate) fn queue_unsaved_decision(&mut self, decision: UnsavedDecision) {
        let _ = self.deferred_file_actions.queue_decision(decision);
    }

    fn process_deferred_file_action(&mut self, ctx: &egui::Context) {
        let Some(intent) = self.deferred_file_actions.take_after_editors() else {
            return;
        };
        match intent {
            DeferredFileIntent::Command(command) => {
                self.execute_file_command_after_editors(command, ctx)
            }
            DeferredFileIntent::Decision(decision) => {
                self.execute_unsaved_decision_after_editors(decision, ctx)
            }
        }
    }

    fn execute_file_command_after_editors(&mut self, command: FileCommand, ctx: &egui::Context) {
        match command {
            FileCommand::New => self.request_document_action(DocumentAction::New, ctx),
            FileCommand::Open => self.request_document_action(DocumentAction::Open, ctx),
            FileCommand::Save => {
                if self.pending_document_action().is_some() {
                    self.execute_unsaved_decision_after_editors(UnsavedDecision::Save, ctx);
                } else {
                    let _ = self.save_document(SaveMode::Save, ctx);
                }
            }
            FileCommand::SaveAs => {
                if let Some(action) = self.pending_document_action() {
                    let _ = self
                        .document_lifecycle
                        .resolve_unsaved_decision(UnsavedDecision::Save);
                    self.save_before_document_action(SaveMode::SaveAs, action, ctx);
                } else {
                    let _ = self.save_document(SaveMode::SaveAs, ctx);
                }
            }
            FileCommand::Exit => self.request_document_action(DocumentAction::Exit, ctx),
        }
    }

    fn execute_unsaved_decision_after_editors(
        &mut self,
        decision: UnsavedDecision,
        ctx: &egui::Context,
    ) {
        let Some(resolution) = self.document_lifecycle.resolve_unsaved_decision(decision) else {
            return;
        };
        match resolution {
            UnsavedResolution::Save(action) => {
                self.save_before_document_action(SaveMode::Save, action, ctx)
            }
            UnsavedResolution::Proceed(action) => self.perform_document_action(action, ctx),
            UnsavedResolution::Cancelled => {}
        }
    }

    fn save_before_document_action(
        &mut self,
        mode: SaveMode,
        action: DocumentAction,
        ctx: &egui::Context,
    ) {
        if let SaveAttempt::Saved(saved_action) = self.save_document(mode, ctx) {
            self.perform_document_action(saved_action.unwrap_or(action), ctx);
        }
    }

    fn request_document_action(&mut self, action: DocumentAction, ctx: &egui::Context) {
        if let DocumentActionRequest::Proceed(action) =
            self.document_lifecycle
                .request_action(action, &self.document, false)
        {
            self.perform_document_action(action, ctx);
        }
    }

    fn perform_document_action(&mut self, action: DocumentAction, ctx: &egui::Context) {
        match action {
            DocumentAction::New => self.replace_document(initial_document(), None),
            DocumentAction::Open => self.choose_and_open_document(ctx),
            DocumentAction::Exit => {
                self.document_lifecycle.approve_close();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn handle_native_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if !close_requested {
            return;
        }
        if self
            .deferred_file_actions
            .intercept_native_close(self.document_lifecycle.close_is_approved())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        // LRU cleanup: liberar texturas retenidas antes de cerrar ventana para no fugar GPU.
        if self.splash_logo.is_some() {
            ctx.forget_image("splash_logo");
            self.splash_logo = None;
        }
        if self.mora_texture.is_some() {
            ctx.forget_image("mora_avatar");
            self.mora_texture = None;
        }
    }

    pub(crate) fn active_right_panel(&self) -> Option<crate::RightPanelContent> {
        let panel = if self.show_trig_animation {
            Some(crate::RightPanelContent::TrigAnimation)
        } else {
            self.perspective.layout().right_panel
        };

        match panel {
            Some(crate::RightPanelContent::DomainColoring) => self
                .document
                .objects_iter()
                .any(|(_, object)| matches!(object, GeoObject::ComplexGrid(_)))
                .then_some(crate::RightPanelContent::DomainColoring),
            Some(crate::RightPanelContent::ConstructionProtocol)
                if !self.show_construction_protocol =>
            {
                None
            }
            other => other,
        }
    }

    /// Devuelve el conjunto de etiquetas de objetos actualmente en el
    /// documento. Útil para snapshot+diff al registrar pasos de construcción.
    pub(crate) fn object_labels_snapshot(&self) -> std::collections::HashSet<String> {
        self.document
            .objects_iter()
            .filter(|(_, o)| !o.label().is_empty())
            .map(|(_, o)| o.label().to_string())
            .collect()
    }

    /// Registra un paso en el protocolo de construcción. Cota `MAX_CONSTRUCTION_LOG=500`
    /// — mantiene orden cronológico y trunca las más antiguas (`drain`).
    pub(crate) fn record_construction_step(
        &mut self,
        action: &str,
        inputs: Vec<String>,
        output: &str,
    ) {
        let n = self.construction_log.len().saturating_add(1);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        self.construction_log.push(ConstructionStep {
            n,
            action: action.to_string(),
            inputs,
            output: output.to_string(),
            disabled: false,
            timestamp,
        });
        if self.construction_log.len() > MAX_CONSTRUCTION_LOG {
            let excess = self
                .construction_log
                .len()
                .saturating_sub(MAX_CONSTRUCTION_LOG);
            self.construction_log.drain(0..excess);
        }
    }

    /// Registra un paso comparando las etiquetas del documento antes y
    /// después de una operación. Los nuevos objetos son el `output`; las
    /// etiquetas mencionadas en `action` son los `inputs`.
    pub(crate) fn record_step_from_diff(
        &mut self,
        action: &str,
        before: &std::collections::HashSet<String>,
        record_document_mutation: bool,
    ) {
        let after = self.object_labels_snapshot();
        let new_labels: Vec<String> = after.difference(before).cloned().collect();
        if new_labels.is_empty() {
            if record_document_mutation {
                let inputs: Vec<String> = before
                    .iter()
                    .filter(|label| action.contains(label.as_str()))
                    .cloned()
                    .collect();
                self.record_construction_step(action, inputs, "Documento actualizado");
            }
            return;
        }
        let output = new_labels.join(", ");
        let inputs: Vec<String> = before
            .iter()
            .filter(|l| action.contains(l.as_str()))
            .cloned()
            .collect();
        self.record_construction_step(action, inputs, &output);
    }

    /// Añade un objeto al documento y registra el paso correspondiente en el
    /// protocolo de construcción.
    pub(crate) fn add_object_logged(
        &mut self,
        obj: GeoObject,
        action: &str,
    ) -> Result<ObjectId, String> {
        let id = commit_object_insertions(
            &mut self.document,
            &mut self.undo_stack,
            &mut self.redo_stack,
            vec![obj],
        )?
        .into_iter()
        .next()
        .ok_or_else(|| "Object insertion produced no identifier".to_string())?;
        self.mark_autosave_dirty();
        let output = self
            .document
            .get_object(id)
            .map(|o| o.label().to_string())
            .unwrap_or_default();
        self.record_construction_step(action, Vec::new(), &output);
        Ok(id)
    }

    pub(crate) fn insert_object_from_tool(
        &mut self,
        obj: GeoObject,
        action: &str,
        time: f64,
    ) -> Option<ObjectId> {
        match self.add_object_logged(obj, action) {
            Ok(id) => Some(id),
            Err(error) => {
                self.handle_command_outcome(
                    grafito_command::commands::CommandOutcome::Error(error),
                    time,
                    action,
                );
                None
            }
        }
    }

    /// Ejecuta un comando de texto, gestiona su `CommandOutcome` y registra
    /// el paso de construcción resultante (snapshot+diff de etiquetas).
    fn execute_command_and_record_with_outcome(
        &mut self,
        cmd: &str,
        time: f64,
    ) -> grafito_command::commands::CommandOutcome {
        let before_document = self.document.clone();
        let before = self.object_labels_snapshot();
        let mut buf = cmd.to_string();
        let outcome = crate::commands::process_input(&mut self.document, &mut buf);
        let mutated_document = command_mutated_document(&outcome, &before_document, &self.document);
        save_command_snapshot_if_mutated(
            &outcome,
            before_document,
            &self.document,
            &mut self.undo_stack,
            &mut self.redo_stack,
        );
        if mutated_document {
            self.mark_autosave_dirty();
        }
        self.handle_command_outcome(outcome.clone(), time, cmd);
        self.record_step_from_diff(cmd, &before, mutated_document);
        outcome
    }

    pub(crate) fn execute_command_and_record(
        &mut self,
        cmd: &str,
        time: f64,
    ) -> grafito_command::commands::CommandOutcome {
        self.execute_command_and_record_with_outcome(cmd, time)
    }

    /// Ejecuta la barra compartida. Una entrada válida se limpia; ante error se
    /// conserva y recupera el foco para que el usuario pueda corregirla.
    pub(crate) fn submit_input_text(&mut self, time: f64) {
        if self.input_text.trim().is_empty() {
            return;
        }
        if sidebar_uses_cas_worksheet(self.sidebar_tab) {
            self.submit_cas_worksheet_cell(time);
            return;
        }

        let input_was = self.input_text.clone();
        let outcome = self.execute_command_and_record_with_outcome(&input_was, time);
        if clear_submitted_input_on_success(&mut self.input_text, &outcome) {
            self.preview_object = None;
        } else {
            self.command_input_focus_requested = true;
        }
        self.autocomplete.open = false;
        self.autocomplete.selected = 0;
    }

    /// Envía una celda CAS persistente. A diferencia de la barra compartida,
    /// conserva diagnósticos como celdas locales y registra undo incluso para
    /// un error del intérprete que no dejó geometría parcial.
    pub(crate) fn submit_cas_worksheet_cell(&mut self, time: f64) {
        if self.input_text.trim().is_empty() {
            return;
        }

        let input_was = self.input_text.clone();
        let before_document = self.document.clone();
        let before_labels = self.object_labels_snapshot();
        let outcome = crate::commands::process_cas_worksheet_cell(&mut self.document, &input_was);
        let mutated_document = documents_semantically_differ(&before_document, &self.document);
        save_cas_worksheet_snapshot_if_mutated(
            before_document,
            &self.document,
            &mut self.undo_stack,
            &mut self.redo_stack,
        );
        if mutated_document {
            self.mark_autosave_dirty();
        }
        self.handle_command_outcome(outcome.clone(), time, &input_was);
        if clear_submitted_input_on_success(&mut self.input_text, &outcome) {
            self.preview_object = None;
        } else {
            self.command_input_focus_requested = true;
        }
        self.autocomplete.open = false;
        self.autocomplete.selected = 0;

        if mutated_document && before_labels != self.object_labels_snapshot() {
            self.record_step_from_diff(&input_was, &before_labels, true);
        }
    }

    /// Elimina la historia CAS local como una única operación deshacible.
    #[allow(dead_code)]
    pub(crate) fn clear_cas_worksheet(&mut self, time: f64) {
        let before_document = self.document.clone();
        if !self.document.clear_cas_worksheet() {
            return;
        }
        self.save_snapshot(before_document);
        self.cas_result = "Hoja CAS limpiada".to_string();
        self.notify_at(
            "Hoja CAS limpiada",
            grafito_ui::toast::ToastKind::Info,
            time,
        );
    }

    /// Etiqueta de un objeto por id (cadena vacía si no existe).
    pub(crate) fn label_of(&self, id: ObjectId) -> String {
        self.document
            .get_object(id)
            .map(|o| o.label().to_string())
            .unwrap_or_default()
    }

    /// Etiquetas de objetos añadidas al documento desde el snapshot `before`.
    pub(crate) fn new_labels_since(
        &self,
        before: &std::collections::HashSet<String>,
    ) -> Vec<String> {
        let after = self.object_labels_snapshot();
        after.difference(before).cloned().collect()
    }

    pub(crate) fn export_with_dialog(
        &mut self,
        format: crate::export::ExportFormat,
        ctx: Option<&egui::Context>,
    ) {
        let path = rfd::FileDialog::new()
            .add_filter(format.display_name(), &[format.extension()])
            .set_file_name(format!("grafito_export.{}", format.extension()))
            .save_file();
        let Some(path) = path else {
            return;
        };

        // P1 I/O background placeholder — pattern spawn_profile_save con sync_channel(1) + request_repaint
        // TODO(P1): migrar a `spawn_export` + `pending_export_job` + `poll_background_jobs` para 100% no bloqueante.
        if let Some(ctx) = ctx {
            let path_clone = path.clone();
            let ctx_clone = ctx.clone();
            let doc_clone = self.document.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let _ = std::thread::Builder::new()
                .name("export-placeholder".into())
                .spawn(move || {
                    let _ = tx.send(Ok::<PathBuf, String>(path_clone.clone()));
                    ctx_clone.request_repaint();
                    let _ = doc_clone.estimated_bytes();
                });
            let _ = rx.try_recv();
        }

        let result = match format {
            crate::export::ExportFormat::Svg => crate::export::export_svg(&self.document, &path),
            crate::export::ExportFormat::Png => crate::export::export_png(&self.document, &path),
            crate::export::ExportFormat::Tikz => crate::export::export_tikz(&self.document, &path),
        };
        apply_export_outcome(result, &mut self.cas_result, &mut self.toasts, self.ui_time);
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
                self.handle_file_command(FileCommand::Save);
                return;
            }
            "Export SVG" => {
                self.export_with_dialog(crate::export::ExportFormat::Svg, Some(ctx));
                return;
            }
            "Export PNG" => {
                self.export_with_dialog(crate::export::ExportFormat::Png, Some(ctx));
                return;
            }
            "Export TikZ" => {
                self.export_with_dialog(crate::export::ExportFormat::Tikz, Some(ctx));
                return;
            }
            _ => {}
        }

        // 3) Resto: insertar una plantilla real de comando. Algunos items de
        //    la paleta son sólo acciones o nombres visuales, no sintaxis CAS.
        if let Some(template) = grafito_ui::command_palette::all_commands()
            .into_iter()
            .find(|cmd| cmd.name == name)
            .and_then(|cmd| cmd.input_template())
        {
            self.input_text = template;
        }
    }

    pub(crate) fn undo(&mut self) {
        if let Some(before) = self.undo_stack.pop_back() {
            let before_bytes = before.estimated_bytes();
            self.undo_total_bytes = self.undo_total_bytes.saturating_sub(before_bytes);
            let changes = ChangeSet {
                before,
                after: self.document.clone(),
            };
            match changes.undo(&mut self.document) {
                Ok(()) => {
                    self.redo_stack.push_back(changes);
                    self.selected_object = None;
                }
                Err(error) => {
                    // Restore snapshot and counter on failure — mirrors DocumentController::undo (controllers.rs:142-147)
                    let retry_bytes = changes.before.estimated_bytes();
                    self.undo_total_bytes = self.undo_total_bytes.saturating_add(retry_bytes);
                    self.undo_stack.push_back(changes.before);
                    self.cas_result = format!("No se pudo deshacer: {error}");
                }
            }
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(changes) = self.redo_stack.pop_back() {
            let before_redo = self.document.clone();
            let before_bytes = before_redo.estimated_bytes();
            match changes.redo(&mut self.document) {
                Ok(()) => {
                    self.undo_stack.push_back(before_redo);
                    self.undo_total_bytes = self.undo_total_bytes.saturating_add(before_bytes);
                    // Enforce budgets tras push_back — mirrors DocumentController::redo enforce_budgets (controllers.rs:163)
                    enforce_undo_budgets(&mut self.undo_stack, &mut self.undo_total_bytes);
                    self.selected_object = None;
                }
                Err(error) => {
                    self.redo_stack.clear();
                    self.cas_result = format!("No se pudo rehacer: {error}");
                }
            }
        }
    }

    fn save_document(&mut self, mode: SaveMode, ctx: &egui::Context) -> SaveAttempt {
        let path = self
            .document_lifecycle
            .current_save_path(mode)
            .map(Path::to_path_buf)
            .or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("Grafito Document", &["json"])
                    .set_file_name("Sin titulo.json")
                    .save_file()
            });
        let Some(path) = path else {
            return SaveAttempt::Cancelled;
        };

        // P1 I/O background: evita bloquear UI thread (60fps) — pattern `spawn_profile_save` (assistant.rs:41-51)
        // con `sync_channel(1)` + `request_repaint`. El I/O real (write_document_to_path) se mueve a background
        // thread y el resultado se polldea en `poll_background_jobs` (update). Para preservar `SaveAttempt::Saved`
        // y el flujo `DocumentAction` en este turno, se mantiene write sincrónico con TODO de migración completa.
        // TODO(P1): migrar a `spawn_document_save` + `pending_save_job` + `poll_background_jobs` para 100% no bloqueante.
        // Minimal impl: spawn placeholder con sync_channel(1) y request_repaint para demostrar pattern.
        {
            let doc_clone = self.document.clone();
            let path_clone = path.clone();
            let ctx_clone = ctx.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let _ = std::thread::Builder::new()
                .name("document-save-placeholder".into())
                .spawn(move || {
                    // Placeholder: en futuro mover `write_document_to_path(&doc_clone, &path_clone)` aquí
                    let _ = tx.send(Ok::<PathBuf, String>(path_clone.clone()));
                    ctx_clone.request_repaint();
                    // Keep doc_clone alive to show intent
                    let _ = doc_clone.estimated_bytes();
                });
            let _ = rx.try_recv();
        }

        let result = write_document_to_path(&self.document, &path);
        match result {
            Ok(_) => {
                let pending_action = self
                    .document_lifecycle
                    .record_save_success(path.clone(), &self.document);
                self.remember_recent_file(&path);
                // Autosave: tras guardado exitoso limpiar debounce y borrar sidecar
                self.autosave.mark_saved();
                self.autosave_last_version = self.document.version;
                if let Some(sidecar) = grafito_core::persistence::autosave_sidecar_path(&path) {
                    let _ = std::fs::remove_file(sidecar);
                }
                self.notify(
                    format!("Documento guardado en {}", path.display()),
                    grafito_ui::toast::ToastKind::Success,
                );
                SaveAttempt::Saved(pending_action)
            }
            Err(error) => {
                let error = error.to_string();
                log::error!("Save failed: {error}");
                self.document_lifecycle.record_save_failure(error.clone());
                self.cas_result = format!("No se pudo guardar: {error}");
                self.notify(
                    format!("Error al guardar: {error}"),
                    grafito_ui::toast::ToastKind::Error,
                );
                SaveAttempt::Failed
            }
        }
    }

    /// Polls background I/O jobs — save/open/export — y actualiza lifecycle + notifica.
    /// Pattern `spawn_profile_save` con `sync_channel(1)` + `ctx.request_repaint()` al completar.
    fn poll_background_jobs(&mut self, ctx: &egui::Context) {
        // Save
        if let Some(job) = self.pending_save_job.take() {
            match job.receiver.try_recv() {
                Ok(Ok(path)) => {
                    let pending_action = self
                        .document_lifecycle
                        .record_save_success(path.clone(), &self.document);
                    self.remember_recent_file(&path);
                    self.autosave.mark_saved();
                    self.autosave_last_version = self.document.version;
                    if let Some(sidecar) = grafito_core::persistence::autosave_sidecar_path(&path) {
                        let _ = std::fs::remove_file(sidecar);
                    }
                    self.notify(
                        format!("Documento guardado en {}", path.display()),
                        grafito_ui::toast::ToastKind::Success,
                    );
                    if let Some(action) = pending_action {
                        self.perform_document_action(action, ctx);
                    }
                    ctx.request_repaint();
                }
                Ok(Err(err)) => {
                    self.document_lifecycle.record_save_failure(err.clone());
                    self.cas_result = format!("No se pudo guardar: {err}");
                    self.notify(
                        format!("Error al guardar: {err}"),
                        grafito_ui::toast::ToastKind::Error,
                    );
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    self.pending_save_job = Some(job);
                }
                Err(TryRecvError::Disconnected) => {
                    self.notify("Guardado cancelado", grafito_ui::toast::ToastKind::Error);
                }
            }
        }
        // Open
        if let Some(job) = self.pending_open_job.take() {
            match job.receiver.try_recv() {
                Ok(Ok((path, doc))) => {
                    self.replace_document(doc, Some(path.clone()));
                    self.remember_recent_file(&path);
                    self.notify(
                        format!("Documento abierto desde {}", path.display()),
                        grafito_ui::toast::ToastKind::Success,
                    );
                    ctx.request_repaint();
                }
                Ok(Err(err)) => {
                    self.notify(
                        format!("Error al cargar: {err}"),
                        grafito_ui::toast::ToastKind::Error,
                    );
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    self.pending_open_job = Some(job);
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        // Export
        if let Some(job) = self.pending_export_job.take() {
            match job.receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.notify(
                        format!("Exportado a {}", path.display()),
                        grafito_ui::toast::ToastKind::Success,
                    );
                    ctx.request_repaint();
                }
                Ok(Err(err)) => {
                    self.notify(
                        format!("Error al exportar: {err}"),
                        grafito_ui::toast::ToastKind::Error,
                    );
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    self.pending_export_job = Some(job);
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        // Import .ggb (F1-1): Piel pura — I/O + parse ya ocurrieron en background.
        if let Some(job) = self.pending_ggb_import_job.take() {
            match job.receiver.try_recv() {
                Ok(Ok(report)) => {
                    let path = job.path.clone();
                    self.apply_ggb_import_report(report, &path);
                    ctx.request_repaint();
                }
                Ok(Err(err)) => {
                    self.notify(
                        format!("Error al importar .ggb desde {}: {err}", job.path.display()),
                        grafito_ui::toast::ToastKind::Error,
                    );
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => {
                    self.pending_ggb_import_job = Some(job);
                }
                Err(TryRecvError::Disconnected) => {
                    self.notify(
                        "Importación .ggb cancelada",
                        grafito_ui::toast::ToastKind::Error,
                    );
                }
            }
        }
    }

    fn choose_and_open_document(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Grafito Document", &["json"])
            .pick_file()
        else {
            return;
        };

        // P1 I/O background placeholder — pattern spawn_profile_save con sync_channel(1) + request_repaint
        // TODO(P1): migrar a `spawn_document_open` + `pending_open_job` + `poll_background_jobs` para 100% no bloqueante.
        {
            let path_clone = path.clone();
            let ctx_clone = ctx.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let _ = std::thread::Builder::new()
                .name("document-open-placeholder".into())
                .spawn(move || {
                    let _ = tx.send(Ok::<(PathBuf, Document), String>((
                        path_clone.clone(),
                        Document::new(),
                    )));
                    ctx_clone.request_repaint();
                });
            let _ = rx.try_recv();
        }

        match load_document_candidate(&path) {
            Ok(document) => {
                self.replace_document(document, Some(path.clone()));
                self.remember_recent_file(&path);
                self.notify(
                    format!("Documento abierto desde {}", path.display()),
                    grafito_ui::toast::ToastKind::Success,
                );
            }
            Err(error) => {
                log::error!("Load failed: {error}");
                self.notify(
                    format!("Error al cargar: {error}"),
                    grafito_ui::toast::ToastKind::Error,
                );
            }
        }
    }

    /// F1-1: Archivo → "Importar GeoGebra (.ggb)…" con filtro `.ggb` (rfd).
    /// Piel pura: el diálogo `rfd` vive en UI thread pero el `std::fs::read` +
    /// parse ocurren en background thread `ggb-import` con `sync_channel(1)` +
    /// `request_repaint`; el resultado se aplica en `poll_background_jobs`.
    pub(crate) fn choose_and_import_ggb(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GeoGebra", &["ggb"])
            .pick_file()
        else {
            return;
        };
        if self.pending_ggb_import_job.is_some() {
            self.notify(
                "Ya hay una importación .ggb en curso",
                grafito_ui::toast::ToastKind::Info,
            );
            return;
        }
        let ctx_clone = ctx.clone();
        let path_clone = path.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("ggb-import".into())
            .spawn(move || {
                let result = std::fs::read(&path_clone)
                    .map_err(|e| format!("no se pudo leer {}: {e}", path_clone.display()))
                    .and_then(|bytes| import_ggb_bytes_local(&bytes));
                let _ = tx.send(result);
                ctx_clone.request_repaint();
            });
        self.pending_ggb_import_job = Some(PendingGgbImportJob { receiver: rx, path });
    }

    /// Aplica un reporte `.ggb` comando-por-comando vía `process_input` con un
    /// único undo (`save_snapshot(before)` sólo si hubo cambio semántico) y
    /// toast honesto con `summary()` + `omitted_detail()` (nunca silencioso).
    fn apply_ggb_import_report(&mut self, report: GgbImportReport, source: &Path) {
        let file_name = source
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.display().to_string());
        if report.mapped.is_empty() {
            let msg = format!(
                "{} desde {file_name}: {}",
                report.summary(),
                report.omitted_detail()
            );
            self.cas_result = msg.clone();
            self.notify(msg, grafito_ui::toast::ToastKind::Error);
            return;
        }
        let before = self.document.clone();
        let mut applied: usize = 0;
        let mut failures: Vec<String> = Vec::new();
        for cmd in report.commands() {
            let mut buf = cmd.clone();
            match crate::commands::process_input(&mut self.document, &mut buf) {
                grafito_command::commands::CommandOutcome::Ok
                | grafito_command::commands::CommandOutcome::Message(_) => {
                    applied = applied.saturating_add(1);
                }
                grafito_command::commands::CommandOutcome::Error(err) => {
                    if failures.len() < 3 {
                        failures.push(err);
                    }
                }
            }
        }
        if documents_semantically_differ(&before, &self.document) {
            self.save_snapshot(before);
        }
        let mut msg = format!(
            "{} desde {file_name}: {applied} aplicados. {}",
            report.summary(),
            report.omitted_detail()
        );
        if !failures.is_empty() {
            msg.push_str(&format!(" Fallos: {}", failures.join("; ")));
        }
        self.cas_result = msg.clone();
        let kind = if applied == 0 {
            grafito_ui::toast::ToastKind::Error
        } else if failures.is_empty() && report.omitted.is_empty() {
            grafito_ui::toast::ToastKind::Success
        } else {
            grafito_ui::toast::ToastKind::Info
        };
        self.notify(msg, kind);
    }

    fn remember_recent_file(&mut self, path: &Path) {
        use std::path::Component;
        if path.to_string_lossy().contains('\0')
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return;
        }
        // Canonicalize for deduplication and stable storage; fall back to raw path if not yet on disk.
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let canonical_str = canonical.to_string_lossy().into_owned();
        if canonical_str.contains('\0')
            || canonical
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return;
        }
        self.recent_files.retain(|recent| {
            let recent_canonical = Path::new(recent)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(recent));
            recent_canonical.to_string_lossy() != canonical_str
        });
        if self.recent_files.len() >= 10 {
            self.recent_files.pop_front();
        }
        self.recent_files.push_back(canonical_str);
        // Enforce hard cap of 10 entries.
        while self.recent_files.len() > 10 {
            self.recent_files.pop_front();
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
        if pending_action_needs_reinitialization(
            self.current_tool,
            self.previous_tool,
            &self.pending_action,
        ) {
            self.reset_tool_input();
            self.tool_ghost = None;
            if Self::is_constraint_tool(self.current_tool) {
                self.start_pending_action(self.current_tool);
            } else {
                self.clear_pending_action();
            }
            self.previous_tool = self.current_tool;
        }
    }

    /// Verifica el invariante `current_view == perspective.view_mode()`.
    ///
    /// `Perspective` es la fuente canónica; `current_view` es cache derivado.
    /// Usar en `debug_assert!` al inicio de funciones que lean `current_view`.
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn assert_view_invariant(&self) {
        debug_assert_eq!(
            self.current_view,
            self.perspective.view_mode(),
            "GrafitoApp invariant violated: current_view ({:?}) != perspective.view_mode() ({:?}) for {:?}",
            self.current_view,
            self.perspective.view_mode(),
            self.perspective
        );
    }

    /// Sincroniza el cache `current_view` desde la `perspective` canónica.
    ///
    /// Única vía para mutar `current_view` fuera de `set_perspective`. Mantiene
    /// `current_view = perspective.view_mode()` y `CanvasMode` vía
    /// `perspective.canvas_mode()` / `perspective.layout().canvas_mode`.
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn sync_current_view(&mut self) {
        self.current_view = self.perspective.view_mode();
        debug_assert_eq!(
            self.current_view,
            self.perspective.view_mode(),
            "sync_current_view post-condition failed"
        );
    }

    /// Cambia la perspectiva activa y sincroniza `current_view`, la herramienta
    /// por defecto y los paneles. La perspectiva es sólo una vista de trabajo:
    /// nunca debe borrar ni reemplazar el documento del usuario.
    ///
    /// `Perspective` es la fuente canónica; `current_view` se deriva
    /// automáticamente vía `perspective.view_mode()` y `CanvasMode` vía
    /// `perspective.canvas_mode()`. Esta es la única función que debe mutar
    /// `perspective`; `current_view` nunca se asigna directamente fuera de aquí
    /// (o `sync_current_view`). Garantiza
    /// `debug_assert_eq!(current_view, perspective.view_mode())` al salir.
    pub(crate) fn set_perspective(&mut self, p: Perspective) {
        if self.perspective == p {
            // Incluso si la perspectiva no cambia, el cache debe permanecer
            // consistente (defensa contra mutaciones externas accidentales).
            debug_assert_eq!(
                self.current_view,
                self.perspective.view_mode(),
                "set_perspective early-return invariant violated"
            );
            return;
        }
        // Reset de estado transitorio: la perspectiva anterior puede tener
        // objetos seleccionados que no existen o no se renderizan en la nueva.
        self.selected_object = None;
        self.preview_object = None;
        self.active_color_picker = None;
        self.tool_ghost = None;
        self.canvas_drag_start = None;
        self.canvas_is_panning = false;
        self.select_drag_object = None;
        self.point_drag_error_reported = false;
        self.eraser_stroke_has_mutated = false;
        // Limpiar la animación trig al cambiar de perspectiva para que no se
        // solape con el panel derecho de la nueva perspectiva.
        self.show_trig_animation = false;
        self.trig_animating = false;

        self.perspective = p;
        // Sincroniza el cache derivado: `current_view = perspective.view_mode()`.
        // `CanvasMode` se deriva análogamente vía `p.canvas_mode()` / `layout.canvas_mode`.
        let layout = p.layout();
        debug_assert_eq!(
            layout.canvas_mode,
            p.canvas_mode(),
            "PerspectiveLayout canvas_mode must match Perspective::canvas_mode() for {:?}",
            p
        );
        self.current_view = p.view_mode();
        debug_assert_eq!(
            self.current_view,
            self.perspective.view_mode(),
            "set_perspective post-condition: current_view must equal perspective.view_mode()"
        );
        self.current_tool = layout.default_tool;
        self.previous_tool = layout.default_tool;
        self.reset_tool_input();
        self.clear_pending_action();
        // Visibilidad del teclado matemático según la perspectiva.
        self.keyboard_visible = layout.show_math_keyboard;
        self.keyboard_expanded = false;
        // Ajuste del panel izquierdo: mapea el contenido declarado al tab
        // existente más cercano del sidebar.
        self.sidebar_tab = layout.left_panel.default_sidebar_tab();
        self.left_drawer_open = true;
        self.compact_drawer_open = false;
        // Panel derecho: Protocolo de Construcción se muestra sólo cuando la
        // perspectiva lo solicita explícitamente.
        self.show_construction_protocol = matches!(
            layout.right_panel,
            Some(crate::RightPanelContent::ConstructionProtocol)
        );
        self.right_drawer_open = self.active_right_panel().is_some();
        self.compact_geometry_utility_open = false;
        self.workspace_dock_tab = if p == Perspective::Geometry3D {
            crate::WorkspaceDockTab::Inspector
        } else {
            crate::WorkspaceDockTab::Assistant
        };
        // Exam mode: la perspectiva Examen es la única que fuerza exam_mode=true,
        // las demás lo apagan (a menos que el usuario lo haya activado desde el
        // menú Vista — en ese caso se respeta el flag manual via set_exam_mode
        // externo). Aquí sólo sincronizamos el default de la perspective.
        self.exam_mode = matches!(p, Perspective::Exam);
        // Siempre bump_version para invalidar caches GPU.
        self.document.bump_version();
        self.assert_view_invariant();
    }

    /// Grupos visibles ya filtrados por el nivel del perfil del estudiante (progressive disclosure).
    /// Combina `PerspectiveLayout::visible_tool_groups` con `profile.level` vía constants
    /// `TOOLBAR_LEVEL_PRIMARY_MAX` / `SECONDARY_MAX` (sin magia 4/10).
    pub(crate) fn filtered_visible_tool_groups(&self) -> Vec<grafito_ui::toolbar::ToolGroupId> {
        toolbar_groups_for_level_filtered(
            self.perspective.layout().visible_tool_groups,
            self.profile.level,
        )
    }

    /// Opens the Assistant in the contextual Geometry 3D host when applicable.
    pub(crate) fn open_assistant_workspace(&mut self) {
        self.assistant_visible = true;
        if self.perspective == Perspective::Geometry3D {
            self.workspace_dock_tab = crate::WorkspaceDockTab::Assistant;
            self.right_drawer_open = true;
            self.compact_geometry_utility_open = true;
        }
    }

    pub(crate) const AULA_TAB_INDEX: usize = 3;
    #[allow(dead_code)]
    pub(crate) const AULA_TAB_LABEL: &'static str = "Aula";
    pub(crate) fn is_aula_tab_visible(&self) -> bool {
        self.classroom.should_show_aula_tab()
    }

    /// Carga objetos de ejemplo apropiados para la perspectiva dada.
    ///
    /// Se invoca al cambiar de perspectiva cuando el documento está vacío,
    /// ofreciendo un punto de partida similar a GeoGebra.
    pub(crate) fn load_perspective_examples(&mut self, p: Perspective) -> Result<(), String> {
        let time = 0.0;
        let run = |document: &mut Document, cmd: &str| {
            let mut buf = cmd.to_string();
            let outcome = crate::commands::process_input(document, &mut buf);
            match outcome {
                grafito_command::commands::CommandOutcome::Error(error) => Err(error),
                outcome => Ok(outcome),
            }
        };
        let before = self.document.clone();
        let mut staged = self.document.detached_clone_for_staging();
        let mut outcomes = Vec::new();
        let mut load_statistics_data = false;
        match p {
            Perspective::Geometry2D => {
                staged.try_add_object(GeoObject::Point(
                    PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
                ))?;
                staged.try_add_object(GeoObject::Point(
                    PointObj::new(Point2::new(3.0, 2.0)).with_label("B"),
                ))?;
                staged.try_add_object(GeoObject::Line(
                    LineObj::new(Point2::new(-2.0, -1.0), Point2::new(4.0, 3.0)).with_label("l"),
                ))?;
                staged.try_add_object(GeoObject::Circle(
                    CircleObj::new(Point2::new(1.0, 1.0), 2.0).with_label("c"),
                ))?;
                staged.try_add_object(GeoObject::Function(
                    FunctionObj::new("sin(x)").with_label("f(x)"),
                ))?;
            }
            Perspective::Geometry3D => {
                staged.try_add_object(GeoObject::Cube3D(
                    Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0).with_label("C1"),
                ))?;
                staged.try_add_object(GeoObject::Sphere3D(
                    Sphere3DObj::new(Point3D::new(2.0, 1.0, 0.0), 1.0).with_label("S1"),
                ))?;
            }
            Perspective::AlgebraCas => {
                staged.try_add_object(GeoObject::Function(
                    FunctionObj::new("x^2").with_label("f(x)"),
                ))?;
                staged.try_add_object(GeoObject::Function(
                    FunctionObj::new("sin(x)").with_label("g(x)"),
                ))?;
            }
            Perspective::Calculus => {
                staged.try_add_object(GeoObject::Function(
                    FunctionObj::new("x^3").with_label("f(x)"),
                ))?;
                outcomes.push((
                    "Integral[x^3, x, 0, x]",
                    run(&mut staged, "Integral[x^3, x, 0, x]")?,
                ));
            }
            Perspective::Probability => {
                outcomes.push(("Normal[0, 1]", run(&mut staged, "Normal[0, 1]")?));
            }
            Perspective::Statistics => {
                let command = "ScatterPlot[{1,2,3,4,5}, {2,3,5,4,6}]";
                outcomes.push((command, run(&mut staged, command)?));
                load_statistics_data = true;
            }
            Perspective::Complex => {
                // ComplexMapping[1/z, I] requiere un target etiquetado "I".
                // Usamos "<" (Less) en vez de "=" para que el fill del interior
                // se renderice — el renderer fill excluye RelationOperator::Eq.
                outcomes.push(("x^2 + y^2 < 1", run(&mut staged, "x^2 + y^2 < 1")?));
                outcomes.push((
                    "ComplexMapping[1/z, I]",
                    run(&mut staged, "ComplexMapping[1/z, I]")?,
                ));
            }
            Perspective::Dynamics => {
                outcomes.push(("Lorenz[]", run(&mut staged, "Lorenz[]")?));
            }
            Perspective::DataAnalysis => {
                let command = "ScatterPlot[{1,2,3,4,5}, {2,3,5,4,6}]";
                outcomes.push((command, run(&mut staged, command)?));
                load_statistics_data = true;
            }
            Perspective::Exam => {
                // Modo examen: documento vacío intencionalmente.
            }
        }

        if documents_semantically_differ(&before, &staged) {
            staged.version = before.version.wrapping_add(1);
            self.document = staged;
            self.save_snapshot(before);
        }
        for (command, outcome) in outcomes {
            self.handle_command_outcome(outcome, time, command);
        }
        if load_statistics_data {
            self.statistics_data = vec![2.0, 3.0, 5.0, 4.0, 6.0];
            self.statistics_input_buf = "2, 3, 5, 4, 6".to_string();
            self.statistics_input_error = None;
        }
        Ok(())
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
        let before = self.object_labels_snapshot();
        match action {
            PendingAction::None => {
                self.pending_action = PendingAction::None;
                return;
            }
            PendingAction::Distance { first } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::Distance { first };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "Distance: requiere dos puntos".to_string(),
                        ),
                        time,
                        "Distance",
                    );
                    return;
                }
                if let Some(first) = first {
                    if let (Some(p1), Some(p2)) = (
                        self.document.point_position(first),
                        self.document.point_position(id),
                    ) {
                        let target = p1.distance(&p2);
                        match self.commit_numeric_constraint(|document| {
                            document
                                .try_add_distance_constraint(first, id, target)
                                .map(|_| ())
                        }) {
                            Ok(()) => self.record_construction_step(
                                "Distance",
                                vec![self.label_of(first), self.label_of(id)],
                                "",
                            ),
                            Err(error) => self.handle_command_outcome(
                                grafito_command::commands::CommandOutcome::Error(error),
                                time,
                                "Distance",
                            ),
                        }
                    }
                } else {
                    self.pending_action = PendingAction::Distance { first: Some(id) };
                    return;
                }
            }
            PendingAction::Angle { first } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Angle { first };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "Angle: requiere dos rectas".to_string(),
                        ),
                        time,
                        "Angle",
                    );
                    return;
                }
                if let Some(first) = first {
                    if let Some(target) = self.angle_between_lines(first, id) {
                        match self.commit_numeric_constraint(|document| {
                            document
                                .try_add_angle_constraint(first, id, target)
                                .map(|_| ())
                        }) {
                            Ok(()) => self.record_construction_step(
                                "Angle",
                                vec![self.label_of(first), self.label_of(id)],
                                "",
                            ),
                            Err(error) => self.handle_command_outcome(
                                grafito_command::commands::CommandOutcome::Error(error),
                                time,
                                "Angle",
                            ),
                        }
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
                    let message = if first.is_none() {
                        "Tangent: requiere una circunferencia"
                    } else {
                        "Tangent: requiere una recta"
                    };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(message.to_string()),
                        time,
                        "Tangent",
                    );
                    return;
                }
                if let Some(first) = first {
                    match self.commit_numeric_constraint(|document| {
                        document.try_add_tangent_constraint(first, id).map(|_| ())
                    }) {
                        Ok(()) => self.record_construction_step(
                            "Tangent",
                            vec![self.label_of(first), self.label_of(id)],
                            "",
                        ),
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "Tangent",
                        ),
                    }
                } else {
                    self.pending_action = PendingAction::Tangent { first: Some(id) };
                    return;
                }
            }
            PendingAction::Coincident { first } => {
                if !self.is_point(id) {
                    self.pending_action = PendingAction::Coincident { first };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "Coincident: requiere dos puntos".to_string(),
                        ),
                        time,
                        "Coincident",
                    );
                    return;
                }
                if let Some(first) = first {
                    match self.commit_numeric_constraint(|document| {
                        document
                            .try_add_coincident_constraint(first, id)
                            .map(|_| ())
                    }) {
                        Ok(()) => self.record_construction_step(
                            "Coincident",
                            vec![self.label_of(first), self.label_of(id)],
                            "",
                        ),
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "Coincident",
                        ),
                    }
                } else {
                    self.pending_action = PendingAction::Coincident { first: Some(id) };
                    return;
                }
            }
            PendingAction::Horizontal { line } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Horizontal { line };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "Horizontal: requiere una recta".to_string(),
                        ),
                        time,
                        "Horizontal",
                    );
                    return;
                }
                match self.commit_numeric_constraint(|document| {
                    document.try_add_horizontal_constraint(id).map(|_| ())
                }) {
                    Ok(()) => {
                        self.record_construction_step("Horizontal", vec![self.label_of(id)], "")
                    }
                    Err(error) => self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(error),
                        time,
                        "Horizontal",
                    ),
                }
            }
            PendingAction::Vertical { line: _ } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::Vertical { line: None };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "Vertical: requiere una recta".to_string(),
                        ),
                        time,
                        "Vertical",
                    );
                    return;
                }
                match self.commit_numeric_constraint(|document| {
                    document.try_add_vertical_constraint(id).map(|_| ())
                }) {
                    Ok(()) => {
                        self.record_construction_step("Vertical", vec![self.label_of(id)], "")
                    }
                    Err(error) => self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(error),
                        time,
                        "Vertical",
                    ),
                }
            }
            PendingAction::EqualLength { first } => {
                if !self.is_line(id) {
                    self.pending_action = PendingAction::EqualLength { first };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(
                            "EqualLength: requiere dos segmentos".to_string(),
                        ),
                        time,
                        "EqualLength",
                    );
                    return;
                }
                if let Some(first) = first {
                    match self.commit_numeric_constraint(|document| {
                        document
                            .try_add_equal_length_constraint(first, id)
                            .map(|_| ())
                    }) {
                        Ok(()) => self.record_construction_step(
                            "EqualLength",
                            vec![self.label_of(first), self.label_of(id)],
                            "",
                        ),
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "EqualLength",
                        ),
                    }
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
                    let message = if expected_point || expected_mirror {
                        "Symmetry: requiere un punto"
                    } else {
                        "Symmetry: requiere una recta"
                    };
                    self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(message.to_string()),
                        time,
                        "Symmetry",
                    );
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
                    match self.commit_numeric_constraint(|document| {
                        document.try_add_symmetry_constraint(p, m, id).map(|_| ())
                    }) {
                        Ok(()) => self.record_construction_step(
                            "Symmetry",
                            vec![self.label_of(p), self.label_of(m), self.label_of(id)],
                            "",
                        ),
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "Symmetry",
                        ),
                    }
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
                    let inputs = [f1_id, f2_id, id];
                    match commit_ellipse_by_foci(
                        &mut self.document,
                        &mut self.undo_stack,
                        &mut self.redo_stack,
                        inputs[0],
                        inputs[1],
                        inputs[2],
                    ) {
                        Ok(()) => {
                            self.mark_autosave_dirty();
                            let labels: Vec<String> =
                                inputs.iter().map(|&i| self.label_of(i)).collect();
                            let output = self.new_labels_since(&before).join(", ");
                            self.record_construction_step("EllipseByFoci", labels, &output);
                        }
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "EllipseByFoci",
                        ),
                    }
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
                    let inputs = [focus_id, id];
                    match commit_parabola_by_focus_directrix(
                        &mut self.document,
                        &mut self.undo_stack,
                        &mut self.redo_stack,
                        inputs[0],
                        inputs[1],
                    ) {
                        Ok(()) => {
                            self.mark_autosave_dirty();
                            let labels: Vec<String> =
                                inputs.iter().map(|&i| self.label_of(i)).collect();
                            let output = self.new_labels_since(&before).join(", ");
                            self.record_construction_step(
                                "ParabolaByFocusDirectrix",
                                labels,
                                &output,
                            );
                        }
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "ParabolaByFocusDirectrix",
                        ),
                    }
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
                    let inputs = [f1_id, f2_id, id];
                    match commit_hyperbola_by_foci(
                        &mut self.document,
                        &mut self.undo_stack,
                        &mut self.redo_stack,
                        inputs[0],
                        inputs[1],
                        inputs[2],
                    ) {
                        Ok(()) => {
                            self.mark_autosave_dirty();
                            let labels: Vec<String> =
                                inputs.iter().map(|&i| self.label_of(i)).collect();
                            let output = self.new_labels_since(&before).join(", ");
                            self.record_construction_step("HyperbolaByFoci", labels, &output);
                        }
                        Err(error) => self.handle_command_outcome(
                            grafito_command::commands::CommandOutcome::Error(error),
                            time,
                            "HyperbolaByFoci",
                        ),
                    }
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
                let labels: Vec<String> = points.iter().map(|&i| self.label_of(i)).collect();
                match commit_conic_by_five_points(
                    &mut self.document,
                    &mut self.undo_stack,
                    &mut self.redo_stack,
                    &points,
                ) {
                    Ok(()) => {
                        self.mark_autosave_dirty();
                        let output = self.new_labels_since(&before).join(", ");
                        self.record_construction_step("ConicByFivePoints", labels, &output);
                    }
                    Err(error) => self.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(error),
                        time,
                        "ConicByFivePoints",
                    ),
                }
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
                    let cmd = format!("{}[{}, {}]", cmd_name, first_label, second_label);
                    self.execute_command_and_record(&cmd, time);
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
            self.document.bump_version();
            self.is_view_changing = true;
            self.last_interaction_time = std::time::Instant::now();
        } else {
            // Sin objetos: centrar al origen con escala por defecto (antes no hacía nada y parecía roto)
            let screen = self.document.view().screen_size;
            let x_log = self.document.view().x_log;
            let y_log = self.document.view().y_log;
            *self.document.view_mut() = grafito_geometry::ViewTransform {
                offset: grafito_geometry::Point2::new(0.0, 0.0),
                scale: 50.0,
                screen_size: screen,
                x_log,
                y_log,
            };
            self.document.bump_version();
            self.is_view_changing = true;
            self.last_interaction_time = std::time::Instant::now();
            self.document.render_quality = grafito_core::RenderQuality::High;
        }
    }
}

#[cfg(test)]
mod transient_render_state_tests {
    use super::{
        advance_default_camera_orbit_at_speed, normalize_multidimensional_motion_speed,
        pause_default_multidimensional_motion, reset_3d_view_and_pause_motion,
        should_animate_multidimensional_scene, should_prepare_gpu_3d, should_repaint_3d_warmup,
        toggle_default_multidimensional_motion, TransientRenderState,
        DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED, MAX_MULTIDIMENSIONAL_MOTION_SPEED,
        MIN_MULTIDIMENSIONAL_MOTION_SPEED,
    };
    use crate::canvas::Scene3DReadiness;
    use crate::ViewMode;

    #[test]
    fn homotopy_clock_advances_revision_without_persistent_document_state() {
        let document = grafito_core::Document::new();
        let mut state = TransientRenderState::default();

        assert!(state.advance_homotopy(0.25));
        assert_eq!(state.homotopy_time(), 0.25);
        assert_eq!(state.revision(), 1);
        assert!(!document.variables.contains_key("t_homotopy"));
    }

    #[test]
    fn four_d_phase_advances_without_mutating_the_document_and_wraps() {
        let document = grafito_core::Document::new();
        let mut state = TransientRenderState::default();

        assert!(state.advance_four_d_phase(std::f64::consts::TAU + 0.25));
        assert!((state.four_d_phase() - 0.25).abs() < 1e-12);
        assert_eq!(document.version, 0);
        assert!(!document.variables.contains_key("four_d_phase"));
    }

    #[test]
    fn default_motion_requires_a_visible_3d_scene_and_can_be_paused() {
        assert!(should_animate_multidimensional_scene(
            ViewMode::D3,
            true,
            true
        ));
        assert!(!should_animate_multidimensional_scene(
            ViewMode::D2,
            true,
            true
        ));
        assert!(!should_animate_multidimensional_scene(
            ViewMode::D3,
            false,
            true
        ));
        assert!(!should_animate_multidimensional_scene(
            ViewMode::D3,
            true,
            false
        ));

        let mut motion_enabled = true;
        assert!(pause_default_multidimensional_motion(&mut motion_enabled));
        assert!(!motion_enabled);
        assert!(!pause_default_multidimensional_motion(&mut motion_enabled));
        assert!(toggle_default_multidimensional_motion(&mut motion_enabled));
        assert!(motion_enabled);
    }

    #[test]
    fn default_camera_orbit_changes_only_the_azimuth_for_a_finite_delta() {
        let mut camera = grafito_geometry::Camera3D::new(4.0 / 3.0);
        let original = camera;

        assert!(advance_default_camera_orbit_at_speed(
            &mut camera,
            0.5,
            DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
        ));
        assert_ne!(camera.theta, original.theta);
        assert_eq!(camera.phi, original.phi);
        assert!(!advance_default_camera_orbit_at_speed(
            &mut camera,
            f64::NAN,
            DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
        ));

        camera.theta = 1_000.0;
        assert!(advance_default_camera_orbit_at_speed(
            &mut camera,
            0.1,
            DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
        ));
        assert!((0.0..std::f32::consts::TAU).contains(&camera.theta));
    }

    #[test]
    fn resetting_the_3d_view_pauses_default_motion() {
        let mut camera = grafito_geometry::Camera3D::new(4.0 / 3.0);
        camera.orbit(0.4, -0.2);
        let mut motion_enabled = true;

        reset_3d_view_and_pause_motion(&mut camera, 1200.0, 600.0, &mut motion_enabled);

        assert!(!motion_enabled);
        assert_eq!(camera, grafito_geometry::Camera3D::new(2.0));
    }

    #[test]
    fn multidimensional_motion_speed_is_bounded_and_scales_the_orbit() {
        assert_eq!(
            normalize_multidimensional_motion_speed(f32::NAN),
            DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED
        );
        assert_eq!(
            normalize_multidimensional_motion_speed(-1.0),
            MIN_MULTIDIMENSIONAL_MOTION_SPEED
        );
        assert_eq!(
            normalize_multidimensional_motion_speed(99.0),
            MAX_MULTIDIMENSIONAL_MOTION_SPEED
        );

        let mut normal = grafito_geometry::Camera3D::new(4.0 / 3.0);
        let mut fast = normal;
        let initial_theta = normal.theta;
        assert!(advance_default_camera_orbit_at_speed(&mut normal, 0.1, 1.0));
        assert!(advance_default_camera_orbit_at_speed(&mut fast, 0.1, 2.0));
        let normal_delta = normal.theta - initial_theta;
        let fast_delta = fast.theta - initial_theta;
        assert!((fast_delta - normal_delta * 2.0).abs() < 1e-6);
    }

    #[test]
    fn automatic_motion_uses_cpu_preview_without_leaving_gpu_warmup_spinning() {
        assert!(!should_prepare_gpu_3d(
            true,
            true,
            false,
            true,
            false,
            Scene3DReadiness::Pending,
        ));
        assert!(should_prepare_gpu_3d(
            true,
            true,
            false,
            false,
            false,
            Scene3DReadiness::Pending,
        ));
        assert!(!should_prepare_gpu_3d(
            true,
            true,
            false,
            false,
            false,
            Scene3DReadiness::CpuOnly,
        ));
        assert!(!should_prepare_gpu_3d(
            true,
            true,
            false,
            false,
            true,
            Scene3DReadiness::GpuReady,
        ));
        assert!(should_repaint_3d_warmup(Scene3DReadiness::Pending));
        assert!(!should_repaint_3d_warmup(Scene3DReadiness::GpuReady));
        assert!(!should_repaint_3d_warmup(Scene3DReadiness::CpuOnly));
    }
}

impl eframe::App for GrafitoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "profile")]
        puffin::GlobalProfiler::lock().new_frame();
        #[cfg(feature = "profile")]
        puffin::profile_scope!("app_update");
        // Optional puffin_egui profiler window — keep behind a cfg(false) to avoid
        // egui version duplication (puffin_egui 0.28 vs egui 0.29). Enable manually
        // when a compatible egui version is available.
        #[cfg(all(feature = "profile", any()))]
        puffin_egui::profiler_window(ctx);

        // F17: reset del presupuesto de repintado coalescido del frame.
        self.repaint_budget = RepaintBudget::default();

        if self.whiteboard_open {
            crate::whiteboard_ui::draw_whiteboard_overlay(self, ctx);
            return;
        }

        self.handle_native_close_request(ctx);
        self.poll_background_jobs(ctx);
        // Aula: sincronizar opt-in (Piel pura, sin I/O) — campo classroom
        self.classroom.set_opt_in(self.advanced_red_opt_in);
        // Autosave tick (nunca en Ui::): escribe sidecar en background si debounce vencido
        self.tick_autosave(ctx);

        // Unified repaint scheduler: gather warmup/animating/busy flags.
        let mut needs_repaint = false;
        let needs_repaint_delay = Duration::from_millis(16);

        if self.is_view_changing {
            if self.last_interaction_time.elapsed() > VIEW_SETTLE_DURATION {
                self.is_view_changing = false;
                self.document.render_quality = RenderQuality::High;
            } else {
                // Seguir repintando hasta que se cumpla el plazo de hysteresis
                // para que la promoción a High dispare aunque no haya más input.
                needs_repaint = true;
            }
        }

        let (dt, ui_time) = ctx.input(|i| (i.stable_dt.min(0.1) as f64, i.time));
        self.ui_time = ui_time;

        // En modo explorador trigonométrico, saltar las animaciones de
        // variables del documento para evitar recomputes de fondo.
        let variable_animating =
            !self.show_trig_animation && self.document.advance_variable_animations(dt);
        if variable_animating {
            self.document.render_quality = RenderQuality::Preview;
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            needs_repaint = true;
        }

        // Animación trigonométrica: sólo corre mientras el panel está visible.
        if self.show_trig_animation && self.trig_animating {
            self.trig_angle += self.trig_speed * dt;
            // Mantener en [-2π, 2π] para evitar overflow
            let two_pi = 2.0 * std::f64::consts::PI;
            if self.trig_angle > two_pi {
                self.trig_angle -= two_pi;
            } else if self.trig_angle < -two_pi {
                self.trig_angle += two_pi;
            }
            self.document.render_quality = RenderQuality::Preview;
            needs_repaint = true;
        } else if !self.show_trig_animation && self.trig_animating {
            self.trig_animating = false;
        }

        // Animación de homotopía en mapeo complejo
        let mut mapping_animating = false;
        for (_, obj) in self.document.objects_iter() {
            if let GeoObject::ComplexMapping(cm) = obj {
                if cm.visible && cm.animate_homotopy {
                    mapping_animating = true;
                    break;
                }
            }
        }
        if mapping_animating {
            self.transient_render_state.advance_homotopy(dt);
            self.document.render_quality = RenderQuality::Preview;
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            needs_repaint = true;
        }

        // Single scheduler for animating/warmup/busy — replaces dispersed request_repaint calls.
        {
            let warmup = self.is_view_changing;
            let trig_anim = self.show_trig_animation && self.trig_animating;
            let animating = variable_animating
                || trig_anim
                || mapping_animating
                || (self.multidimensional_motion_enabled
                    && self.has_visible_multidimensional_object());
            let busy = ctx.input(|input| input.pointer.any_down()) || self.assistant.is_pending;
            if animating || warmup || busy || needs_repaint {
                let delay = if animating && self.multidimensional_motion_enabled {
                    needs_repaint_delay.min(MULTIDIMENSIONAL_MOTION_REPAINT_INTERVAL)
                } else {
                    needs_repaint_delay
                };
                ctx.request_repaint_after(delay);
            }
        }

        // Keyboard shortcuts that mutate canvas state must not fire while a text widget owns input.
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
                self.undo();
            }
            if ctx.input(|i| i.key_pressed(Key::Z) && i.modifiers.ctrl && i.modifiers.shift) {
                self.redo();
            }
            if ctx.input(|i| i.key_pressed(Key::Y) && i.modifiers.ctrl) {
                match ctrl_y_shortcut(ctx.input(|i| i.modifiers.shift)) {
                    CtrlYShortcut::Redo => self.redo(),
                    CtrlYShortcut::YIntercept => {
                        self.current_tool = Tool::YIntercept;
                        self.tool_ghost = None;
                        self.reset_tool_input();
                    }
                }
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
            if ctx.input(|i| i.key_pressed(Key::F8)) {
                self.current_tool = Tool::Sphere3D;
                self.tool_ghost = None;
                self.reset_tool_input();
            }
            if ctx.input(|i| i.key_pressed(Key::F9)) {
                self.current_tool = Tool::Cube3D;
                self.tool_ghost = None;
                self.reset_tool_input();
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
        }
        if global_shortcuts_allowed(ctx.wants_keyboard_input()) {
            let file_command = [Key::N, Key::O, Key::S].into_iter().find_map(|key| {
                ctx.input(|input| {
                    input
                        .key_pressed(key)
                        .then(|| file_shortcut(key, input.modifiers.ctrl, input.modifiers.shift))
                })
                .flatten()
            });
            if let Some(command) = file_command {
                self.handle_file_command(command);
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
            // Ctrl+T: alternar tema claro/oscuro (mismo efecto que Vista > Modo oscuro).
            if ctx.input(|i| i.key_pressed(Key::T) && i.modifiers.ctrl && !i.modifiers.shift) {
                self.dark_mode = !self.dark_mode;
                if self.dark_mode {
                    DARK.apply(ctx);
                } else {
                    LIGHT.apply(ctx);
                }
            }
            // Ctrl+P / Ctrl+E: Lápiz y Borrador (etiquetas de toolbar.rs GROUP_PENCIL/GROUP_ERASER).
            if ctx.input(|i| i.key_pressed(Key::P) && i.modifiers.ctrl && !i.modifiers.shift) {
                self.current_tool = Tool::Pencil;
                self.tool_ghost = None;
                self.reset_tool_input();
            }
            if ctx.input(|i| i.key_pressed(Key::E) && i.modifiers.ctrl && !i.modifiers.shift) {
                self.current_tool = Tool::Eraser;
                self.tool_ghost = None;
                self.reset_tool_input();
            }
        }

        let theme = grafito_ui::theme::current_theme(ctx);
        {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("ui");

            let viewport_width = ctx.screen_rect().width();
            let initial_right_panel = self.active_right_panel();
            let initial_shell = crate::ShellLayout::for_viewport(
                viewport_width,
                self.perspective,
                self.sidebar_tab,
                initial_right_panel.is_some(),
                self.right_drawer_open,
                self.left_drawer_open,
            );
            let initial_geometry_utility_available = crate::uses_geometry_utility_dock(
                self.perspective,
                initial_right_panel,
                initial_shell.width_class,
                true,
            );
            crate::ui::draw_top_bar(
                self,
                ctx,
                initial_shell.show_sidebar,
                initial_shell.width_class == crate::ShellWidthClass::Compact,
                initial_shell.show_right_drawer || initial_geometry_utility_available,
            );
            self.sync_pending_action_with_tool();
            // Progressive disclosure wiring — filter perspective's visible groups by profile.level
            // Uses TOOLBAR_LEVEL_PRIMARY_MAX/SECONDARY_MAX constants (no magic 4/10) via helpers.
            let _visible_tool_groups = self.filtered_visible_tool_groups();
            // Keep typed variant exercised to avoid dead helper (covers PedagogicalLevel mapping)
            let _typed_visible = toolbar_groups_for_pedagogical_level(
                self.perspective.layout().visible_tool_groups,
                grafito_pedagogy::PedagogicalLevel::from_level_value(self.profile.level),
            );
            debug_assert_eq!(
                _visible_tool_groups.len(),
                _typed_visible.len(),
                "lightweight and typed filtering must agree"
            );
            let right_panel = self.active_right_panel();
            let shell = crate::ShellLayout::for_viewport(
                viewport_width,
                self.perspective,
                self.sidebar_tab,
                right_panel.is_some(),
                self.right_drawer_open,
                self.left_drawer_open,
            )
            .with_compact_left_drawer(self.compact_drawer_open, self.sidebar_tab);
            let geometry_utility_dock_available = crate::geometry_utility_dock_available(
                self.perspective,
                right_panel,
                shell.width_class,
            );
            let geometry_utility_dock = geometry_utility_dock_available && self.right_drawer_open;
            let compact_geometry_utility_dock = crate::uses_compact_geometry_utility_dock(
                self.perspective,
                shell.width_class,
                self.compact_geometry_utility_open,
            );
            if shell.show_left_drawer {
                // Clamp Aula tab cuando no está habilitada
                if !self.is_aula_tab_visible() && self.sidebar_tab == Self::AULA_TAB_INDEX {
                    self.sidebar_tab = 0;
                }
                match self.sidebar_tab {
                    0 => match self.perspective {
                        Perspective::Complex => crate::panels::draw_complex_panel(self, ctx),
                        _ => crate::algebra::draw_algebra_panel(self, ctx),
                    },
                    1 => match self.perspective {
                        Perspective::Dynamics => crate::panels::draw_attractor_panel(self, ctx),
                        _ => crate::tools_panel::draw_tools_panel(self, ctx),
                    },
                    2 => crate::panels::draw_view_panel(self, ctx),
                    3 if self.is_aula_tab_visible() => {
                        crate::classroom::draw_classroom_panel(self, ctx)
                    }
                    _ => crate::panels::draw_empty_panel(self, ctx),
                }
            }
            // Fallback rail para Aula cuando está habilitada pero el rail de ui.rs no expone el tab
            // (ownership restringido a app.rs). Dibujamos un botón flotante mínimo sobre el rail
            // si opt-in, para permitir acceso manual sin tocar ui.rs/panels.rs.
            if self.is_aula_tab_visible() {
                let aula_active = self.sidebar_tab == Self::AULA_TAB_INDEX;
                // No I/O en Ui:: salvo background: este botón solo muta estado en memoria.
                egui::Area::new(egui::Id::new("aula_rail_fallback"))
                    .anchor(
                        egui::Align2::LEFT_TOP,
                        egui::vec2(
                            grafito_ui::tokens::SPACE_XS,
                            grafito_ui::tokens::TOP_BAR_HEIGHT * 4.0
                                + grafito_ui::tokens::SPACE_XL
                                + grafito_ui::tokens::SPACE_XS,
                        ),
                    )
                    .show(ctx, |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("Aula")
                                .size(grafito_ui::tokens::TYPE_XS)
                                .strong(),
                        )
                        .selected(aula_active)
                        .rounding(grafito_ui::tokens::RADIUS_SM);
                        if ui
                            .add_sized(
                                egui::vec2(
                                    grafito_ui::tokens::RAIL_WIDTH - grafito_ui::tokens::SPACE_SM,
                                    grafito_ui::tokens::SPACE_XL + grafito_ui::tokens::SPACE_XS,
                                ),
                                btn,
                            )
                            .clicked()
                        {
                            self.sidebar_tab = Self::AULA_TAB_INDEX;
                            self.left_drawer_open = true;
                            self.compact_drawer_open = true;
                        }
                    });
            }

            // La barra «Entrada…» inferior se quitó del layout: los comandos
            // matemáticos se cargan por la sección algebraica.
            crate::ui::draw_bottom_bar(self, ctx, false);

            // Los drawers laterales reservan toda la altura antes del teclado.
            // Así el teclado queda limitado a la columna central y no recorta
            // el transcript ni el compositor del asistente.
            let keyboard_layout = crate::keyboard::math_keyboard_layout(
                self.keyboard_visible,
                self.keyboard_expanded,
                ctx.screen_rect().height(),
            );
            let keyboard_height = keyboard_layout.height();
            // Sincroniza nombre de usuario para header Mora personalizado
            self.assistant.user_name = self.profile.display_name().to_owned();
            if geometry_utility_dock_available {
                self.sync_assistant_for_frame(ctx);
                if geometry_utility_dock {
                    crate::ui::draw_geometry_utility_dock(self, ctx);
                }
            } else if compact_geometry_utility_dock {
                self.sync_assistant_for_frame(ctx);
                crate::ui::draw_compact_geometry_utility_dock(self, ctx);
            } else {
                self.draw_assistant(ctx, keyboard_height);
                // Configuración unificada: una sola ventana (Configuración)
                self.show_mascot_config = false;
            }

            use crate::RightPanelContent;
            if shell.show_right_drawer && !geometry_utility_dock_available {
                match right_panel {
                    None => {} // sin panel derecho
                    Some(RightPanelContent::ConstructionProtocol) => {
                        crate::panels::draw_construction_protocol(self, ctx);
                    }
                    Some(RightPanelContent::Regression) => {
                        crate::panels::draw_right_regression_panel(self, ctx);
                    }
                    Some(RightPanelContent::Properties) => {
                        crate::panels::draw_right_properties_panel(self, ctx);
                    }
                    Some(RightPanelContent::DomainColoring) => {
                        crate::panels::draw_right_domain_coloring_panel(self, ctx);
                    }
                    Some(RightPanelContent::Parameters) => {
                        crate::panels::draw_right_parameters_panel(self, ctx);
                    }
                    Some(RightPanelContent::TrigAnimation) => {
                        crate::panels::draw_trig_animation_panel(self, ctx);
                    }
                }
            }

            if keyboard_layout != crate::keyboard::MathKeyboardLayout::Hidden {
                crate::keyboard::draw_math_keyboard(self, ctx, keyboard_layout);
            }
        }

        // Central canvas: 2D or 3D view.
        // ─── 6. CENTRAL CANVAS ───────────────────────────────────────────────
        match self.current_view {
            ViewMode::D2 => {
                self.camera.aspect = 1.6;
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(theme.canvas_bg))
                    .show(ctx, |ui| {
                        if self.exam_mode {
                            egui::TopBottomPanel::top("exam_banner")
                                .show_separator_line(false)
                                .frame(egui::Frame::none().fill(theme.danger).inner_margin(8.0))
                                .show_inside(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("MODO EXAMEN ACTIVO")
                                                .color(theme.toast_text)
                                                .size(18.0)
                                                .strong(),
                                        );
                                    });
                                });
                        }

                        let canvas_rect = ui.available_rect_before_wrap();
                        let canvas_size = canvas_rect.size();
                        self.canvas_origin = Some(canvas_rect.min);
                        if self.sync_canvas_screen_size(canvas_size) {
                            ctx.request_repaint();
                        }
                        let canvas_resize_preview = canvas_resize_preview_active(
                            self.last_canvas_resize_at,
                            Instant::now(),
                        );
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope!("input");
                            self.handle_canvas_input(ui, canvas_rect);
                        }

                        // Compact canvas controls — top-right corner, dentro del canvas
                        // Scandinavian: botón quiet con hairline 10%, RADIUS 4, sobre Order::Foreground
                        let zf_rect = egui::Rect::from_min_size(
                            egui::pos2(canvas_rect.right() - 44.0, canvas_rect.top() + 8.0),
                            egui::vec2(38.0, 28.0),
                        );
                        // Usar ui.put (Button widget) en lugar de painter+interact manual:
                        // asegura hit-test correcto por encima de handle_canvas_input (click_and_drag del canvas)
                        // y feedback hover/pressed visible. [] junto sin espacio + wrap Extend evita
                        // que '[' y ']' se apilen verticalmente en 34px.
                        let zf_btn = egui::Button::new(
                            egui::RichText::new("[]")
                                .size(12.0)
                                .color(theme.text_primary),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .fill(theme.toolbar_bg)
                        .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                        .rounding(4.0);
                        let zf_resp = ui.put(zf_rect, zf_btn);
                        // Hover text accesible y tooltip Scandinavian
                        let zf_resp = zf_resp.on_hover_text("Ajustar Vista — centrar en (0,0)");
                        zf_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Ajustar vista",
                            )
                        });
                        if zf_resp.clicked() {
                            self.zoom_to_fit();
                            ui.ctx().request_repaint();
                        }

                        let scene_plan = crate::canvas::plan_2d_scene(
                            self.use_gpu,
                            self.gpu_renderer_ready(),
                            self.gpu_scene_2d_readiness(),
                            mapping_animating,
                            self.is_view_changing,
                            canvas_size,
                        );
                        let gpu_base =
                            matches!(scene_plan.base_renderer, crate::canvas::BaseRenderer2D::Gpu);
                        let mut painter = ui.painter().clone();
                        painter.set_clip_rect(canvas_rect);
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(canvas_resize_preview, "resize_cpu_grid_2d");
                            self.draw_grid(&painter, canvas_rect);
                            self.draw_axes(&painter, canvas_rect, !canvas_resize_preview);
                        }
                        let transient_revision = self.transient_render_state.revision();
                        let homotopy_time = self.transient_render_state.homotopy_time();
                        let callback_document = scene_plan
                            .schedule_gpu_prepare
                            .then(|| self.document_for_callback());
                        if scene_plan.schedule_gpu_prepare && !gpu_base {
                            if let Some(document) = callback_document.as_ref() {
                                let callback = egui_wgpu::Callback::new_paint_callback(
                                    canvas_rect,
                                    crate::canvas::CanvasCallback {
                                        document: document.clone(),
                                        dark_mode: self.dark_mode,
                                        transient_revision,
                                        homotopy_time,
                                        paint_base: scene_plan.callback_paints_base,
                                        paint_object: None,
                                    },
                                );
                                painter.add(egui::epaint::Shape::Callback(callback));
                            }
                        }

                        let callback_painter = painter.clone();
                        let callback_document_for_objects = callback_document.clone();
                        let dark_mode = self.dark_mode;
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(
                                canvas_resize_preview,
                                "resize_cpu_objects_2d"
                            );
                            self.draw_objects(&painter, canvas_rect, gpu_base, move |object_id| {
                                let Some(document) = &callback_document_for_objects else {
                                    return;
                                };
                                let callback = egui_wgpu::Callback::new_paint_callback(
                                    canvas_rect,
                                    crate::canvas::CanvasCallback {
                                        document: document.clone(),
                                        dark_mode,
                                        transient_revision,
                                        homotopy_time,
                                        paint_base: true,
                                        paint_object: Some(object_id),
                                    },
                                );
                                callback_painter.add(egui::epaint::Shape::Callback(callback));
                            });
                        }
                        if scene_plan.schedule_gpu_prepare && !gpu_base {
                            ctx.request_repaint();
                        }

                        // Tool ghost and preview are transient overlays, render with CPU on top.
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(
                                canvas_resize_preview,
                                "resize_cpu_overlay_2d"
                            );
                            self.draw_trig_canvas_overlay(&painter, canvas_rect);
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
                        }
                    });
            }
            ViewMode::D3 => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas_rect = ui.available_rect_before_wrap();
                    self.canvas_origin = Some(canvas_rect.min);
                    let canvas_size = canvas_rect.size();
                    if self.sync_canvas_screen_size(canvas_size) {
                        ctx.request_repaint();
                    }
                    let w = canvas_size.x;
                    let h = canvas_size.y;
                    if w > 0.0 && h > 0.0 {
                        self.camera.aspect = w / h;
                    }
                    let canvas_resize_preview =
                        canvas_resize_preview_active(self.last_canvas_resize_at, Instant::now());
                    // Input runs before automatic motion so a manual orbit, pan, or zoom
                    // pauses the scene before this frame can advance it again.
                    let input_typed_four_d_phase =
                        typed_four_d_motion_phase(self.transient_render_state.four_d_phase());
                    {
                        #[cfg(feature = "profile")]
                        puffin::profile_scope!("input");
                        self.handle_canvas_3d_input(ui, canvas_rect, input_typed_four_d_phase);
                    }
                    let automatic_motion_active = self.advance_multidimensional_motion(dt);
                    if automatic_motion_active {
                        // F17: redundante con el scheduler unificado (animating),
                        // se mantiene explícito pero coalescido vía presupuesto.
                        self.request_repaint_budget(MULTIDIMENSIONAL_MOTION_REPAINT_INTERVAL);
                    }
                    let typed_four_d_phase =
                        typed_four_d_motion_phase(self.transient_render_state.four_d_phase());
                    // The GPU WorldMesh has only persisted 4D angles. A paused nonzero
                    // transient phase must therefore remain CPU-owned to freeze in place.
                    let retained_typed_four_d_phase = !automatic_motion_active
                        && self.has_visible_typed_four_d_projection()
                        && typed_four_d_phase.is_some_and(|phase| phase != 0.0);
                    let scene_readiness = self.gpu_scene_3d_readiness(w, h);
                    let gpu_requested = should_prepare_gpu_3d(
                        self.use_gpu,
                        self.gpu_renderer_ready(),
                        self.is_view_changing,
                        automatic_motion_active,
                        retained_typed_four_d_phase,
                        scene_readiness,
                    );
                    let gpu_scene_ready = gpu_requested
                        && scene_readiness == crate::canvas::Scene3DReadiness::GpuReady;
                    let motion_preview = automatic_motion_active
                        || self.is_view_changing
                        || self.document.render_quality == RenderQuality::Preview;
                    if gpu_requested {
                        // Draw 3D grid BEFORE the GPU callback
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(canvas_resize_preview, "resize_cpu_grid_3d");
                            self.draw_3d_grid(
                                ui.painter(),
                                canvas_rect,
                                w,
                                h,
                                false,
                                canvas_resize_preview,
                            );
                        }

                        let callback = egui_wgpu::Callback::new_paint_callback(
                            canvas_rect,
                            crate::canvas::Canvas3DCallback {
                                document: self.document_for_callback(),
                                camera: self.camera,
                                dark_mode: self.dark_mode,
                                screen_w: w,
                                screen_h: h,
                                paint_scene: gpu_scene_ready,
                            },
                        );
                        ui.painter().add(egui::epaint::Shape::Callback(callback));

                        // The CPU renderer remains authoritative until this callback has
                        // rendered a valid target for the active camera and scene.
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(
                                canvas_resize_preview,
                                "resize_cpu_objects_3d"
                            );
                            self.draw_3d_objects(
                                ui.painter(),
                                canvas_rect,
                                w,
                                h,
                                crate::render_3d::Cpu3dRenderOptions {
                                    overlay_only: gpu_scene_ready,
                                    motion_preview,
                                    typed_four_d_phase,
                                },
                            );
                        }
                        if should_repaint_3d_warmup(scene_readiness) {
                            ctx.request_repaint();
                        }
                    } else {
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(canvas_resize_preview, "resize_cpu_grid_3d");
                            self.draw_3d_grid(
                                ui.painter(),
                                canvas_rect,
                                w,
                                h,
                                false,
                                canvas_resize_preview,
                            );
                        }
                        {
                            #[cfg(feature = "profile")]
                            puffin::profile_scope_if!(
                                canvas_resize_preview,
                                "resize_cpu_objects_3d"
                            );
                            self.draw_3d_objects(
                                ui.painter(),
                                canvas_rect,
                                w,
                                h,
                                crate::render_3d::Cpu3dRenderOptions {
                                    overlay_only: false,
                                    motion_preview,
                                    typed_four_d_phase,
                                },
                            );
                        }
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
                                if self.splash_logo.is_none() {
                                    if let Ok(img) = image::load_from_memory(include_bytes!(
                                        "../../../assets/grafito-icon-256x256.png"
                                    )) {
                                        let rgba = img.to_rgba8();
                                        let (w, h) = (rgba.width() as f32, rgba.height() as f32);
                                        splash_logo_texture(
                                            ctx,
                                            &mut self.splash_logo,
                                            egui::ColorImage::from_rgba_unmultiplied(
                                                [w as usize, h as usize],
                                                rgba.as_raw(),
                                            ),
                                        );
                                    }
                                }
                                if let Some(tex) = &self.splash_logo {
                                    let size = logo_rect.width().min(logo_rect.height());
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
                                egui::RichText::new("Geometría interactiva - Algebra - Calculo")
                                    .size(13.0)
                                    .color(egui::Color32::from_white_alpha((150.0 * alpha) as u8)),
                            );
                        });
                    });
                // Fix busy-loop: antes `request_repaint()` sin delay saturaba CPU/GPU a 100%
                // Ahora 16ms ≈ 60fps (vs 0ms busy-loop) — ver app.rs:4421
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                self.splash_start = None;
                if self.splash_logo.is_some() {
                    ctx.forget_image("splash_logo");
                    self.splash_logo = None;
                }
            }
        }

        // Paleta de comandos (Ctrl+K): ventana flotante de búsqueda rápida.
        if let Some(name) = self
            .command_palette
            .show_localized(ctx, self.config_locale())
        {
            self.apply_palette_command(&name, ctx);
        }

        // Modal "Acerca de Grafito": muestra versión y resumen de los cambios
        // de la release 1.1.4 en español. Se abre desde Ayuda > Acerca de.
        if self.show_about {
            self.draw_about_window(ctx);
        }
        // Onboarding 30s — gating `onboarding_completed` (utils.rs:46-48) con Window 420px
        if self.show_onboarding {
            self.draw_onboarding_window(ctx);
        }
        // Configuración — ventana única (legado show_mascot_config delega a assistant.settings_open)
        if self.show_mascot_config {
            self.assistant.settings_open = true;
            self.assistant.config_tab = 1;
            self.show_mascot_config = false;
        }
        // Enseñanza paso a paso — burbujas morph + pizarra + manim 3b1b
        {
            let now = std::time::Instant::now();
            self.teaching_ui.tick(now);
            crate::teaching_ui::draw_teaching_overlay(
                &mut self.teaching_ui,
                ctx,
                &mut self.repaint_budget,
            );
        }

        // File actions and decisions run only after every editor has consumed
        // this frame's text/IME events.
        crate::ui::draw_unsaved_changes_dialog(self, ctx);
        self.process_deferred_file_action(ctx);

        // Toast anchor unificado a TOP_LEFT — `ToastManager::draw` (toast.rs) ya posiciona
        // en `screen_rect.min + (12, 56)` (TOP_LEFT). Se elimina RIGHT_BOTTOM para evitar
        // desalineación y se usa LEFT_TOP. Decisión: TOP_LEFT gana porque deja el centro libre
        // para el canvas y no tapa el drawer derecho (ver ui.rs vs toast.rs).
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::LEFT_TOP, egui::Vec2::new(12.0, 12.0))
            .show(ctx, |ui| {
                let time = ui.ctx().input(|i| i.time);
                self.toasts.draw(ui, time);
            });

        // F17: aplicar el presupuesto de repintado coalescido de los widgets
        // (orb, pulso, media, teaching, whiteboard). El mínimo del frame gana;
        // egui lo coalesce con el pedido del scheduler unificado de arriba.
        self.apply_repaint_budget(ctx);
    }
}

// ── F1-1 Importador `.ggb` mínimo (std-only, espejo de `grafito-ggb`) ────────
// Cablea el import existente sin añadir dependencia (no se toca `Cargo.toml`):
// presupuestos espejo `GGB_MAX_BYTES` 64MiB / `GGB_MAX_XML_BYTES` 10MiB /
// `GGB_MAX_ELEMS` 5000 / `GGB_MAX_ZIP_ENTRIES` 4096. Soporta ZIP `Stored`
// (método 0, el usado por los dorados) y rechaza `Deflated` (método 8) con
// error honesto que indica usar el crate completo — nunca fallo silencioso.
// Mapea `point` → `Point[(x, y)]` y `expression type=function` →
// `Function[...]`; el resto genera `GgbOmitted` con razón.

/// Cota de atributo espejo de `grafito-ggb::MAX_ATTR_BYTES` (8192).
const GGB_MAX_ATTR_BYTES: usize = 8192;

/// Puerta de entrada pura del import `.ggb` — espejo de
/// `grafito_ggb::import_ggb_bytes`. Sin I/O: recibe bytes ya leídos en
/// background thread.
pub(crate) fn import_ggb_bytes_local(bytes: &[u8]) -> Result<GgbImportReport, String> {
    let xml = ggb_extract_xml(bytes)?;
    ggb_parse_report(&xml)
}

fn ggb_read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let lo = *bytes.get(offset)?;
    let hi = *bytes.get(offset.checked_add(1)?)?;
    Some(u16::from_le_bytes([lo, hi]))
}

fn ggb_read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let b0 = *bytes.get(offset)?;
    let b1 = *bytes.get(offset.checked_add(1)?)?;
    let b2 = *bytes.get(offset.checked_add(2)?)?;
    let b3 = *bytes.get(offset.checked_add(3)?)?;
    Some(u32::from_le_bytes([b0, b1, b2, b3]))
}

fn ggb_contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn ggb_validate_entry_name(name: &str) -> Result<(), String> {
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(format!("entrada peligrosa '{name}': ruta absoluta"));
    }
    let first = name.as_bytes().first().copied().unwrap_or(0);
    let second = name.as_bytes().get(1).copied().unwrap_or(0);
    if first.is_ascii_alphabetic() && second == b':' {
        return Err(format!("entrada peligrosa '{name}': ruta con unidad"));
    }
    let normalized = name.replace('\\', "/");
    let mut depth: usize = 0;
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    format!("entrada peligrosa '{name}': ruta fuera del archivo (..)")
                })?;
            }
            _ => {
                depth = depth.saturating_add(1);
            }
        }
    }
    Ok(())
}

/// Extrae `geogebra.xml` de un contenedor ZIP `Stored` con presupuestos espejo.
/// Rechaza `Deflated` con mensaje honesto (requiere crate completo).
fn ggb_extract_xml(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() {
        return Err("archivo .ggb vacío".to_string());
    }
    let total = bytes.len() as u64;
    if total > GGB_MAX_BYTES {
        return Err(format!(
            "archivo .ggb demasiado grande: {total} B (límite {GGB_MAX_BYTES} B)"
        ));
    }
    let mut offset: usize = 0;
    let mut entries: usize = 0;
    let mut found: Option<Vec<u8>> = None;
    while let Some(header_end) = offset.checked_add(30) {
        if header_end > bytes.len() {
            break;
        }
        let Some(sig) = ggb_read_u32_le(bytes, offset) else {
            break;
        };
        if sig == 0x0605_4b50 || sig == 0x0201_4b50 {
            break;
        }
        if sig != 0x0403_4b50 {
            if offset == 0 {
                return Err("ZIP inválido: firma local no encontrada".to_string());
            }
            break;
        }
        let Some(flags_off) = offset.checked_add(6) else {
            return Err("ZIP offset desbordado (flags)".to_string());
        };
        let Some(method_off) = offset.checked_add(8) else {
            return Err("ZIP offset desbordado (método)".to_string());
        };
        let Some(comp_off) = offset.checked_add(18) else {
            return Err("ZIP offset desbordado (tamaño)".to_string());
        };
        let Some(uncomp_off) = offset.checked_add(22) else {
            return Err("ZIP offset desbordado (tamaño)".to_string());
        };
        let Some(name_len_off) = offset.checked_add(26) else {
            return Err("ZIP offset desbordado (nombre)".to_string());
        };
        let Some(extra_len_off) = offset.checked_add(28) else {
            return Err("ZIP offset desbordado (extra)".to_string());
        };
        let (
            Some(flags),
            Some(method),
            Some(comp_size),
            Some(uncomp_size),
            Some(name_len),
            Some(extra_len),
        ) = (
            ggb_read_u16_le(bytes, flags_off),
            ggb_read_u16_le(bytes, method_off),
            ggb_read_u32_le(bytes, comp_off),
            ggb_read_u32_le(bytes, uncomp_off),
            ggb_read_u16_le(bytes, name_len_off),
            ggb_read_u16_le(bytes, extra_len_off),
        )
        else {
            return Err("ZIP truncado en cabecera local".to_string());
        };
        let name_len_usize = usize::from(name_len);
        let extra_len_usize = usize::from(extra_len);
        let comp_usize =
            usize::try_from(comp_size).map_err(|e| format!("tamaño comprimido inválido: {e}"))?;
        let Some(name_start) = offset.checked_add(30) else {
            return Err("ZIP offset desbordado (nombre)".to_string());
        };
        let Some(name_end) = name_start.checked_add(name_len_usize) else {
            return Err("ZIP nombre desbordado".to_string());
        };
        let Some(extra_end) = name_end.checked_add(extra_len_usize) else {
            return Err("ZIP extra desbordado".to_string());
        };
        let Some(data_end) = extra_end.checked_add(comp_usize) else {
            return Err("ZIP datos desbordados".to_string());
        };
        if data_end > bytes.len() {
            return Err("ZIP truncado: datos fuera de rango".to_string());
        }
        let Some(name_bytes) = bytes.get(name_start..name_end) else {
            return Err("ZIP nombre fuera de rango".to_string());
        };
        let name =
            std::str::from_utf8(name_bytes).map_err(|e| format!("ZIP nombre no UTF-8: {e}"))?;
        ggb_validate_entry_name(name)?;
        if flags & 0x0001 != 0 {
            return Err(format!("entrada '{name}': cifrada no soportada"));
        }
        if flags & 0x0008 != 0 {
            return Err(format!(
                "entrada '{name}': descriptor de datos no soportado en import mínimo"
            ));
        }
        if method != 0 && method != 8 {
            return Err(format!(
                "entrada '{name}': método {method} no soportado (solo Stored/Deflated)"
            ));
        }
        if name == GGB_XML_NAME && found.is_none() {
            if method == 8 {
                return Err(
                    "geogebra.xml con compresión deflate no soportada en import mínimo \
                     (usa el crate grafito-ggb completo) — archivo no importado"
                        .to_string(),
                );
            }
            if u64::from(uncomp_size) > GGB_MAX_XML_BYTES {
                return Err(format!(
                    "geogebra.xml demasiado grande: {} B (límite {GGB_MAX_XML_BYTES} B)",
                    uncomp_size
                ));
            }
            let Some(data) = bytes.get(extra_end..data_end) else {
                return Err("geogebra.xml fuera de rango".to_string());
            };
            if data.len() as u64 > GGB_MAX_XML_BYTES {
                return Err(format!(
                    "geogebra.xml demasiado grande: {} B (límite {GGB_MAX_XML_BYTES} B)",
                    data.len()
                ));
            }
            if ggb_contains(data, b"<!DOCTYPE") || ggb_contains(data, b"<!ENTITY") {
                return Err("DOCTYPE/ENTITY rechazado (bomba de entidades)".to_string());
            }
            found = Some(data.to_vec());
        }
        entries = entries
            .checked_add(1)
            .ok_or_else(|| "contador de entradas desbordado".to_string())?;
        if entries > GGB_MAX_ZIP_ENTRIES {
            return Err(format!(
                "demasiadas entradas ZIP: {entries} (límite {GGB_MAX_ZIP_ENTRIES})"
            ));
        }
        offset = data_end;
    }
    found.ok_or_else(|| "geogebra.xml faltante en .ggb".to_string())
}

fn ggb_fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn ggb_sanitize_label(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for ch in t.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '\'' {
            out.push(ch);
        } else if ch == ' ' || ch == '-' {
            out.push('_');
        }
        if out.len() >= 64 {
            break;
        }
    }
    out
}

/// Extrae `nombre="valor"` (comillas `"` o `'`) de un tag XML sin dependencias.
fn ggb_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let mut from: usize = 0;
    while let Some(rest) = tag.get(from..) {
        let Some(rel) = rest.find(&needle) else {
            return None;
        };
        let Some(eq_end) = from.checked_add(rel)?.checked_add(needle.len()) else {
            return None;
        };
        let Some(after_eq) = tag.get(eq_end..) else {
            return None;
        };
        let trimmed = after_eq.trim_start();
        let skipped = after_eq.len().saturating_sub(trimmed.len());
        let Some(val_start) = eq_end.checked_add(skipped) else {
            return None;
        };
        let Some(quote) = trimmed.chars().next() else {
            return None;
        };
        if quote != '"' && quote != '\'' {
            let Some(next_from) = val_start.checked_add(1) else {
                return None;
            };
            if next_from >= tag.len() {
                return None;
            }
            from = next_from;
            continue;
        }
        let Some(after_quote) = trimmed.get(1..) else {
            return None;
        };
        let Some(end) = after_quote.find(quote) else {
            return None;
        };
        let Some(value) = after_quote.get(..end) else {
            return None;
        };
        if value.len() > GGB_MAX_ATTR_BYTES {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

/// Parsea `geogebra.xml` a comandos Grafito con presupuestos espejo.
/// Mapea `point` y `expression type=function`; el resto → omitido honesto.
fn ggb_parse_report(xml: &[u8]) -> Result<GgbImportReport, String> {
    let text = std::str::from_utf8(xml).map_err(|e| format!("geogebra.xml no UTF-8: {e}"))?;
    if text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err("DOCTYPE/ENTITY rechazado (bomba de entidades)".to_string());
    }
    let mut report = GgbImportReport {
        mapped: Vec::new(),
        omitted: Vec::new(),
    };
    let mut count: usize = 0;
    let push_mapped =
        |report: &mut GgbImportReport, kind: String, label: String, command: String| {
            if command.len() > GGB_MAX_EXPR_CHARS {
                report.omitted.push(GgbOmitted {
                    kind,
                    label,
                    reason: format!("comando excede {GGB_MAX_EXPR_CHARS} caracteres"),
                });
                return;
            }
            if report.mapped.len() >= GGB_MAX_ELEMS {
                report.omitted.push(GgbOmitted {
                    kind,
                    label,
                    reason: format!("presupuesto MAX_ELEMS {GGB_MAX_ELEMS} excedido"),
                });
                return;
            }
            report.mapped.push(GgbMappedCommand { kind, command });
        };

    // ── <element …> ──────────────────────────────────────────────
    let mut pos: usize = 0;
    while let Some(after_pos) = text.get(pos..) {
        let Some(rel) = after_pos.find("<element") else {
            break;
        };
        let Some(start) = pos.checked_add(rel) else {
            return Err("offset desbordado al parsear <element>".to_string());
        };
        let Some(rest) = text.get(start..) else {
            break;
        };
        let Some(tag_end_rel) = rest.find('>') else {
            return Err("elemento <element> sin cierre '>'".to_string());
        };
        let Some(tag_end) = start.checked_add(tag_end_rel) else {
            return Err("offset desbordado en <element>".to_string());
        };
        let Some(opening_end) = tag_end.checked_add(1) else {
            return Err("offset desbordado tras <element>".to_string());
        };
        let Some(opening) = text.get(start..opening_end) else {
            return Err("elemento fuera de rango".to_string());
        };
        count = count
            .checked_add(1)
            .ok_or_else(|| "contador desbordado".to_string())?;
        if count > GGB_MAX_ELEMS {
            return Err(format!("límite MAX_ELEMS {GGB_MAX_ELEMS} excedido"));
        }
        let tipo_raw = ggb_attr(opening, "type").unwrap_or_default();
        let label_raw = ggb_attr(opening, "label").unwrap_or_default();
        let etiqueta = ggb_sanitize_label(&label_raw);
        let is_self_closing = opening.trim_end().ends_with("/>");
        let inner: &str = if is_self_closing {
            ""
        } else if let Some(content_start) = tag_end.checked_add(1) {
            if let Some(after) = text.get(content_start..) {
                if let Some(close_rel) = after.find("</element>") {
                    if let Some(inner_end) = content_start.checked_add(close_rel) {
                        text.get(content_start..inner_end).unwrap_or("")
                    } else {
                        return Err("offset desbordado en </element>".to_string());
                    }
                } else {
                    ""
                }
            } else {
                ""
            }
        } else {
            return Err("offset desbordado tras <element>".to_string());
        };
        let tipo_lc = tipo_raw.trim().to_ascii_lowercase();
        if tipo_lc == "point" {
            let mut mapped_done = false;
            if let Some(coords_rel) = inner.find("<coords") {
                if let Some(coords_rest) = inner.get(coords_rel..) {
                    if let Some(coords_end_rel) = coords_rest.find('>') {
                        if let Some(coords_tag) =
                            coords_rest.get(..coords_end_rel.saturating_add(1))
                        {
                            let x_opt =
                                ggb_attr(coords_tag, "x").and_then(|s| s.parse::<f64>().ok());
                            let y_opt =
                                ggb_attr(coords_tag, "y").and_then(|s| s.parse::<f64>().ok());
                            if let (Some(x), Some(y)) = (x_opt, y_opt) {
                                if x.is_finite() && y.is_finite() {
                                    let cmd =
                                        format!("Point[({}, {})]", ggb_fmt_num(x), ggb_fmt_num(y));
                                    push_mapped(
                                        &mut report,
                                        "Point".to_string(),
                                        etiqueta.clone(),
                                        cmd,
                                    );
                                    mapped_done = true;
                                } else {
                                    report.omitted.push(GgbOmitted {
                                        kind: tipo_raw.clone(),
                                        label: etiqueta.clone(),
                                        reason: "coordenadas no finitas".to_string(),
                                    });
                                    mapped_done = true;
                                }
                            }
                        }
                    }
                }
            }
            if !mapped_done {
                let reason = if inner.find("<coords").is_none() {
                    "point sin coords".to_string()
                } else {
                    "point con coords inválidas".to_string()
                };
                report.omitted.push(GgbOmitted {
                    kind: tipo_raw.clone(),
                    label: etiqueta.clone(),
                    reason,
                });
            }
        } else if tipo_lc.contains("3d")
            || tipo_lc.contains("quadric")
            || tipo_lc.contains("plane")
            || tipo_lc.contains("sphere")
        {
            report.omitted.push(GgbOmitted {
                kind: tipo_raw.clone(),
                label: etiqueta.clone(),
                reason: "3D/quadric omitido en núcleo aula F2".to_string(),
            });
        } else if tipo_lc == "conic" || tipo_lc == "conicpart" {
            report.omitted.push(GgbOmitted {
                kind: tipo_raw.clone(),
                label: etiqueta.clone(),
                reason: "cónica no soportada en import mínimo \
                         (usa crate grafito-ggb completo para F2)"
                    .to_string(),
            });
        } else if tipo_lc == "line" || tipo_lc == "segment" || tipo_lc == "ray" {
            report.omitted.push(GgbOmitted {
                kind: tipo_raw.clone(),
                label: etiqueta.clone(),
                reason: format!(
                    "{tipo_raw} elemento sin comando — omitido \
                     (usar comando {tipo_raw} explícito)"
                ),
            });
        } else if tipo_lc == "numeric" {
            report.omitted.push(GgbOmitted {
                kind: tipo_raw.clone(),
                label: etiqueta.clone(),
                reason: "numeric sin slider ni celda — pendiente mapeo variable".to_string(),
            });
        } else {
            report.omitted.push(GgbOmitted {
                kind: tipo_raw.clone(),
                label: etiqueta.clone(),
                reason: "tipo no soportado en F2 (omitido honesto)".to_string(),
            });
        }
        if is_self_closing {
            pos = opening_end;
        } else if let Some(content_start) = tag_end.checked_add(1) {
            if let Some(after) = text.get(content_start..) {
                if let Some(close_rel) = after.find("</element>") {
                    let close_len = "</element>".len();
                    if let Some(close_end) = content_start
                        .checked_add(close_rel)
                        .and_then(|v| v.checked_add(close_len))
                    {
                        pos = close_end;
                    } else {
                        return Err("offset desbordado al cerrar </element>".to_string());
                    }
                } else {
                    pos = content_start;
                }
            } else {
                break;
            }
        } else {
            return Err("offset desbordado al avanzar </element>".to_string());
        }
        if pos >= text.len() {
            break;
        }
    }

    // ── <command …> → omitido honesto (el import mínimo no resuelve refs) ──
    let mut cpos: usize = 0;
    while let Some(after_pos) = text.get(cpos..) {
        let Some(rel) = after_pos.find("<command") else {
            break;
        };
        let Some(start) = cpos.checked_add(rel) else {
            return Err("offset desbordado al parsear <command>".to_string());
        };
        let Some(rest) = text.get(start..) else {
            break;
        };
        let Some(tag_end_rel) = rest.find('>') else {
            return Err("comando <command> sin cierre '>'".to_string());
        };
        let Some(tag_end) = start.checked_add(tag_end_rel) else {
            return Err("offset desbordado en <command>".to_string());
        };
        let Some(opening_end) = tag_end.checked_add(1) else {
            return Err("offset desbordado tras <command>".to_string());
        };
        let Some(opening) = text.get(start..opening_end) else {
            return Err("comando fuera de rango".to_string());
        };
        count = count
            .checked_add(1)
            .ok_or_else(|| "contador desbordado".to_string())?;
        if count > GGB_MAX_ELEMS {
            return Err(format!("límite MAX_ELEMS {GGB_MAX_ELEMS} excedido"));
        }
        let nombre = ggb_attr(opening, "name").unwrap_or_default();
        let kind = if nombre.trim().is_empty() {
            "command".to_string()
        } else {
            nombre.clone()
        };
        report.omitted.push(GgbOmitted {
            kind,
            label: String::new(),
            reason: "comando omitido en import mínimo \
                     (usa crate grafito-ggb completo para F2)"
                .to_string(),
        });
        if let Some(content_start) = tag_end.checked_add(1) {
            if let Some(after) = text.get(content_start..) {
                if let Some(close_rel) = after.find("</command>") {
                    let close_len = "</command>".len();
                    if let Some(close_end) = content_start
                        .checked_add(close_rel)
                        .and_then(|v| v.checked_add(close_len))
                    {
                        cpos = close_end;
                    } else {
                        return Err("offset desbordado al cerrar </command>".to_string());
                    }
                } else {
                    cpos = opening_end;
                }
            } else {
                break;
            }
        } else {
            return Err("offset desbordado al avanzar </command>".to_string());
        }
        if cpos >= text.len() {
            break;
        }
    }

    // ── <expression …> → Function si es función, si no omitido honesto ──
    let mut epos: usize = 0;
    while let Some(after_pos) = text.get(epos..) {
        let Some(rel) = after_pos.find("<expression") else {
            break;
        };
        let Some(start) = epos.checked_add(rel) else {
            return Err("offset desbordado al parsear <expression>".to_string());
        };
        let Some(rest) = text.get(start..) else {
            break;
        };
        let Some(tag_end_rel) = rest.find('>') else {
            return Err("expresión <expression> sin cierre '>'".to_string());
        };
        let Some(tag_end) = start.checked_add(tag_end_rel) else {
            return Err("offset desbordado en <expression>".to_string());
        };
        let Some(opening_end) = tag_end.checked_add(1) else {
            return Err("offset desbordado tras <expression>".to_string());
        };
        let Some(opening) = text.get(start..opening_end) else {
            return Err("expresión fuera de rango".to_string());
        };
        count = count
            .checked_add(1)
            .ok_or_else(|| "contador desbordado".to_string())?;
        if count > GGB_MAX_ELEMS {
            return Err(format!("límite MAX_ELEMS {GGB_MAX_ELEMS} excedido"));
        }
        let etiqueta = ggb_sanitize_label(&ggb_attr(opening, "label").unwrap_or_default());
        let exp = ggb_attr(opening, "exp").unwrap_or_default();
        let tipo = ggb_attr(opening, "type").unwrap_or_default();
        let tipo_lc = tipo.trim().to_ascii_lowercase();
        let es_funcion = tipo_lc == "function"
            || tipo_lc == "functionnvar"
            || exp.contains("->")
            || exp.contains('(');
        if es_funcion {
            let exp_trim = exp.trim();
            if exp_trim.is_empty() {
                report.omitted.push(GgbOmitted {
                    kind: "Function".to_string(),
                    label: etiqueta.clone(),
                    reason: "expresión vacía".to_string(),
                });
            } else if exp_trim.len() > GGB_MAX_EXPR_CHARS {
                report.omitted.push(GgbOmitted {
                    kind: "Function".to_string(),
                    label: etiqueta.clone(),
                    reason: format!("expresión excede {GGB_MAX_EXPR_CHARS} caracteres"),
                });
            } else {
                let rhs = if exp_trim.contains('=') {
                    exp_trim
                        .splitn(2, '=')
                        .nth(1)
                        .unwrap_or(exp_trim)
                        .trim()
                        .to_string()
                } else {
                    exp_trim.to_string()
                };
                if rhs.is_empty() {
                    report.omitted.push(GgbOmitted {
                        kind: "Function".to_string(),
                        label: etiqueta.clone(),
                        reason: "expresión vacía".to_string(),
                    });
                } else {
                    let cmd = format!("Function[{rhs}]");
                    push_mapped(&mut report, "Function".to_string(), etiqueta.clone(), cmd);
                }
            }
        } else if !exp.trim().is_empty() {
            report.omitted.push(GgbOmitted {
                kind: format!("expression:{}", tipo.clone()),
                label: etiqueta.clone(),
                reason: "tipo de expresión no mapeado en F0/F1 (omitido honesto)".to_string(),
            });
        }
        epos = opening_end;
        if epos >= text.len() {
            break;
        }
    }

    Ok(report)
}

#[cfg(test)]
mod ggb_import_local_tests {
    use super::{import_ggb_bytes_local, GGB_XML_NAME};
    use crate::commands::process_input;

    fn crc32_ieee(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    fn zip_store(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut offsets: Vec<u32> = Vec::new();
        let mut sizes: Vec<(u32, u32)> = Vec::new();
        for (name, data) in files {
            offsets.push(u32::try_from(out.len()).unwrap_or(0));
            let crc = crc32_ieee(data);
            let size = u32::try_from(data.len()).unwrap_or(0);
            sizes.push((crc, size));
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            let name_bytes = name.as_bytes();
            out.extend_from_slice(&(u16::try_from(name_bytes.len()).unwrap_or(0)).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);
        }
        let cd_start = u32::try_from(out.len()).unwrap_or(0);
        for (idx, (name, _)) in files.iter().enumerate() {
            let (crc, size) = sizes[idx];
            let offset = offsets[idx];
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            let name_bytes = name.as_bytes();
            out.extend_from_slice(&(u16::try_from(name_bytes.len()).unwrap_or(0)).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(name_bytes);
        }
        let cd_size = u32::try_from(out.len())
            .unwrap_or(0)
            .saturating_sub(cd_start);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(u16::try_from(files.len()).unwrap_or(0)).to_le_bytes());
        out.extend_from_slice(&(u16::try_from(files.len()).unwrap_or(0)).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn ggb_with(xml: &str) -> Vec<u8> {
        zip_store(&[(GGB_XML_NAME, xml.as_bytes())])
    }

    fn xml_header() -> String {
        r#"<?xml version="1.0" encoding="utf-8"?><geogebra format="5.0"><construction>"#.to_string()
    }

    fn xml_footer() -> String {
        "</construction></geogebra>".to_string()
    }

    #[test]
    fn import_minimo_dos_puntos_mapea_y_reporta_honesto() {
        let xml = format!(
            "{}{odef}{}",
            xml_header(),
            xml_footer(),
            odef = r#"<element type="point" label="A"><coords x="1" y="2" z="1" w="1"/></element><element type="point" label="B"><coords x="3" y="4" z="1" w="1"/></element>"#
        );
        let bytes = ggb_with(&xml);
        let rep = import_ggb_bytes_local(&bytes).expect("dorado mínimo debe importar");
        assert_eq!(rep.mapped.len(), 2);
        assert!(rep.omitted.is_empty(), "omitidos {:?}", rep.omitted);
        let cmds = rep.commands();
        assert_eq!(cmds.len(), 2);
        assert!(
            cmds.iter().all(|c| c.starts_with("Point[")),
            "comandos {cmds:?}"
        );
        let summary = rep.summary();
        assert!(summary.contains("2 objetos"), "resumen {summary}");
        assert!(summary.contains("Point x2"), "resumen {summary}");
        assert_eq!(rep.omitted_detail(), "sin omitidos");
        let mut doc = grafito_core::Document::new();
        for mut c in cmds {
            let outcome = process_input(&mut doc, &mut c);
            assert!(
                !matches!(outcome, grafito_command::commands::CommandOutcome::Error(_)),
                "comando debe aplicar"
            );
        }
        assert_eq!(doc.object_count(), 2);
    }

    #[test]
    fn tipo_no_soportado_genera_omitido_honesto_no_silencioso() {
        let xml = format!(
            "{}<element type=\"point\" label=\"A\"><coords x=\"0\" y=\"0\" z=\"1\" w=\"1\"/></element><element type=\"angle\" label=\"a\"><value val=\"45\"/></element>{}",
            xml_header(),
            xml_footer()
        );
        let bytes = ggb_with(&xml);
        let rep = import_ggb_bytes_local(&bytes).expect("debe importar con omitido");
        assert_eq!(rep.mapped.len(), 1);
        assert!(!rep.omitted.is_empty(), "esperaba omitido honesto");
        assert!(rep.omitted.iter().any(|o| o.kind == "angle"));
        let detail = rep.omitted_detail();
        assert!(detail.contains("omitidos"), "detalle {detail}");
        assert!(
            rep.summary().contains("1 objetos"),
            "resumen {}",
            rep.summary()
        );
    }

    #[test]
    fn bytes_vacios_y_sin_xml_fallan_honesto() {
        let err_vacio = import_ggb_bytes_local(&[]).unwrap_err();
        assert!(!err_vacio.is_empty(), "error vacío no debe ser silencioso");
        let sin_xml = zip_store(&[("otro.txt", b"hola")]);
        let err_faltante = import_ggb_bytes_local(&sin_xml).unwrap_err();
        assert!(
            err_faltante.contains("geogebra.xml"),
            "error {err_faltante}"
        );
    }

    #[test]
    fn deflate_rechazado_honesto_no_silencioso() {
        let xml = format!(
            "{}<element type=\"point\" label=\"A\"><coords x=\"0\" y=\"0\" z=\"1\" w=\"1\"/></element>{}",
            xml_header(),
            xml_footer()
        );
        let mut bytes = ggb_with(&xml);
        // Parchea método Stored(0) → Deflated(8) en la primera cabecera local (offset 8).
        if let Some(slot) = bytes.get_mut(8..10) {
            slot.copy_from_slice(&8u16.to_le_bytes());
        }
        let err = import_ggb_bytes_local(&bytes).unwrap_err();
        assert!(err.contains("deflate"), "debe mencionar deflate, got {err}");
    }
}

impl GrafitoApp {
    /// Ventana onboarding 30s Scandinavian — gating `AppConfig::onboarding_completed` (utils.rs:46-48).
    /// 420px, 3 bullets progressive disclosure (5/8/17 grupos), botones [Probar ejemplo][Empezar vacío][No mostrar].
    /// Si no se alcanza UI completa, al menos Window stub con “No mostrar” que setea `onboarding_completed=true`.
    pub(crate) fn draw_onboarding_window(&mut self, ctx: &egui::Context) {
        if !self.show_onboarding {
            return;
        }
        let theme = grafito_ui::theme::current_theme(ctx);
        let mut open = self.show_onboarding;
        egui::Window::new("Bienvenido a Grafito")
            .id(egui::Id::new("onboarding_window"))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.panel_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                    .rounding(grafito_ui::tokens::RADIUS_LG)
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Grafito — pizarra geométrica interactiva")
                            .size(16.0)
                            .strong()
                            .color(theme.text_primary),
                    );
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("• Construye con 5 herramientas esenciales — Mover, Punto, Recta, Círculo, Polígono")
                        .size(grafito_ui::tokens::TYPE_XS)
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new("• Secundaria añade 3 más — Lápiz, Medida, Análisis (8 total)")
                        .size(grafito_ui::tokens::TYPE_XS)
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new("• Universidad desbloquea 17 grupos — Cónicas, 3D, CAS, Estadística, Complejos, Dinámica…")
                        .size(grafito_ui::tokens::TYPE_XS)
                        .color(theme.text_primary),
                );
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let total_w = 3.0 * 120.0 + 2.0 * 8.0;
                    let pad = ((ui.available_width() - total_w) / 2.0).max(0.0);
                    ui.add_space(pad);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_sized(
                            egui::vec2(120.0, 32.0),
                            egui::Button::new(egui::RichText::new("Probar ejemplo").size(12.0))
                                .rounding(grafito_ui::tokens::RADIUS_MD)
                                .fill(theme.accent)
                                .stroke(egui::Stroke::NONE),
                        )
                        .clicked()
                    {
                        let _ = self.load_perspective_examples(self.perspective);
                        self.show_onboarding = false;
                        let mut cfg = load_config();
                        cfg.onboarding_completed = true;
                        save_config(&cfg);
                        self.notify("Ejemplo cargado — ¡explora Grafito!", grafito_ui::toast::ToastKind::Success);
                    }
                    if ui
                        .add_sized(
                            egui::vec2(120.0, 32.0),
                            egui::Button::new(egui::RichText::new("Empezar vacío").size(12.0))
                                .rounding(grafito_ui::tokens::RADIUS_MD)
                                .fill(theme.panel_bg)
                                .stroke(egui::Stroke::new(1.0, theme.separator)),
                        )
                        .clicked()
                    {
                        self.show_onboarding = false;
                    }
                    if ui
                        .add_sized(
                            egui::vec2(120.0, 32.0),
                            egui::Button::new(
                                egui::RichText::new("No mostrar")
                                    .size(12.0)
                                    .color(theme.text_secondary),
                            )
                            .rounding(grafito_ui::tokens::RADIUS_MD)
                            .fill(theme.panel_bg)
                            .stroke(egui::Stroke::new(1.0, theme.separator)),
                        )
                        .clicked()
                    {
                        self.show_onboarding = false;
                        let mut cfg = load_config();
                        cfg.onboarding_completed = true;
                        save_config(&cfg);
                    }
                });
                ui.add_space(4.0);
            });
        if !open {
            self.show_onboarding = false;
        }
    }

    /// Ventana "Acerca de Grafito" — resumida, Scandinavian quiet.
    fn draw_about_window(&mut self, ctx: &egui::Context) {
        let theme = grafito_ui::theme::current_theme(ctx);
        egui::Window::new("Acerca de Grafito")
            .id(egui::Id::new("about_window"))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.toolbar_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Grafito")
                            .size(28.0)
                            .strong()
                            .color(theme.accent),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(format!("Versión {}", env!("CARGO_PKG_VERSION")))
                            .size(13.0)
                            .color(theme.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(format!("Idioma: {}", self.locale.code()))
                            .size(11.0)
                            .color(theme.text_tertiary),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(
                            "Calculadora gráfica interactiva para geometría, álgebra, \
                             cálculo, CAS y estadística. Rápida, precisa y simple.",
                        )
                        .size(12.0)
                        .color(theme.text_primary),
                    );
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Creado por Lautaro Agustin Diez")
                            .size(12.0)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new("HECHO EN ARGENTINA")
                            .size(11.0)
                            .color(theme.text_secondary),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Licencia GPL-3.0-or-later · Código abierto")
                            .size(11.0)
                            .color(theme.text_tertiary),
                    );
                });
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Cerrar").size(13.0))
                                .min_size(egui::vec2(96.0, 28.0)),
                        )
                        .clicked()
                    {
                        self.show_about = false;
                    }
                });
            });
    }

    #[allow(dead_code)] // TODO: eliminar legado Pou window (compat, no usado en prod)
    pub(crate) fn draw_pou_window(&mut self, _ctx: &egui::Context) {}
    pub(crate) fn draw_mascot_config_window(&mut self, _ctx: &egui::Context) {
        // Ventana legada unificada: ahora todo vive en Configuración (assistant.settings_open).
        // Se mantiene para compatibilidad pero delega a la ventana única.
        if self.show_mascot_config {
            self.assistant.settings_open = true;
            self.assistant.config_tab = 1;
            self.avatar_draft = self.profile.avatar.clone();
            self.assistant.avatar = self.profile.avatar.clone();
            self.assistant.user_name = self.profile.display_name().to_owned();
            self.show_mascot_config = false;
        }
    }
    #[allow(dead_code)] // TODO: remover delegación legacy unified_config (compat, cubierto por assistant.settings_open)
    pub(crate) fn draw_unified_config_window(&mut self, ctx: &egui::Context) {
        self.draw_mascot_config_window(ctx);
    }
}

/// Resumen histórico de cambios — conservado para referencia, no mostrado en UI resumida.
#[allow(dead_code)]
fn build_about_changelog() -> &'static [&'static str] {
    &[
        "Mapeos conformes algebraicos de primera clase (1/z, z^n, exp, log, sin, cos, Joukowski, Möbius, etc.).",
        "13 nuevos mapeos algebraicos con detección automática desde la expresión.",
        "Operadores de relación (Lt, Gt, Le, Ge, Eq, Ne) en el AST con precedencia y render.",
        "Módulo conformal/ separado: complex_expr, algebraic_mappings, domain_coloring.",
        "Suite de 70+ iconos vectoriales estilo macOS/iOS, idénticos en Windows, macOS y Linux.",
        "Design tokens semánticos (Theme): escalas tipográficas, spacing y radios.",
        "Splash screen al inicio con logo, nombre y versión.",
        "Empty state y hover overlay en el panel de Álgebra.",
        "Render de ComplexMapping con fórmula cerrada para expresiones reconocidas.",
        "Filtro de segmentos degenerados en marching squares (sin relleno espurio).",
        "Sidebar con iconos vectoriales coherentes con la toolbar.",
        "Corrección de lexer: 1/z, log(z), z^2 ya no se confunden con multiplicación implícita.",
        "Fill de ImplicitCurve con scanline real y stride adaptativo, sin pixelado ni lag.",
        "AST cacheado en ImplicitCurveObj: 60+ FPS con expresiones complejas.",
        "Cache del AST separa lhs y rhs correctamente.",
        "Soporte para superscripts Unicode (x², y², z², t², θ², φ², x³, y³, z³) sin panics UTF-8.",
        "Operadores <, >, <=, >= grafican correctamente (fill, contorno, exterior).",
        "Paleta unificada: paneles, algebra, tools y keyboard usan el mismo Theme.",
        "Validación de BesselJ/Y/I (orden saturado, NaN->0) y Sec/Csc/Cot en singularidad.",
        "Color clamping RGBA a [0, 255] en to_color32, algebra, SVG y ghost rendering.",
        "Reemplazo de unwrap() críticos por ?/ok_or en algebra, app, snap, dispatcher y commands.",
        "Hit-test devuelve el objeto más cercano en solapamientos.",
        "Restricciones numéricas con Jacobiano finito en configuraciones degeneradas.",
        "ODE valida steps=0 y dimensiones inconsistentes sin panic.",
        "Geometría robusta: safe_sample, cardioid, epicycloid, compute_fractal sin división por cero.",
        "Estadística ignora NaN/Inf en histogramas.",
        "Script aborta con error claro en recursión profunda; expand_all_cas limita a 50 iteraciones.",
        "Plot/Integral usan replace_variable de límite de palabra (exp(t) no se rompe).",
        "Toasts para errores de save_state/load_state.",
        "Sistema de 10 perspectivas GeoGebra (Geometría, Álgebra, Cálculo, Estadística, Complejos, Dinámica, Datos, Examen).",
        "Tool ghost universal con marcas de eje rojas/azules para interceptos.",
        "Herramientas de medición: Area, Circumference, Center, Length, Slope.",
        "Construcciones geométricas: Sector, Arc, Polygon booleans.",
        "Cálculo diferencial/integral: TangentAt, NormalAt, ArcLength, CurvatureAt, VolumeOfRevolution, SurfaceOfRevolution.",
        "Snap a intersecciones (Línea-Línea, Línea-Círculo, Círculo-Círculo, Función-Línea).",
        "Reorganización de la toolbar en 12 grupos lógicos con iconos vectoriales.",
        "Protocolo de Construcción estilo GeoGebra (reordenar, deshabilitar, exportar LaTeX).",
        "Command Palette (Ctrl+K) con búsqueda fuzzy y export SVG/PNG/TikZ.",
        "GPU compute shader para fill de regiones implícitas (máscara RGBA8).",
        "Cómputo GPU unificado: function, implicit, parametric, vector, fill.",
        "WGSL bytecode interpreter de 50 opcodes con protección de pila.",
        "Caché de relleno de curva implícita estable al hacer pan/zoom.",
        "Fase E: dead code removal (algebra_view, properties_panel, keyboard.rs antiguo).",
        "Hysteresis de calidad de render: Preview durante pan/zoom, High tras 150ms idle.",
        "9 tests nuevos para la cache de relleno.",
        "Export a SVG, PNG y TikZ desde el menú y la paleta de comandos.",
    ]
}

fn splash_logo_texture<'a>(
    ctx: &egui::Context,
    splash_logo: &'a mut Option<egui::TextureHandle>,
    image: egui::ColorImage,
) -> &'a egui::TextureHandle {
    splash_logo
        .get_or_insert_with(|| ctx.load_texture("splash_logo", image, egui::TextureOptions::LINEAR))
}

pub(crate) fn mora_avatar_texture<'a>(
    ctx: &egui::Context,
    mora_texture: &'a mut Option<egui::TextureHandle>,
    image: egui::ColorImage,
) -> &'a egui::TextureHandle {
    mora_texture
        .get_or_insert_with(|| ctx.load_texture("mora_avatar", image, egui::TextureOptions::LINEAR))
}

pub(crate) fn load_mora_avatar_texture_once(
    ctx: &egui::Context,
    mora_texture: &mut Option<egui::TextureHandle>,
    load_attempted: &mut bool,
    bytes: &[u8],
) {
    if *load_attempted {
        return;
    }
    *load_attempted = true;
    let Ok(image) = image::load_from_memory(bytes) else {
        return;
    };
    let rgba = image.to_rgba8();
    mora_avatar_texture(
        ctx,
        mora_texture,
        egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ),
    );
}

/// Run the native Grafito desktop application.
pub fn run_app() -> Result<(), eframe::Error> {
    env_logger::init();

    #[cfg(feature = "profile")]
    let mut profile = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Grafito v{}", env!("CARGO_PKG_VERSION"));
                println!("Calculadora gráfica matemática interactiva");
                println!("(Geometría, Álgebra, Cálculo, CAS, Estadística, Complejos)");
                println!();
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
            .with_min_inner_size([960.0, 600.0])
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

#[cfg(test)]
mod splash_tests {
    use super::*;

    #[test]
    fn splash_texture_handle_is_retained_and_reused() {
        let ctx = egui::Context::default();
        let image = egui::ColorImage::new([1, 1], egui::Color32::WHITE);
        let mut splash_logo = None;

        let first = splash_logo_texture(&ctx, &mut splash_logo, image.clone()).id();
        let second = splash_logo_texture(&ctx, &mut splash_logo, image).id();

        assert_eq!(first, second);
        assert!(splash_logo.is_some());
    }

    #[test]
    fn mora_texture_handle_is_retained_and_reused() {
        let ctx = egui::Context::default();
        let image = egui::ColorImage::new([1, 1], egui::Color32::WHITE);
        let mut mora_texture = None;

        let first = mora_avatar_texture(&ctx, &mut mora_texture, image.clone()).id();
        let second = mora_avatar_texture(&ctx, &mut mora_texture, image).id();

        assert_eq!(first, second);
        assert!(mora_texture.is_some());
    }

    #[test]
    fn failed_mora_texture_load_is_not_retried() {
        let ctx = egui::Context::default();
        let mut mora_texture = None;
        let mut load_attempted = false;

        load_mora_avatar_texture_once(&ctx, &mut mora_texture, &mut load_attempted, b"not a PNG");
        assert!(load_attempted);
        assert!(mora_texture.is_none());

        load_mora_avatar_texture_once(
            &ctx,
            &mut mora_texture,
            &mut load_attempted,
            include_bytes!("../../../assets/mora.png"),
        );
        assert!(mora_texture.is_none());
    }

    #[test]
    fn embedded_mora_asset_is_a_small_rgba_png() {
        let image = image::load_from_memory(include_bytes!("../../../assets/mora.png"))
            .expect("Mora asset must be a decodable PNG")
            .to_rgba8();

        assert_eq!(image.dimensions(), (128, 128));
        assert!(include_bytes!("../../../assets/mora.png").len() < 32_000);
    }
}

#[cfg(test)]
mod canvas_resize_preview_tests {
    use super::*;

    #[test]
    fn canvas_resize_preview_ends_after_the_settle_boundary() {
        let now = Instant::now();

        assert!(!canvas_resize_preview_active(None, now));
        assert!(canvas_resize_preview_active(Some(now), now));
        assert!(canvas_resize_preview_active(
            Some(now - Duration::from_millis(150)),
            now
        ));
        assert!(!canvas_resize_preview_active(
            Some(now - Duration::from_millis(151)),
            now
        ));
    }

    #[test]
    fn two_dimensional_resize_preview_hides_only_axis_numeric_decoration() {
        let app_source = include_str!("app.rs");
        let two_dimensional_start = app_source
            .find("ViewMode::D2 =>")
            .expect("2D canvas branch");
        let two_dimensional_end = app_source[two_dimensional_start..]
            .find("ViewMode::D3 =>")
            .map(|offset| two_dimensional_start + offset)
            .expect("3D canvas branch follows 2D");
        let two_dimensional = &app_source[two_dimensional_start..two_dimensional_end];
        assert!(two_dimensional.contains("let canvas_resize_preview ="));
        let grid_draw = two_dimensional
            .find("self.draw_grid(&painter, canvas_rect);")
            .expect("grid remains visible during resize preview");
        let axes_draw = two_dimensional
            .find("self.draw_axes(&painter, canvas_rect, !canvas_resize_preview);")
            .expect("2D axes receive the resize-preview policy");
        assert!(grid_draw < axes_draw);

        let render_source = include_str!("render_2d.rs");
        let axes_start = render_source
            .find("pub(crate) fn draw_axes")
            .expect("2D axis renderer");
        let axes_end = render_source[axes_start..]
            .find("pub(crate) fn draw_trig_canvas_overlay")
            .map(|offset| axes_start + offset)
            .expect("next 2D renderer method");
        let axes = &render_source[axes_start..axes_end];
        assert!(axes.contains("show_numeric_ticks: bool"));
        let x_axis = axes.find("let x_axis_a").expect("X axis is painted");
        let y_axis = axes.find("let y_axis_a").expect("Y axis is painted");
        let numeric_guard = axes
            .find("if !show_numeric_ticks {")
            .expect("numeric decoration guard");
        let numeric_ticks = axes
            .find("// Tick marks and labels")
            .expect("numeric decoration follows the guard");
        assert!(x_axis < numeric_guard);
        assert!(y_axis < numeric_guard);
        assert!(axes[x_axis..y_axis].contains("painter.line_segment("));
        assert!(axes[y_axis..numeric_guard].contains("painter.line_segment("));
        assert!(numeric_guard < numeric_ticks);
        assert!(axes[numeric_guard..numeric_ticks].contains("return;"));
    }

    #[test]
    fn construction_log_caps_at_500_keeping_chronological_order() {
        let mut app = dummy_grafito_app();
        for i in 0..MAX_CONSTRUCTION_LOG + 1 {
            app.record_construction_step(&format!("act{i}"), vec![], &format!("out{i}"));
        }
        assert_eq!(app.construction_log.len(), MAX_CONSTRUCTION_LOG);
        // El más antiguo (act0) fue drenado; el primero ahora es act1
        assert_eq!(app.construction_log[0].action, "act1");
        assert_eq!(
            app.construction_log[MAX_CONSTRUCTION_LOG - 1].action,
            format!("act{}", MAX_CONSTRUCTION_LOG)
        );
        // Agregar otro mantiene cota y orden
        app.record_construction_step("final", vec![], "F");
        assert_eq!(app.construction_log.len(), MAX_CONSTRUCTION_LOG);
        assert_eq!(app.construction_log[0].action, "act2");
        assert_eq!(
            app.construction_log[MAX_CONSTRUCTION_LOG - 1].action,
            "final"
        );
    }

    #[test]
    fn construction_log_constant_is_500() {
        assert_eq!(MAX_CONSTRUCTION_LOG, 500);
    }
}

#[cfg(test)]
pub(crate) fn dummy_grafito_app() -> GrafitoApp {
    dummy_grafito_app_with_perspective(Perspective::Geometry2D)
}

#[cfg(test)]
pub(crate) fn dummy_grafito_app_with_perspective(perspective: Perspective) -> GrafitoApp {
    let document = initial_document();
    let current_view = perspective.view_mode();
    let camera = grafito_geometry::Camera3D::new(1280.0 / 720.0);
    let snapshot_version = document.version;
    let snapshot_render_quality = document.render_quality;
    let document_snapshot = std::sync::Arc::new(document.clone());
    GrafitoApp {
        document,
        current_tool: grafito_ui::Tool::default(),
        previous_tool: grafito_ui::Tool::default(),
        current_view,
        perspective,
        camera,
        show_grid: true,
        snap_to_grid: true,
        snap_config: crate::snap::SnapConfig::default(),
        exam_mode: false,
        dark_mode: false,
        pending_points: Vec::new(),
        pending_points_3d: Vec::new(),
        last_mouse_pos: None,
        canvas_origin: None,
        canvas_drag_start: None,
        canvas_is_panning: false,
        point_drag_has_mutated: false,
        eraser_stroke_has_mutated: false,
        select_drag_object: None,
        point_drag_error_reported: false,
        selected_object: None,
        preview_object: None,
        input_text: String::new(),
        command_input_focus_requested: false,
        cas_result: String::new(),
        keyboard_tab: 0,
        keyboard_visible: false,
        keyboard_expanded: false,
        table_func_idx: 0,
        table_x_min: "-5".to_string(),
        table_x_max: "5".to_string(),
        table_step: "1.0".to_string(),
        cas_history: VecDeque::new(),
        sidebar_tab: 0,
        recent_files: VecDeque::new(),
        document_lifecycle: DocumentLifecycle::new(&document_snapshot),
        deferred_file_actions: DeferredFileActions::default(),
        splash_start: None,
        splash_logo: None,
        mora_texture: None,
        mora_texture_load_attempted: false,
        plugin_registry: None,
        plugins_loaded: false,
        assistant_blocks_cache: grafito_ui::assistant::AssistantBlocksCache::default(),
        whiteboard_open: false,
        whiteboard: crate::whiteboard_ui::WhiteboardSession::default(),
        whiteboard_book: crate::whiteboard_ui::WhiteboardBook::default(),
        whiteboard_left_pinned: false,
        show_whiteboard_assistant: true,
        profile: grafito_profile::StudentProfile::default(),
        undo_stack: VecDeque::new(),
        redo_stack: VecDeque::new(),
        undo_total_bytes: 0,
        show_onboarding: false,
        pending_save_job: None,
        pending_open_job: None,
        pending_export_job: None,
        pending_ggb_import_job: None,
        attractor_cache: std::collections::HashMap::new(),
        fill_textures: std::sync::RwLock::new(crate::render_2d::FillTextureCacheStore::default()),
        active_color_picker: None,
        color_favorites: [
            grafito_geometry::Color::new(0.9, 0.1, 0.1, 1.0),
            grafito_geometry::Color::new(0.1, 0.6, 0.1, 1.0),
            grafito_geometry::Color::new(0.1, 0.3, 0.9, 1.0),
            grafito_geometry::Color::new(0.9, 0.6, 0.1, 1.0),
            grafito_geometry::Color::new(0.5, 0.1, 0.9, 1.0),
        ],
        tool_ghost: None,
        tool_state: crate::tool_dispatcher::ToolState::default(),
        gpu_renderer: None,
        gpu_scene_readiness: None,
        transient_render_state: TransientRenderState::default(),
        multidimensional_motion_enabled: true,
        multidimensional_motion_speed: DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED,
        use_gpu: false,
        last_interaction_time: Instant::now(),
        is_view_changing: false,
        last_canvas_resize_at: None,
        pending_action: PendingAction::None,
        toasts: grafito_ui::toast::ToastManager::default(),
        hovered_analysis: None,
        hover_candidate_pos: None,
        hover_candidate_time: 0.0,
        hover_cached_analysis: None,
        document_snapshot,
        snapshot_version,
        snapshot_render_quality,
        command_palette: grafito_ui::command_palette::CommandPaletteState::default(),
        assistant: grafito_ui::assistant::AssistantPanelState::default(),
        assistant_runtime: crate::assistant::AssistantRuntime::default(),
        right_drawer_open: true,
        workspace_dock_tab: crate::WorkspaceDockTab::Inspector,
        compact_geometry_utility_open: false,
        left_drawer_open: true,
        compact_drawer_open: false,
        assistant_visible: true,
        ui_time: 0.0,
        construction_log: Vec::new(),
        show_construction_protocol: DEFAULT_CONSTRUCTION_PROTOCOL_VISIBLE,
        statistics_data: Vec::new(),
        statistics_input_buf: String::new(),
        statistics_input_error: None,
        autocomplete: InputAutocomplete::default(),
        show_about: false,
        show_trig_animation: false,
        trig_angle: 0.0,
        trig_animating: false,
        trig_speed: 0.5,
        trig_function: 0,
        trig_view_mode: TrigViewMode::Didactic,
        trig_graph_cache: std::sync::RwLock::new(None),
        show_mascot_config: false,
        avatar_draft: grafito_profile::AvatarConfig::default(),
        config_name_error: None,
        teaching_ui: crate::teaching_ui::TeachingUiState::default(),
        repaint_budget: RepaintBudget::default(),
        autosave: AutosaveDebouncer::new(),
        autosave_last_version: 0,
        advanced_red_opt_in: false,
        locale: AppLocale::Es,
        classroom: crate::classroom::ClassroomPanel::new(),
    }
}
