//! Tipos versionados y transportables para el asistente seguro de Grafito.
//!
//! Este crate no conoce red, almacenamiento ni el modelo de documento. Sus
//! valores se pueden serializar para una vista previa o para un proveedor que
//! el usuario haya habilitado explícitamente.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versión de las estructuras públicas del asistente.
pub const ASSISTANT_SCHEMA_VERSION: u32 = 1;

/// Máximo de operaciones tipadas que puede incluir una propuesta del asistente.
pub const MAX_PROPOSED_PLAN_OPERATIONS: usize = 8;
/// Version de los receipts locales de staging de propuestas.
pub const ASSISTANT_PLAN_RECEIPT_SCHEMA_VERSION: u32 = 1;
/// Version de la politica local usada para generar un receipt.
pub const ASSISTANT_PLAN_RECEIPT_POLICY_VERSION: u32 = 1;
/// Máximo de turnos textuales que se reenvían como contexto conversacional.
pub const MAX_CONVERSATION_TURNS: usize = 6;
/// Máximo de caracteres de un turno conservado en memoria.
pub const MAX_CONVERSATION_TURN_CHARS: usize = 4_096;
/// Máximo de caracteres del resumen del objeto enfocado.
pub const MAX_FOCUS_SUMMARY_CHARS: usize = 4_096;
/// Encabezado conservador que el transporte añade antes de un objeto enfocado.
pub const REMOTE_FOCUS_PROMPT_PREFIX: &str =
    "\n\nFocused object (use this unless the user asks otherwise):\n";
/// Bytes reservados para el encabezado remoto de un objeto enfocado.
pub const REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES: usize = REMOTE_FOCUS_PROMPT_PREFIX.len();
/// Encabezado que el transporte añade antes del catálogo de herramientas relevante.
pub const REMOTE_TOOL_CATALOG_PROMPT_PREFIX: &str =
    "\n\nRelevant Grafito tools (use only these exact signatures when applicable):\n";
/// Bytes reservados para el encabezado remoto del catálogo de herramientas.
pub const REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES: usize =
    REMOTE_TOOL_CATALOG_PROMPT_PREFIX.len();
/// Encabezado que el transporte añade al diagnóstico local de una propuesta remota.
pub const REMOTE_REPAIR_FEEDBACK_PROMPT_PREFIX: &str =
    "\n\nLocal proposal verification feedback (repair once; use only executable catalog syntax):\n";
/// Bytes reservados para el encabezado de diagnóstico de reparación.
pub const REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES: usize =
    REMOTE_REPAIR_FEEDBACK_PROMPT_PREFIX.len();
/// Máximo de propuestas rechazadas que se devuelven al proveedor para una reparación.
pub const MAX_ASSISTANT_REPAIR_FAILURES: usize = 4;
/// Límite de bytes del diagnóstico local enviado durante una única reparación.
pub const MAX_ASSISTANT_REPAIR_FEEDBACK_BYTES: usize = 768;

/// Motivo local y seguro por el cual una propuesta del proveedor no pudo verificarse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantRepairFailureKind {
    InvalidSyntax,
    UnsupportedCommand,
    InvalidArity,
    CommandRejected,
    NoNewObject,
    WrongRenderSpace,
    NotVisible,
}

impl AssistantRepairFailureKind {
    /// Explicación estable para el prompt remoto, sin exponer errores internos.
    pub const fn prompt_label(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "the command syntax is incomplete or malformed",
            Self::UnsupportedCommand => "the command is not executable in Grafito",
            Self::InvalidArity => "the command argument count is invalid",
            Self::CommandRejected => "Grafito rejected one or more command values",
            Self::NoNewObject => "the command did not create a graph object",
            Self::WrongRenderSpace => "the command created an object in the wrong graph view",
            Self::NotVisible => "the staged result did not produce visible geometry",
        }
    }
}

/// Un único rechazo local que puede ayudar al proveedor a reparar una propuesta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantRepairFailure {
    /// Identificador de comando saneado, no el texto completo devuelto por el proveedor.
    pub command: String,
    /// Clasificación local estable del rechazo.
    pub kind: AssistantRepairFailureKind,
    /// Firmas de referencia derivadas localmente del registro de comandos.
    pub expected_syntax: Vec<String>,
}

impl AssistantRepairFailure {
    /// Valida el subconjunto seguro que puede serializarse para una reparación.
    pub fn validate(&self) -> Result<(), String> {
        if self.command.is_empty()
            || self.command.len() > 64
            || !self
                .command
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err("assistant repair failure command identifier is invalid".into());
        }
        if self.expected_syntax.len() > 3 {
            return Err("assistant repair failure has too many expected signatures".into());
        }
        if self.expected_syntax.iter().any(|syntax| {
            syntax.is_empty()
                || syntax.len() > 256
                || !syntax.chars().all(|character| {
                    (character.is_ascii_graphic() || character == ' ') && character != '`'
                })
        }) {
            return Err("assistant repair failure syntax is invalid".into());
        }
        Ok(())
    }

    fn prompt_line(&self) -> String {
        let mut line = format!("- `{}`: {}.", self.command, self.kind.prompt_label());
        if !self.expected_syntax.is_empty() {
            line.push_str(" Expected syntax: ");
            for (index, syntax) in self.expected_syntax.iter().enumerate() {
                if index > 0 {
                    line.push_str(" or ");
                }
                line.push('`');
                line.push_str(syntax);
                line.push('`');
            }
            line.push('.');
        }
        line
    }
}

/// Diagnóstico local acotado para una única reparación remota de propuestas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantRepairFeedback {
    pub failures: Vec<AssistantRepairFailure>,
}

impl AssistantRepairFeedback {
    /// Valida tamaño, estructura y contenido seguro antes de construir el prompt remoto.
    pub fn validate(&self) -> Result<(), String> {
        if self.failures.is_empty() || self.failures.len() > MAX_ASSISTANT_REPAIR_FAILURES {
            return Err("assistant repair feedback count is outside the allowed range".into());
        }
        let mut bytes = 0_usize;
        for failure in &self.failures {
            failure.validate()?;
            bytes = bytes
                .checked_add(failure.prompt_line().len())
                .and_then(|total| total.checked_add(1))
                .ok_or_else(|| "assistant repair feedback budget overflow".to_string())?;
        }
        if bytes > MAX_ASSISTANT_REPAIR_FEEDBACK_BYTES {
            return Err("assistant repair feedback exceeds the allowed size".into());
        }
        Ok(())
    }

