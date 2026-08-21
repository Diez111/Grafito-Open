//! Puente de Grafito hacia motores de animación externos.
//!
//! El motor corre fuera del proceso Rust (p. ej. Python + Manim) y habla un
//! protocolo JSON versionado sobre stdio. Este crate gestiona el ciclo de vida
//! del worker, los jobs y los presupuestos, sin dependencias de egui ni de red.

pub mod engine;
pub mod protocol;

pub use engine::{run_job, AnimEngine, EngineConfig, JobEvent};
pub use protocol::{
    downcast, kinds, AnimDuration, AnimJobId, AnimParams, AnimRequest, AnimResult, ExportFormat,
    RenderProgress, Resolution, WireMessage,
};
