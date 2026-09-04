//! Errores tipados del crate core — una sola fuente de verdad para `Document` y persistencia.
//!
//! Cada variante lleva contexto (id, etiqueta, límite) para que el consumidor pueda
//! hacer `match` sin parsear texto y para que `?` preserve información.

use crate::ObjectId;
use thiserror::Error;

/// Error principal del modelo de documento.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CoreError {
    #[error("validación: {0}")]
    Validation(String),

    #[error("objeto no encontrado: {id}")]
    NotFound { id: ObjectId },

    #[error("etiqueta duplicada o inválida / duplicate label: '{label}': {reason}")]
    Label { label: String, reason: String },

    #[error("límite excedido / limit exceeded: {what} ({provided} > {maximum}) maximum")]
    LimitExceeded {
        what: String,
        provided: usize,
        maximum: usize,
    },

    #[error("expresión inválida '{expression}': {reason}")]
    InvalidExpression { expression: String, reason: String },

    #[error("restricción: {0}")]
    Constraint(String),

    #[error("restricción numérica: {0}")]
    NumericConstraint(String),

    #[error("solver: {0}")]
    Solver(String),

    #[error("persistencia: {0}")]
    Persistence(String),

    #[error("operación no permitida: {0}")]
    IllegalOperation(String),

    #[error("Transformed object nesting exceeds maximum {maximum} (depth {depth} > {maximum})")]
    TransformDepthExceeded { depth: usize, maximum: usize },

    #[error("Transformed Jacobian singular for '{expr}': {reason}")]
    TransformJacobianSingular { expr: String, reason: String },

    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
    pub fn constraint(msg: impl Into<String>) -> Self {
        Self::Constraint(msg.into())
    }
    pub fn solver(msg: impl Into<String>) -> Self {
        Self::Solver(msg.into())
    }
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl From<String> for CoreError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for CoreError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string())
    }
}

// Compatibilidad: quienes aún devuelven Result<_, String> pueden usar `.to_string()`
// o `map_err(|e: CoreError| e.to_string())`. Para migración gradual, permitimos
// convertir CoreError -> String via Display.
impl From<CoreError> for String {
    fn from(err: CoreError) -> Self {
        err.to_string()
    }
}

/// Alias de conveniencia para operaciones del documento.
pub type CoreResult<T> = Result<T, CoreError>;
