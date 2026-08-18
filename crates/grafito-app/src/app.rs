//! Main application state and eframe orchestration.
//!
//! Holds `GrafitoApp`, its constructor, file/undo helpers, and the top-level
//! `eframe::App::update` loop that dispatches rendering to focused UI modules.

use crate::utils::{load_config, save_config, AppConfig};
use crate::{Perspective, ViewMode};
use egui::{Key, Pos2};
use grafito_core::{
    ChangeSet, CircleObj, Cube3DObj, Document, EllipseObj, FunctionObj, GeoObject, HyperbolaObj,
    LineObj, ObjectId, ParabolaObj, PointObj, RenderQuality, Sphere3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, ViewTransform};
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::Tool;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use grafito_command::commands::{register_gpu_function_evaluator, GpuFunctionEvaluator};

const MAX_UNDO: usize = 50;
const TRIG_GRAPH_LABEL: &str = "TrigGraph";
const TRIG_VALUE_LABEL: &str = "TrigValue";
const VIEW_SETTLE_DURATION: Duration = Duration::from_millis(150);
const MULTIDIMENSIONAL_MOTION_REPAINT_INTERVAL: Duration = Duration::from_millis(33);
pub(crate) const DEFAULT_3D_ORBIT_RADIANS_PER_SECOND: f32 = 0.3;
pub(crate) const DEFAULT_4D_ROTATION_RADIANS_PER_SECOND: f64 = 0.55;
pub(crate) const MIN_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 0.25;
pub(crate) const DEFAULT_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 1.0;
pub(crate) const MAX_MULTIDIMENSIONAL_MOTION_SPEED: f32 = 2.0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileCommand {
    New,
    Open,
    Save,
    SaveAs,
    Exit,
}

