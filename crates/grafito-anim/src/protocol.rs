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
///
/// TODO(v3-webm): WebM NO soportado. Sería trivial solo si ffmpeg ya estuviera
/// cableado en el worker python (hoy el worker solo escribe gif/png/mp4 vía
/// placeholder + manim); añadir `Webm` aquí exige: (1) rama en el worker +
/// `ALLOW_EXPORT`, (2) validación de `ffmpeg` presente, (3) tests de wire.
/// El test `export_format_solo_tres_variantes` pinnea el estado actual: "webm"
/// se rechaza en deserialización.
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

// ── Params vivos v3 ─────────────────────────────────────────────────────
// Claves canónicas que los renderers nativos leen de `AnimRequest.params`
// (`anim_native::render_*_with_params`, crate grafito-app).
//
// | Clave   | Plantilla         | Significado                    | Default |
// |---------|-------------------|--------------------------------|---------|
// | `x0`    | derivative-slope  | centro del barrido en [-3, 3]  | 0.0     |
// | `span`  | derivative-slope  | semiancho del barrido          | 1.5     |
// | `a`     | integral-area     | cota inferior en [-3, 3]       | 0.0     |
// | `b`     | integral-area     | cota superior en [-3, 3]       | 2.0     |
// | `terms` | euler / fourier   | nº máx. parciales/armónicos    | 7 / 6   |
//
// NOTA: `taylor-series` IGNORA `terms` (su animación es un fade de orden 3
// fijo, no un conteo; cambiarlo rompería el histórico — TODO si se quiere
// orden paramétrico). El resto de plantillas también ignoran params por ahora
// (TODO); el dispatcher con params (`render_anim_for_concept_with_params`)
// documenta cuáles atienden y cuáles delegan sin cambios.
//
// Reglas: valor ausente / NaN / inf → default; fuera de rango → clamp.
// Un mapa vacío reproduce el comportamiento histórico exacto (los wrappers
// `render_*_frames(w, h)` delegan con mapa vacío). Presupuestos intactos:
// `AnimRequest::validate` ya exige params finitos; aquí nunca hay panic.
//

/// Clave del punto central del barrido (derivative-slope).
pub const SCENE_PARAM_X0: &str = "x0";
/// Clave del semiancho del barrido (derivative-slope).
pub const SCENE_PARAM_SPAN: &str = "span";
/// Clave de la cota inferior del área (integral-area).
pub const SCENE_PARAM_A: &str = "a";
/// Clave de la cota superior del área (integral-area).
pub const SCENE_PARAM_B: &str = "b";
/// Clave del nº máximo de parciales/armónicos (euler [1,7] / fourier [1,6]).
pub const SCENE_PARAM_TERMS: &str = "terms";

/// Lee un param finito o devuelve el default (ausente/NaN/inf → default).
pub fn scene_param(
    params: &std::collections::BTreeMap<String, f64>,
    key: &str,
    default: f64,
) -> f64 {
    params
        .get(key)
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// Idem + clamp a `[min, max]`. Si `min > max`, devuelve `default` (nunca panic).
pub fn scene_param_clamped(
    params: &std::collections::BTreeMap<String, f64>,
    key: &str,
    default: f64,
    min: f64,
    max: f64,
) -> f64 {
    if min.is_nan() || max.is_nan() || min > max {
        return default;
    }
    scene_param(params, key, default).clamp(min, max)
}

// ── Timeline v3 (keyframes lineales puros) ───────────────────────────────
// El easing NO vive aquí: se aplica en la Piel por NOMBRE (`EASING_NAMES`)
// con las fns existentes `grafito_ui::animation::easing::*`. No se inventa
// ningún enum nuevo y el puente no depende de grafito-ui (Cerebro puro).
// La UI (`anim_ui.rs`) mapea nombre → fn y vincula el scrub del deslizador
// a `Timeline::sample` sobre el tiempo con easing aplicado.

/// Tope de keyframes por timeline (acota memoria de la UI).
pub const MAX_TIMELINE_KEYFRAMES: usize = 64;
/// Duración máxima de un timeline (igual que `AnimDuration` 30 s).
pub const MAX_TIMELINE_DURATION_MS: u64 = 30_000;

/// Vocabulario compartido de easings: 1:1 con `grafito-ui/src/animation.rs`
/// (`easing::{linear, quadratic_in, quadratic_out, cubic_in, cubic_out,
/// cubic_in_out, sin_in_out, ease_out_back}`). Sin enum nuevo: el wire usa
/// el nombre y la Piel lo resuelve a la fn existente.
pub const EASING_NAMES: &[&str] = &[
    "linear",
    "quadratic_in",
    "quadratic_out",
    "cubic_in",
    "cubic_out",
    "cubic_in_out",
    "sin_in_out",
    "ease_out_back",
];

/// ¿El nombre de easing existe en `grafito-ui/src/animation.rs`?
pub fn is_known_easing(name: &str) -> bool {
    EASING_NAMES.contains(&name.trim())
}

/// Un keyframe: valor en el instante `t_ms` (interpolación lineal entre keys).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub t_ms: u64,
    pub value: f32,
}

/// Timeline de scrub: duración + keys ordenadas. `sample` nunca panics
/// (vacío → 0.0); `validate` es la versión estricta para construir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub duration_ms: u64,
    pub keyframes: Vec<Keyframe>,
}

impl Timeline {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.duration_ms == 0 || self.duration_ms > MAX_TIMELINE_DURATION_MS {
            return Err(ProtocolError::InvalidField {
                field: "timeline.duration_ms",
                reason: format!(
                    "{} fuera de 1..={MAX_TIMELINE_DURATION_MS}",
                    self.duration_ms
                ),
            });
        }
        if self.keyframes.is_empty() {
            return Err(ProtocolError::InvalidField {
                field: "timeline.keyframes",
                reason: "vacío (mínimo 1)".into(),
            });
        }
        if self.keyframes.len() > MAX_TIMELINE_KEYFRAMES {
            return Err(ProtocolError::InvalidField {
                field: "timeline.keyframes",
                reason: format!("{} excede {MAX_TIMELINE_KEYFRAMES}", self.keyframes.len()),
            });
        }
        let mut prev: Option<u64> = None;
        for k in &self.keyframes {
            if !k.value.is_finite() {
                return Err(ProtocolError::InvalidField {
                    field: "timeline.value",
                    reason: "no es finito".into(),
                });
            }
            if k.t_ms > self.duration_ms {
                return Err(ProtocolError::InvalidField {
                    field: "timeline.t_ms",
                    reason: format!("{} excede duración {}", k.t_ms, self.duration_ms),
                });
            }
            if let Some(p) = prev {
                if k.t_ms <= p {
                    return Err(ProtocolError::InvalidField {
                        field: "timeline.t_ms",
                        reason: "debe ser estrictamente creciente".into(),
                    });
                }
            }
            prev = Some(k.t_ms);
        }
        Ok(())
    }

    /// Muestra el valor en `t_ms` (lerp lineal, clamp en los extremos).
    /// Total aunque el timeline no esté validado: nunca panics.
    pub fn sample(&self, t_ms: u64) -> f32 {
        let keys = &self.keyframes;
        if keys.is_empty() {
            return 0.0;
        }
        if keys.len() == 1 {
            return keys[0].value;
        }
        if t_ms <= keys[0].t_ms {
            return keys[0].value;
        }
        if let Some(last) = keys.last() {
            if t_ms >= last.t_ms {
                return last.value;
            }
        }
        for pair in keys.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if t_ms >= a.t_ms && t_ms <= b.t_ms {
                let span = b.t_ms.saturating_sub(a.t_ms);
                if span == 0 {
                    return a.value;
                }
                let f = (t_ms.saturating_sub(a.t_ms) as f32) / (span as f32);
                return a.value + (b.value - a.value) * f;
            }
        }
        keys.last().map_or(0.0, |k| k.value)
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