    /// Texto seguro que el transporte añade después del catálogo de herramientas.
    pub fn prompt_text(&self) -> String {
        self.failures
            .iter()
            .map(AssistantRepairFailure::prompt_line)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Política de privacidad de una solicitud del asistente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// No se permite enviar contenido fuera del proceso local.
    #[default]
    LocalOnly,
    /// Un proveedor remoto puede usarse sólo después de una acción explícita.
    RemoteAllowed,
}

/// Límites acotados por solicitud, válidos tanto para ejecución local como remota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestBudget {
    /// Máximo de bytes UTF-8 de consulta, foco e historial enviados al proveedor.
    pub max_input_chars: usize,
    /// Máximo de caracteres que puede producir el proveedor remoto.
    pub max_output_chars: usize,
    /// Máximo de pasos que se muestran como derivación.
    pub max_steps: usize,
    /// Límite de espera de una operación remota en milisegundos.
    pub timeout_ms: u64,
}

impl Default for RequestBudget {
    fn default() -> Self {
        Self {
            max_input_chars: 4_096,
            max_output_chars: 4_096,
            max_steps: 24,
            timeout_ms: 15_000,
        }
    }
}

impl RequestBudget {
    /// Verifica que los límites sean finitos y suficientemente pequeños para el MVP.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_input_chars == 0 || self.max_input_chars > 32_768 {
            return Err("assistant input budget is outside the allowed range".into());
        }
        if self.max_output_chars == 0 || self.max_output_chars > 32_768 {
            return Err("assistant output budget is outside the allowed range".into());
        }
        if self.max_steps == 0 || self.max_steps > 128 {
            return Err("assistant derivation step budget is outside the allowed range".into());
        }
        if !(100..=120_000).contains(&self.timeout_ms) {
            return Err("assistant timeout is outside the allowed range".into());
        }
        Ok(())
    }
}

/// Límites de seguridad para adjuntos de imagen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLimits {
    /// Tamaño máximo de un adjunto codificado, sin nombre ni ruta de origen.
    pub max_bytes: usize,
    /// Cantidad máxima de píxeles decodificados por una imagen.
    pub max_pixels: u64,
    /// Cantidad máxima de imágenes de una solicitud.
    pub max_attachments: usize,
    /// Presupuesto total de bytes de payload de todos los adjuntos.
    pub max_total_bytes: usize,
    /// Presupuesto total de píxeles decodificados de todos los adjuntos.
    pub max_total_pixels: u64,
}

impl Default for AttachmentLimits {
    fn default() -> Self {
        Self {
            max_bytes: 5 * 1024 * 1024,
            max_pixels: 20_000_000,
            max_attachments: 3,
            max_total_bytes: 8 * 1024 * 1024,
            max_total_pixels: 20_000_000,
        }
    }
}

/// Procedencia declarada de una transcripción editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionSource {
    /// El usuario escribió el texto directamente.
    #[default]
    Manual,
    /// El usuario corrigió una transcripción previa.
    UserEdited,
    /// Un proveedor remoto con visión propuso un texto que requiere revisión.
    ProviderVision,
}

/// Texto asociado a una imagen, siempre editable antes de resolverlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EditableTranscription {
    /// Texto que el usuario puede revisar y modificar.
    pub text: String,
    /// Cómo se obtuvo inicialmente el texto.
    pub source: TranscriptionSource,
    /// Indica si el usuario confirmó el contenido actual.
    pub reviewed: bool,
}

impl EditableTranscription {
    /// Guarda una edición del usuario y exige una nueva revisión explícita.
    pub fn edit(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.source = TranscriptionSource::UserEdited;
        self.reviewed = false;
    }
}

/// Adjunto de imagen sin ruta ni nombre de archivo del sistema local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageAttachment {
    /// Tipo MIME declarado; sólo se aceptan formatos ráster permitidos.
    pub media_type: String,
    /// Codificación importada que el transporte debe decodificar y volver a
    /// codificar, sin metadatos, antes de enviarla a un proveedor con visión.
    pub bytes: Vec<u8>,
    /// Ancho declarado por el importador de UI.
    pub pixel_width: u32,
    /// Alto declarado por el importador de UI.
    pub pixel_height: u32,
    /// Transcripción que se puede editar sin reenviar la imagen.
    pub transcription: EditableTranscription,
}

impl ImageAttachment {
    /// Construye un adjunto sin ningún nombre o ruta de archivo.
    pub fn new(
        media_type: impl Into<String>,
        bytes: Vec<u8>,
        pixel_width: u32,
        pixel_height: u32,
    ) -> Self {
        Self {
            media_type: media_type.into(),
            bytes,
            pixel_width,
            pixel_height,
            transcription: EditableTranscription::default(),
        }
    }

    /// Comprueba los límites que se pueden validar sin decodificar la imagen.
    ///
    /// El crate de transporte debe comprobar el formato y dimensiones reales y
    /// volver a codificar sólo los píxeles antes de enviar bytes a un proveedor.
    pub fn validate(&self, limits: &AttachmentLimits) -> Result<(), String> {
        if !matches!(self.media_type.as_str(), "image/png" | "image/jpeg") {
            return Err("assistant attachment media type is not allowed".into());
        }
        if self.bytes.is_empty() || self.bytes.len() > limits.max_bytes {
            return Err("assistant attachment byte limit exceeded".into());
        }
        Ok(())
    }
}

/// Capacidades declaradas por un proveedor o por un modelo remoto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderCapabilities {
    /// El proveedor acepta peticiones estilo OpenAI Chat Completions.
    pub openai_compatible: bool,
    /// El proveedor acepta bloques de imagen en las peticiones.
    pub vision: bool,
    /// El proveedor puede emitir eventos incrementales.
    pub streaming: bool,
}

/// Perfil remoto predefinido, sin incluir credenciales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProfile {
    /// Perfil para una instancia compatible de OpenCodeGo.
    OpenCodeGo,
    /// Perfil para DeepSeek.
    DeepSeek,
    /// Perfil local de Ollama expuesto en loopback.
    OllamaLocal,
    /// Endpoint OpenAI-compatible configurado explícitamente con otra clave.
    CustomOpenAiCompatible,
}

impl ProviderProfile {
    /// Nombre de variable de entorno que puede contener la clave de este perfil.
    pub const fn api_key_env(self) -> Option<&'static str> {
        match self {
            Self::OpenCodeGo => Some("OPENCODEGO_API_KEY"),
            Self::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Self::OllamaLocal | Self::CustomOpenAiCompatible => None,
        }
    }

    /// Indica las capacidades conservadoras que se asumen para el perfil.
    pub const fn capabilities(self) -> ProviderCapabilities {
        match self {
            Self::OpenCodeGo | Self::DeepSeek => ProviderCapabilities {
                openai_compatible: true,
                vision: false,
                streaming: true,
            },
            Self::OllamaLocal => ProviderCapabilities {
                openai_compatible: true,
                vision: false,
                streaming: true,
            },
            Self::CustomOpenAiCompatible => ProviderCapabilities {
                openai_compatible: true,
                vision: false,
                streaming: false,
            },
        }
    }
}

