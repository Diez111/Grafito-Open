//! Controladores delegados para descomponer `GrafitoApp` (God Object split).
//!
//! `GrafitoApp` (~75 campos, `update` ~820 L) se descompone incrementalmente en
//! controladores especializados. Este módulo ya no es stub: expone lógica real
//! con tests y sin `#[allow(dead_code)]`. `GrafitoApp` conserva sus campos
//! directos para compatibilidad pero la nueva lógica de historial usa
//! `DocumentController::push_snapshot` / `undo` / `redo` con `Document::estimated_bytes()`
//! y contador `undo_total_bytes` O(1).
//!
//! Estructura objetivo:
//! - `DocumentController` — document, undo/redo, lifecycle, snapshots (O(1) pop_front + presupuesto 50 MiB).
//! - `ViewController` — camera, view, render quality, canvas state (fuente canónica `Perspective`).
//! - `AssistantController` — assistant state, visibilidad.

use grafito_core::{ChangeSet, Document};
use std::collections::VecDeque;

/// Máximo de entradas de undo — `VecDeque::pop_front` O(1) (corrige `Vec::remove(0)` O(n)).
pub const MAX_UNDO: usize = 50;
/// Presupuesto global de memoria para undo: 50 MiB. Ver `Document::estimated_bytes()`.
pub const MAX_UNDO_BYTES: usize = 50 * 1024 * 1024;

// ── DocumentController ───────────────────────────────────────────────────────

/// Gestiona el documento y su historial (undo/redo) con presupuesto acotado.
///
/// - `undo_stack` es `VecDeque<Document>` con `pop_front` O(1) (no `Vec` O(n) shift).
/// - `undo_total_bytes` es contador running `Σ estimated_bytes` — evita recalcular
///   `iter().map(estimated_bytes).sum()` O(n) en cada `push_snapshot` (n≤50 bounded,
///   pero el contador es O(1) por push/pop y documenta la evolución desde `app.rs`).
/// - `Document::estimated_bytes()` = `max(object_count*200KiB, json_len, 8KiB)`.
pub struct DocumentController {
    /// Documento activo.
    pub document: Document,
    /// Pila de undo acotada a `MAX_UNDO` (50), con `VecDeque` para `pop_front` O(1).
    pub undo_stack: VecDeque<Document>,
    /// Pila de redo (sin presupuesto de bytes; se limpia en cada push).
    pub redo_stack: VecDeque<ChangeSet>,
    /// Suma running de `estimated_bytes` de `undo_stack` — O(1) por push/pop.
    undo_total_bytes: usize,
}

impl DocumentController {
    /// Crea un controlador con documento vacío y pilas vacías.
    pub fn new() -> Self {
        Self {
            document: crate::app::initial_document(),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            undo_total_bytes: 0,
        }
    }

    /// Crea un controlador a partir de un documento existente.
    pub fn with_document(document: Document) -> Self {
        Self {
            document,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            undo_total_bytes: 0,
        }
    }

    /// Referencia al documento activo.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Referencia mutable al documento activo.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// Reemplaza el documento y limpia el historial (open/new).
    pub fn replace_document(&mut self, document: Document) {
        self.document = document;
        self.clear_history();
    }

