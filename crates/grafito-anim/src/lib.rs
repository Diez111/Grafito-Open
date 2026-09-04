#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Puente de Grafito hacia motores de animación externos.
//!
//! El motor corre fuera del proceso Rust (p. ej. Python + Manim) y habla un
//! protocolo JSON versionado sobre stdio. Este crate gestiona el ciclo de vida
//! del worker, los jobs y los presupuestos, sin dependencias de egui ni de red.
//! Incluye generador universal estilo canal de YouTube: cualquier texto produce
//! una animación profesional en <2s con fallback garantizado.

pub mod engine;
pub mod protocol;

pub use engine::{
    run_job, AnimEngine, EngineConfig, JobEvent, CANCEL_GRACE, DEFAULT_IDLE_TIMEOUT_SECS,
    DEFAULT_JOB_TIMEOUT_SECS, DEFAULT_LINE_CAP_BYTES, MAX_IDLE_TIMEOUT_SECS, MAX_JOB_TIMEOUT_SECS,
    MAX_LINE_CAP_BYTES, MIN_IDLE_TIMEOUT_SECS, MIN_JOB_TIMEOUT_SECS, MIN_LINE_CAP_BYTES,
};
pub use protocol::{
    downcast, kinds, localize_worker_error, normalize_concept, request_for_concept,
    sanitize_error_code, sanitize_template, template_for_concept, truncate_worker_message,
    AnimDuration, AnimJobId, AnimParams, AnimRequest, AnimResult, ExportFormat, RenderProgress,
    Resolution, WireMessage, WorkerError, MAX_ERROR_CODE_LEN, MAX_WORKER_MESSAGE_LEN,
};
