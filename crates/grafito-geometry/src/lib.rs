//! Grafito Geometry — Mathematical primitives and computational geometry.

pub mod ast;
pub mod attractors;
pub mod cas;
pub mod complex_expr;
pub mod dd;
pub mod expr;
pub mod fractals;
pub mod intersections;
pub mod interval;
pub mod lines;
pub mod matrices;
pub mod ode;
pub mod precision;
pub mod special_curves;
pub mod special_functions;
pub mod statistics;
pub mod symbolic;
pub mod types;
pub mod types3d;

pub use lines::*;
pub use types::*;
pub use types3d::*;

#[cfg(test)]
mod tests;