    /// Limpia el historial y resetea el contador de bytes.
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.undo_total_bytes = 0;
    }

    /// Número de entradas en undo.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Número de entradas en redo.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Si hay algo para deshacer.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Si hay algo para rehacer.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Total estimado de bytes en `undo_stack` (running counter O(1)).
    pub fn total_bytes(&self) -> usize {
        self.undo_total_bytes
    }

    /// Guarda un snapshot previo a una mutación, limpia redo y aplica presupuestos.
    ///
    /// - `redo_stack.clear()` — una nueva mutación invalida redo.
    /// - `undo_total_bytes` se actualiza O(1) por push/pop; no se escanea toda la deque.
    /// - Evicción por `MAX_UNDO` (50) y `MAX_UNDO_BYTES` (50 MiB) con `pop_front` O(1).
    pub fn push_snapshot(&mut self, before: Document) {
        let bytes = before.estimated_bytes();
        self.undo_stack.push_back(before);
        self.redo_stack.clear();
        self.undo_total_bytes = self.undo_total_bytes.saturating_add(bytes);
        self.enforce_budgets();
    }

    /// Deshace la última mutación registrada.
    pub fn undo(&mut self) -> Result<(), String> {
        let Some(before) = self.undo_stack.pop_back() else {
            return Err("No hay nada para deshacer".to_string());
        };
        let before_bytes = before.estimated_bytes();
        self.undo_total_bytes = self.undo_total_bytes.saturating_sub(before_bytes);
        let changes = ChangeSet {
            before,
            after: self.document.clone(),
        };
        match changes.undo(&mut self.document) {
            Ok(()) => {
                self.redo_stack.push_back(changes);
                Ok(())
            }
            Err(error) => {
                // Restaura el snapshot al undo_stack y el contador si el restore falla.
                // Re-push el before (su bytes ya restado, volver a sumar)
                let retry_bytes = changes.before.estimated_bytes();
                self.undo_total_bytes = self.undo_total_bytes.saturating_add(retry_bytes);
                self.undo_stack.push_back(changes.before);
                Err(error)
            }
        }
    }

    /// Rehace la última mutación deshecha.
    pub fn redo(&mut self) -> Result<(), String> {
        let Some(changes) = self.redo_stack.pop_back() else {
            return Err("No hay nada para rehacer".to_string());
        };
        let before_redo = self.document.clone();
        let before_bytes = before_redo.estimated_bytes();
        match changes.redo(&mut self.document) {
            Ok(()) => {
                self.undo_stack.push_back(before_redo);
                self.undo_total_bytes = self.undo_total_bytes.saturating_add(before_bytes);
                self.enforce_budgets();
                Ok(())
            }
            Err(error) => {
                self.redo_stack.clear();
                Err(error)
            }
        }
    }

    fn enforce_budgets(&mut self) {
        while self.undo_stack.len() > MAX_UNDO {
            if let Some(front) = self.undo_stack.pop_front() {
                self.undo_total_bytes = self
                    .undo_total_bytes
                    .saturating_sub(front.estimated_bytes());
            } else {
                break;
            }
        }
        while self.undo_total_bytes > MAX_UNDO_BYTES && self.undo_stack.len() > 1 {
            if let Some(front) = self.undo_stack.pop_front() {
                self.undo_total_bytes = self
                    .undo_total_bytes
                    .saturating_sub(front.estimated_bytes());
            } else {
                break;
            }
        }
        debug_assert!(
            self.undo_total_bytes
                <= self
                    .undo_stack
                    .iter()
                    .map(|d| d.estimated_bytes())
                    .fold(0usize, |a, b| a.saturating_add(b)),
            "running counter must not exceed recomputed sum"
        );
    }

    /// Recalcula `total_bytes` escaneando la deque — sólo para tests/debug.
    #[cfg(test)]
    pub fn recomputed_total_bytes(&self) -> usize {
        self.undo_stack
            .iter()
            .map(|d| d.estimated_bytes())
            .fold(0usize, |a, b| a.saturating_add(b))
    }
}

impl Default for DocumentController {
    fn default() -> Self {
        Self::new()
    }
}

// ── ViewController ───────────────────────────────────────────────────────────

/// Gestiona el estado de vista y cámara.
///
/// `perspective` es la fuente canónica; `current_view` es cache derivado
/// (`perspective.view_mode()`) sincronizado vía `ViewController::set_perspective`
/// / `sync_view`. Invariante: `debug_assert_eq!(current_view, perspective.view_mode())`.
pub struct ViewController {
    /// Cámara 3D activa.
    pub camera: grafito_geometry::Camera3D,
    /// Cache derivado de `perspective.view_mode()` — no es fuente independiente.
    pub current_view: crate::ViewMode,
    /// Perspectiva activa — fuente canónica única para `current_view` y `CanvasMode`.
    pub perspective: crate::Perspective,
    /// Calidad de render actual.
    pub render_quality: grafito_core::RenderQuality,
}

impl ViewController {
    /// Crea un controlador de vista con valores por defecto.
    pub fn new() -> Self {
        let perspective = crate::Perspective::default();
        let current_view = perspective.view_mode();
        debug_assert_eq!(current_view, crate::ViewMode::D2);
        Self {
            camera: grafito_geometry::Camera3D::default(),
            current_view,
            perspective,
            render_quality: grafito_core::RenderQuality::Normal,
        }
    }

    /// Crea un controlador desde una perspectiva y cámara dadas.
    pub fn with_perspective_and_camera(
        perspective: crate::Perspective,
        camera: grafito_geometry::Camera3D,
    ) -> Self {
        let current_view = perspective.view_mode();
        Self {
            camera,
            current_view,
            perspective,
            render_quality: grafito_core::RenderQuality::Normal,
        }
    }

