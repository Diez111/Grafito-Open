#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::uninlined_format_args)]
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
pub mod assumptions;
pub mod ast;
pub mod attractors;
pub mod boolean;
pub mod cas;
pub mod cas_extra;
pub mod cas_steps;
pub mod complex_solve;
pub mod dd;
pub mod derivation;
pub mod discrete;
pub mod exact;
pub mod exact_linalg;
pub mod expr;
pub mod fourier;
pub mod fractals;
pub mod function_sampling;
pub mod implicit_curve;
pub mod improper;
pub mod integral;
pub mod intersections;
pub mod interval;
pub mod latex;
pub mod lines;
pub mod list_ops;
pub mod locus_equation;
pub mod matrices;
pub mod measure;
pub mod measure_extra;
pub mod morph;
pub mod number_theory;
pub mod ode;
pub mod optimize;
pub mod outcome;
pub mod pde;
pub mod planes3d;
pub mod poly_tools;
pub mod polytopes;
pub mod precision;
pub mod prove;
pub mod quadrics;
pub mod search;
pub mod solve;
pub mod special_curves;
pub mod special_functions;
pub mod statistics;
pub mod stats_extra;
pub mod symbolic;
pub mod text_ops;
pub mod types;
pub mod types3d;
pub mod value;

pub use assumptions::{Assumption, Assumptions};
pub use boolean::*;
pub use exact::{ExactRational, ExactRationalError};
pub use lines::*;
pub use outcome::{MathError, MathOperation, MathResult, MAX_MATH_INPUT_BYTES};
pub use planes3d::*;
pub use polytopes::*;
pub use types::*;
pub use types3d::*;

#[cfg(test)]
mod tests;
