//! Módulo de mapeos conformes y aritmética compleja.
//!
//! Este módulo agrupa todo lo relacionado con mapeos conformes
//! (`ComplexMapping`), coloreado de dominio y la aritmética de
//! números complejos de Grafito. Históricamente vivía como un
//! módulo plano en `grafito_geometry::complex_expr`; se mudó aquí
//! para reflejar el dominio de aplicación.
//!
//! # Compatibilidad hacia atrás
//!
//! Los call-sites existentes (`use grafito_geometry::complex_expr::...`)
//! siguen funcionando porque `lib.rs` re-exporta este submódulo.

pub mod algebraic_mappings;
pub mod complex_calculus;
pub mod complex_expr;
pub mod complex_opcode;

pub use complex_expr::{eval_complex_batch, parse as parse_complex, ComplexExpr};
