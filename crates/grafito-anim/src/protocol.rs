//! Protocolo JSON v1 entre Grafito y el motor de animaciones externo.
//!
//! Wire protocol (líneas JSON sobre stdio, `\n` terminado, UTF-8):
//!
//! - Rust → Python: `render_request`, `ping`, `shutdown`.
//! - Python → Rust: `hello`, `pong`, `progress`, `render_result`, `error`.
//!
//! Ejemplos (cada línea termina en `\n`):
//! ```text
//! R→P {"type":"render_request","job_id":"job-1","template":"derivative-slope","concept":"derivada","params":{},"spec":null,"export":"png","canvas":[640,480],"duration_ms":2000}
//! P→R {"type":"hello","protocol_version":1,"capabilities":["derivative-slope","integral-area"]}
//! P→R {"type":"pong"}
//! P→R {"type":"progress","job_id":"job-1","step":"render","percent":30}
//! P→R {"type":"progress","job_id":"job-1","step":"manim","percent":60}
//! P→R {"type":"progress","job_id":"job-1","step":"render","percent":100}
//! P→R {"type":"render_result","job_id":"job-1","media_path":"/tmp/w/job-1.png","frames":1,"duration_ms":120}
//! P→R {"type":"error","job_id":"job-1","code":"render_failed","message":"detalle acotado a 500 chars"}
//! R→P {"type":"ping"}
//! R→P {"type":"shutdown"}
//! ```
//!
//! Progreso REAL: el worker emite `progress` con `percent` 0..=100.
//! `RenderProgress::fraction()` lo expone como fracción 0..1 (`percent/100.0`)
//! sin inventar valores en el lado Rust. Errores del worker viajan tipados
//! como `WorkerError { code, message }` con mensaje acotado a 500 chars y
//! localización al español vía [`localize_worker_error`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Versión del protocolo que este puente habla.
pub const ANIM_PROTOCOL_VERSION: u32 = 1;

/// Identificador opaco de un job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnimJobId(pub String);

impl AnimJobId {
    pub fn new(s: String) -> Result<Self, ProtocolError> {
        Self::try_new(s)
    }
    pub fn try_new(s: String) -> Result<Self, ProtocolError> {
        if s.is_empty()
            || s.len() > 64
            || !s
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ProtocolError::InvalidJobId(s));
        }
        Ok(Self(s))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<AnimJobId> for String {
    fn from(id: AnimJobId) -> Self {
        id.0
    }
}
impl PartialEq<String> for AnimJobId {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}
impl PartialEq<AnimJobId> for String {
    fn eq(&self, other: &AnimJobId) -> bool {
        self == &other.0
    }
}
impl PartialEq<&str> for AnimJobId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Display for AnimJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Error tipado del protocolo.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("campo faltante: {field}")]
    MissingField { field: &'static str },
    #[error("campo inválido {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("versión no soportada {got} (soportado {min}..={max})")]
    UnsupportedVersion { got: u32, min: u32, max: u32 },
    #[error("formato de exportación desconocido: {0}")]
    UnsupportedExport(String),
    #[error("percent fuera de rango: {got} > 100")]
    PercentOutOfRange { got: u8 },
    #[error("canvas inválido: {0}")]
    InvalidCanvas(String),
    #[error("job_id inválido: {0}")]
    InvalidJobId(String),
    #[error("json: {0}")]
    Json(String),
    #[error("tipo de mensaje desconocido: {0}")]
    UnknownKind(String),
}

pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// Longitud máxima del mensaje de error del worker (acotado para la UI).
pub const MAX_WORKER_MESSAGE_LEN: usize = 500;
/// Longitud máxima del código de error del worker.
pub const MAX_ERROR_CODE_LEN: usize = 64;

/// Sanea un código de error a `[A-Za-z0-9_-]{1,64}`; si no cumple, `"error"`.
pub fn sanitize_error_code(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty()
        || t.len() > MAX_ERROR_CODE_LEN
        || !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return "error".to_string();
    }
    t.to_string()
}

/// Trunca un mensaje a 500 chars (por caracteres, no bytes) y recorta bordes.
pub fn truncate_worker_message(msg: &str) -> String {
    let s = msg.trim();
    if s.chars().count() <= MAX_WORKER_MESSAGE_LEN {
        s.to_string()
    } else {
        s.chars().take(MAX_WORKER_MESSAGE_LEN).collect()
    }
}

