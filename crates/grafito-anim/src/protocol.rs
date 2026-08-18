//! Protocolo JSON v1 entre Grafito y el motor de animaciones externo.

use serde::{Deserialize, Serialize};

/// Versión del protocolo que este puente habla.
pub const ANIM_PROTOCOL_VERSION: u32 = 1;

/// Identificador opaco de un job.
pub type AnimJobId = String;

/// Formato de exportación pedido al motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    #[serde(rename = "gif")]
    Gif,
    #[serde(rename = "png")]
    PngSequence,
    #[serde(rename = "mp4")]
    Mp4,
}

impl ExportFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::PngSequence => "png",
            Self::Mp4 => "mp4",
        }
    }
}

/// Pedido de una animación: o un concepto en lenguaje natural o un spec JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimRequest {
    /// Nombre de la plantilla escénica (p. ej. derivative-slope).
    #[serde(default)]
    pub template: String,
    /// Descripción natural del concepto (si el motor analiza/genera).
    #[serde(default)]
    pub concept: String,
    /// Parámetros numéricos finitos de la escena.
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, f64>,
    /// Spec JSON opcional ya estructurado.
    #[serde(default)]
    pub spec: Option<serde_json::Value>,
    pub export: ExportFormat,
    /// Dimensiones del lienzo en píxeles.
    pub canvas: (u32, u32),
}

/// Progreso parcial de un render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderProgress {
    pub job_id: AnimJobId,
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub percent: u8,
}

/// Resultado de un render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimResult {
    pub job_id: AnimJobId,
    pub media_path: String,
    #[serde(default)]
    pub frames: usize,
    #[serde(default)]
    pub duration_ms: u64,
}

/// Tipos de mensaje del protocolo (etiqueta `type`).
pub mod kinds {
    pub const HELLO: &str = "hello";
    pub const RENDER_REQUEST: &str = "render_request";
    pub const PROGRESS: &str = "progress";
    pub const RENDER_RESULT: &str = "render_result";
    pub const ERROR: &str = "error";
    pub const PING: &str = "ping";
    pub const PONG: &str = "pong";
    pub const SHUTDOWN: &str = "shutdown";
}

/// Convierte un valor JSON genérico al tipo de mensaje concreto.
pub fn downcast(value: &serde_json::Value) -> Option<WireMessage> {
    let kind = value.get("type")?.as_str()?;
    match kind {
        kinds::HELLO => {
            let protocol_version = value
                .get("protocol_version")
                .and_then(serde_json::Value::as_u64)?;
            let capabilities = value
                .get("capabilities")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(WireMessage::Hello {
                protocol_version: protocol_version as u32,
                capabilities,
            })
        }
        kinds::PROGRESS => serde_json::from_value::<RenderProgress>(value.clone())
            .ok()
            .map(WireMessage::Progress),
        kinds::RENDER_RESULT => serde_json::from_value::<AnimResult>(value.clone())
            .ok()
            .map(WireMessage::Result),
        kinds::ERROR => {
            let message = value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown engine error")
                .to_owned();
            let code = value
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("error")
                .to_owned();
            Some(WireMessage::Error { code, message })
        }
        kinds::PONG => Some(WireMessage::Pong),
        _ => None,
    }
}

/// Mensajes tipados que el puente puede recibir del motor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireMessage {
    Hello {
        protocol_version: u32,
        capabilities: Vec<String>,
    },
    Progress(RenderProgress),
    Result(AnimResult),
    Error {
        code: String,
        message: String,
    },
    Pong,
}
