//! Atajos de teclado de `GrafitoApp` — extracción F4 sin cambio de conducta.
//!
//! Bloque movido tal cual desde `app.rs::update` (docs `4088-4290` → hoy
//! `5313-5506` por crecimiento commiteado desde BUILD 2026-09-04). Submódulo
//! hijo de `crate::app` (registrado con `#[path = "shortcuts.rs"] mod shortcuts;`
//! en `app.rs`) para conservar acceso a campos/métodos privados sin cambiar
//! visibilidad. `update` llama una vez a `handle_keyboard_shortcuts(ctx)`;
//! los tests existentes de atajos siguen pasando.

use super::{ctrl_y_shortcut, global_shortcuts_allowed, CtrlYShortcut, GrafitoApp};
use crate::lifecycle::file_shortcut;
use crate::Perspective;
use egui::Key;
use grafito_ui::theme::{DARK, LIGHT};
use grafito_ui::Tool;

impl GrafitoApp {
    /// Ejecuta todos los atajos de teclado (canvas + globales). Extraído mecánico de `update`.
    pub(crate) fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
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
                        // P1a-1: en examen el atajo no bypassea (toast + Err, sin mutar).
                        let _ = self.try_set_perspective(p);
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
    }
}
