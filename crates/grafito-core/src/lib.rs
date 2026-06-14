//! Grafito Core — Document model, objects, and constraints.

pub mod constraints;
pub mod document;
pub mod function_sampling;
pub mod id;
pub mod implicit_curve;
pub mod macros;
pub mod object;
pub mod parametric_sampling;
pub mod spatial;
pub mod validation;

pub use constraints::*;
pub use document::*;
pub use id::*;
pub use object::*;
pub use spatial::*;

/// Rendering quality hint used to trade fidelity for responsiveness while
/// the user is interacting with the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderQuality {
    #[default]
    Normal,
    /// Low resolution while panning / zooming.
    Preview,
    /// High resolution once the view has been idle for a short time.
    High,
}

#[cfg(test)]
mod tests;
