//! Grafito Geometry — Mathematical primitives and computational geometry.

pub mod types;
pub mod types3d;
pub mod expr;
pub mod cas;
pub mod symbolic;
pub mod precision;
pub mod interval;

#[cfg(test)]
mod tests;

pub use types::*;
pub use types3d::*;
