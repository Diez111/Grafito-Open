//! Aplicación de escritorio Grafito — raíz de módulos.
//!
//! La lógica de la aplicación está dividida en módulos específicos:
//! `app`, `canvas`, `input`, `ui`, `commands` y `utils`. Los helpers de
//! renderizado legado y el despachador de herramientas permanecen como
//! módulos internos del crate.

pub(crate) mod algebra;
pub(crate) mod app;
pub(crate) mod canvas;
pub(crate) mod commands;
pub(crate) mod export;
pub(crate) mod input;
pub(crate) mod keyboard;
pub(crate) mod panels;
pub(crate) mod render_2d;
pub(crate) mod render_3d;
pub(crate) mod tool_dispatcher;
pub(crate) mod ui;
pub(crate) mod utils;

#[cfg(test)]
mod tests;

/// Cantidad de muestras MSAA usada por el renderizador GPU y la superficie de eframe.
pub const MSAA_SAMPLES: u16 = 4;

/// Modo de vista 2D/3D actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    D2,
    D3,
}

pub use app::run_app;
pub(crate) use app::GrafitoApp;
pub(crate) use app::PendingAction;
pub(crate) use utils::to_color32;