    /// Sincroniza `current_view` desde la `perspective` canónica.
    pub fn sync_view(&mut self) {
        self.current_view = self.perspective.view_mode();
        debug_assert_eq!(self.current_view, self.perspective.view_mode());
    }

    /// Cambia la perspectiva y sincroniza `current_view`.
    pub fn set_perspective(&mut self, perspective: crate::Perspective) {
        self.perspective = perspective;
        self.sync_view();
    }

    /// Verifica el invariante `current_view == perspective.view_mode()`.
    pub fn assert_view_invariant(&self) {
        debug_assert_eq!(
            self.current_view,
            self.perspective.view_mode(),
            "ViewController invariant violated"
        );
    }

    /// Indica si la vista actual es 3D.
    pub fn is_3d(&self) -> bool {
        self.current_view == crate::ViewMode::D3
    }

    /// Actualiza la cámara y mantiene el invariante.
    pub fn set_camera(&mut self, camera: grafito_geometry::Camera3D) {
        self.camera = camera;
    }
}

impl Default for ViewController {
    fn default() -> Self {
        Self::new()
    }
}

// ── AssistantController ──────────────────────────────────────────────────────

/// Gestiona el asistente y su visibilidad.
///
/// `state` es el `AssistantPanelState` puro (sin I/O). El `AssistantRuntime`
/// con I/O vive en `GrafitoApp` y se migrará en P2.
pub struct AssistantController {
    /// Estado del panel del asistente.
    pub state: grafito_ui::assistant::AssistantPanelState,
    /// Si el panel está visible.
    pub visible: bool,
}

impl AssistantController {
    /// Crea un controlador de asistente con estado por defecto.
    pub fn new() -> Self {
        Self {
            state: grafito_ui::assistant::AssistantPanelState::default(),
            visible: true,
        }
    }

    /// Crea con visibilidad explícita.
    pub fn with_visibility(visible: bool) -> Self {
        Self {
            state: grafito_ui::assistant::AssistantPanelState::default(),
            visible,
        }
    }

    /// Referencia al estado.
    pub fn state(&self) -> &grafito_ui::assistant::AssistantPanelState {
        &self.state
    }

    /// Referencia mutable al estado.
    pub fn state_mut(&mut self) -> &mut grafito_ui::assistant::AssistantPanelState {
        &mut self.state
    }

    /// Alterna visibilidad.
    pub fn toggle_visibility(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }

    /// Establece visibilidad.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Limpia propuestas verificadas y foco (usado al reemplazar documento).
    pub fn clear_transient(&mut self) {
        self.state.verified_proposals.clear();
        self.state.focus = None;
        self.state.invalidate_proposal_correction();
    }
}