/// Capacidades explícitas del modelo elegido dentro de un perfil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Identificador de modelo del proveedor.
    pub model: String,
    /// Capacidades que pueden ser más restrictivas que las del perfil.
    pub capabilities: ProviderCapabilities,
}

/// Resumen estable de un objeto visible en el contexto de documento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContextObject {
    /// Etiqueta visible para el usuario.
    pub label: String,
    /// Tipo estable, por ejemplo `Function` o `Point`.
    pub kind: String,
    /// Huella del objeto serializado, sin nombres o rutas de archivos locales.
    pub fingerprint: String,
}

/// Resumen explícito del objeto que el usuario eligió para la consulta actual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantFocus {
    /// Etiqueta visible del objeto en Grafito.
    pub label: String,
    /// Tipo estable del objeto, por ejemplo `Function`.
    pub kind: String,
    /// Descripción matemática acotada, sin rutas, cachés ni datos de origen.
    pub summary: String,
}

impl AssistantFocus {
    /// Construye un foco para una función explícita o integral seleccionada.
    pub fn function(
        label: impl Into<String>,
        expression: impl Into<String>,
        domain_min: Option<f64>,
        domain_max: Option<f64>,
        is_integral: bool,
    ) -> Self {
        let label = label.into();
        let expression = expression.into();
        let display_label = label.trim();
        let mut summary = if display_label.ends_with(')') {
            format!("{display_label} = {expression}")
        } else {
            format!("{display_label}(x) = {expression}")
        };
        if let (Some(min), Some(max)) = (domain_min, domain_max) {
            summary.push_str(&format!(", domain x in [{min}, {max}]"));
        }
        if is_integral {
            summary.push_str(", accumulated integral");
        }
        Self {
            label,
            kind: "Function".into(),
            summary,
        }
    }

    /// Comprueba que el resumen sea seguro de incluir en una solicitud remota.
    pub fn validate(&self) -> Result<(), String> {
        if self.label.trim().is_empty()
            || self.label.chars().count() > 256
            || self.kind.trim().is_empty()
            || self.kind.chars().count() > 128
            || self.summary.trim().is_empty()
            || self.summary.chars().count() > MAX_FOCUS_SUMMARY_CHARS
        {
            return Err("assistant focus is outside the allowed size".into());
        }
        Ok(())
    }
}

/// Autor de un turno textual dentro de una conversación acotada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRole {
    /// Texto enviado explícitamente por el usuario.
    User,
    /// Texto devuelto por un proveedor o resolución previa.
    Assistant,
}

/// Procedencia pública de una respuesta del asistente.
///
/// La interfaz sólo puede presentar estas categorías. El proveedor, el modelo y
/// el endpoint permanecen dentro de la configuración avanzada y nunca viajan
/// con el historial remoto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantExecutionOrigin {
    /// Resultado obtenido íntegramente en el proceso local.
    Local,
    /// Resultado de una consulta cuyo envío remoto fue autorizado por el usuario.
    AuthorizedRemote,
}

impl AssistantExecutionOrigin {
    /// Etiqueta apta para la interfaz normal, sin identidad técnica del transporte.
    pub const fn public_label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::AuthorizedRemote => "Consulta remota autorizada",
        }
    }
}

/// Turno textual conservado solamente durante la sesión de la aplicación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// Autor del turno.
    pub role: ConversationRole,
    /// Texto acotado sin adjuntos ni credenciales.
    pub content: String,
    /// Procedencia de una respuesta del asistente. No se serializa ni se reenvía.
    #[serde(skip)]
    pub origin: Option<AssistantExecutionOrigin>,
}

impl ConversationTurn {
    /// Crea un turno del usuario.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ConversationRole::User,
            content: content.into(),
            origin: None,
        }
    }

    /// Crea un turno de respuesta del asistente.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ConversationRole::Assistant,
            content: content.into(),
            origin: None,
        }
    }

    /// Crea una respuesta del asistente con su procedencia pública.
    pub fn assistant_with_origin(
        content: impl Into<String>,
        origin: AssistantExecutionOrigin,
    ) -> Self {
        Self {
            role: ConversationRole::Assistant,
            content: content.into(),
            origin: Some(origin),
        }
    }

    /// Comprueba el presupuesto individual del turno.
    pub fn validate(&self) -> Result<(), String> {
        if self.content.trim().is_empty()
            || self.content.chars().count() > MAX_CONVERSATION_TURN_CHARS
        {
            return Err("assistant conversation turn is outside the allowed size".into());
        }
        Ok(())
    }
}

/// Contexto inmutable, reducido y determinista de un documento de Grafito.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmutableDocumentContext {
    /// Versión de esquema del contexto.
    pub schema_version: u32,
    /// Revisión del documento que originó la solicitud.
    pub revision: u64,
    /// Variables en orden léxico determinista.
    pub variables: BTreeMap<String, f64>,
    /// Objetos visibles resumidos, ordenados deterministamente.
    pub objects: Vec<DocumentContextObject>,
    /// Huella FNV-1a estable del contenido anterior.
    pub digest: String,
}

impl ImmutableDocumentContext {
    /// Crea un contexto vacío ligado a una revisión.
    pub fn empty(revision: u64) -> Self {
        Self::from_parts(revision, BTreeMap::new(), Vec::new())
    }

    /// Construye un contexto sólo de variables para solicitudes simples y tests.
    pub fn from_variables(
        revision: u64,
        variables: impl IntoIterator<Item = (String, f64)>,
    ) -> Self {
        Self::from_parts(revision, variables.into_iter().collect(), Vec::new())
    }

    /// Construye y canoniza el contexto antes de calcular su huella.
    pub fn from_parts(
        revision: u64,
        variables: BTreeMap<String, f64>,
        mut objects: Vec<DocumentContextObject>,
    ) -> Self {
        objects.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.fingerprint.cmp(&right.fingerprint))
        });
        let digest = digest_context(revision, &variables, &objects);
        Self {
            schema_version: ASSISTANT_SCHEMA_VERSION,
            revision,
            variables,
            objects,
            digest,
        }
    }

    /// Comprueba que el contexto usa una versión de esquema compatible.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ASSISTANT_SCHEMA_VERSION {
            return Err("assistant document context schema version is unsupported".into());
        }
        Ok(())
    }

    /// Base mínima con la que una propuesta se puede validar antes de aplicarse.
    pub fn basis(&self) -> PlanBasis {
        PlanBasis {
            revision: self.revision,
            digest: self.digest.clone(),
        }
    }
}