pub(crate) const fn file_shortcut(key: Key, ctrl: bool, shift: bool) -> Option<FileCommand> {
    if !ctrl {
        return None;
    }
    match (key, shift) {
        (Key::N, false) => Some(FileCommand::New),
        (Key::O, false) => Some(FileCommand::Open),
        (Key::S, false) => Some(FileCommand::Save),
        (Key::S, true) => Some(FileCommand::SaveAs),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveMode {
    Save,
    SaveAs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentAction {
    New,
    Open,
    Exit,
}

impl DocumentAction {
    pub(crate) const fn prompt_message(self) -> &'static str {
        match self {
            Self::New => {
                "Hay cambios sin guardar. ¿Querés guardarlos antes de crear un documento nuevo?"
            }
            Self::Open => {
                "Hay cambios sin guardar. ¿Querés guardarlos antes de abrir otro documento?"
            }
            Self::Exit => "Hay cambios sin guardar. ¿Querés guardarlos antes de salir?",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentActionRequest {
    Proceed(DocumentAction),
    AwaitDecision(DocumentAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsavedDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeferredFileIntent {
    Command(FileCommand),
    Decision(UnsavedDecision),
}

#[derive(Debug, Default)]
pub(crate) struct DeferredFileActions {
    pending: Option<DeferredFileIntent>,
}

impl DeferredFileActions {
    pub(crate) fn queue_command(&mut self, command: FileCommand) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.pending = Some(DeferredFileIntent::Command(command));
        true
    }

    pub(crate) fn queue_decision(&mut self, decision: UnsavedDecision) -> bool {
        if matches!(self.pending, Some(DeferredFileIntent::Decision(_))) {
            return false;
        }
        self.pending = Some(DeferredFileIntent::Decision(decision));
        true
    }

    pub(crate) fn intercept_native_close(&mut self, close_approved: bool) -> bool {
        if close_approved {
            return false;
        }
        let _ = self.queue_command(FileCommand::Exit);
        true
    }

    #[cfg(test)]
    pub(crate) const fn pending(&self) -> Option<DeferredFileIntent> {
        self.pending
    }

    pub(crate) fn take_after_editors(&mut self) -> Option<DeferredFileIntent> {
        self.pending.take()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsavedResolution {
    Save(DocumentAction),
    Proceed(DocumentAction),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveAttempt {
    Saved(Option<DocumentAction>),
    Cancelled,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct DocumentLifecycle {
    current_path: Option<PathBuf>,
    saved_baseline: Result<serde_json::Value, String>,
    pending_action: Option<DocumentAction>,
    save_error: Option<String>,
    close_approved: bool,
}

impl DocumentLifecycle {
    pub(crate) fn new(document: &Document) -> Self {
        Self {
            current_path: None,
            saved_baseline: semantic_document_baseline(document),
            pending_action: None,
            save_error: None,
            close_approved: false,
        }
    }

    pub(crate) fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub(crate) fn current_save_path(&self, mode: SaveMode) -> Option<&Path> {
        match mode {
            SaveMode::Save => self.current_path(),
            SaveMode::SaveAs => None,
        }
    }

    pub(crate) fn is_dirty(&self, document: &Document, has_document_bound_drafts: bool) -> bool {
        if has_document_bound_drafts {
            return true;
        }
        match (&self.saved_baseline, semantic_document_baseline(document)) {
            (Ok(saved), Ok(current)) => saved != &current,
            _ => true,
        }
    }

    pub(crate) fn request_action(
        &mut self,
        action: DocumentAction,
        document: &Document,
        has_document_bound_drafts: bool,
    ) -> DocumentActionRequest {
        self.close_approved = false;
        if self.pending_action != Some(action) {
            self.save_error = None;
        }
        if self.is_dirty(document, has_document_bound_drafts) {
            self.pending_action = Some(action);
            DocumentActionRequest::AwaitDecision(action)
        } else {
            self.pending_action = None;
            DocumentActionRequest::Proceed(action)
        }
    }

    pub(crate) const fn pending_action(&self) -> Option<DocumentAction> {
        self.pending_action
    }

    pub(crate) fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    pub(crate) fn resolve_unsaved_decision(
        &mut self,
        decision: UnsavedDecision,
    ) -> Option<UnsavedResolution> {
        let action = self.pending_action?;
        self.save_error = None;
        match decision {
            UnsavedDecision::Save => Some(UnsavedResolution::Save(action)),
            UnsavedDecision::Discard => {
                self.pending_action = None;
                Some(UnsavedResolution::Proceed(action))
            }
            UnsavedDecision::Cancel => {
                self.pending_action = None;
                self.close_approved = false;
                Some(UnsavedResolution::Cancelled)
            }
        }
    }

    pub(crate) fn record_save_success(
        &mut self,
        path: PathBuf,
        document: &Document,
    ) -> Option<DocumentAction> {
        self.current_path = Some(path);
        self.saved_baseline = semantic_document_baseline(document);
        self.save_error = None;
        self.pending_action.take()
    }

    pub(crate) fn record_save_failure(&mut self, error: impl Into<String>) {
        self.close_approved = false;
        self.save_error = Some(error.into());
    }

    pub(crate) fn establish_opened_document(&mut self, path: PathBuf, document: &Document) {
        self.current_path = Some(path);
        self.saved_baseline = semantic_document_baseline(document);
        self.pending_action = None;
        self.save_error = None;
        self.close_approved = false;
    }

    pub(crate) fn establish_new_document(&mut self, document: &Document) {
        self.current_path = None;
        self.saved_baseline = semantic_document_baseline(document);
        self.pending_action = None;
        self.save_error = None;
        self.close_approved = false;
    }

    fn approve_close(&mut self) {
        self.close_approved = true;
    }

    const fn close_is_approved(&self) -> bool {
        self.close_approved
    }
}

fn semantic_document_baseline(document: &Document) -> Result<serde_json::Value, String> {
    let mut baseline = serde_json::to_value(document).map_err(|error| error.to_string())?;
    if let Some(view) = baseline
        .get_mut("view")
        .and_then(serde_json::Value::as_object_mut)
    {
        // Canvas dimensions follow the native viewport and are not authored content.
        view.remove("screen_size");
    }
    Ok(baseline)
}

pub(crate) fn load_document_candidate(path: &Path) -> Result<Document, String> {
    if let Some(path) = path.to_str() {
        crate::export::load_document(path).map_err(|error| error.to_string())
    } else {
        grafito_core::read_document_file(path).map_err(|error| error.to_string())
    }
}

fn write_document_to_path(document: &Document, path: &Path) -> Result<(), String> {
    if let Some(path) = path.to_str() {
        crate::export::save_document(document, path).map_err(|error| error.to_string())
    } else {
        grafito_core::write_document_atomic(document, path).map_err(|error| error.to_string())
    }
}

pub(crate) fn documents_semantically_differ(before: &Document, after: &Document) -> bool {
    match (serde_json::to_value(before), serde_json::to_value(after)) {
        (Ok(before), Ok(after)) => before != after,
        _ => false,
    }
}

pub(crate) fn command_mutated_document(
    outcome: &grafito_command::commands::CommandOutcome,
    before: &Document,
    after: &Document,
) -> bool {
    !matches!(outcome, grafito_command::commands::CommandOutcome::Error(_))
        && documents_semantically_differ(before, after)
}

/// Adds and solves a numeric constraint on detached state before replacing the
/// live document. UI callers can therefore defer undo history and feedback
/// until the complete constraint operation succeeds.
pub(crate) fn try_stage_numeric_constraint<F>(
    document: &mut Document,
    add_constraint: F,
) -> Result<(), String>
where
    F: FnOnce(&mut Document) -> Result<(), String>,
{
    let mut staged = document.detached_clone_for_staging();
    add_constraint(&mut staged)?;
    staged.try_re_evaluate_constraints(&[])?;
    *document = staged;
    Ok(())
}

pub(crate) fn apply_command_outcome(
    outcome: &grafito_command::commands::CommandOutcome,
    cas_result: &mut String,
    cas_history: &mut Vec<String>,
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
                cas_history.remove(0);
            }
            cas_history.push(format!("> {}\n  {}", input_was, feedback));
            toasts.push(
                wrap_toast_message(feedback, 52),
                grafito_ui::toast::ToastKind::Info,
                time,
            );
        }
        grafito_command::commands::CommandOutcome::Error(message) => {
            *cas_result = message.clone();
            if cas_history.len() > 20 {
                cas_history.remove(0);
            }
            cas_history.push(format!("> {}\n  Error: {}", input_was, message));
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

fn push_history_snapshot(
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
    snapshot: Document,
) {
    undo_stack.push(snapshot);
    redo_stack.clear();
    if undo_stack.len() > MAX_UNDO {
        undo_stack.remove(0);
    }
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
        undo_stack: &mut Vec<Document>,
        redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    undo_stack: &mut Vec<Document>,
    redo_stack: &mut Vec<ChangeSet>,
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
    pub cas_history: Vec<String>,
    pub sidebar_tab: usize,
    pub recent_files: Vec<String>,
    document_lifecycle: DocumentLifecycle,
    deferred_file_actions: DeferredFileActions,
    /// Timestamp de inicio de la app (splash screen). None = ya pasó.
    pub splash_start: Option<Instant>,
    /// Textura retenida mientras el splash la referencia en sus primitivas egui.
    splash_logo: Option<egui::TextureHandle>,
    /// Avatar local de Mora, cargado una vez cuando el asistente se vuelve visible.
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
    pub undo_stack: Vec<Document>,
    pub redo_stack: Vec<ChangeSet>,
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
}

pub(crate) const DEFAULT_KEYBOARD_VISIBLE: bool = true;
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
            cas_history: Vec::new(),
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
            recent_files: Vec::new(),
            document_lifecycle,
            deferred_file_actions: DeferredFileActions::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
        }
    }

    /// Persiste preferencias de interfaz; las claves del asistente viven sólo en el llavero.
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
            enabled_plugins: existing.enabled_plugins,
            disabled_plugins: existing.disabled_plugins,
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

    pub(crate) fn save_snapshot(&mut self, snapshot: Document) {
        push_history_snapshot(&mut self.undo_stack, &mut self.redo_stack, snapshot);
    }

    /// Guarda un único estado previo sólo cuando una interacción de panel
    /// alteró el contenido persistible del documento. Esto evita que los
    /// repaints y los `bump_version` internos generen entradas de undo.
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
                    let _ = self.save_document(SaveMode::Save);
                }
            }
            FileCommand::SaveAs => {
                if let Some(action) = self.pending_document_action() {
                    let _ = self
                        .document_lifecycle
                        .resolve_unsaved_decision(UnsavedDecision::Save);
                    self.save_before_document_action(SaveMode::SaveAs, action, ctx);
                } else {
                    let _ = self.save_document(SaveMode::SaveAs);
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
        if let SaveAttempt::Saved(saved_action) = self.save_document(mode) {
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
            DocumentAction::Open => self.choose_and_open_document(),
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

    /// Registra un paso en el protocolo de construcción.
    pub(crate) fn record_construction_step(
        &mut self,
        action: &str,
        inputs: Vec<String>,
        output: &str,
    ) {
        let n = self.construction_log.len() + 1;
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

    pub(crate) fn export_with_dialog(&mut self, format: crate::export::ExportFormat) {
        let path = rfd::FileDialog::new()
            .add_filter(format.display_name(), &[format.extension()])
            .set_file_name(format!("grafito_export.{}", format.extension()))
            .save_file();
        let Some(path) = path else {
            return;
        };

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
                self.export_with_dialog(crate::export::ExportFormat::Svg);
                return;
            }
            "Export PNG" => {
                self.export_with_dialog(crate::export::ExportFormat::Png);
                return;
            }
            "Export TikZ" => {
                self.export_with_dialog(crate::export::ExportFormat::Tikz);
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
        if let Some(before) = self.undo_stack.pop() {
            let changes = ChangeSet {
                before,
                after: self.document.clone(),
            };
            match changes.undo(&mut self.document) {
                Ok(()) => {
                    self.redo_stack.push(changes);
                    self.selected_object = None;
                }
                Err(error) => {
                    self.undo_stack.push(changes.before);
                    self.cas_result = format!("No se pudo deshacer: {error}");
                }
            }
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(changes) = self.redo_stack.pop() {
            let before_redo = self.document.clone();
            match changes.redo(&mut self.document) {
                Ok(()) => {
                    self.undo_stack.push(before_redo);
                    self.selected_object = None;
                }
                Err(error) => {
                    self.redo_stack.clear();
                    self.cas_result = format!("No se pudo rehacer: {error}");
                }
            }
        }
    }

    fn save_document(&mut self, mode: SaveMode) -> SaveAttempt {
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

        let result = write_document_to_path(&self.document, &path);
        match result {
            Ok(_) => {
                let pending_action = self
                    .document_lifecycle
                    .record_save_success(path.clone(), &self.document);
                self.remember_recent_file(&path);
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

    fn choose_and_open_document(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Grafito Document", &["json"])
            .pick_file()
        else {
            return;
        };

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

    fn remember_recent_file(&mut self, path: &Path) {
        let path = path.to_string_lossy().into_owned();
        self.recent_files.retain(|recent| recent != &path);
        if self.recent_files.len() >= 10 {
            self.recent_files.remove(0);
        }
        self.recent_files.push(path);
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

    /// Cambia la perspectiva activa y sincroniza `current_view`, la herramienta
    /// por defecto y los paneles. La perspectiva es sólo una vista de trabajo:
    /// nunca debe borrar ni reemplazar el documento del usuario.
    pub(crate) fn set_perspective(&mut self, p: Perspective) {
        if self.perspective == p {
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
        let layout = p.layout();
        let target_view = p.view_mode();
        self.current_view = target_view;
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
        puffin::profile_scope!("app_update");

        if self.whiteboard_open {
            crate::whiteboard_ui::draw_whiteboard_overlay(self, ctx);
            return;
        }

        self.handle_native_close_request(ctx);

        if self.is_view_changing {
            if self.last_interaction_time.elapsed() > VIEW_SETTLE_DURATION {
                self.is_view_changing = false;
                self.document.render_quality = RenderQuality::High;
            } else {
                // Seguir repintando hasta que se cumpla el plazo de hysteresis
                // para que la promoción a High dispare aunque no haya más input.
                ctx.request_repaint();
            }
        }

        let (dt, ui_time) = ctx.input(|i| (i.stable_dt.min(0.1) as f64, i.time));
        self.ui_time = ui_time;

        // En modo explorador trigonométrico, saltar las animaciones de
        // variables del documento para evitar recomputes de fondo.
        if !self.show_trig_animation && self.document.advance_variable_animations(dt) {
            self.document.render_quality = RenderQuality::Preview;
            self.is_view_changing = true;
            self.last_interaction_time = Instant::now();
            ctx.request_repaint();
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
            ctx.request_repaint();
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
            ctx.request_repaint();
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
                match self.sidebar_tab {
                    0 => match self.perspective {
                        Perspective::Complex => crate::panels::draw_complex_panel(self, ctx),
                        _ => crate::algebra::draw_algebra_panel(self, ctx),
                    },
                    1 => match self.perspective {
                        Perspective::Dynamics => crate::panels::draw_attractor_panel(self, ctx),
                        _ => crate::tools_panel::draw_tools_panel(self, ctx),
                    },
                    2 => crate::panels::draw_cas_panel(self, ctx),
                    3 => match self.perspective {
                        Perspective::Probability
                        | Perspective::Statistics
                        | Perspective::DataAnalysis => {
                            crate::panels::draw_statistics_panel(self, ctx)
                        }
                        _ => crate::panels::draw_empty_panel(self, ctx),
                    },
                    4 => crate::panels::draw_view_panel(self, ctx),
                    _ => crate::panels::draw_empty_panel(self, ctx),
                }
            }

            crate::ui::draw_bottom_bar(self, ctx, shell.show_bottom_input);

            // Los drawers laterales reservan toda la altura antes del teclado.
            // Así el teclado queda limitado a la columna central y no recorta
            // el transcript ni el compositor del asistente.
            let keyboard_layout = crate::keyboard::math_keyboard_layout(
                self.keyboard_visible,
                self.keyboard_expanded,
                ctx.screen_rect().height(),
            );
            let keyboard_height = keyboard_layout.height();
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
                            theme.toolbar_bg,
                            egui::Stroke::new(1.0, theme.separator),
                        );
                        painter.text(
                            zf_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "[ ]",
                            egui::FontId::proportional(16.0),
                            theme.text_primary,
                        );
                        let zoom_fit =
                            ui.interact(zf_rect, ui.id().with("zf"), egui::Sense::click());
                        zoom_fit.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Ajustar vista",
                            )
                        });
                        if zoom_fit.on_hover_text("Ajustar Vista").clicked() {
                            self.zoom_to_fit();
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
                            let callback = egui_wgpu::Callback::new_paint_callback(
                                canvas_rect,
                                crate::canvas::CanvasCallback {
                                    document: callback_document
                                        .as_ref()
                                        .expect("scheduled callback has a document")
                                        .clone(),
                                    dark_mode: self.dark_mode,
                                    transient_revision,
                                    homotopy_time,
                                    paint_base: scene_plan.callback_paints_base,
                                    paint_object: None,
                                },
                            );
                            painter.add(egui::epaint::Shape::Callback(callback));
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
                        ctx.request_repaint_after(MULTIDIMENSIONAL_MOTION_REPAINT_INTERVAL);
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
                ctx.request_repaint();
            } else {
                self.splash_start = None;
                self.splash_logo = None;
            }
        }

        // Paleta de comandos (Ctrl+K): ventana flotante de búsqueda rápida.
        if let Some(name) = self.command_palette.show(ctx) {
            self.apply_palette_command(&name, ctx);
        }

        // Modal "Acerca de Grafito": muestra versión y resumen de los cambios
        // de la release 1.1.4 en español. Se abre desde Ayuda > Acerca de.
        if self.show_about {
            self.draw_about_window(ctx);
        }

        // File actions and decisions run only after every editor has consumed
        // this frame's text/IME events.
        crate::ui::draw_unsaved_changes_dialog(self, ctx);
        self.process_deferred_file_action(ctx);

        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-12.0, -12.0))
            .show(ctx, |ui| {
                let time = ui.ctx().input(|i| i.time);
                self.toasts.draw(ui, time);
            });
    }
}

impl GrafitoApp {
    /// Dibuja la ventana modal "Acerca de Grafito": versión, licencia, autor y
    /// un resumen en español de los cambios principales de la release actual.
    fn draw_about_window(&mut self, ctx: &egui::Context) {
        let theme = grafito_ui::theme::current_theme(ctx);
        egui::Window::new("Acerca de Grafito")
            .id(egui::Id::new("about_window"))
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.toolbar_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator))
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
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Geometría interactiva - Algebra - Calculo - CAS")
                            .size(12.0)
                            .color(theme.text_tertiary),
                    );
                });
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("¿Qué es Grafito?")
                                .size(14.0)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(
                                "Grafito es una calculadora gráfica matemática \
                                 moderna y de alto rendimiento escrita en Rust. \
                                 Permite graficar funciones en 2D y 3D, curvas \
                                 paramétricas e implícitas, resolver EDOs y \
                                 sistemas, hacer análisis simbólico (raíces, \
                                 extremos, integrales, tangentes, curvatura) y \
                                 trabajar con mapeos complejos, estadística, \
                                 probabilidad y mucho más.",
                            )
                            .size(12.0)
                            .color(theme.text_primary),
                        );

                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("Cambios principales de esta versión")
                                .size(14.0)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(4.0);

                        let cambios = build_about_changelog();
                        for linea in cambios {
                            ui.label(
                                egui::RichText::new(format!("- {}", linea))
                                    .size(12.0)
                                    .color(theme.text_primary),
                            );
                        }

                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("Información")
                                .size(14.0)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(
                                "Licencia: GPL-3.0-or-later. Código abierto. \
                                 Hecho con Rust + egui + wgpu.",
                            )
                            .size(12.0)
                            .color(theme.text_secondary),
                        );
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
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
}

/// Resumen en español de los cambios de la release 1.1.4 para la ventana
/// "Acerca de Grafito". Se mantiene en un helper para que sea fácil de
/// actualizar cuando se libera una nueva versión.
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
}