/// Error tipado del worker: código + mensaje acotado a 500 chars.
///
/// Se construye con [`WorkerError::try_new`] que sanea y trunca; nunca deja
/// pasar inglés crudo a la UI sin pasar por [`localize_worker_error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerError {
    pub code: String,
    pub message: String,
}

impl WorkerError {
    pub fn try_new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: sanitize_error_code(&code.into()),
            message: truncate_worker_message(&message.into()),
        }
    }
    /// Mensaje listo para la UI, siempre en español.
    pub fn localized(&self) -> String {
        localize_worker_error(&self.code, &self.message)
    }
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.localized())
    }
}

impl std::error::Error for WorkerError {}

/// Localiza un error del worker al español para la UI.
///
/// Nunca devuelve inglés crudo: cada código conocido tiene plantilla en
/// español; los desconocidos usan `"error del motor (<code>): <msg>"`.
/// `message` se trunca a 500 chars por seguridad.
pub fn localize_worker_error(code: &str, message: &str) -> String {
    let code = sanitize_error_code(code);
    let msg = truncate_worker_message(message);
    let detail = if msg.is_empty() {
        String::new()
    } else {
        format!(": {msg}")
    };
    match code.as_str() {
        "invalid_request" => format!("petición inválida{detail}"),
        "render_failed" => format!("falló el render{detail}"),
        "path_escape" => "ruta de salida fuera del área de trabajo".to_string(),
        "handshake_timeout" => "el motor no respondió al saludo (tiempo agotado)".to_string(),
        "handshake_error" => format!("error de conexión con el motor{detail}"),
        "version_mismatch" => format!("versión de protocolo incompatible{detail}"),
        "engine_exit" => format!("el motor se cerró inesperadamente{detail}"),
        "protocol" => "el motor emitió una línea demasiado larga (límite 64 KiB)".to_string(),
        "job_timeout" | "timeout" | "timed_out" => {
            "tiempo agotado esperando al motor (límite 90 s por defecto)".to_string()
        }
        "cancelled" => "cancelado por el usuario".to_string(),
        "error" => {
            if msg.is_empty() {
                "error del motor".to_string()
            } else {
                format!("error del motor{detail}")
            }
        }
        _ => format!("error del motor ({code}){detail}"),
    }
}

/// Resolución validada del lienzo (type-safe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    /// Crea una resolución validada (64..=4096 por lado, coincide con python MIN/MAX).
    pub fn try_new(width: u32, height: u32) -> Result<Self, ProtocolError> {
        if width < 64 || height < 64 {
            return Err(ProtocolError::InvalidCanvas(format!(
                "{width}x{height} < 64"
            )));
        }
        if width > 4096 || height > 4096 {
            return Err(ProtocolError::InvalidCanvas(format!(
                "{width}x{height} > 4096"
            )));
        }
        Ok(Self { width, height })
    }
    pub fn as_tuple(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
        }
    }
}

/// Duración validada de una animación en segundos (type-safe).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimDuration(pub f64);

impl AnimDuration {
    pub fn try_new(secs: f64) -> Result<Self, ProtocolError> {
        if !secs.is_finite() || !(0.1..=30.0).contains(&secs) {
            return Err(ProtocolError::InvalidField {
                field: "duration",
                reason: format!("{secs} fuera de 0.1..=30"),
            });
        }
        Ok(Self(secs))
    }
    pub fn as_secs(self) -> f64 {
        self.0
    }
    pub fn as_millis(self) -> u64 {
        (self.0 * 1000.0).round() as u64
    }
}

impl Default for AnimDuration {
    fn default() -> Self {
        Self(2.0)
    }
}

/// Parámetros de alto nivel para construir un AnimRequest de forma type-safe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimParams {
    pub template: String,
    pub concept: String,
    pub params: std::collections::BTreeMap<String, f64>,
    pub duration: AnimDuration,
    pub resolution: Resolution,
    pub export: ExportFormat,
    pub spec: Option<serde_json::Value>,
}