fn digest_context(
    revision: u64,
    variables: &BTreeMap<String, f64>,
    objects: &[DocumentContextObject],
) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    fn write(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    write(&mut hash, &ASSISTANT_SCHEMA_VERSION.to_le_bytes());
    write(&mut hash, &revision.to_le_bytes());
    for (name, value) in variables {
        write(&mut hash, &(name.len() as u64).to_le_bytes());
        write(&mut hash, name.as_bytes());
        write(&mut hash, &value.to_bits().to_le_bytes());
    }
    for object in objects {
        write(&mut hash, &(object.label.len() as u64).to_le_bytes());
        write(&mut hash, object.label.as_bytes());
        write(&mut hash, &(object.kind.len() as u64).to_le_bytes());
        write(&mut hash, object.kind.as_bytes());
        write(&mut hash, &(object.fingerprint.len() as u64).to_le_bytes());
        write(&mut hash, object.fingerprint.as_bytes());
    }
    format!("fnv1a64:{hash:016x}")
}

/// Revisión y huella de documento obligatorias para aplicar una propuesta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanBasis {
    /// Revisión del documento de origen.
    pub revision: u64,
    /// Huella determinista del contexto de origen.
    pub digest: String,
}

/// Paso verificable de una derivación determinista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationStep {
    /// Expresión o estado anterior.
    pub before: String,
    /// Expresión o estado posterior.
    pub after: String,
    /// Regla matemática usada para la transformación.
    pub rule: String,
    /// Comprobación independiente del paso o del resultado.
    pub verification: String,
}

/// Operaciones deliberadamente estrechas que una propuesta puede solicitar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum AssistantOperation {
    /// Cambia una sola variable escalar existente o nueva.
    SetVariable {
        /// Nombre de variable permitido por el integrador.
        name: String,
        /// Valor finito requerido por el integrador.
        value: f64,
    },
    /// Añade una función 2D simple que el integrador debe validar de nuevo.
    CreateGraph {
        /// Expresión de `y` en función de `variable`.
        expression: String,
        /// Variable independiente; el MVP admite `x`.
        variable: String,
        /// Límite inferior del dominio visible.
        domain_min: f64,
        /// Límite superior del dominio visible.
        domain_max: f64,
    },
}

impl AssistantOperation {
    /// Indica si la operación es la creación de un gráfico simple.
    pub const fn is_graph(&self) -> bool {
        matches!(self, Self::CreateGraph { .. })
    }

    fn add_display_characters(&self, total: &mut usize) -> Result<(), String> {
        match self {
            Self::SetVariable { name, .. } => add_display_characters(total, name),
            Self::CreateGraph {
                expression,
                variable,
                ..
            } => {
                add_display_characters(total, expression)?;
                add_display_characters(total, variable)
            }
        }
    }
}

/// Propuesta que nunca se ejecuta sin validación por el integrador de comandos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedPlan {
    /// Versión de esquema de la propuesta.
    pub schema_version: u32,
    /// Contexto exacto que la propuesta presupone.
    pub basis: PlanBasis,
    /// Descripción corta para la vista previa del usuario.
    pub summary: String,
    /// Operaciones tipadas y allowlisted.
    pub operations: Vec<AssistantOperation>,
}

impl ProposedPlan {
    /// Construye una propuesta sin operaciones implícitas ni comandos de texto.
    pub fn new(basis: PlanBasis, operations: Vec<AssistantOperation>) -> Self {
        Self {
            schema_version: ASSISTANT_SCHEMA_VERSION,
            basis,
            summary: "Safe assistant proposal".into(),
            operations,
        }
    }

    /// Comprueba que la propuesta cabe en el presupuesto estructural del MVP.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ASSISTANT_SCHEMA_VERSION {
            return Err("assistant plan schema version is unsupported".into());
        }
        if self.summary.chars().count() > 1_024 {
            return Err("assistant plan summary exceeds the allowed size".into());
        }
        if self.operations.is_empty() || self.operations.len() > MAX_PROPOSED_PLAN_OPERATIONS {
            return Err("assistant plan operation count is outside the allowed range".into());
        }
        Ok(())
    }

    fn add_display_characters(&self, total: &mut usize) -> Result<(), String> {
        add_display_characters(total, &self.basis.digest)?;
        add_display_characters(total, &self.summary)?;
        for operation in &self.operations {
            operation.add_display_characters(total)?;
        }
        Ok(())
    }
}

/// Algoritmo de compromiso permitido para receipts locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantPlanReceiptDigestAlgorithm {
    Sha256,
}

/// Estado de documento representado por hashes, sin contenido del documento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantPlanReceiptState {
    /// Version del formato de documento usada para el compromiso semantico.
    pub document_schema_version: u32,
    /// Revision local en la que se calculo el estado.
    pub revision: u64,
    /// Huella reducida del contexto de la propuesta.
    pub context_digest: String,
    /// Compromiso SHA-256 del estado semantico completo, no serializado aqui.
    pub semantic_commitment: String,
}

impl AssistantPlanReceiptState {
    fn validate(&self) -> Result<(), String> {
        if self.document_schema_version == 0 {
            return Err("assistant receipt document schema version is invalid".into());
        }
        if !is_context_digest(&self.context_digest) {
            return Err("assistant receipt context digest is invalid".into());
        }
        if !is_sha256_commitment(&self.semantic_commitment) {
            return Err("assistant receipt semantic commitment is invalid".into());
        }
        Ok(())
    }
}

/// Conteos verificables del cambio staged, sin valores de operaciones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantPlanReceiptDelta {
    /// Cantidad total de operaciones allowlisted staged.
    pub operation_count: u8,
    /// Cantidad de operaciones `SetVariable` solicitadas.
    pub set_variable_count: u8,
    /// Cantidad de operaciones `CreateGraph` solicitadas.
    pub create_graph_count: u8,
    /// Objetos creados realmente durante el staging.
    pub created_object_count: u8,
    /// Variables cuyo valor final cambio realmente durante el staging.
    pub changed_variable_count: u8,
}

impl AssistantPlanReceiptDelta {
    fn validate(&self) -> Result<(), String> {
        if self.operation_count == 0
            || usize::from(self.operation_count) > MAX_PROPOSED_PLAN_OPERATIONS
            || self
                .set_variable_count
                .saturating_add(self.create_graph_count)
                != self.operation_count
            || self.created_object_count > self.create_graph_count
            || self.changed_variable_count > self.set_variable_count
        {
            return Err("assistant receipt delta is invalid".into());
        }
        if self.created_object_count == 0 && self.changed_variable_count == 0 {
            return Err("assistant receipt must describe a semantic change".into());
        }
        Ok(())
    }
}

