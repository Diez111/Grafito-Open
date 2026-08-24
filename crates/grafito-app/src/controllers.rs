//! Controladores delegados para descomponer `GrafitoApp` (God Object split).
//!
//! `GrafitoApp` (~130 campos, `update` ~820 L) se descompone incrementalmente en
//! controladores especializados. Este módulo define los stubs de P1; la migración
//! completa de lógica se hará en P2-P3 sin romper tests.
//!
//! Estructura objetivo:
//! - `DocumentController` — document, undo/redo, lifecycle, snapshots.
//! - `ViewController` — camera, view, render quality, canvas state.
//! - `AssistantController` — assistant state, runtime, plugin registry.

use grafito_core::{ChangeSet, Document};
use std::collections::VecDeque;

// TODO: extraer DocumentController, ViewController, AssistantController como módulos delegados

/// Gestiona el documento y su historial (undo/redo).
#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
pub struct DocumentController {
    /// Documento activo.
    pub document: Document,
    /// Pila de undo acotada a `MAX_UNDO` (50), con `VecDeque` para `pop_front` O(1).
    pub undo_stack: VecDeque<Document>,
    /// Pila de redo.
    pub redo_stack: VecDeque<ChangeSet>,
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl DocumentController {
    /// Crea un controlador con documento vacío y pilas vacías.
    pub fn new() -> Self {
        Self {
            document: crate::app::initial_document(),
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    /// Crea un controlador a partir de un documento existente.
    pub fn with_document(document: Document) -> Self {
        Self {
            document,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    /// Limpia el historial (usado al reemplazar documento).
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl Default for DocumentController {
    fn default() -> Self {
        Self::new()
    }
}

/// Gestiona el estado de vista y cámara.
#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
pub struct ViewController {
    /// Cámara 3D activa.
    pub camera: grafito_geometry::Camera3D,
    /// Vista 2D/3D.
    pub current_view: crate::ViewMode,
    /// Perspectiva activa.
    pub perspective: crate::Perspective,
    /// Calidad de render actual.
    pub render_quality: grafito_core::RenderQuality,
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl ViewController {
    /// Crea un controlador de vista con valores por defecto.
    pub fn new() -> Self {
        Self {
            camera: grafito_geometry::Camera3D::default(),
            current_view: crate::ViewMode::D2,
            perspective: crate::Perspective::default(),
            render_quality: grafito_core::RenderQuality::Normal,
        }
    }
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl Default for ViewController {
    fn default() -> Self {
        Self::new()
    }
}

/// Gestiona el asistente y su runtime.
#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
pub struct AssistantController {
    /// Estado del panel del asistente.
    pub state: grafito_ui::assistant::AssistantPanelState,
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl AssistantController {
    /// Crea un controlador de asistente con estado por defecto.
    pub fn new() -> Self {
        Self {
            state: grafito_ui::assistant::AssistantPanelState::default(),
        }
    }
}

#[allow(dead_code)] // TODO P1: remover cuando se migre God Object
impl Default for AssistantController {
    fn default() -> Self {
        Self::new()
    }
}
