//! Grafito Core — Modelo de documento, objetos geométricos y restricciones.
//!
//! Este crate define el modelo central de Grafito: el [`Document`], los 32 tipos
//! de objetos geométricos representados por [`GeoObject`], los índices
//! espaciales y los dos sistemas de restricciones (constructivas y numéricas).
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_core::{Document, GeoObject, PointObj};
//! use grafito_geometry::Point2;
//!
//! let mut doc = Document::new();
//! let id = doc.add_object(GeoObject::Point(
//!     PointObj::new(Point2::new(1.0, 2.0)).with_label("A"),
//! ));
//!
//! assert!(doc.get_object(id).is_some());
//! ```

pub mod constraints;
pub mod document;
pub mod function_sampling;
pub mod id;
pub mod implicit_curve;
pub mod macros;
pub mod numeric_solver;
pub mod object;
pub mod parametric_sampling;
pub mod spatial;
pub mod validation;
pub mod vector_field_sampling;

pub mod numeric_constraints;

#[cfg(test)]
mod numeric_constraints_tests;

pub use constraints::*;
pub use document::*;
pub use id::*;
pub use object::*;
pub use spatial::*;

/// Indicador de calidad de renderizado usado para intercambiar fidelidad por
/// capacidad de respuesta mientras el usuario interactúa con la vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderQuality {
    #[default]
    Normal,
    /// Baja resolución mientras se panea / hace zoom.
    Preview,
    /// Alta resolución una vez que la vista ha estado inactiva por un breve tiempo.
    High,
}

#[cfg(test)]
mod tests;