/// Evidencia local serializable de un staging, sin plan ni contenido sensible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantPlanReceipt {
    /// Version del formato de receipt.
    pub schema_version: u32,
    /// Version de la politica local que permitio el plan.
    pub policy_version: u32,
    /// Algoritmo usado por todos los compromisos del receipt.
    pub digest_algorithm: AssistantPlanReceiptDigestAlgorithm,
    /// SHA-256 del `ProposedPlan` canonicamente serializado, no el plan mismo.
    pub plan_commitment: String,
    /// Estado de documento antes de staging.
    pub base: AssistantPlanReceiptState,
    /// Estado staged que se aplicaria tras una aprobacion explicita.
    pub staged: AssistantPlanReceiptState,
    /// Resumen cuantitativo del delta staged.
    pub delta: AssistantPlanReceiptDelta,
    /// SHA-256 de todos los campos de evidencia anteriores.
    pub evidence_commitment: String,
}

impl AssistantPlanReceipt {
    /// Valida estructura y presupuestos sin necesitar un documento ni contenido original.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ASSISTANT_PLAN_RECEIPT_SCHEMA_VERSION {
            return Err("assistant receipt schema version is unsupported".into());
        }
        if self.policy_version != ASSISTANT_PLAN_RECEIPT_POLICY_VERSION {
            return Err("assistant receipt policy version is unsupported".into());
        }
        if !matches!(
            self.digest_algorithm,
            AssistantPlanReceiptDigestAlgorithm::Sha256
        ) || !is_sha256_commitment(&self.plan_commitment)
            || !is_sha256_commitment(&self.evidence_commitment)
        {
            return Err("assistant receipt commitment is invalid".into());
        }
        self.base.validate()?;
        self.staged.validate()?;
        if self.staged.document_schema_version != self.base.document_schema_version
            || self.staged.revision != self.base.revision.wrapping_add(1)
        {
            return Err("assistant receipt staged state is invalid".into());
        }
        self.delta.validate()
    }
}

fn is_sha256_commitment(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_context_digest(value: &str) -> bool {
    value.strip_prefix("fnv1a64:").is_some_and(|digest| {
        digest.len() == 16 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) || value
        .strip_prefix("sha256:")
        .is_some_and(is_sha256_commitment)
}

/// Solicitud completa que puede resolverse localmente o, con consentimiento, remotamente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantRequest {
    /// Versión de esquema de la solicitud.
    pub schema_version: u32,
    /// Política seleccionada por el usuario.
    pub privacy_mode: PrivacyMode,
    /// Problema pegado o escrito por el usuario.
    pub problem: String,
    /// Contexto inmutable del documento.
    pub context: ImmutableDocumentContext,
    /// Objeto seleccionado que el usuario quiere analizar, si lo hubiera.
    pub focus: Option<AssistantFocus>,
    /// Historial textual de sesión, acotado y nunca persistido en configuración.
    pub conversation: Vec<ConversationTurn>,
    /// Catálogo acotado de herramientas nativas relevantes para esta consulta.
    #[serde(default)]
    pub tool_catalog: String,
    /// Diagnóstico local, opcional y acotado, para una única reparación remota.
    #[serde(default)]
    pub repair_feedback: Option<AssistantRepairFeedback>,
    /// Presupuesto de ejecución y salida.
    pub budget: RequestBudget,
    /// Imágenes validadas, sin rutas ni nombres locales.
    pub attachments: Vec<ImageAttachment>,
    /// Texto revisable, útil si una imagen no se puede procesar localmente.
    pub transcription: EditableTranscription,
    /// Consentimiento separado para transmitir bytes de imagen a un proveedor remoto.
    pub image_upload_consent: bool,
}

impl AssistantRequest {
    /// Construye una solicitud local y privada con los límites por defecto.
    pub fn local(problem: impl Into<String>, context: ImmutableDocumentContext) -> Self {
        Self {
            schema_version: ASSISTANT_SCHEMA_VERSION,
            privacy_mode: PrivacyMode::LocalOnly,
            problem: problem.into(),
            context,
            focus: None,
            conversation: Vec::new(),
            tool_catalog: String::new(),
            repair_feedback: None,
            budget: RequestBudget::default(),
            attachments: Vec::new(),
            transcription: EditableTranscription::default(),
            image_upload_consent: false,
        }
    }

    /// Construye una solicitud remota que requiere acción explícita del usuario.
    pub fn remote(problem: impl Into<String>, context: ImmutableDocumentContext) -> Self {
        let mut request = Self::local(problem, context);
        request.privacy_mode = PrivacyMode::RemoteAllowed;
        request
    }

    /// Comprueba límites de solicitud antes de resolver o serializarla.
    pub fn validate(&self, attachment_limits: &AttachmentLimits) -> Result<(), String> {
        if self.schema_version != ASSISTANT_SCHEMA_VERSION {
            return Err("assistant request schema version is unsupported".into());
        }
        self.context.validate()?;
        self.budget.validate()?;
        if self.conversation.len() > MAX_CONVERSATION_TURNS {
            return Err("assistant conversation exceeds the allowed turn count".into());
        }
        if let Some(focus) = &self.focus {
            focus.validate()?;
        }
        if let Some(feedback) = &self.repair_feedback {
            feedback.validate()?;
        }
        if self.repair_feedback.is_some()
            && (!self.attachments.is_empty() || self.image_upload_consent)
        {
            return Err(
                "assistant repair requests cannot include images or image-upload consent".into(),
            );
        }
        if self.conversation.len() % 2 != 0 {
            return Err("assistant conversation must contain complete exchanges".into());
        }
        for pair in self.conversation.chunks_exact(2) {
            if !matches!(pair[0].role, ConversationRole::User)
                || !matches!(pair[1].role, ConversationRole::Assistant)
            {
                return Err(
                    "assistant conversation roles must alternate from user to assistant".into(),
                );
            }
            pair[0].validate()?;
            pair[1].validate()?;
        }
        let text_bytes = self
            .problem
            .len()
            .saturating_add(self.transcription.text.len())
            .saturating_add(
                self.focus
                    .as_ref()
                    .map(|focus| focus.summary.len())
                    .unwrap_or_default(),
            )
            .saturating_add(
                self.focus
                    .as_ref()
                    .map(|_| REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                    .unwrap_or_default(),
            )
            .saturating_add(
                self.conversation
                    .iter()
                    .map(|turn| turn.content.len())
                    .sum::<usize>(),
            )
            .saturating_add(self.tool_catalog.len())
            .saturating_add(if self.tool_catalog.is_empty() {
                0
            } else {
                REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES
            })
            .saturating_add(
                self.repair_feedback
                    .as_ref()
                    .map(|feedback| {
                        feedback
                            .prompt_text()
                            .len()
                            .saturating_add(REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES)
                    })
                    .unwrap_or_default(),
            )
            .saturating_add(
                self.attachments
                    .iter()
                    .map(|attachment| attachment.transcription.text.len())
                    .sum::<usize>(),
            );
        if text_bytes > self.budget.max_input_chars {
            return Err("assistant input text exceeds the configured input budget".into());
        }
        if self.attachments.len() > attachment_limits.max_attachments {
            return Err("assistant attachment count exceeds the configured limit".into());
        }
        let mut total_attachment_bytes = 0_usize;
        let mut total_attachment_pixels = 0_u64;
        for attachment in &self.attachments {
            attachment.validate(attachment_limits)?;
            total_attachment_bytes = total_attachment_bytes
                .checked_add(attachment.bytes.len())
                .ok_or_else(|| "assistant attachment payload budget overflow".to_string())?;
            total_attachment_pixels = total_attachment_pixels
                .checked_add(
                    u64::from(attachment.pixel_width)
                        .checked_mul(u64::from(attachment.pixel_height))
                        .ok_or_else(|| "assistant attachment pixel count overflow".to_string())?,
                )
                .ok_or_else(|| "assistant attachment pixel budget overflow".to_string())?;
        }
        if total_attachment_bytes > attachment_limits.max_total_bytes {
            return Err("assistant attachment payload budget exceeded".into());
        }
        if total_attachment_pixels > attachment_limits.max_total_pixels {
            return Err("assistant attachment decoded pixel budget exceeded".into());
        }
        Ok(())
    }
}

/// Estado explícito de una resolución local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalAssistantStatus {
    /// La solicitud se resolvió determinísticamente en el proceso local.
    Solved,
    /// La petición se reconoce pero queda fuera del MVP local.
    Unsupported,
    /// Se recibió una imagen cuya transcripción requiere revisión o visión remota.
    VisionUnavailable,
    /// La entrada o sus adjuntos no cumplen las reglas de seguridad.
    Rejected,
}

/// Resultado final, autocontenido y apto para renderizar por cualquier frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantResponse {
    /// Versión de esquema del resultado.
    pub schema_version: u32,
    /// Estado explícito, sin inferir capacidades inexistentes.
    pub status: LocalAssistantStatus,
    /// Respuesta breve para mostrar al usuario.
    pub answer: String,
    /// Pasos locales con antes, después, regla y verificación.
    pub derivation: Vec<DerivationStep>,
    /// Propuesta opcional que requiere previsualización y aplicación explícita.
    pub plan: Option<ProposedPlan>,
}

