//! Grafito Desktop Application — thin module root.
//!
//! The application's logic is split into focused modules:
//! `app`, `canvas`, `input`, `ui`, `commands`, and `utils`. The legacy render
//! helpers and tool dispatcher remain as crate-internal modules.

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

/// MSAA sample count used by the GPU renderer and the eframe surface.
pub const MSAA_SAMPLES: u16 = 4;

/// Current 2D/3D view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    D2,
    D3,
}

pub use app::run_app;
pub(crate) use app::GrafitoApp;
pub(crate) use app::PendingAction;
pub(crate) use utils::to_color32;