impl AnimParams {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.template.is_empty() && self.concept.is_empty() && self.spec.is_none() {
            return Err(ProtocolError::InvalidField {
                field: "template/concept/spec",
                reason: "al menos uno requerido".into(),
            });
        }
        if self.template.len() > 64 {
            return Err(ProtocolError::InvalidField {
                field: "template",
                reason: "excede 64 chars".into(),
            });
        }
        for (k, v) in &self.params {
            if !v.is_finite() {
                return Err(ProtocolError::InvalidField {
                    field: "params",
                    reason: format!("{k} no es finito"),
                });
            }
        }
        // duration ya validada en try_new 0.1..=30s; resolution 64..=4096.
        // Re-validar aquí para detectar构造 via struct literal que bypasee try_new.
        Resolution::try_new(self.resolution.width, self.resolution.height)?;
        AnimDuration::try_new(self.duration.0)?;
        Ok(())
    }
    pub fn into_request(self) -> AnimRequest {
        AnimRequest {
            template: self.template,
            concept: self.concept,
            params: self.params,
            spec: self.spec,
            export: self.export,
            canvas: self.resolution.as_tuple(),
            duration_ms: self.duration.as_millis(),
        }
    }
}

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
    /// Duración en ms (propagada desde AnimParams::duration). Default 2000 si falta (compat v1).
    #[serde(default = "default_duration_ms")]
    pub duration_ms: u64,
}

fn default_duration_ms() -> u64 {
    2000
}

impl AnimRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.template.is_empty() && self.concept.is_empty() && self.spec.is_none() {
            return Err(ProtocolError::InvalidField {
                field: "template/concept/spec",
                reason: "al menos uno requerido".into(),
            });
        }
        if self.template.len() > 64 {
            return Err(ProtocolError::InvalidField {
                field: "template",
                reason: "excede 64 chars".into(),
            });
        }
        if !self.template.is_empty()
            && !self
                .template
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ProtocolError::InvalidField {
                field: "template",
                reason: "caracteres inválidos".into(),
            });
        }
        for (k, v) in &self.params {
            if !v.is_finite() {
                return Err(ProtocolError::InvalidField {
                    field: "params",
                    reason: format!("{k} no es finito"),
                });
            }
        }
        let (w, h) = self.canvas;
        if w == 0 || h == 0 {
            return Err(ProtocolError::InvalidCanvas("cero".into()));
        }
        if w < 64 || h < 64 {
            return Err(ProtocolError::InvalidCanvas(format!("{w}x{h} < 64")));
        }
        // Límite estricto 4096: coincide con Resolution::try_new y con el motor Python (MIN/MAX_CANVAS).
        // Antes 8192 se aceptaba silencioso y el motor clampaba sin error — ahora es error tipado.
        if w > 4096 || h > 4096 {
            return Err(ProtocolError::InvalidCanvas(format!(
                "{w}x{h} > 4096 (máximo soportado)"
            )));
        }
        // Valida duration_ms propagada (0.1..30s → 100..30000ms).
        if self.duration_ms != 0 && (self.duration_ms < 100 || self.duration_ms > 30000) {
            return Err(ProtocolError::InvalidField {
                field: "duration_ms",
                reason: format!("{} fuera de 100..=30000", self.duration_ms),
            });
        }
        Ok(())
    }
}

/// Progreso parcial de un render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderProgress {
    pub job_id: String,
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub percent: u8,
}

impl RenderProgress {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.percent > 100 {
            return Err(ProtocolError::PercentOutOfRange { got: self.percent });
        }
        if self.job_id.is_empty() {
            return Err(ProtocolError::InvalidJobId(self.job_id.clone()));
        }
        Ok(())
    }
    /// Fracción REAL 0..1 del progreso reportado por el worker (`percent/100`).
    ///
    /// No inventa valores: si el worker no ha emitido `progress`, el llamante
    /// debe mostrar indeterminado en lugar de llamar a esto con datos falsos.
    pub fn fraction(&self) -> f32 {
        (f32::from(self.percent.min(100))) / 100.0
    }
}

/// Resultado de un render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnimResult {
    pub job_id: String,
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

/// Convierte un valor JSON genérico al tipo de mensaje concreto (permisivo, compat).
pub fn downcast(value: &serde_json::Value) -> Option<WireMessage> {
    try_downcast(value).ok()
}