impl AssistantResponse {
    /// Construye una respuesta sin operaciones de documento.
    pub fn message(status: LocalAssistantStatus, answer: impl Into<String>) -> Self {
        Self {
            schema_version: ASSISTANT_SCHEMA_VERSION,
            status,
            answer: answer.into(),
            derivation: Vec::new(),
            plan: None,
        }
    }

    /// Comprueba que una respuesta no supera los límites solicitados.
    pub fn validate(&self, budget: &RequestBudget) -> Result<(), String> {
        if self.schema_version != ASSISTANT_SCHEMA_VERSION {
            return Err("assistant response schema version is unsupported".into());
        }
        budget.validate()?;
        let mut display_characters = 0;
        add_display_characters(&mut display_characters, &self.answer)?;
        for step in &self.derivation {
            add_display_characters(&mut display_characters, &step.before)?;
            add_display_characters(&mut display_characters, &step.after)?;
            add_display_characters(&mut display_characters, &step.rule)?;
            add_display_characters(&mut display_characters, &step.verification)?;
        }
        if let Some(plan) = &self.plan {
            plan.validate()?;
            plan.add_display_characters(&mut display_characters)?;
        }
        if display_characters > budget.max_output_chars {
            return Err("assistant response exceeds the configured output budget".into());
        }
        if self.derivation.len() > budget.max_steps {
            return Err("assistant response exceeds the configured derivation step budget".into());
        }
        Ok(())
    }
}

fn add_display_characters(total: &mut usize, text: &str) -> Result<(), String> {
    *total = total
        .checked_add(text.chars().count())
        .ok_or_else(|| "assistant response display size overflow".to_string())?;
    Ok(())
}