impl Default for AssistantController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{Document, GeoObject, PointObj};
    use grafito_geometry::Point2;

    fn doc_with_points(n: usize) -> Document {
        let mut doc = Document::new();
        for i in 0..n {
            let mut p = PointObj::new(Point2::new(i as f64, 0.0));
            // Evitar auto-label collisions: asignar label determinístico
            p.label = format!("P{i}");
            let _ = doc.try_add_object(GeoObject::Point(p));
        }
        doc
    }

    #[test]
    fn document_controller_push_evicts_by_count_vecdeque_o1() {
        let mut ctl = DocumentController::new();
        // Push 55 snapshots of a small document (8 KiB each) -> debe quedar 50 (MAX_UNDO) con pop_front O(1)
        // Usamos documento vacío para no activar el presupuesto por bytes (50 MiB) y probar sólo la cota por cantidad.
        for _ in 0..55 {
            let snap = Document::new();
            ctl.push_snapshot(snap);
            // Mutación ligera sin aumentar object_count*200KiB: variable pequeña mantiene bytes ~8 KiB
            let _ = ctl
                .document
                .try_set_variable(format!("v{}", ctl.undo_len()), ctl.undo_len() as f64);
        }
        assert_eq!(ctl.undo_len(), MAX_UNDO);
        assert_eq!(ctl.undo_stack.len(), MAX_UNDO);
        // running counter debe coincidir con recomputed
        assert_eq!(ctl.total_bytes(), ctl.recomputed_total_bytes());
        assert!(ctl.total_bytes() <= MAX_UNDO_BYTES);
    }

    #[test]
    fn document_controller_push_evicts_by_bytes_budget() {
        let mut ctl = DocumentController::new();
        // Crear documento con muchos objetos para que estimated_bytes sea grande
        let mut big = Document::new();
        for i in 0..100 {
            let mut p = PointObj::new(Point2::new(i as f64, i as f64));
            p.label = format!("B{i}");
            let _ = big.try_add_object(GeoObject::Point(p));
        }
        let big_bytes = big.estimated_bytes();
        assert!(big_bytes >= 100 * 200 * 1024 || big_bytes >= 8 * 1024);
        // Push suficientes big docs para superar 50 MiB
        let pushes = (MAX_UNDO_BYTES / big_bytes) + 5;
        for _ in 0..pushes {
            ctl.push_snapshot(big.clone());
        }
        assert!(ctl.total_bytes() <= MAX_UNDO_BYTES || ctl.undo_len() == 1);
        assert_eq!(ctl.total_bytes(), ctl.recomputed_total_bytes());
    }

    #[test]
    fn document_controller_undo_redo_running_counter() {
        let mut ctl = DocumentController::new();
        let before = ctl.document.clone();
        // Mutar
        let mut p = PointObj::new(Point2::new(1.0, 2.0));
        p.label = "A".to_string();
        let _ = ctl.document.try_add_object(GeoObject::Point(p));
        ctl.push_snapshot(before);
        let total_after_push = ctl.total_bytes();
        assert_eq!(ctl.undo_len(), 1);
        assert_eq!(total_after_push, ctl.recomputed_total_bytes());

        // Undo debe decrementar contador O(1)
        ctl.undo().expect("undo ok");
        assert_eq!(ctl.undo_len(), 0);
        assert_eq!(ctl.redo_len(), 1);
        assert_eq!(ctl.total_bytes(), 0);

        // Redo debe incrementar contador y respetar presupuesto
        ctl.redo().expect("redo ok");
        assert_eq!(ctl.undo_len(), 1);
        assert_eq!(ctl.total_bytes(), ctl.recomputed_total_bytes());
    }

    #[test]
    fn document_controller_clear_resets_counter() {
        let mut ctl = DocumentController::new();
        for _ in 0..3 {
            ctl.push_snapshot(ctl.document.clone());
            let mut p = PointObj::new(Point2::new(0.0, 0.0));
            p.label = format!("C{}", ctl.undo_len());
            let _ = ctl.document.try_add_object(GeoObject::Point(p));
        }
        assert!(ctl.total_bytes() > 0);
        ctl.clear_history();
        assert_eq!(ctl.undo_len(), 0);
        assert_eq!(ctl.total_bytes(), 0);
        assert_eq!(ctl.recomputed_total_bytes(), 0);
    }

    #[test]
    fn document_estimated_bytes_uses_object_count_and_json() {
        let empty = Document::new();
        assert!(empty.estimated_bytes() >= 8 * 1024);
        let with_objs = doc_with_points(5);
        let bytes = with_objs.estimated_bytes();
        assert!(bytes >= 5 * 200 * 1024);
        assert!(bytes >= 8 * 1024);
        // JSON path: documento con poca geometría pero spreadsheet grande
        let mut with_sheet = Document::new();
        with_sheet
            .set_spreadsheet_cell(0, 0, "a".repeat(5000))
            .unwrap();
        let sheet_bytes = with_sheet.estimated_bytes();
        // Debe ser al menos el json len (5000+ overhead) y max con object bound
        assert!(sheet_bytes >= 5000);
    }

    #[test]
    fn view_controller_sync_maintains_invariant() {
        let mut vc = ViewController::new();
        assert_eq!(vc.current_view, crate::ViewMode::D2);
        vc.set_perspective(crate::Perspective::Geometry3D);
        assert_eq!(vc.current_view, crate::ViewMode::D3);
        assert!(vc.is_3d());
        vc.assert_view_invariant();
        vc.set_perspective(crate::Perspective::AlgebraCas);
        assert_eq!(vc.current_view, crate::ViewMode::D2);
        assert!(!vc.is_3d());
    }

    #[test]
    fn assistant_controller_toggle_and_clear() {
        let mut ac = AssistantController::new();
        assert!(ac.visible);
        assert!(!ac.toggle_visibility());
        assert!(!ac.visible);
        ac.set_visible(true);
        assert!(ac.visible);
        ac.clear_transient();
        assert!(ac.state.verified_proposals.is_empty());
    }
}