/// Versión estricta que retorna error tipado.
pub fn try_downcast(value: &serde_json::Value) -> ProtocolResult<WireMessage> {
    let kind = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or(ProtocolError::MissingField { field: "type" })?;
    match kind {
        kinds::HELLO => {
            let protocol_version = value
                .get("protocol_version")
                .and_then(|v| v.as_u64())
                .ok_or(ProtocolError::MissingField {
                    field: "protocol_version",
                })?;
            let protocol_version =
                u32::try_from(protocol_version).map_err(|_| ProtocolError::UnsupportedVersion {
                    got: protocol_version as u32,
                    min: 1,
                    max: 1,
                })?;
            if protocol_version != ANIM_PROTOCOL_VERSION {
                return Err(ProtocolError::UnsupportedVersion {
                    got: protocol_version,
                    min: 1,
                    max: 1,
                });
            }
            let capabilities = value
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .take(32)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(WireMessage::Hello {
                protocol_version,
                capabilities,
            })
        }
        kinds::PROGRESS => {
            let progress: RenderProgress = serde_json::from_value(value.clone())
                .map_err(|e| ProtocolError::Json(e.to_string()))?;
            if progress.percent > 100 {
                return Err(ProtocolError::PercentOutOfRange {
                    got: progress.percent,
                });
            }
            Ok(WireMessage::Progress(progress))
        }
        kinds::RENDER_RESULT => {
            let result: AnimResult = serde_json::from_value(value.clone())
                .map_err(|e| ProtocolError::Json(e.to_string()))?;
            Ok(WireMessage::Result(result))
        }
        kinds::ERROR => {
            let message_raw = value
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or(ProtocolError::MissingField { field: "message" })?;
            let code_raw = value
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("error");
            // Tipado + acotado: código saneado y mensaje truncado a 500 chars.
            let code = sanitize_error_code(code_raw);
            let message = truncate_worker_message(message_raw);
            Ok(WireMessage::Error { code, message })
        }
        kinds::PONG => Ok(WireMessage::Pong),
        other => Err(ProtocolError::UnknownKind(other.to_owned())),
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

// ── Generador universal estilo canal de YouTube ───────────────────────────
/// Normaliza un concepto libre (trim, colapso de espacios, truncado 500 chars).
pub fn normalize_concept(concept: &str) -> String {
    let mut s = concept.trim().replace(['\n', '\r', '\t'], " ");
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    s = out;
    if s.is_empty() {
        return "matem\u{00e1}tica".to_string();
    }
    if s.len() > 500 {
        s = s.chars().take(500).collect();
    }
    s
}

/// Elige la mejor plantilla para cualquier texto (ES+EN), como un canal profesional.
/// Garantiza siempre una plantilla valida conocida.
pub fn template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    if c.contains("pit\u{00e1}goras")
        || c.contains("pitagoras")
        || c.contains("pythag")
        || (c.contains("triang") && (c.contains("rect") || c.contains("hipoten")))
    {
        return "pitagoras";
    }
    if c.contains("integral")
        || c.contains("\u{00e1}rea")
        || (c.contains("area")
            && (c.contains("bajo") || c.contains("curva") || c.contains("riemann")))
        || c.contains("\u{00e1}rea bajo")
    {
        return "integral-area";
    }
    if c.contains("taylor")
        || c.contains("maclaurin")
        || (c.contains("serie") && (c.contains("potencia") || c.contains("aprox")))
        || c.contains("aproxima")
    {
        return "taylor-series";
    }
    if c.contains("conformal")
        || c.contains("conforme")
        || c.contains("complej")
        || c.contains("complex")
        || c.contains("fractal")
        || c.contains("mandelb")
    {
        return "conformal-map";
    }
    if c.contains("deriv")
        || c.contains("pendiente")
        || c.contains("tangente")
        || c.contains("slope")
        || (c.contains("l\u{00ed}mite") && c.contains("cociente"))
    {
        return "derivative-slope";
    }
    if c.contains("vector") || (c.contains("campo") && c.contains("vectorial")) {
        return "conformal-map";
    }
    if c.contains("euler")
        || c.contains("número e")
        || c.contains("numero e")
        || c.contains("exp(")
        || c.contains("exponencial")
    {
        return "euler";
    }
    if c.contains("fourier")
        || c.contains("armónico")
        || c.contains("armonico")
        || c.contains("serie trigonométrica")
        || c.contains("serie trigonometrica")
    {
        return "fourier";
    }
    if c.contains("probab") || c.contains("binom") || c.contains("distrib") || c.contains("estad") {
        return "integral-area";
    }
    if c.contains("sin(") || c.contains("cos(") || c.contains("seno") || c.contains("coseno") {
        return "taylor-series";
    }
    "derivative-slope"
}

/// Sanitiza un template libre a uno conocido; si es desconocido, elige por concepto.
pub fn sanitize_template(template: &str, concept: &str) -> String {
    let t = template.trim().to_lowercase();
    match t.as_str() {
        "derivative-slope" | "integral-area" | "taylor-series" | "conformal-map" | "pitagoras"
        | "pythagoras" | "euler" | "fourier" => {
            if t == "pythagoras" {
                "pitagoras".to_string()
            } else {
                t
            }
        }
        "" | "universal" | "auto" => template_for_concept(concept).to_string(),
        _ => template_for_concept(concept).to_string(),
    }
}

/// Construye un AnimRequest universal a partir de cualquier texto libre.
/// Garantiza validacion y valores por defecto profesionales.
pub fn request_for_concept(concept: &str, template_hint: &str) -> AnimRequest {
    let concept_norm = normalize_concept(concept);
    let template = sanitize_template(template_hint, &concept_norm);
    AnimRequest {
        template,
        concept: concept_norm,
        params: std::collections::BTreeMap::new(),
        spec: None,
        export: ExportFormat::Gif,
        canvas: (640, 480),
        duration_ms: 2000,
    }
}

#[cfg(test)]
mod universal_tests {
    use super::*;
    #[test]
    fn normalize_handles_any_text() {
        assert_eq!(normalize_concept("  hola   mundo  "), "hola mundo");
        assert!(!normalize_concept("").is_empty());
        assert!(!normalize_concept("   ").is_empty());
        let long = "a".repeat(1000);
        assert!(normalize_concept(&long).len() <= 500);
        assert_eq!(normalize_concept("\u{1f600} emoji"), "\u{1f600} emoji");
    }
    #[test]
    fn template_for_any_text_is_valid() {
        let cases = [
            ("teorema de pit\u{00e1}goras", "pitagoras"),
            ("integral area bajo curva", "integral-area"),
            ("serie de taylor seno", "taylor-series"),
            ("mapeo conforme complejo", "conformal-map"),
            ("derivada pendiente tangente", "derivative-slope"),
            ("hola mundo sin matem\u{00e1}tica", "derivative-slope"),
            ("", "derivative-slope"),
            ("   ", "derivative-slope"),
            ("probabilidad binomial", "integral-area"),
            ("fractal mandelbrot", "conformal-map"),
        ];
        for (concept, expected) in cases {
            assert_eq!(
                template_for_concept(concept),
                expected,
                "concept: {concept}"
            );
        }
        // any arbitrary text must return a known template
        let known = [
            "derivative-slope",
            "integral-area",
            "taylor-series",
            "conformal-map",
            "pitagoras",
        ];
        for txt in [
            "random",
            "foo bar baz",
            "12345",
            "\u{1f4da} libros",
            &"x".repeat(200),
        ] {
            assert!(
                known.contains(&template_for_concept(txt)),
                "unknown mapping for {txt}"
            );
        }
    }
    #[test]
    fn request_for_concept_validates() {
        let req = request_for_concept("derivada", "");
        assert!(req.validate().is_ok());
        let req2 = request_for_concept("", "unknown-template");
        assert!(req2.validate().is_ok());
        let req3 = request_for_concept(&"a".repeat(1000), "auto");
        assert!(req3.validate().is_ok());
        assert!(req3.concept.len() <= 500);
    }
    #[test]
    fn placeholder_budget_under_2s_for_any_text() {
        let start = std::time::Instant::now();
        for i in 0..200 {
            let concept = format!("concepto {i} con texto libre y alguna matem\u{00e1}tica");
            let tmpl = template_for_concept(&concept);
            let _req = request_for_concept(&concept, tmpl);
        }
        assert!(
            start.elapsed().as_millis() < 1800,
            "universal mapping debe ser <1.8s para 200 conceptos"
        );
    }
    #[test]
    fn sanitize_template_fallback() {
        assert_eq!(
            sanitize_template("derivative-slope", "hola"),
            "derivative-slope"
        );
        assert_eq!(sanitize_template("pythagoras", "hola"), "pitagoras");
        assert_eq!(
            sanitize_template("", "integral de riemann"),
            "integral-area"
        );
        assert_eq!(
            sanitize_template("unknown", "taylor serie"),
            "taylor-series"
        );
    }
}
