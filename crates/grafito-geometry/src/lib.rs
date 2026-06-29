//! Grafito Geometry — Primitivas matemáticas y geometría computacional.
//!
//! Contiene el motor matemático: álgebra simbólica (CAS), evaluación de
//! expresiones, análisis numérico de funciones y curvas (raíces, extremos,
//! inflexiones, interceptos, asíntotas, Taylor), estadística, probabilidad,
//! ODE, curvas especiales, atractores, fractales, matrices y operaciones
//! booleanas 2D sobre polígonos.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_geometry::{Point2, expr::evaluate};
//!
//! let p = Point2::new(2.0, 3.0);
//! let y = evaluate("x^2", &[("x".to_string(), p.x)]).unwrap();
//!
//! assert!((y - 4.0).abs() < 1e-9);
//! ```

pub mod analysis;
pub mod ast;
pub mod attractors;
pub mod boolean;
pub mod cas;
pub mod dd;
pub mod expr;
pub mod fractals;
pub mod integral;
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
pub mod value;

pub use boolean::*;
pub use lines::*;
pub use types::*;
pub use types3d::*;

#[cfg(test)]
mod tests;