/// Eventos serializables que una interfaz puede consumir si un proveedor transmite salida.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AssistantStreamEvent {
    /// Se aceptó la solicitud con la versión indicada.
    Started { schema_version: u32 },
    /// Fragmento de texto incremental.
    TextDelta { text: String },
    /// Paso matemático ya completo.
    Derivation { step: DerivationStep },
    /// Propuesta estructurada lista para previsualizar.
    Plan { plan: ProposedPlan },
    /// Flujo terminado correctamente.
    Completed { response: AssistantResponse },
    /// El flujo se canceló sin aplicar cambios al documento.
    Cancelled,
    /// Error controlado apto para mostrar al usuario.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_context_digest_is_independent_of_input_order() {
        let left = ImmutableDocumentContext::from_variables(
            7,
            [("b".to_string(), 2.0), ("a".to_string(), 1.0)],
        );
        let right = ImmutableDocumentContext::from_variables(
            7,
            [("a".to_string(), 1.0), ("b".to_string(), 2.0)],
        );

        assert_eq!(left.digest, right.digest);
    }

    #[test]
    fn attachment_limits_reject_oversized_or_unsafe_images() {
        let limits = AttachmentLimits::default();
        let oversized = ImageAttachment::new("image/png", vec![0; limits.max_bytes + 1], 1, 1);
        assert!(oversized.validate(&limits).is_err());

        let unsupported = ImageAttachment::new("image/svg+xml", vec![0; 8], 1, 1);
        assert!(unsupported.validate(&limits).is_err());
    }

    #[test]
    fn request_rejects_aggregate_attachment_payload_and_pixel_budgets() {
        let attachment = ImageAttachment::new("image/png", vec![0; 4], 2, 2);
        let mut request = AssistantRequest::local("x", ImmutableDocumentContext::empty(0));
        request.attachments = vec![attachment.clone(), attachment];

        let payload_limited = AttachmentLimits {
            max_bytes: 4,
            max_pixels: 4,
            max_attachments: 2,
            max_total_bytes: 6,
            max_total_pixels: 8,
        };
        assert!(request.validate(&payload_limited).is_err());

        let pixel_limited = AttachmentLimits {
            max_total_bytes: 8,
            max_total_pixels: 7,
            ..payload_limited
        };
        assert!(request.validate(&pixel_limited).is_err());
    }

    #[test]
    fn public_boundaries_reject_unsupported_schema_versions() {
        let mut context = ImmutableDocumentContext::empty(0);
        context.schema_version = ASSISTANT_SCHEMA_VERSION + 1;
        assert!(context.validate().is_err());

        let mut request = AssistantRequest::local("x", ImmutableDocumentContext::empty(0));
        request.schema_version = ASSISTANT_SCHEMA_VERSION + 1;
        assert!(request.validate(&AttachmentLimits::default()).is_err());

        let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "x");
        response.schema_version = ASSISTANT_SCHEMA_VERSION + 1;
        assert!(response.validate(&RequestBudget::default()).is_err());
    }

    #[test]
    fn request_rejects_combined_problem_and_transcription_over_the_input_budget() {
        let mut request = AssistantRequest::local("abcde", ImmutableDocumentContext::empty(0));
        request.budget.max_input_chars = 8;
        request.transcription.text = "fghi".into();

        assert!(request.validate(&AttachmentLimits::default()).is_err());
    }

    #[test]
    fn remote_request_bounds_conversation_and_focus_without_transcription() {
        let mut request = AssistantRequest::remote("analyze f", ImmutableDocumentContext::empty(1));
        request.focus = Some(AssistantFocus::function(
            "f",
            "x^2",
            Some(-2.0),
            Some(2.0),
            false,
        ));
        request.conversation = (0..=MAX_CONVERSATION_TURNS)
            .map(|index| ConversationTurn::user(format!("turn {index}")))
            .collect();

        assert!(request.validate(&AttachmentLimits::default()).is_err());
        assert!(request.transcription.text.is_empty());
    }

    #[test]
    fn function_focus_keeps_an_existing_parameter_list_once() {
        let named = AssistantFocus::function("f(x)", "sin(a*x)", None, None, false);
        let bare = AssistantFocus::function("g", "cos(x)", None, None, false);

        assert_eq!(named.summary, "f(x) = sin(a*x)");
        assert_eq!(bare.summary, "g(x) = cos(x)");
    }

    #[test]
    fn input_budget_counts_utf8_bytes_not_unicode_scalar_values() {
        let mut request = AssistantRequest::remote("éé", ImmutableDocumentContext::empty(1));
        request.budget.max_input_chars = 3;

        assert!(request.validate(&AttachmentLimits::default()).is_err());
    }

    #[test]
    fn request_rejects_unpaired_or_non_alternating_conversation_turns() {
        let mut request = AssistantRequest::remote("consulta", ImmutableDocumentContext::empty(1));
        request.conversation = vec![ConversationTurn::user("sin respuesta")];
        assert!(request.validate(&AttachmentLimits::default()).is_err());

        request.conversation = vec![
            ConversationTurn::assistant("primera"),
            ConversationTurn::assistant("segunda"),
        ];
        assert!(request.validate(&AttachmentLimits::default()).is_err());
    }

    #[test]
    fn conversation_turn_accepts_the_default_remote_output_limit() {
        let turn =
            ConversationTurn::assistant("x".repeat(RequestBudget::default().max_output_chars));

        assert!(turn.validate().is_ok());
    }

    #[test]
    fn conversation_turn_rejects_one_character_over_the_default_remote_output_limit() {
        let turn = ConversationTurn::assistant(
            "x".repeat(RequestBudget::default().max_output_chars.saturating_add(1)),
        );

        assert!(turn.validate().is_err());
    }

    #[test]
    fn conversation_turn_origin_remains_session_only_when_serialized() {
        let turn = ConversationTurn::assistant_with_origin(
            "4",
            AssistantExecutionOrigin::AuthorizedRemote,
        );

        let serialized = serde_json::to_value(&turn).expect("turn serializes");

        assert_eq!(serialized["role"], "assistant");
        assert_eq!(serialized["content"], "4");
        assert!(serialized.get("origin").is_none());
    }

    #[test]
    fn tool_catalog_wrapper_counts_toward_the_input_budget() {
        let mut request = AssistantRequest::remote("x", ImmutableDocumentContext::empty(1));
        request.tool_catalog = "Function[expr]".into();
        request.budget.max_input_chars = request.problem.len()
            + request.tool_catalog.len()
            + REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES
            - 1;

        assert!(request.validate(&AttachmentLimits::default()).is_err());
    }

    #[test]
    fn repair_feedback_is_sanitized_bounded_and_counted_in_the_input_budget() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };
        assert!(feedback.validate().is_ok());
        assert!(feedback.prompt_text().contains("Polyhedron"));
        assert!(feedback.prompt_text().contains("not executable"));

        let mut request = AssistantRequest::remote("x", ImmutableDocumentContext::empty(1));
        request.repair_feedback = Some(feedback.clone());
        request.budget.max_input_chars = request.problem.len()
            + feedback.prompt_text().len()
            + REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES
            - 1;
        assert!(request.validate(&AttachmentLimits::default()).is_err());

        let unsafe_feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron[erase]".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };
        assert!(unsafe_feedback.validate().is_err());
    }

    #[test]
    fn repair_feedback_requests_never_allow_attachments_or_image_consent() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };
        let mut with_attachment =
            AssistantRequest::remote("tetraedro", ImmutableDocumentContext::empty(1));
        with_attachment.repair_feedback = Some(feedback.clone());
        with_attachment.image_upload_consent = true;
        with_attachment
            .attachments
            .push(ImageAttachment::new("image/png", vec![1], 1, 1));

        assert!(matches!(
            with_attachment.validate(&AttachmentLimits::default()),
            Err(error) if error.contains("repair requests cannot include images")
        ));

        let mut with_consent =
            AssistantRequest::remote("tetraedro", ImmutableDocumentContext::empty(1));
        with_consent.repair_feedback = Some(feedback);
        with_consent.image_upload_consent = true;

        assert!(matches!(
            with_consent.validate(&AttachmentLimits::default()),
            Err(error) if error.contains("repair requests cannot include images")
        ));
    }

    #[test]
    fn document_context_digest_orders_duplicate_object_labels_stably() {
        let first = DocumentContextObject {
            label: "f".into(),
            kind: "Function".into(),
            fingerprint: "first".into(),
        };
        let second = DocumentContextObject {
            label: "f".into(),
            kind: "Function".into(),
            fingerprint: "second".into(),
        };

        let left = ImmutableDocumentContext::from_parts(
            1,
            BTreeMap::new(),
            vec![first.clone(), second.clone()],
        );
        let right = ImmutableDocumentContext::from_parts(1, BTreeMap::new(), vec![second, first]);

        assert_eq!(left.digest, right.digest);
    }

    #[test]
    fn unknown_operations_are_rejected_by_typed_deserialization() {
        let plan = r#"{
            "schema_version":1,
            "basis":{"revision":0,"digest":"fnv1a64:0000000000000000"},
            "summary":"bad",
            "operations":[{"operation":"script","source":"erase"}]
        }"#;

        assert!(serde_json::from_str::<ProposedPlan>(plan).is_err());
    }

    #[test]
    fn response_rejects_excessive_steps_and_plan_operations() {
        let budget = RequestBudget {
            max_steps: 1,
            ..RequestBudget::default()
        };
        let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "2");
        response.derivation = vec![
            DerivationStep {
                before: "1 + 1".into(),
                after: "2".into(),
                rule: "evaluate".into(),
                verification: "direct".into(),
            },
            DerivationStep {
                before: "2".into(),
                after: "2".into(),
                rule: "identity".into(),
                verification: "direct".into(),
            },
        ];
        assert!(response.validate(&budget).is_err());

        let oversized = AssistantResponse::message(LocalAssistantStatus::Solved, "12");
        let output_limited = RequestBudget {
            max_output_chars: 1,
            ..RequestBudget::default()
        };
        assert!(oversized.validate(&output_limited).is_err());

        let plan = ProposedPlan::new(
            ImmutableDocumentContext::empty(0).basis(),
            (0..=MAX_PROPOSED_PLAN_OPERATIONS)
                .map(|index| AssistantOperation::SetVariable {
                    name: format!("v{index}"),
                    value: index as f64,
                })
                .collect(),
        );
        let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "x");
        response.plan = Some(plan);
        assert!(response.validate(&RequestBudget::default()).is_err());
    }

    #[test]
    fn response_output_budget_includes_every_derivation_text_field() {
        let budget = RequestBudget {
            max_output_chars: 8,
            ..RequestBudget::default()
        };

        for derivation in [
            DerivationStep {
                before: "x".repeat(8),
                after: String::new(),
                rule: String::new(),
                verification: String::new(),
            },
            DerivationStep {
                before: String::new(),
                after: "x".repeat(8),
                rule: String::new(),
                verification: String::new(),
            },
            DerivationStep {
                before: String::new(),
                after: String::new(),
                rule: "x".repeat(8),
                verification: String::new(),
            },
            DerivationStep {
                before: String::new(),
                after: String::new(),
                rule: String::new(),
                verification: "x".repeat(8),
            },
        ] {
            let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "x");
            response.derivation = vec![derivation];

            assert!(response.validate(&budget).is_err());
        }
    }

    #[test]
    fn response_output_budget_includes_plan_text_and_operation_strings() {
        let budget = RequestBudget {
            max_output_chars: 8,
            ..RequestBudget::default()
        };
        let basis = ImmutableDocumentContext::empty(0).basis();
        let empty_basis = PlanBasis {
            revision: basis.revision,
            digest: String::new(),
        };

        for plan in [
            ProposedPlan {
                schema_version: ASSISTANT_SCHEMA_VERSION,
                basis: PlanBasis {
                    revision: basis.revision,
                    digest: "x".repeat(8),
                },
                summary: String::new(),
                operations: vec![AssistantOperation::SetVariable {
                    name: String::new(),
                    value: 0.0,
                }],
            },
            ProposedPlan {
                schema_version: ASSISTANT_SCHEMA_VERSION,
                basis: empty_basis.clone(),
                summary: "x".repeat(8),
                operations: vec![AssistantOperation::SetVariable {
                    name: String::new(),
                    value: 0.0,
                }],
            },
            ProposedPlan {
                schema_version: ASSISTANT_SCHEMA_VERSION,
                basis: empty_basis.clone(),
                summary: String::new(),
                operations: vec![AssistantOperation::SetVariable {
                    name: "x".repeat(8),
                    value: 0.0,
                }],
            },
            ProposedPlan {
                schema_version: ASSISTANT_SCHEMA_VERSION,
                basis: empty_basis.clone(),
                summary: String::new(),
                operations: vec![AssistantOperation::CreateGraph {
                    expression: "x".repeat(8),
                    variable: String::new(),
                    domain_min: -1.0,
                    domain_max: 1.0,
                }],
            },
            ProposedPlan {
                schema_version: ASSISTANT_SCHEMA_VERSION,
                basis: empty_basis,
                summary: String::new(),
                operations: vec![AssistantOperation::CreateGraph {
                    expression: String::new(),
                    variable: "x".repeat(8),
                    domain_min: -1.0,
                    domain_max: 1.0,
                }],
            },
        ] {
            let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "x");
            response.plan = Some(plan);

            assert!(response.validate(&budget).is_err());
        }
    }

    #[test]
    fn response_accepts_displayable_content_at_the_output_budget_boundary() {
        let budget = RequestBudget {
            max_output_chars: 8,
            ..RequestBudget::default()
        };
        let mut response = AssistantResponse::message(LocalAssistantStatus::Solved, "ok");
        response.derivation = vec![DerivationStep {
            before: "a".into(),
            after: "b".into(),
            rule: "c".into(),
            verification: "d".into(),
        }];
        response.plan = Some(ProposedPlan {
            schema_version: ASSISTANT_SCHEMA_VERSION,
            basis: PlanBasis {
                revision: 0,
                digest: String::new(),
            },
            summary: "e".into(),
            operations: vec![AssistantOperation::SetVariable {
                name: "f".into(),
                value: 0.0,
            }],
        });

        assert!(response.validate(&budget).is_ok());
    }

    #[test]
    fn plan_receipts_are_versioned_and_do_not_contain_source_content() {
        let commitment = "a".repeat(64);
        let receipt = AssistantPlanReceipt {
            schema_version: ASSISTANT_PLAN_RECEIPT_SCHEMA_VERSION,
            policy_version: ASSISTANT_PLAN_RECEIPT_POLICY_VERSION,
            digest_algorithm: AssistantPlanReceiptDigestAlgorithm::Sha256,
            plan_commitment: commitment.clone(),
            base: AssistantPlanReceiptState {
                document_schema_version: 5,
                revision: 7,
                context_digest: "fnv1a64:0123456789abcdef".into(),
                semantic_commitment: commitment.clone(),
            },
            staged: AssistantPlanReceiptState {
                document_schema_version: 5,
                revision: 8,
                context_digest: "fnv1a64:fedcba9876543210".into(),
                semantic_commitment: commitment.clone(),
            },
            delta: AssistantPlanReceiptDelta {
                operation_count: 2,
                set_variable_count: 1,
                create_graph_count: 1,
                created_object_count: 1,
                changed_variable_count: 1,
            },
            evidence_commitment: commitment,
        };

        receipt.validate().expect("bounded hash-only receipt");
        let serialized = serde_json::to_string(&receipt).expect("receipt serializes");
        let round_trip: AssistantPlanReceipt =
            serde_json::from_str(&serialized).expect("receipt deserializes");

        assert_eq!(round_trip, receipt);
        for sensitive_source in [
            "plot secret_expression(x)",
            "secret_expression(x)",
            "private-label",
            "/home/user/private.png",
            "api-key",
        ] {
            assert!(!serialized.contains(sensitive_source));
        }
    }
}