/// ¿El texto (ya en minúsculas) trae `palabra` como token exacto?
///
/// Los `contains` pelados mienten ("tarea" contiene "area", "ecosistema"
/// contiene "sistema"): para claves cortas se exige palabra separada por
/// no-alfabéticos. Espejo de `parametric::pedido_menciona_area` (T1).
/// Puro, sin I/O, sin `unwrap`.
pub fn contiene_palabra(minusculas: &str, palabra: &str) -> bool {
    if palabra.is_empty() {
        return false;
    }
    minusculas
        .split(|ch: char| !ch.is_alphabetic())
        .any(|tok| tok == palabra)
}

/// Elige la mejor plantilla para cualquier texto (ES+EN), como un canal profesional.
/// Garantiza siempre una plantilla valida conocida.
/// El fallback es `universal` (placeholder neutro honesto, sin curva falsa):
/// jamás se devuelve una plantilla con renderer matemático para un pedido
/// que no la menciona.
pub fn template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    if c.contains("pit\u{00e1}goras")
        || c.contains("pitagoras")
        || c.contains("pythag")
        || (c.contains("triang") && (c.contains("rect") || c.contains("hipoten")))
    {
        return "pitagoras";
    }
    if c.contains("integral") || contiene_palabra(&c, "area") || contiene_palabra(&c, "área") {
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
    // Pedagógicas v3 (base compartida con `anim_native::detect_template_for_concept`,
    // que delega acá tras sus extras F5 — wrapper explícito T2):
    // antes caían al fallback derivative-slope aunque el nativo sí las cubre.
    if c.contains("logist") || c.contains("bifurc") {
        return "logistic-bifurcation";
    }
    if c.contains("gradiente") || c.contains("gradient") {
        return "gradient-field";
    }
    if c.contains("mobius") || c.contains("m\u{00f6}bius") || c.contains("moebius") {
        return "mobius-transform";
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
    // Fallback honesto T2: pedido desconocido → `universal` (placeholder
    // neutro rotulado, jamás curva falsa). Antes era `derivative-slope`,
    // que dibujaba parábola+tangente fingiendo respuesta.
    "universal"
}

/// Registro canónico de plantillas (sync 11↔11↔11, ANIM-REVIVE).
///
/// Única fuente del protocolo: las 11 canónicas con renderer nativo propio.
/// `sanitize_template` la usa (sin `match` duplicado que diverja);
/// `anim_native::NATIVE_TEMPLATES` y `anim_ui::PLANTILLAS_COMBO` (crate
/// `grafito-app`) se pinean iguales por test, en el mismo orden.
/// `limit-epsilon` / `ode-*` NO están aquí: no tienen renderer propio y caen
/// al fallback por concepto (ver `native_dispatch_for` en `anim_native`).
pub const CANONICAL_TEMPLATES: &[&str] = &[
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "euler",
    "fourier",
    "logistic-bifurcation",
    "gradient-field",
    "mobius-transform",
    "universal",
];

/// Plantillas que el worker Python SÍ renderiza (M3-10, espejo documentado
/// de `ALLOW_TEMPLATE` en
/// `crates/grafito-anim/engines/python/manim_engine/__main__.py`).
///
/// NO se puede generar desde `CANONICAL_TEMPLATES`: el worker es otro
/// proceso con 6 renderers (sin euler/fourier/logistic/gradient/mobius,
/// que solo existen en el nativo Rust). La divergencia queda pineada en
/// `python_worker_divergencia_11_vs_6_documentada`: si el worker suma una
/// plantilla, este const + el test Python `TestParidad11_6` gritan juntos.
pub const PYTHON_WORKER_TEMPLATES: &[&str] = &[
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "universal",
];

/// Sanitiza un template libre a uno conocido; si es desconocido, elige por concepto.
pub fn sanitize_template(template: &str, concept: &str) -> String {
    let t = template.trim().to_lowercase();
    // Alias histórico: pasa a su canónica.
    if t == "pythagoras" {
        return "pitagoras".to_string();
    }
    // Canónicas v3 con renderer nativo propio (sync con anim_native):
    // pasan literales para que el dispatcher nativo las atienda en vez
    // de degradarlas por concepto. "universal" también pasa literal: el
    // nativo lo renderiza (placeholder neutro honesto) y python lo mapea
    // por concepto.
    if CANONICAL_TEMPLATES.contains(&t.as_str()) {
        return t;
    }
    if t.is_empty() || t == "auto" {
        return template_for_concept(concept).to_string();
    }
    template_for_concept(concept).to_string()
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
            ("hola mundo sin matemática", "universal"),
            ("", "universal"),
            ("   ", "universal"),
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
            "euler",
            "fourier",
            "logistic-bifurcation",
            "gradient-field",
            "mobius-transform",
            // T2: el fallback honesto SÍ es `universal` (placeholder neutro);
            // nunca una plantilla con renderer matemático para texto libre.
            "universal",
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
    fn area_token_no_matchea_tarea() {
        // T2: "tarea" contiene "area" como substring pero no es área.
        assert!(!contiene_palabra("tarea de matemática", "area"));
        assert_eq!(template_for_concept("tarea de matemática"), "universal");
        assert_eq!(template_for_concept("área bajo curva"), "integral-area");
        assert_eq!(template_for_concept("area bajo curva"), "integral-area");
        assert_eq!(template_for_concept("integral de riemann"), "integral-area");
        assert!(!contiene_palabra("cualquier texto", ""));
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

    // ── v3: params vivos + timeline + sync plantillas + webm ────────────
    #[test]
    fn scene_param_default_y_finito() {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        assert_eq!(scene_param(&m, "x0", 0.0), 0.0);
        m.insert("x0".to_string(), 2.0);
        assert_eq!(scene_param(&m, "x0", 0.0), 2.0);
        // NaN / inf → default (presupuesto: params siempre finitos).
        m.insert("x0".to_string(), f64::NAN);
        assert_eq!(scene_param(&m, "x0", 0.0), 0.0);
        m.insert("x0".to_string(), f64::INFINITY);
        assert_eq!(scene_param(&m, "x0", 0.0), 0.0);
    }

    #[test]
    fn scene_param_clamped_acota_sin_panic() {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        m.insert("x0".to_string(), 99.0);
        assert_eq!(scene_param_clamped(&m, "x0", 0.0, -3.0, 3.0), 3.0);
        m.insert("x0".to_string(), -99.0);
        assert_eq!(scene_param_clamped(&m, "x0", 0.0, -3.0, 3.0), -3.0);
        // min > max → default, nunca panic.
        assert_eq!(scene_param_clamped(&m, "x0", 1.5, 5.0, -5.0), 1.5);
    }

    #[test]
    fn timeline_validate_y_sample() {
        let tl = Timeline {
            duration_ms: 2000,
            keyframes: vec![
                Keyframe {
                    t_ms: 0,
                    value: 0.0,
                },
                Keyframe {
                    t_ms: 1000,
                    value: 0.5,
                },
                Keyframe {
                    t_ms: 2000,
                    value: 1.0,
                },
            ],
        };
        assert!(tl.validate().is_ok());
        assert_eq!(tl.sample(0), 0.0);
        assert!((tl.sample(500) - 0.25).abs() < 1e-6);
        assert!((tl.sample(1500) - 0.75).abs() < 1e-6);
        // Clamp en extremos + más allá.
        assert_eq!(tl.sample(9999), 1.0);
        // Vacío nunca panics (0.0) pero no valida.
        let vacio = Timeline {
            duration_ms: 2000,
            keyframes: vec![],
        };
        assert_eq!(vacio.sample(500), 0.0);
        assert!(vacio.validate().is_err());
        // No creciente / fuera de duración / NaN → Err.
        let desorden = Timeline {
            duration_ms: 2000,
            keyframes: vec![
                Keyframe {
                    t_ms: 100,
                    value: 0.0,
                },
                Keyframe {
                    t_ms: 100,
                    value: 1.0,
                },
            ],
        };
        assert!(desorden.validate().is_err());
        let fuera = Timeline {
            duration_ms: 1000,
            keyframes: vec![Keyframe {
                t_ms: 2000,
                value: 0.0,
            }],
        };
        assert!(fuera.validate().is_err());
        let cero = Timeline {
            duration_ms: 0,
            keyframes: vec![Keyframe {
                t_ms: 0,
                value: 0.0,
            }],
        };
        assert!(cero.validate().is_err());
    }

    #[test]
    fn easing_vocabulario_sync_con_grafito_ui() {
        // 8 nombres 1:1 con grafito-ui/src/animation.rs `easing::*`.
        assert_eq!(EASING_NAMES.len(), 8);
        for n in ["linear", "cubic_in_out", "sin_in_out", "ease_out_back"] {
            assert!(is_known_easing(n), "{n} debe ser conocido");
        }
        assert!(!is_known_easing("bounce"));
        assert!(!is_known_easing(""));
        assert!(is_known_easing("  linear  "));
    }

    #[test]
    fn sanitize_pasa_canonicas_v3_en_lugar_de_degradar() {
        for t in [
            "logistic-bifurcation",
            "gradient-field",
            "mobius-transform",
            "universal",
        ] {
            assert_eq!(sanitize_template(t, "cualquier concepto"), t, "{t}");
        }
        assert_eq!(
            template_for_concept("bifurcación logística r=3.5"),
            "logistic-bifurcation"
        );
        assert_eq!(
            template_for_concept("campo de gradiente de f(x,y)"),
            "gradient-field"
        );
        assert_eq!(
            template_for_concept("transformación de Möbius"),
            "mobius-transform"
        );
        // Comportamiento histórico intacto para el resto.
        assert_eq!(template_for_concept("hola mundo"), "universal");
        assert_eq!(
            sanitize_template("auto", "integral de riemann"),
            "integral-area"
        );
    }

    #[test]
    fn export_format_solo_tres_variantes_webm_todo() {
        // Pinnea estado actual: gif/png/mp4 roundtrip; "webm" se rechaza.
        for (f, s) in [
            (ExportFormat::Gif, "\"gif\""),
            (ExportFormat::PngSequence, "\"png\""),
            (ExportFormat::Mp4, "\"mp4\""),
        ] {
            let v = serde_json::to_string(&f).unwrap();
            assert_eq!(v, s);
        }
        let webm: Result<ExportFormat, _> = serde_json::from_str("\"webm\"");
        assert!(webm.is_err(), "webm aún no soportado (TODO v3-webm)");
    }

    #[test]
    fn canonical_templates_once_y_sanitize_roundtrip() {
        // Sync 11↔11↔11: este registro es la fuente; anim_native y anim_ui
        // se pinean iguales por test en grafito-app (mismo orden).
        assert_eq!(CANONICAL_TEMPLATES.len(), 11);
        for t in CANONICAL_TEMPLATES {
            assert_eq!(sanitize_template(t, "cualquier concepto"), *t, "{t}");
        }
        // Alias y auto intactos tras el refactor a `contains`.
        assert_eq!(sanitize_template("pythagoras", "hola"), "pitagoras");
        assert_eq!(sanitize_template("  EULER  ", "hola"), "euler");
        assert_eq!(
            sanitize_template("", "integral de riemann"),
            "integral-area"
        );
        assert_eq!(
            sanitize_template("auto", "integral de riemann"),
            "integral-area"
        );
        assert_eq!(
            sanitize_template("limit-epsilon", "derivada"),
            "derivative-slope"
        );
    }

    #[test]
    fn python_worker_divergencia_11_vs_6_documentada() {
        // M3-10: el worker Python no se genera desde CANONICAL (otro
        // proceso, 6 renderers). La divergencia explícita es exactamente
        // estas 5 nativo-solo; pedirlas al worker da `error unsupported`
        // (ver `CANONICAL_SOLO_RUST` + `preparar_render` en `__main__.py`
        // y `TestParidad11_6` del lado Python).
        use std::collections::BTreeSet;
        assert_eq!(PYTHON_WORKER_TEMPLATES.len(), 6);
        let canon: BTreeSet<&&str> = CANONICAL_TEMPLATES.iter().collect();
        let worker: BTreeSet<&&str> = PYTHON_WORKER_TEMPLATES.iter().collect();
        assert!(
            worker.is_subset(&canon),
            "el worker no puede ofrecer fuera de CANONICAL"
        );
        let solo_rust: BTreeSet<&&str> = canon.difference(&worker).copied().collect();
        let esperadas: BTreeSet<&str> = [
            "euler",
            "fourier",
            "logistic-bifurcation",
            "gradient-field",
            "mobius-transform",
        ]
        .into_iter()
        .collect();
        let obtenidas: BTreeSet<&str> = solo_rust.into_iter().copied().collect();
        assert_eq!(obtenidas, esperadas, "divergencia 11/6 pineada");
    }
}

// ── F2b: Composición estilo Manim (Succession / Group / Wait) ─────────────
// `Playlist` es una `Succession`: steps secuenciales `{request, run_time,
// wait_after}` estilo `Succession(*anims)` de Manim. `AnimationGroup` es el
// `Group` simultáneo con `lag_ratio` estilo `LaggedStart` (solo timings de
// arranque escalonado; el compositado simultáneo de píxeles NO se hace acá,
// ver doc de `AnimationGroup`). `Wait` es silencio que congela el último
// frame: o `wait_after_ms` tras un step o un step de pausa (`request: None`).
//
// Todo puro, sin I/O, sin `unwrap` en prod. Todo lo que excede presupuestos
// (`steps ≤ 8`, `frames totales ≤ 96`, OOM por set) es `Err` honesto en
// español: jamás nada parcial en silencio.
//
// El scheduler mapea tiempo global → `(step, t_local)` reutilizando
// `Timeline::sample` (el mismo camino que `media_frame_at` del player del
// chat): `step_timeline()` arma keys en los bordes de cada step y
// `sample_at()` redondea hacia abajo; `global_timeline()` + `playlist_frame_at()`
// extienden el scrub por animación a tiempo global sin tocar la UI.

/// Tope de steps por playlist (Succession acotada, anti-OOM de cola).
pub const PLAYLIST_MAX_STEPS: usize = 8;
/// Tope de fotogramas totales del set concatenado (todas las plantillas
/// nativas dan 48; dos de 48 entran justo, tres no).
pub const PLAYLIST_MAX_FRAMES_TOTAL: usize = 96;
/// `run_time` mínimo por step animado (igual que `AnimDuration` 0.1 s).
pub const PLAYLIST_MIN_RUN_MS: u64 = 100;
/// `run_time` máximo por step animado (igual que `AnimDuration` 30 s).
pub const PLAYLIST_MAX_RUN_MS: u64 = MAX_TIMELINE_DURATION_MS;
/// Silencio máximo por espera (`wait_after` o pausa sola).
pub const PLAYLIST_MAX_WAIT_MS: u64 = 10_000;
/// Bytes por píxel RGBA (espejo de `egui::ColorImage`, solo para estimar OOM).
pub const PLAYLIST_BYTES_PER_PIXEL: usize = 4;

/// Un step de la playlist: animación + sus timings estilo Manim.
///
/// `request: None` = `Wait` silencio puro (congela el último frame, 0 frames
/// propios). `wait_after_ms` = silencio tras el step (también congelando).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistStep {
    /// `None` = pausa silenciosa (ver `PlaylistStep::pausa`).
    pub request: Option<AnimRequest>,
    /// Tiempo de corrida en ms (anim: 100..=30000; pausa: 1..=10000).
    pub run_time_ms: u64,
    /// Silencio posterior en ms (0..=10000, congela el último frame).
    pub wait_after_ms: u64,
}

impl PlaylistStep {
    /// Step animado estilo Manim (`run_time` + `wait` posterior).
    pub fn anim(
        request: AnimRequest,
        run_time_ms: u64,
        wait_after_ms: u64,
    ) -> Result<Self, ProtocolError> {
        let step = Self {
            request: Some(request),
            run_time_ms,
            wait_after_ms,
        };
        step.validate()?;
        Ok(step)
    }

    /// `Wait` silencio puro de `wait_ms` (0 frames, congela el último frame).
    pub fn pausa(wait_ms: u64) -> Result<Self, ProtocolError> {
        let step = Self {
            request: None,
            run_time_ms: wait_ms,
            wait_after_ms: 0,
        };
        step.validate()?;
        Ok(step)
    }

    /// ¿Es silencio puro (sin animación)?
    pub fn is_wait(&self) -> bool {
        self.request.is_none()
    }

    /// Duración total del step (`run + wait_after`, saturada, nunca panic).
    pub fn duration_ms(&self) -> u64 {
        self.run_time_ms.saturating_add(self.wait_after_ms)
    }

    /// Validación estricta (todo `Err` en español, sin pánicos).
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.wait_after_ms > PLAYLIST_MAX_WAIT_MS {
            return Err(ProtocolError::InvalidField {
                field: "playlist.wait_after_ms",
                reason: format!(
                    "{} excede el máximo de {PLAYLIST_MAX_WAIT_MS}",
                    self.wait_after_ms
                ),
            });
        }
        match &self.request {
            Some(request) => {
                request
                    .validate()
                    .map_err(|e| ProtocolError::InvalidField {
                        field: "playlist.request",
                        reason: e.to_string(),
                    })?;
                if !(PLAYLIST_MIN_RUN_MS..=PLAYLIST_MAX_RUN_MS).contains(&self.run_time_ms) {
                    return Err(ProtocolError::InvalidField {
                        field: "playlist.run_time_ms",
                        reason: format!(
                            "{} fuera de {PLAYLIST_MIN_RUN_MS}..={PLAYLIST_MAX_RUN_MS}",
                            self.run_time_ms
                        ),
                    });
                }
            }
            None => {
                if self.wait_after_ms != 0 {
                    return Err(ProtocolError::InvalidField {
                        field: "playlist.wait_after_ms",
                        reason: "la pausa sola no lleva espera posterior: usá otro step".into(),
                    });
                }
                if self.run_time_ms == 0 || self.run_time_ms > PLAYLIST_MAX_WAIT_MS {
                    return Err(ProtocolError::InvalidField {
                        field: "playlist.run_time_ms",
                        reason: format!(
                            "pausa de {} fuera de 1..={PLAYLIST_MAX_WAIT_MS}",
                            self.run_time_ms
                        ),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Un step ya ubicado en tiempo global (salida pura de `Playlist::schedule`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledStep {
    /// Índice en `Playlist::steps`.
    pub index: usize,
    /// Arranque global en ms.
    pub start_ms: u64,
    /// Corrida en ms (igual que el step).
    pub run_ms: u64,
    /// Silencio posterior en ms (igual que el step).
    pub wait_after_ms: u64,
    /// `true` si es pausa silenciosa.
    pub is_wait: bool,
}

impl ScheduledStep {
    /// Fin global (`start + run + wait`, saturado).
    pub fn end_ms(&self) -> u64 {
        self.start_ms
            .saturating_add(self.run_ms)
            .saturating_add(self.wait_after_ms)
    }
}

/// Playlist estilo Manim `Succession`: steps estrictamente secuenciales.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// 1..=`PLAYLIST_MAX_STEPS` steps en orden de reproducción.
    pub steps: Vec<PlaylistStep>,
}

impl Playlist {
    /// Constructor validado (todo `Err` honesto, nada parcial en silencio).
    pub fn try_new(steps: Vec<PlaylistStep>) -> Result<Self, ProtocolError> {
        if steps.is_empty() {
            return Err(ProtocolError::InvalidField {
                field: "playlist.steps",
                reason: "vacía: pasame al menos 1 step (o una pausa)".into(),
            });
        }
        if steps.len() > PLAYLIST_MAX_STEPS {
            return Err(ProtocolError::InvalidField {
                field: "playlist.steps",
                reason: format!(
                    "{} steps exceden el tope de {PLAYLIST_MAX_STEPS}: partila en dos playlists",
                    steps.len()
                ),
            });
        }
        for step in &steps {
            step.validate()?;
        }
        Ok(Self { steps })
    }

    /// Cantidad de steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// ¿Vacía? (nunca tras `try_new`, pero la deserialización puede).
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Validación estricta (para valores armados por struct literal).
    pub fn validate(&self) -> Result<(), ProtocolError> {
        Self::try_new(self.steps.clone()).map(|_| ())
    }

    /// Duración total (`Σ run + wait_after`, saturada, nunca panic).
    pub fn total_duration_ms(&self) -> u64 {
        let mut total: u64 = 0;
        for step in &self.steps {
            total = total.saturating_add(step.duration_ms());
        }
        total
    }

    /// Agenda secuencial: cada step arranca donde terminó el anterior.
    /// Pura, sin pánicos (sumas saturadas).
    pub fn schedule(&self) -> Vec<ScheduledStep> {
        let mut out = Vec::with_capacity(self.steps.len());
        let mut cursor: u64 = 0;
        for (index, step) in self.steps.iter().enumerate() {
            out.push(ScheduledStep {
                index,
                start_ms: cursor,
                run_ms: step.run_time_ms,
                wait_after_ms: step.wait_after_ms,
                is_wait: step.is_wait(),
            });
            cursor = cursor.saturating_add(step.duration_ms());
        }
        out
    }

    /// Timeline global step→índice para `Timeline::sample`.
    ///
    /// Keys en cada borde (`start → índice`) + key final (`total → último
    /// índice`): el sample interpola entre enteros y `sample_at` redondea
    /// hacia abajo. `None` si no hay steps o la duración es 0. Pura.
    pub fn step_timeline(&self) -> Option<Timeline> {
        if self.steps.is_empty() {
            return None;
        }
        let total = self.total_duration_ms();
        if total == 0 {
            return None;
        }
        let mut keyframes = Vec::with_capacity(self.steps.len().saturating_add(1));
        for item in self.schedule() {
            keyframes.push(Keyframe {
                t_ms: item.start_ms,
                value: item.index as f32,
            });
        }
        if let Some(last_index) = self.steps.len().checked_sub(1) {
            keyframes.push(Keyframe {
                t_ms: total,
                value: last_index as f32,
            });
        }
        let timeline = Timeline {
            duration_ms: total,
            keyframes,
        };
        timeline.validate().ok()?;
        Some(timeline)
    }

    /// Mapea tiempo global → `(step, t_local)` reutilizando `Timeline::sample`.
    ///
    /// El índice sale del sample (floor + clamp + verificación por
    /// intervalos, por si el redondeo flotante cae un borde afuera);
    /// `t_local` es `global - start` congelado en `run_ms` durante el
    /// `wait_after` (silencio = último frame quieto). `None` si la playlist
    /// está vacía o `global_ms` ya pasó el total (terminó). Pura, sin pánicos.
    pub fn sample_at(&self, global_ms: u64) -> Option<(usize, u64)> {
        let agenda = self.schedule();
        if agenda.is_empty() {
            return None;
        }
        let total = self.total_duration_ms();
        if global_ms >= total {
            return None;
        }
        let timeline = self.step_timeline()?;
        let value = timeline.sample(global_ms);
        let mut candidate = if value.is_finite() {
            value.floor() as usize
        } else {
            0
        };
        if candidate >= agenda.len() {
            candidate = agenda.len().saturating_sub(1);
        }
        // Verificación por intervalos (el sample puede redondear un borde).
        let inside = |item: &ScheduledStep| global_ms >= item.start_ms && global_ms < item.end_ms();
        if !inside(&agenda[candidate]) {
            let mut found = None;
            for item in &agenda {
                if inside(item) {
                    found = Some(item.index);
                    break;
                }
            }
            candidate = found.unwrap_or(0);
        }
        let item = agenda.iter().find(|item| item.index == candidate)?;
        let elapsed = global_ms.saturating_sub(item.start_ms);
        // En la espera posterior el tiempo local se congela al final del run.
        let t_local = elapsed.min(item.run_ms);
        Some((item.index, t_local))
    }

    /// Suma chequeada de fotogramas por step (`None` = desborde).
    /// Pura, con `checked_add` (nunca panic ni wrap silencioso).
    pub fn checked_total_frames(counts: &[usize]) -> Option<usize> {
        let mut total: usize = 0;
        for count in counts {
            total = total.checked_add(*count)?;
        }
        Some(total)
    }

    /// Valida el presupuesto de frames: `len` igual a steps, pausas con 0,
    /// animados con ≥1 y total ≤ `PLAYLIST_MAX_FRAMES_TOTAL`.
    /// Todo `Err` honesto (nada parcial en silencio).
    pub fn validate_frame_counts(&self, frames_per_step: &[usize]) -> Result<usize, ProtocolError> {
        if frames_per_step.len() != self.steps.len() {
            return Err(ProtocolError::InvalidField {
                field: "playlist.frames",
                reason: format!(
                    "tenés {} conteos para {} steps: pasalos 1 a 1",
                    frames_per_step.len(),
                    self.steps.len()
                ),
            });
        }
        for (index, (step, count)) in self.steps.iter().zip(frames_per_step.iter()).enumerate() {
            if step.is_wait() {
                if *count != 0 {
                    return Err(ProtocolError::InvalidField {
                        field: "playlist.frames",
                        reason: format!("el step {index} es pausa (0 frames), no {count}"),
                    });
                }
            } else if *count == 0 {
                return Err(ProtocolError::InvalidField {
                    field: "playlist.frames",
                    reason: format!("el step {index} animado necesita al menos 1 frame"),
                });
            }
        }
        let Some(total) = Self::checked_total_frames(frames_per_step) else {
            return Err(ProtocolError::InvalidField {
                field: "playlist.frames",
                reason: "la suma de frames desborda el contador: achicá los steps".into(),
            });
        };
        if total == 0 {
            return Err(ProtocolError::InvalidField {
                field: "playlist.frames",
                reason: "la playlist no tiene ningún frame: agregá un step animado".into(),
            });
        }
        if total > PLAYLIST_MAX_FRAMES_TOTAL {
            return Err(ProtocolError::InvalidField {
                field: "playlist.frames",
                reason: format!(
                    "{total} frames exceden el tope de {PLAYLIST_MAX_FRAMES_TOTAL}: sacá un step o bajá los frames por step"
                ),
            });
        }
        Ok(total)
    }

    /// Timeline global de scrub para `frames_per_step` (extiende el scrub por
    /// animación a tiempo global sin tocar la UI).
    ///
    /// Dos keys lineales `0 → 0.0` y `total → total_frames-1`, igual que
    /// `media_scrub_timeline`: el player existente mapea fracción → `t_ms` →
    /// `playlist_frame_at` (`Timeline::sample` + round + clamp). Valida el
    /// presupuesto de frames antes de armar (todo `Err` honesto).
    pub fn global_timeline(&self, frames_per_step: &[usize]) -> Result<Timeline, ProtocolError> {
        let total_frames = self.validate_frame_counts(frames_per_step)?;
        let total_ms = self.total_duration_ms();
        if total_ms == 0 {
            return Err(ProtocolError::InvalidField {
                field: "playlist.duration_ms",
                reason: "duración total 0: revisá los run_time".into(),
            });
        }
        let last = total_frames.saturating_sub(1) as f32;
        let timeline = Timeline {
            duration_ms: total_ms,
            keyframes: vec![
                Keyframe {
                    t_ms: 0,
                    value: 0.0,
                },
                Keyframe {
                    t_ms: total_ms,
                    value: last,
                },
            ],
        };
        timeline
            .validate()
            .map_err(|e| ProtocolError::InvalidField {
                field: "playlist.timeline",
                reason: e.to_string(),
            })?;
        Ok(timeline)
    }

    /// Estima los bytes RGBA del set concatenado (`w*h*4*total`). `None` si
    /// desborda (`checked`, sin pánicos). Puro, sin allocs.
    pub fn estimate_set_bytes(w: usize, h: usize, total_frames: usize) -> Option<usize> {
        w.checked_mul(h)
            .and_then(|v| v.checked_mul(PLAYLIST_BYTES_PER_PIXEL))
            .and_then(|v| v.checked_mul(total_frames))
    }
}

/// Índice global a mostrar en `t_ms` vía `Timeline::sample` (scrub total).
///
/// Lerp + round + clamp a `0..total`: mismo camino que `media_frame_at`, pero
/// sobre el timeline global de la playlist. Timeline vacío o `total == 0` →
/// 0. Pura, sin pánicos.
pub fn playlist_frame_at(timeline: &Timeline, t_ms: u64, total_frames: usize) -> usize {
    if total_frames == 0 {
        return 0;
    }
    let value = timeline.sample(t_ms);
    if !value.is_finite() {
        return 0;
    }
    (value.round() as usize).min(total_frames.saturating_sub(1))
}

/// `Group` simultáneo estilo Manim (`LaggedStart`): arranque escalonado +
/// composición alfa simultánea (M4).
///
/// Dos caras del mismo grupo:
/// - timings: `lag_ratio` 0..=1 desplaza cada sub-animación
///   `lag_ratio * run_ms` tras la anterior (`0` = todo junto, `1` = una tras
///   otra); el runner FIFO los ejecuta en orden con estos offsets
///   (`start_offsets_ms` / `span_ms`);
/// - píxeles: `composicion_esperada` valida que los sets referenciados tengan
///   el MISMO N y el MISMO viewport (si difieren → `Err` honesto, jamás
///   reescaleo silencioso) y `mezclar_pixel_alfa` es el over con el que
///   `grafito-app/src/anim_native.rs::componer_grupo_nativo` los fusiona
///   frame a frame (`sets[0]` fondo → último frente).
///
///  refiere a posiciones de la playlist (validar con
/// `validate_for_playlist`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationGroup {
    /// Posiciones de la playlist que arrancan escalonadas (2..=8, únicas).
    pub indices: Vec<usize>,
    /// Desfase entre arranques como fracción del run (0..=1, finito).
    pub lag_ratio: f32,
}

impl AnimationGroup {
    /// Constructor validado (`indices` contra el tope grueso 8; lo exacto va
    /// en `validate_for_playlist`). Todo `Err` honesto.
    pub fn try_new(indices: Vec<usize>, lag_ratio: f32) -> Result<Self, ProtocolError> {
        if !lag_ratio.is_finite() || !(0.0..=1.0).contains(&lag_ratio) {
            return Err(ProtocolError::InvalidField {
                field: "group.lag_ratio",
                reason: format!("{lag_ratio} fuera de 0..=1: 0 es todo junto, 1 es uno tras otro"),
            });
        }
        if indices.len() < 2 {
            return Err(ProtocolError::InvalidField {
                field: "group.indices",
                reason: "el grupo necesita al menos 2 animaciones (para 1 sola usá la playlist)"
                    .into(),
            });
        }
        if indices.len() > PLAYLIST_MAX_STEPS {
            return Err(ProtocolError::InvalidField {
                field: "group.indices",
                reason: format!(
                    "{} animaciones exceden el tope de {PLAYLIST_MAX_STEPS}",
                    indices.len()
                ),
            });
        }
        let mut ordenados = indices.clone();
        ordenados.sort_unstable();
        ordenados.dedup();
        if ordenados.len() != indices.len() {
            return Err(ProtocolError::InvalidField {
                field: "group.indices",
                reason: "hay índices repetidos: cada animación entra una sola vez".into(),
            });
        }
        for index in &indices {
            if *index >= PLAYLIST_MAX_STEPS {
                return Err(ProtocolError::InvalidField {
                    field: "group.indices",
                    reason: format!("índice {index} fuera de 0..{PLAYLIST_MAX_STEPS}"),
                });
            }
        }
        Ok(Self { indices, lag_ratio })
    }

    /// Valida los índices contra una playlist concreta (`len` real).
    pub fn validate_for_playlist(&self, playlist_len: usize) -> Result<(), ProtocolError> {
        for index in &self.indices {
            if *index >= playlist_len {
                return Err(ProtocolError::InvalidField {
                    field: "group.indices",
                    reason: format!("índice {index} fuera de la playlist de {playlist_len} steps"),
                });
            }
        }
        Ok(())
    }

    /// Offsets de arranque en ms para un `run_ms` dado.
    ///
    /// `offset[i] = round(lag_ratio * run_ms * i)` (el primero siempre 0).
    /// Puro, sin pánicos (f64 finito + saturación a `u64`).
    pub fn start_offsets_ms(&self, run_ms: u64) -> Vec<u64> {
        let lag = f64::from(self.lag_ratio);
        let run = run_ms as f64;
        self.indices
            .iter()
            .enumerate()
            .map(|(orden, _)| {
                let offset = lag * run * (orden as f64);
                if !offset.is_finite() || offset <= 0.0 {
                    0
                } else if offset >= u64::MAX as f64 {
                    u64::MAX
                } else {
                    offset.round() as u64
                }
            })
            .collect()
    }

    /// Extensión total del grupo (`run + lag*run*(n-1)`, saturada).
    pub fn span_ms(&self, run_ms: u64) -> u64 {
        let lag = f64::from(self.lag_ratio);
        let extra = lag * (run_ms as f64) * ((self.indices.len().saturating_sub(1)) as f64);
        let extra = if !extra.is_finite() || extra <= 0.0 {
            0
        } else if extra >= u64::MAX as f64 {
            u64::MAX
        } else {
            extra.round() as u64
        };
        run_ms.saturating_add(extra)
    }

    /// Valida la composición simultánea de los sets que el grupo referencia
    /// (M4, cara píxeles del `Group`).
    ///
    /// `conteos` = frames por step de la playlist y `tamanos` = `(w, h)` por
    /// step, ambos indexados por posición de playlist. Exige: mismos largos
    /// de arreglo, índices dentro, al menos 2 sets referenciados, todos con
    /// el MISMO N (> 0) y el MISMO viewport (lados 1..=4096); el set
    /// compuesto debe entrar en `GROUP_MAX_SET_BYTES`. Si algo difiere →
    /// `Err` honesto en español (jamás reescaleo silencioso). Devuelve
    /// `(N, (w, h))` del compuesto. Puro, sin pánicos.
    pub fn composicion_esperada(
        &self,
        conteos: &[usize],
        tamanos: &[(usize, usize)],
    ) -> Result<(usize, (usize, usize)), ProtocolError> {
        if conteos.len() != tamanos.len() {
            return Err(ProtocolError::InvalidField {
                field: "group.composicion",
                reason: format!(
                    "conteos ({}) y tamaños ({}) desparejos: pasalos por posición de playlist",
                    conteos.len(),
                    tamanos.len()
                ),
            });
        }
        self.validate_for_playlist(conteos.len())?;
        if self.indices.len() < 2 {
            return Err(ProtocolError::InvalidField {
                field: "group.indices",
                reason: "el compuesto necesita al menos 2 animaciones".into(),
            });
        }
        let primero = self.indices[0];
        let n0 = match conteos.get(primero) {
            Some(n) => *n,
            None => {
                return Err(ProtocolError::InvalidField {
                    field: "group.indices",
                    reason: format!(
                        "índice {primero} fuera de la playlist de {} steps",
                        conteos.len()
                    ),
                });
            }
        };
        if n0 == 0 {
            return Err(ProtocolError::InvalidField {
                field: "group.frames",
                reason: format!("el set {primero} está vacío: sin frames no hay qué componer"),
            });
        }
        let t0 = match tamanos.get(primero) {
            Some(t) => *t,
            None => {
                return Err(ProtocolError::InvalidField {
                    field: "group.indices",
                    reason: format!(
                        "índice {primero} fuera de la playlist de {} steps",
                        conteos.len()
                    ),
                });
            }
        };
        valida_lado_compuesto(t0)?;
        for index in &self.indices {
            let n = conteos.get(*index).copied().unwrap_or(0);
            if n != n0 {
                return Err(ProtocolError::InvalidField {
                    field: "group.frames",
                    reason: format!(
                        "el set {index} trae {n} frames y el grupo pide {n0}: igualá N (sin reescaleo silencioso)"
                    ),
                });
            }
            let t = tamanos.get(*index).copied().unwrap_or((0, 0));
            valida_lado_compuesto(t)?;
            if t != t0 {
                return Err(ProtocolError::InvalidField {
                    field: "group.viewport",
                    reason: format!(
                        "el set {index} mide {}x{} y el grupo pide {}x{}: igualá el viewport (sin reescaleo silencioso)",
                        t.0, t.1, t0.0, t0.1
                    ),
                });
            }
        }
        match t0
            .0
            .checked_mul(t0.1)
            .and_then(|v| v.checked_mul(PLAYLIST_BYTES_PER_PIXEL))
            .and_then(|v| v.checked_mul(n0))
        {
            Some(got) if got <= GROUP_MAX_SET_BYTES => Ok((n0, t0)),
            other => Err(ProtocolError::InvalidField {
                field: "group.presupuesto",
                reason: match other {
                    Some(got) => format!(
                        "el compuesto estimado ({got} bytes) excede el tope de {GROUP_MAX_SET_BYTES}: bajá resolución o fotogramas"
                    ),
                    None => format!(
                        "el compuesto estimado desborda el contador (tope {GROUP_MAX_SET_BYTES}): bajá resolución o fotogramas"
                    ),
                },
            }),
        }
    }
}

/// Tope del set compuesto en RAM (M4, paridad con `PARAMETRIC_MAX_BYTES` y
/// `NATIVE_MAX_SET_BYTES`): 64 MiB. El compuesto tiene N frames (no N×sets),
/// pero cada capa suma I/O de lectura; la cota cubre el peor caso honesto.
pub const GROUP_MAX_SET_BYTES: usize = 64 * 1024 * 1024;

/// Valida un lado del compuesto (1..=4096 por lado, paridad con `Resolution`).
fn valida_lado_compuesto(t: (usize, usize)) -> Result<(), ProtocolError> {
    if t.0 == 0 || t.1 == 0 || t.0 > 4096 || t.1 > 4096 {
        return Err(ProtocolError::InvalidCanvas(format!(
            "viewport {}x{} fuera de 1..=4096 para componer",
            t.0, t.1
        )));
    }
    Ok(())
}

/// Mezcla alfa `frente` sobre `fondo` (Porter-Duff over en alfa directo).
///
/// Canales `[R, G, B, A]` 0..=255. `frente` opaco tapa, `frente`
/// transparente conserva el fondo, a medias promedia honesto. Puro, sin
/// pánicos (todo f64 finito + round + clamp).
pub fn mezclar_pixel_alfa(fondo: [u8; 4], frente: [u8; 4]) -> [u8; 4] {
    let af = f64::from(frente[3]) / 255.0;
    let ab = f64::from(fondo[3]) / 255.0;
    if !af.is_finite() || !ab.is_finite() {
        return fondo;
    }
    let ao = af + ab * (1.0 - af);
    if !ao.is_finite() || ao <= 0.0 {
        return [0, 0, 0, 0];
    }
    let mut out = [0_u8; 4];
    for k in 0..3 {
        let cf = f64::from(frente[k]);
        let cb = f64::from(fondo[k]);
        let v = (cf * af + cb * ab * (1.0 - af)) / ao;
        out[k] = if v.is_finite() {
            v.round().clamp(0.0, 255.0) as u8
        } else {
            fondo[k]
        };
    }
    let a = ao * 255.0;
    out[3] = if a.is_finite() {
        a.round().clamp(0.0, 255.0) as u8
    } else {
        fondo[3]
    };
    out
}

/// Timings estilo Manim (`run_time` + `wait` por animación).
///
/// `items`: `(request, run_secs, wait_after_secs)` con `run` 0.1..=30 y
/// `wait` 0..=10 (finitos). Convierte a ms con round y arma la `Succession`
/// validada (1..=8 steps, cada request validado). Todo `Err` honesto.
pub fn build_animations_with_timings(
    items: Vec<(AnimRequest, f64, f64)>,
) -> Result<Playlist, ProtocolError> {
    if items.is_empty() {
        return Err(ProtocolError::InvalidField {
            field: "playlist.steps",
            reason: "vacía: pasame al menos 1 animación con su timing".into(),
        });
    }
    if items.len() > PLAYLIST_MAX_STEPS {
        return Err(ProtocolError::InvalidField {
            field: "playlist.steps",
            reason: format!(
                "{} animaciones exceden el tope de {PLAYLIST_MAX_STEPS}",
                items.len()
            ),
        });
    }
    let mut steps = Vec::with_capacity(items.len());
    for (orden, (request, run_s, wait_s)) in items.into_iter().enumerate() {
        if !run_s.is_finite() || !(0.1..=30.0).contains(&run_s) {
            return Err(ProtocolError::InvalidField {
                field: "playlist.run_time_ms",
                reason: format!("el step {orden} pide run_time {run_s}s (válido 0.1..=30)"),
            });
        }
        if !wait_s.is_finite() || !(0.0..=10.0).contains(&wait_s) {
            return Err(ProtocolError::InvalidField {
                field: "playlist.wait_after_ms",
                reason: format!("el step {orden} pide espera {wait_s}s (válido 0..=10)"),
            });
        }
        let run_ms = (run_s * 1000.0).round() as u64;
        let wait_ms = (wait_s * 1000.0).round() as u64;
        steps.push(PlaylistStep::anim(request, run_ms, wait_ms)?);
    }
    Playlist::try_new(steps)
}

/// `Succession` con defaults honestos: cada request corre su `duration_ms`
/// (0 = compat → 2000 ms) y sin espera posterior.
pub fn build_succession(requests: Vec<AnimRequest>) -> Result<Playlist, ProtocolError> {
    if requests.is_empty() {
        return Err(ProtocolError::InvalidField {
            field: "playlist.steps",
            reason: "vacía: pasame al menos 1 animación".into(),
        });
    }
    if requests.len() > PLAYLIST_MAX_STEPS {
        return Err(ProtocolError::InvalidField {
            field: "playlist.steps",
            reason: format!(
                "{} animaciones exceden el tope de {PLAYLIST_MAX_STEPS}",
                requests.len()
            ),
        });
    }
    let mut steps = Vec::with_capacity(requests.len());
    for request in requests {
        let run_ms = if request.duration_ms == 0 {
            2000
        } else {
            request.duration_ms
        };
        steps.push(PlaylistStep::anim(request, run_ms, 0)?);
    }
    Playlist::try_new(steps)
}

#[cfg(test)]
mod playlist_f2b_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn pedido(template: &str, concept: &str) -> AnimRequest {
        AnimRequest {
            template: template.to_string(),
            concept: concept.to_string(),
            params: BTreeMap::new(),
            spec: None,
            export: ExportFormat::Gif,
            canvas: (640, 480),
            duration_ms: 2000,
        }
    }

    #[test]
    fn succession_secuencial_agenda_y_mapea_tiempo_global() {
        let lista = build_animations_with_timings(vec![
            (pedido("derivative-slope", "derivada"), 2.0, 0.5),
            (pedido("integral-area", "integral"), 1.0, 0.0),
        ])
        .unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!(lista.total_duration_ms(), 3500);
        let agenda = lista.schedule();
        assert_eq!(agenda[0].start_ms, 0);
        assert_eq!(agenda[1].start_ms, 2500);
        // Scheduler: 0..2000 step 0 run, 2000..2500 step 0 espera (t_local
        // congelado en 2000), 2500..3500 step 1, 3500 fin.
        assert_eq!(lista.sample_at(0), Some((0, 0)));
        assert_eq!(lista.sample_at(1500), Some((0, 1500)));
        assert_eq!(lista.sample_at(2200), Some((0, 2000)));
        assert_eq!(lista.sample_at(2500), Some((1, 0)));
        assert_eq!(lista.sample_at(3499), Some((1, 999)));
        assert_eq!(lista.sample_at(3500), None);
        assert_eq!(lista.sample_at(99_999), None);
    }

    #[test]
    fn wait_silencio_congela_sin_frames_propios() {
        let lista = Playlist::try_new(vec![
            PlaylistStep::anim(pedido("derivative-slope", "derivada"), 1000, 500).unwrap(),
            PlaylistStep::pausa(1000).unwrap(),
        ])
        .unwrap();
        assert_eq!(lista.total_duration_ms(), 2500);
        // La espera posterior congela t_local al final del run.
        assert_eq!(lista.sample_at(1200), Some((0, 1000)));
        // La pausa sola ocupa su intervalo con t_local = espera vivida.
        assert_eq!(lista.sample_at(1500), Some((1, 0)));
        assert_eq!(lista.sample_at(2000), Some((1, 500)));
        assert_eq!(lista.sample_at(2500), None);
        // Presupuesto de frames: la pausa aporta 0.
        assert_eq!(lista.validate_frame_counts(&[48, 0]).unwrap(), 48);
        assert!(lista.validate_frame_counts(&[48, 1]).is_err());
    }

    #[test]
    fn scrub_global_reutiliza_timeline_sample() {
        let lista = build_succession(vec![
            pedido("derivative-slope", "derivada"),
            pedido("integral-area", "integral"),
        ])
        .unwrap();
        let timeline = lista.global_timeline(&[48, 48]).unwrap();
        assert_eq!(timeline.duration_ms, 4000);
        // Mismo camino que el player: sample + round + clamp.
        assert_eq!(playlist_frame_at(&timeline, 0, 96), 0);
        assert_eq!(playlist_frame_at(&timeline, 4000, 96), 95);
        assert_eq!(playlist_frame_at(&timeline, 2000, 96), 48);
        assert_eq!(playlist_frame_at(&timeline, 99_999, 96), 95);
        assert_eq!(playlist_frame_at(&timeline, 0, 0), 0);
    }

    #[test]
    fn presupuestos_fallan_honesto_nada_parcial() {
        // 0 steps.
        assert!(Playlist::try_new(vec![]).is_err());
        // 9 steps (>8).
        let nueve: Vec<PlaylistStep> = (0..9)
            .map(|_| PlaylistStep::anim(pedido("derivative-slope", "d"), 1000, 0).unwrap())
            .collect();
        let err = Playlist::try_new(nueve).unwrap_err().to_string();
        assert!(err.contains("8"), "tope 8 en el mensaje, got: {err}");
        // 97 frames (>96).
        let lista = build_succession(vec![
            pedido("derivative-slope", "d"),
            pedido("integral-area", "i"),
        ])
        .unwrap();
        let err = lista
            .validate_frame_counts(&[48, 49])
            .unwrap_err()
            .to_string();
        assert!(err.contains("96"), "tope 96 en el mensaje, got: {err}");
        // Conteos desparejos o step animado sin frames.
        assert!(lista.validate_frame_counts(&[48]).is_err());
        assert!(lista.validate_frame_counts(&[0, 48]).is_err());
        // run_time fuera de rango y espera gigante.
        assert!(PlaylistStep::anim(pedido("derivative-slope", "d"), 50, 0).is_err());
        assert!(PlaylistStep::anim(pedido("derivative-slope", "d"), 31_000, 0).is_err());
        assert!(PlaylistStep::anim(pedido("derivative-slope", "d"), 1000, 99_999).is_err());
        assert!(PlaylistStep::pausa(0).is_err());
        assert!(PlaylistStep::pausa(99_999).is_err());
        // Timings no finitos o fuera de rango.
        assert!(build_animations_with_timings(vec![(
            pedido("derivative-slope", "d"),
            f64::NAN,
            0.0
        )])
        .is_err());
        assert!(build_animations_with_timings(vec![(
            pedido("derivative-slope", "d"),
            2.0,
            f64::INFINITY
        )])
        .is_err());
        assert!(build_succession(vec![]).is_err());
    }

    #[test]
    fn group_lag_ratio_escalona_arranques() {
        let grupo = AnimationGroup::try_new(vec![0, 1, 2], 0.5).unwrap();
        assert_eq!(grupo.start_offsets_ms(2000), vec![0, 1000, 2000]);
        assert_eq!(grupo.span_ms(2000), 4000);
        // Extremos honestos: 0 = todo junto, 1 = uno tras otro.
        assert_eq!(
            AnimationGroup::try_new(vec![0, 1], 0.0)
                .unwrap()
                .start_offsets_ms(2000),
            vec![0, 0]
        );
        assert_eq!(
            AnimationGroup::try_new(vec![0, 1], 1.0)
                .unwrap()
                .start_offsets_ms(2000),
            vec![0, 2000]
        );
        grupo.validate_for_playlist(3).unwrap();
        assert!(grupo.validate_for_playlist(2).is_err());
        // Malformados: lag fuera de rango, repetidos, de a 1.
        assert!(AnimationGroup::try_new(vec![0, 1], 1.5).is_err());
        assert!(AnimationGroup::try_new(vec![0, 1], f32::NAN).is_err());
        assert!(AnimationGroup::try_new(vec![0, 0], 0.5).is_err());
        assert!(AnimationGroup::try_new(vec![0], 0.5).is_err());
        assert!(AnimationGroup::try_new(vec![0, 99], 0.5).is_err());
    }

    #[test]
    fn oom_por_set_con_checked_sin_panic() {
        assert_eq!(
            Playlist::estimate_set_bytes(640, 480, 96),
            Some(640 * 480 * 4 * 96)
        );
        assert_eq!(Playlist::estimate_set_bytes(usize::MAX, 480, 96), None);
        assert_eq!(Playlist::checked_total_frames(&[48, 48]), Some(96));
        assert_eq!(
            Playlist::checked_total_frames(&[usize::MAX, 1]),
            None,
            "desborde honesto, no wrap"
        );
    }
}

// ── M4: Group simultáneo real (solo tests, sin tocar prod) ───────────────
#[cfg(test)]
mod group_compose_m4_tests {
    use super::*;

    #[test]
    fn composicion_exige_mismo_n_y_viewport() {
        let grupo = AnimationGroup::try_new(vec![0, 1], 0.0).unwrap();
        // Caso feliz: mismo N y mismo viewport.
        assert_eq!(
            grupo.composicion_esperada(&[48, 48], &[(64, 48), (64, 48)]),
            Ok((48, (64, 48)))
        );
        // N distinto → Err honesto (sin reescaleo silencioso).
        let err = grupo
            .composicion_esperada(&[48, 24], &[(64, 48), (64, 48)])
            .unwrap_err()
            .to_string();
        assert!(err.contains("24") && err.contains("48"), "got: {err}");
        // Viewport distinto → Err honesto.
        let err = grupo
            .composicion_esperada(&[48, 48], &[(64, 48), (96, 72)])
            .unwrap_err()
            .to_string();
        assert!(err.contains("viewport"), "got: {err}");
        // Set vacío, arreglos desparejos e índice fuera.
        assert!(grupo
            .composicion_esperada(&[0, 0], &[(64, 48), (64, 48)])
            .is_err());
        assert!(grupo.composicion_esperada(&[48, 48], &[(64, 48)]).is_err());
        assert!(grupo.composicion_esperada(&[48], &[(64, 48)]).is_err());
        // Lado fuera de 1..=4096 y presupuesto excedido.
        assert!(grupo
            .composicion_esperada(&[48, 48], &[(0, 48), (0, 48)])
            .is_err());
        assert!(grupo
            .composicion_esperada(&[48, 48], &[(4096, 4096), (4096, 4096)])
            .is_err());
    }

    #[test]
    fn mezcla_alfa_over_correcta() {
        // Frente opaco tapa.
        assert_eq!(
            mezclar_pixel_alfa([0, 0, 255, 255], [255, 0, 0, 255]),
            [255, 0, 0, 255]
        );
        // Frente transparente conserva el fondo.
        assert_eq!(
            mezclar_pixel_alfa([0, 0, 255, 255], [255, 0, 0, 0]),
            [0, 0, 255, 255]
        );
        // Nada sobre nada = transparente.
        assert_eq!(mezclar_pixel_alfa([0, 0, 0, 0], [0, 0, 0, 0]), [0, 0, 0, 0]);
        // SPEC R1-10: rojo 50% sobre azul opaco da [128,0,127,255] exacto en
        // aritmética real; el ±1 es la precisión documentada del `round()`
        // f64 (no calibración del bug): 128/255 no es exacto en binario.
        let m = mezclar_pixel_alfa([0, 0, 255, 255], [255, 0, 0, 128]);
        assert_eq!(m[3], 255);
        assert!((m[0] as i16 - 128).abs() <= 1, "r={}", m[0]);
        assert_eq!(m[1], 0);
        assert!((m[2] as i16 - 127).abs() <= 1, "b={}", m[2]);
    }
}
