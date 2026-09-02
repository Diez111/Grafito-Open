#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Núcleo agentico de Grafito.
//!
//! Provee el contrato de herramientas (schema), el enrutamiento de modelos por
//! tarea, y el loop de agente acotado. Es una hoja del DAG: no depende de
//! egui, red ni del documento; los proveedores reales y los despachadores
//! viven en crates consumidores (grafito-assistant).

pub mod ledger;
pub mod loop_engine;
pub mod router;
pub mod schema;

pub use ledger::{JSpaceLedger, MAX_LEDGER_RENDER_BYTES};
pub use loop_engine::{
    run_agent, AgentBudget, AgentChatResponse, AgentCompleter, AgentEvent, AgentOutcome,
    Cancellation, ToolDispatcher,
};
pub use router::{classify_band, classify_route, ModelRoute, TaskBand};
pub use schema::{parse_tool_calls, ToolCall, ToolResult, ToolSchema, MAX_TOOL_RESULT_CHARS};

/// Versión del protocolo de herramientas que este núcleo emite a los modelos.
pub const AGENT_PROTOCOL_VERSION: u32 = 1;
