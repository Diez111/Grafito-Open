//! Ciclo de vida del documento: comandos de archivo, estado sucio, diálogos de guardado.
//!
//! Extraído de `app.rs` para reducir el god file y aislar la lógica de persistencia
//! transaccional. El módulo es puro (no depende de `egui`) salvo por
//! `file_shortcut` que mapea teclas a comandos.

use egui::Key;
use grafito_core::Document;
use std::path::{Path, PathBuf};

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
    pub(crate) fn prompt_message(self, current_path: Option<&Path>) -> String {
        let file = current_path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "Sin título".to_string());
        match self {
            Self::New => {
                format!("¿Guardar cambios en \"{file}\" antes de crear un documento nuevo?")
            }
            Self::Open => format!("¿Guardar cambios en \"{file}\" antes de abrir otro documento?"),
            Self::Exit => format!("¿Guardar cambios en \"{file}\" antes de salir?"),
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
pub(crate) enum SaveAttempt {
    /// Guardado sin diálogo previo que terminó en este turno (reservado para
    /// futuros paths sincrónicos; el flujo normal es `Pending` + poll).
    #[allow(dead_code)]
    Saved(Option<DocumentAction>),
    /// Guardado delegado a worker: el resultado llega en `poll_background_jobs`,
    /// que continúa la acción pendiente vía `record_save_success`.
    Pending,
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
    pub(crate) fn approve_close(&mut self) {
        self.close_approved = true;
    }
    pub(crate) const fn close_is_approved(&self) -> bool {
        self.close_approved
    }
}

pub(crate) fn semantic_document_baseline(document: &Document) -> Result<serde_json::Value, String> {
    let mut baseline = serde_json::to_value(document).map_err(|error| error.to_string())?;
    if let Some(view) = baseline
        .get_mut("view")
        .and_then(serde_json::Value::as_object_mut)
    {
        // Canvas dimensions follow the native viewport and are not authored content.
        // Se ignora para dirty-check y comparación semántica.
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

pub(crate) fn write_document_to_path(document: &Document, path: &Path) -> Result<(), String> {
    if let Some(path) = path.to_str() {
        crate::export::save_document(document, path).map_err(|error| error.to_string())
    } else {
        grafito_core::write_document_atomic(document, path).map_err(|error| error.to_string())
    }
}

/// Compara dos documentos ignorando `view.screen_size` (viewport transitorio).
/// Usa `semantic_document_baseline` para que dirty-check y `documents_semantically_differ`
/// compartan la misma definición de "cambio semántico".
pub(crate) fn documents_semantically_differ(before: &Document, after: &Document) -> bool {
    // Fast path: versión y cantidad de objetos son O(1). Sólo cae a JSON si
    // screen_size podría ser el único cambio (que ignoramos).
    if std::ptr::eq(before, after) {
        return false;
    }
    if before.version == after.version
        && before.objects_iter().count() == after.objects_iter().count()
        && before.view().screen_size == after.view().screen_size
    {
        return false;
    }
    match (
        semantic_document_baseline(before),
        semantic_document_baseline(after),
    ) {
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
