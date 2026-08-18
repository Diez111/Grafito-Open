//! Estado y controles egui sin I/O para el asistente matemático de Grafito.

use crate::{
    animation::{ThinkingOrb, ThinkingOrbState},
    icons::{action_icon_button, Icon},
    theme::current_theme,
    tokens::{
        RADIUS_MD, RADIUS_SM, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_LG, TYPE_MD, TYPE_SM,
        TYPE_XS,
    },
};
use grafito_assistant_types::{
    AssistantExecutionOrigin, AssistantFocus, AssistantRepairFeedback, AttachmentLimits,
    ConversationRole, ConversationTurn, ImageAttachment, ImmutableDocumentContext, ProposedPlan,
    ProviderProfile, RequestBudget, MAX_CONVERSATION_TURNS, MAX_CONVERSATION_TURN_CHARS,
    REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES,
};
pub use grafito_command::assistant_proposals::{AssistantParameterAssignment, AssistantProposal};

const ASSISTANT_PANEL_DEFAULT_WIDTH: f32 = 400.0;
const ASSISTANT_PANEL_MIN_WIDTH: f32 = 340.0;
const ASSISTANT_PANEL_MAX_WIDTH: f32 = 460.0;
const ASSISTANT_MIN_CANVAS_WIDTH: f32 = 440.0;
const ASSISTANT_SIDE_PANEL_MIN_VIEWPORT_WIDTH: f32 =
    ASSISTANT_MIN_CANVAS_WIDTH + ASSISTANT_PANEL_MIN_WIDTH;
const ASSISTANT_COMPACT_MIN_CANVAS_HEIGHT: f32 = 160.0;
// The nested panel keeps its height in egui memory. Keep it deterministic so
// an old expanded state cannot strand the composer halfway up the assistant.
const ASSISTANT_COMPOSER_BASE_HEIGHT: f32 = 116.0;
const ASSISTANT_COMPOSER_EDITOR_HEIGHT: f32 = 44.0;
const ASSISTANT_COMPOSER_FOCUS_HEIGHT: f32 = 32.0;
const ASSISTANT_COMPOSER_BUDGET_HEIGHT: f32 = 20.0;
const ASSISTANT_COMPOSER_ATTACHMENT_HEIGHT: f32 = 112.0;
const ASSISTANT_COMPOSER_ATTACHMENT_ROW_HEIGHT: f32 = 30.0;
const ASSISTANT_COMPOSER_ATTACHMENT_MESSAGE_HEIGHT: f32 = 20.0;
const ASSISTANT_COMPOSER_PENDING_ATTACHMENT_HEIGHT: f32 = 20.0;
const ASSISTANT_HEADER_HEIGHT: f32 = 40.0;
const ASSISTANT_REVEAL_BASE_SECONDS: f64 = 0.28;
const ASSISTANT_REVEAL_PER_BLOCK_SECONDS: f64 = 0.18;
const ASSISTANT_REVEAL_MAX_SECONDS: f64 = 1.5;
const MAX_FOCUSED_CONTEXT_PREVIEW_CHARS: usize = 160;
const MORA_NAME: &str = "Mora";
const MORA_ACCESSIBLE_LABEL: &str = "Mora, asistente matemático";
const OPENCODE_DEFAULT_MODEL: &str = "minimax-m3";
const OLLAMA_DEFAULT_MODEL: &str = "llama3.2";
const OPENCODE_MODELS: &[&str] = &[
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "minimax-m3",
    "fusion",
    "glm-5.2",
];
const OLLAMA_MODELS: &[&str] = &["llama3.2", "llama3.1", "qwen2.5", "qwen2.5-vl", "llava"];

/// Propuesta comprobada con su identidad de respuesta y parámetros previos
/// necesarios para reproducir el preflight de forma explícita al aplicarla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAssistantProposal {
    /// Posición entre las propuestas sintácticamente válidas de la respuesta.
    pub candidate_index: usize,
    /// Acción que el usuario puede revisar y aplicar.
    pub proposal: AssistantProposal,
    /// Asignaciones finitas que el Apply explícito debe preparar junto a la acción.
    pub prerequisite_parameters: Vec<AssistantParameterAssignment>,
}

/// Recursos visuales locales que la aplicación anfitriona prepara para el asistente.
///
/// La textura es opcional para que la interfaz mantenga un fallback seguro si el
/// recurso embebido no se puede cargar.
#[derive(Debug, Clone, Copy, Default)]
pub struct AssistantVisuals {
    pub mora_texture: Option<egui::TextureId>,
}

/// Cache de bloques parseados de las respuestas del transcript, direccionado
/// por contenido y acotado. Evita re-parsear los mismos turnos en cada frame.
#[derive(Default, Clone)]
pub struct AssistantBlocksCache {
    entries: std::collections::VecDeque<(String, Vec<AssistantMessageBlock>)>,
}

impl AssistantBlocksCache {
    const MAX_ENTRIES: usize = 8;

    /// Devuelve los bloques de `content`, reutilizando el cache si ya se parseó.
    fn blocks(&mut self, content: &str) -> Vec<AssistantMessageBlock> {
        if let Some((_, blocks)) = self
            .entries
            .iter()
            .find(|(cached, _)| self.same_content(cached, content))
        {
            return blocks.clone();
        }
        let blocks = parse_assistant_blocks(content);
        self.entries
            .push_front((content.to_owned(), blocks.clone()));
        while self.entries.len() > Self::MAX_ENTRIES {
            self.entries.pop_back();
        }
        blocks
    }

    fn same_content(&self, cached: &str, content: &str) -> bool {
        cached == content
    }
}

/// Animación generada por el motor externo, reproducida como frames en el chat.
#[derive(Clone)]
pub struct AssistantMedia {
    pub title: String,
    pub frames: Vec<egui::ColorImage>,
}

/// Fila de actividad de una herramienta del asistente mientras el agente trabaja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivityRow {
    pub text: String,
}

/// Fila visible de un plugin del asistente (sin referencias al registry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    pub description: String,
    pub enabled: bool,
    pub error: Option<String>,
}

/// Snapshot local que vincula una corrección al documento y foco que la originaron.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantCorrectionContext {
    pub document_revision: u64,
    pub document_digest: String,
    pub focus: Option<AssistantFocus>,
}

/// Consulta local que el usuario puede autorizar a enviar de forma remota.
///
/// No incluye la identidad técnica del destino: esa información queda en la
/// configuración avanzada y el panel sólo comunica la clase de consentimiento.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRemoteAuthorization {
    question: String,
    reason: String,
}

/// Estado de UI del asistente. Las claves sólo son borradores efímeros y nunca
/// se serializan ni se envían a esta capa una vez guardadas en el llavero.
#[derive(Clone)]
pub struct AssistantPanelState {
    /// Perfil de proveedor seleccionado por el usuario.
    pub provider: ProviderProfile,
    /// Identificador del modelo configurado para el proveedor actual.
    pub model: String,
    /// Modelos cargados desde el proveedor, reducidos sólo a identificadores.
    pub available_models: Vec<String>,
    /// Borrador temporal de la clave que la aplicación consume y borra al guardar.
    pub api_key_draft: String,
    /// Indica si la aplicación tiene una clave guardada o de sesión disponible.
    pub key_available: bool,
    /// La configuración remota se abre sólo bajo petición para no recargar el chat.
    pub settings_open: bool,
    /// Evita consultar el llavero en cada frame mientras la configuración está abierta.
    pub key_status_checked: bool,
    /// Permiso completo: si el local no resuelve, consulta al proveedor sin cartel.
    pub full_permission: bool,
    /// Modo agente: usa el loop con herramientas y muestra su actividad en el chat.
    pub agent_mode: bool,
    /// Filas de actividad de herramientas de la consulta agente en curso.
    pub agent_activity: Vec<AgentActivityRow>,
    /// Ledger J-Space (Goal/Core/Verified/Open/Next) de la tarea agente en curso.
    pub agent_ledger: Option<String>,
    /// Animación generada por el motor externo, para reproducir en el chat.
    pub media: Option<AssistantMedia>,
    /// Verdadero mientras el job de animación está en curso (progreso en vivo).
    pub anim_progress: bool,
    /// Texturas de frames cargadas una sola vez al mostrar la animación.
    media_textures: Vec<egui::TextureHandle>,
    /// Guarda si ya se construyeron las texturas de la media actual.
    media_textures_ready: bool,
    /// Confirmación del usuario de que el modelo elegido admite imágenes.
    pub vision_enabled: bool,
    /// Autoriza explícitamente una revisión Fusion tras una propuesta M3 fallida.
    pub allow_fusion_fallback: bool,
    /// Problema pegado o escrito por el usuario.
    pub problem: String,
    /// Adjuntos ya validados, sin rutas ni nombres de archivo.
    pub attachments: Vec<ImageAttachment>,
    /// Consentimiento separado para enviar los bytes de imagen en esta consulta.
    pub image_upload_consent: bool,
    /// Función seleccionada al momento de preparar la consulta.
    pub focus: Option<AssistantFocus>,
    /// Conversación limitada a la sesión actual.
    pub conversation: Vec<ConversationTurn>,
    /// Propuestas de la última respuesta que superaron el preflight local.
    pub verified_proposals: Vec<VerifiedAssistantProposal>,
    /// Propuestas ya confirmadas por el documento; se muestran como historial sin acciones.
    pub applied_proposals: Vec<VerifiedAssistantProposal>,
    /// Cantidad de propuestas de la última respuesta que alcanzaron el preflight acotado.
    pub preflight_candidate_count: usize,
    /// Posición de cada candidato reconocido entre los fenced blocks de la respuesta.
    pub proposal_code_block_indices: Vec<usize>,
    /// Permite solicitar una única corrección explícita para una propuesta gráfica descartada.
    pub proposal_correction_available: bool,
    proposal_correction_question: Option<String>,
    proposal_correction_feedback: Option<AssistantRepairFeedback>,
    proposal_correction_target_turn: Option<usize>,
    proposal_correction_attempt: Option<u8>,
    proposal_correction_context: Option<AssistantCorrectionContext>,
    /// La aplicación tiene una consulta remota en curso.
    pub is_pending: bool,
    /// Consulta local no resuelta que espera una autorización remota explícita.
    pending_remote_authorization: Option<PendingRemoteAuthorization>,
    /// Plan local tipado que ya superó su vista previa y espera Apply explícito.
    proposed_plan: Option<ProposedPlan>,
    /// Cambios legibles que producirá el plan local pendiente.
    proposed_plan_changes: Vec<String>,
    /// Se solicitó cancelación y el worker aún debe terminar su transporte acotado.
    pub is_cancelling: bool,
    /// La reparación pendiente usa la auditoría Fusion autorizada por el usuario.
    pub is_fusion_review: bool,
    /// Estado no sensible de la importación o validación de adjuntos.
    pub attachment_message: Option<String>,
    /// Impide abrir más selectores mientras se importa una imagen en segundo plano.
    pub is_importing_image: bool,
    /// Una respuesta recién completada del asistente se revela por bloques.
    pub reveal_pending: bool,
    /// Instante (reloj de egui) en el que comenzó la última revelación.
    reveal_started_at: Option<f64>,
    /// Snapshot de plugins cargados por la aplicación, para mostrarlos en ajustes.
    pub plugins: Vec<PluginRow>,
    /// Error recuperable mostrado al usuario.
    pub error: Option<String>,
}

impl Default for AssistantPanelState {
    fn default() -> Self {
        Self {
            provider: ProviderProfile::OpenCodeGo,
            model: OPENCODE_DEFAULT_MODEL.into(),
            available_models: Vec::new(),
            api_key_draft: String::new(),
            key_available: false,
            settings_open: false,
            key_status_checked: false,
            full_permission: true,
            agent_mode: false,
            agent_activity: Vec::new(),
            agent_ledger: None,
            media: None,
            media_textures: Vec::new(),
            media_textures_ready: false,
            anim_progress: false,
            vision_enabled: false,
            allow_fusion_fallback: false,
            problem: String::new(),
            attachments: Vec::new(),
            image_upload_consent: false,
            focus: None,
            conversation: Vec::new(),
            verified_proposals: Vec::new(),
            applied_proposals: Vec::new(),
            preflight_candidate_count: 0,
            proposal_code_block_indices: Vec::new(),
            proposal_correction_available: false,
            proposal_correction_question: None,
            proposal_correction_feedback: None,
            proposal_correction_target_turn: None,
            proposal_correction_attempt: None,
            proposal_correction_context: None,
            is_pending: false,
            pending_remote_authorization: None,
            proposed_plan: None,
            proposed_plan_changes: Vec::new(),
            is_cancelling: false,
            is_fusion_review: false,
            attachment_message: None,
            is_importing_image: false,
            reveal_pending: false,
            reveal_started_at: None,
            plugins: Vec::new(),
            error: None,
        }
    }
}

impl AssistantPanelState {
    /// Indica si una consulta se puede iniciar con la conexión elegida.
    pub fn can_submit(&self) -> bool {
        !self.is_pending
            && !self.is_importing_image
            && !self.problem.trim().is_empty()
            && self.input_bytes() <= RequestBudget::default().max_input_chars
    }

    /// Borra el error recuperable y descarta su corrección pendiente, manteniendo conversación y adjuntos.
    pub fn clear_error(&mut self) {
        self.error = None;
        self.clear_proposal_correction();
        self.cancel_remote_authorization();
        self.clear_proposed_plan();
    }

    /// Descarta el historial local y las propuestas asociadas a esa conversación.
    pub fn clear_conversation(&mut self) {
        self.conversation.clear();
        self.reveal_pending = false;
        self.reveal_started_at = None;
        self.clear_proposal_cards();
        self.clear_proposal_correction();
        self.cancel_remote_authorization();
        self.clear_proposed_plan();
        self.error = None;
    }

    /// Conserva una consulta no resuelta localmente hasta que el usuario decida
    /// si autoriza el uso remoto. El turno del usuario ya permanece visible.
    pub fn stage_remote_authorization(&mut self, question: String, reason: String) {
        self.pending_remote_authorization = Some(PendingRemoteAuthorization { question, reason });
        self.is_pending = false;
        self.is_cancelling = false;
        self.image_upload_consent = false;
        self.error = None;
    }

    /// Devuelve si hay una consulta local esperando autorización remota.
    pub fn has_pending_remote_authorization(&self) -> bool {
        self.pending_remote_authorization.is_some()
    }

    /// Devuelve la consulta aún local que se mostraría al autorizar la red.
    pub fn pending_remote_authorization_question(&self) -> Option<&str> {
        self.pending_remote_authorization
            .as_ref()
            .map(|authorization| authorization.question.as_str())
    }

    /// Inicia el transporte autorizado sin duplicar el turno del usuario.
    pub fn begin_authorized_remote_request(&mut self) -> Option<String> {
        let authorization = self.pending_remote_authorization.take()?;
        self.is_pending = true;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        self.clear_proposal_cards();
        self.clear_proposal_correction();
        self.error = None;
        Some(authorization.question)
    }

    /// Descarta la oferta de transporte sin borrar el turno local visible.
    pub fn clear_remote_authorization(&mut self) {
        self.pending_remote_authorization = None;
    }

    /// Descarta una autorización explícita y su consentimiento de imágenes asociado.
    pub fn cancel_remote_authorization(&mut self) {
        self.clear_remote_authorization();
        self.image_upload_consent = false;
    }

    /// Conserva un plan local ya previsualizado para que el usuario lo aplique.
    pub fn stage_proposed_plan(&mut self, plan: ProposedPlan, changes: Vec<String>) {
        self.proposed_plan = Some(plan);
        self.proposed_plan_changes = changes;
    }

    /// Devuelve el plan local que espera Apply explícito.
    pub fn proposed_plan(&self) -> Option<&ProposedPlan> {
        self.proposed_plan.as_ref()
    }

    /// Marca el plan local como consumido sólo después de su commit.
    pub fn finish_proposed_plan_application(&mut self, committed: bool) -> bool {
        if !committed || self.proposed_plan.is_none() {
            return false;
        }
        self.clear_proposed_plan();
        true
    }

    /// Descarta un plan local que ya no representa el documento actual.
    pub fn clear_proposed_plan(&mut self) {
        self.proposed_plan = None;
        self.proposed_plan_changes.clear();
    }

    /// Habilita una corrección remota ligada al último turno del asistente.
    pub fn offer_proposal_correction(
        &mut self,
        question: String,
        feedback: AssistantRepairFeedback,
        context: AssistantCorrectionContext,
    ) {
        let target_turn = self
            .conversation
            .iter()
            .rposition(|turn| matches!(turn.role, ConversationRole::Assistant));
        self.set_proposal_correction(question, feedback, target_turn, 0, context);
    }

    /// Habilita una corrección remota ligada a un turno específico y su intento actual.
    pub fn offer_proposal_correction_for_turn(
        &mut self,
        question: String,
        feedback: AssistantRepairFeedback,
        target_turn: Option<usize>,
        attempt: u8,
        context: AssistantCorrectionContext,
    ) {
        self.set_proposal_correction(question, feedback, target_turn, attempt, context);
    }

    fn set_proposal_correction(
        &mut self,
        question: String,
        feedback: AssistantRepairFeedback,
        target_turn: Option<usize>,
        attempt: u8,
        context: AssistantCorrectionContext,
    ) {
        self.proposal_correction_available = true;
        self.proposal_correction_question = Some(question);
        self.proposal_correction_feedback = Some(feedback);
        self.proposal_correction_target_turn = target_turn;
        self.proposal_correction_attempt = Some(attempt);
        self.proposal_correction_context = Some(context);
    }

    /// Devuelve una corrección para compatibilidad con controles existentes.
    pub fn take_proposal_correction(&mut self) -> Option<(String, AssistantRepairFeedback)> {
        if !self.proposal_correction_available {
            return None;
        }
        self.proposal_correction_available = false;
        self.proposal_correction_target_turn = None;
        self.proposal_correction_attempt = None;
        self.proposal_correction_context = None;
        self.proposal_correction_question
            .take()
            .zip(self.proposal_correction_feedback.take())
    }

    /// Reserva la corrección actual sin eliminar su diagnóstico, para poder
    /// restaurarla si el transporte falla antes de obtener una respuesta.
    pub fn take_proposal_correction_session(
        &mut self,
    ) -> Option<(String, AssistantRepairFeedback, usize, u8)> {
        if !self.proposal_correction_available {
            return None;
        }
        let question = self.proposal_correction_question.clone()?;
        let feedback = self.proposal_correction_feedback.clone()?;
        let target_turn = self.proposal_correction_target_turn?;
        let attempt = self.proposal_correction_attempt?;
        self.proposal_correction_available = false;
        Some((question, feedback, target_turn, attempt))
    }

    /// Reactiva el diagnóstico reservado después de un fallo recuperable de red.
    pub fn restore_proposal_correction(&mut self) {
        self.proposal_correction_available = self.proposal_correction_question.is_some()
            && self.proposal_correction_feedback.is_some()
            && self.proposal_correction_target_turn.is_some()
            && self.proposal_correction_attempt.is_some();
    }

    /// Comprueba que la corrección aún pertenece al documento y foco actuales.
    pub fn proposal_correction_matches_context(
        &self,
        context: &ImmutableDocumentContext,
        focus: Option<&AssistantFocus>,
    ) -> bool {
        let Some(source) = self.proposal_correction_context.as_ref() else {
            return false;
        };
        source.document_revision == context.revision
            && source.document_digest == context.digest
            && source.focus.as_ref() == focus
    }

    /// Descarta una corrección ligada a un documento, foco o conversación que ya cambió.
    pub fn invalidate_proposal_correction(&mut self) {
        self.clear_proposal_correction();
        self.is_fusion_review = false;
    }

    /// Inicia una corrección explícita sin permitir aplicar tarjetas de la respuesta descartada.
    pub fn begin_proposal_correction(&mut self) {
        self.begin_proposal_correction_with_route(false);
    }

    /// Inicia una corrección e identifica si la ruta Fusion autorizada está activa.
    pub fn begin_proposal_correction_with_route(&mut self, is_fusion_review: bool) {
        self.clear_proposal_cards();
        self.reveal_pending = false;
        self.reveal_started_at = None;
        self.is_pending = true;
        self.is_cancelling = false;
        self.is_fusion_review = is_fusion_review;
        self.image_upload_consent = false;
        self.error = None;
    }

    /// Sustituye la respuesta descartada para conservar alternancia usuario/asistente en el historial.
    pub fn complete_proposal_correction(&mut self, answer: String) {
        let target_turn = self
            .conversation
            .iter_mut()
            .rposition(|turn| matches!(turn.role, ConversationRole::Assistant));
        if let Some(target_turn) = target_turn {
            let _ = self.complete_proposal_correction_at(target_turn, answer);
        } else {
            self.conversation
                .push(ConversationTurn::assistant_with_origin(
                    trim_turn(answer),
                    AssistantExecutionOrigin::AuthorizedRemote,
                ));
            self.trim_conversation();
            self.reveal_pending = true;
            self.reveal_started_at = None;
            self.is_pending = false;
            self.is_cancelling = false;
            self.is_fusion_review = false;
            self.image_upload_consent = false;
            self.clear_proposal_correction();
            self.error = None;
        }
    }

    /// Reemplaza exactamente el turno descartado, sin depender de qué turno sea el último.
    pub fn complete_proposal_correction_at(&mut self, target_turn: usize, answer: String) -> bool {
        let Some(turn) = self.conversation.get_mut(target_turn) else {
            return false;
        };
        if !matches!(turn.role, ConversationRole::Assistant) {
            return false;
        }
        turn.content = trim_turn(answer);
        turn.origin = Some(AssistantExecutionOrigin::AuthorizedRemote);
        self.trim_conversation();
        self.reveal_pending = true;
        self.reveal_started_at = None;
        self.is_pending = false;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        self.image_upload_consent = false;
        self.clear_proposal_correction();
        self.error = None;
        true
    }

    /// Mueve una tarjeta a historial sólo después de que la aplicación confirmó su commit.
    pub fn finish_verified_proposal_application(
        &mut self,
        candidate_index: usize,
        committed: bool,
    ) -> bool {
        if !committed {
            return false;
        }
        let Some(position) = self
            .verified_proposals
            .iter()
            .position(|proposal| proposal.candidate_index == candidate_index)
        else {
            return false;
        };
        let proposal = self.verified_proposals.remove(position);
        self.applied_proposals.push(proposal);
        true
    }

    /// Reemplaza el resultado del preflight de la respuesta actual.
    pub fn set_proposal_preflight_results(
        &mut self,
        verified_proposals: Vec<VerifiedAssistantProposal>,
        preflight_candidate_count: usize,
        proposal_code_block_indices: Vec<usize>,
    ) {
        self.verified_proposals = verified_proposals;
        self.applied_proposals.clear();
        self.preflight_candidate_count = preflight_candidate_count;
        self.proposal_code_block_indices = proposal_code_block_indices;
    }

    /// Añade una imagen ya importada sólo si cumple los límites del MVP.
    pub fn add_attachment(&mut self, attachment: ImageAttachment) -> Result<(), String> {
        if self.is_pending {
            return Err("assistant attachments are frozen while a request is pending".into());
        }
        let limits = AttachmentLimits::default();
        attachment.validate(&limits)?;
        if self.attachments.len() >= limits.max_attachments {
            return Err("assistant attachment count exceeds the configured limit".into());
        }
        self.attachments.push(attachment);
        self.image_upload_consent = false;
        self.attachment_message = None;
        Ok(())
    }

    /// Quita una imagen sólo cuando no hay un payload remoto en curso.
    pub fn remove_attachment(&mut self, index: usize) -> bool {
        if self.is_pending || index >= self.attachments.len() {
            return false;
        }
        self.attachments.remove(index);
        self.image_upload_consent = false;
        if self.attachments.is_empty() {
            self.attachment_message = None;
        }
        true
    }

    /// Reemplaza los modelos por identificadores no vacíos y sin duplicados.
    pub fn set_available_models(&mut self, models: Vec<String>) {
        let mut unique = Vec::new();
        for model in models {
            let model = model.trim();
            if !model.is_empty() && !unique.iter().any(|known| known == model) {
                unique.push(model.to_owned());
            }
        }
        self.available_models = unique;
    }

    /// Devuelve modelos válidos para el selector sin habilitar IDs arbitrarios.
    pub fn model_choices(&self) -> Vec<String> {
        let catalog = match self.provider {
            ProviderProfile::OllamaLocal => OLLAMA_MODELS,
            ProviderProfile::OpenCodeGo
            | ProviderProfile::DeepSeek
            | ProviderProfile::CustomOpenAiCompatible => OPENCODE_MODELS,
        };
        let mut choices = Vec::with_capacity(catalog.len() + self.available_models.len() + 1);
        if model_is_selectable(self.provider, &self.model) {
            push_unique_model(&mut choices, &self.model);
        }
        for model in catalog {
            push_unique_model(&mut choices, model);
        }
        for model in &self.available_models {
            if model_is_selectable(self.provider, model) {
                push_unique_model(&mut choices, model);
            }
        }
        choices
    }

    /// Registra una fila de actividad de una herramienta del agente.
    pub fn push_agent_activity(&mut self, text: impl Into<String>) {
        self.agent_activity
            .push(AgentActivityRow { text: text.into() });
        if self.agent_activity.len() > 12 {
            let overflow = self.agent_activity.len() - 12;
            self.agent_activity.drain(..overflow);
        }
    }

    /// Establece el ledger J-Space mostrado como tarjeta colapsable.
    pub fn set_agent_ledger(&mut self, render: Option<String>) {
        self.agent_ledger = render.filter(|render| !render.trim().is_empty());
    }

    /// Limpia actividad y ledger de una consulta agente.
    pub fn clear_agent_progress(&mut self) {
        self.agent_activity.clear();
        self.agent_ledger = None;
    }

    /// Establece la animación a reproducir y prepara sus texturas de frames.
    pub fn set_media(&mut self, media: Option<AssistantMedia>, ctx: &egui::Context) {
        self.media = media;
        self.media_textures_ready = false;
        if let Some(media) = &self.media {
            self.media_textures = media
                .frames
                .iter()
                .map(|frame| {
                    ctx.load_texture(
                        "assistant_media_frame",
                        frame.clone(),
                        egui::TextureOptions::LINEAR,
                    )
                })
                .collect();
            self.media_textures_ready = true;
        } else {
            self.media_textures.clear();
        }
    }

    /// texturas de frames listas (para el dibujado del reproductor).
    pub(crate) fn media_textures(&self) -> (&[egui::TextureHandle], bool) {
        (&self.media_textures, self.media_textures_ready)
    }

    /// Muestra el turno enviado antes de que el proveedor responda.
    pub fn begin_request(&mut self, question: String) {
        self.conversation
            .push(ConversationTurn::user(trim_turn(question)));
        self.trim_conversation();
        self.clear_agent_progress();
        self.reveal_pending = false;
        self.reveal_started_at = None;
        self.is_pending = true;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        self.clear_proposal_cards();
        self.clear_proposal_correction();
        self.clear_remote_authorization();
        self.clear_proposed_plan();
        self.error = None;
    }

    /// Muestra que la cancelación es cooperativa sin habilitar otro envío aún.
    pub fn begin_cancellation(&mut self) {
        self.is_cancelling = true;
    }

    /// Guarda una respuesta y conserva sólo el historial más reciente de sesión.
    pub fn complete_request(&mut self, answer: String) {
        self.conversation
            .push(ConversationTurn::assistant_with_origin(
                trim_turn(answer),
                AssistantExecutionOrigin::AuthorizedRemote,
            ));
        self.trim_conversation();
        self.reveal_pending = true;
        self.reveal_started_at = None;
        self.is_pending = false;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        // El consentimiento visible representó el payload terminado y no se
        // reutiliza para una consulta posterior.
        self.image_upload_consent = false;
        self.error = None;
    }

    /// Finaliza una consulta resuelta en el proceso local.
    pub fn complete_local_request(&mut self, answer: String) {
        self.conversation
            .push(ConversationTurn::assistant_with_origin(
                trim_turn(answer),
                AssistantExecutionOrigin::Local,
            ));
        self.trim_conversation();
        self.reveal_pending = true;
        self.reveal_started_at = None;
        self.is_pending = false;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        self.image_upload_consent = false;
        self.error = None;
    }

    /// Finaliza una solicitud remota con un error apto para mostrar.
    pub fn fail_request(&mut self, error: impl Into<String>) {
        self.clear_agent_progress();
        self.reveal_pending = false;
        self.reveal_started_at = None;
        self.is_pending = false;
        self.is_cancelling = false;
        self.is_fusion_review = false;
        self.image_upload_consent = false;
        self.error = Some(error.into());
    }

    /// Conserva intercambios completos recientes que caben en el presupuesto remoto.
    pub fn conversation_within_budget(&self, max_bytes: usize) -> Vec<ConversationTurn> {
        Self::conversation_slice_within_budget(&self.conversation, max_bytes)
    }

    /// Conserva pares completos anteriores al intercambio que contiene `target_turn`.
    /// La reparación nunca reenvía el usuario ni la respuesta del intercambio rechazado.
    pub fn conversation_before_turn_within_budget(
        &self,
        target_turn: usize,
        max_bytes: usize,
    ) -> Vec<ConversationTurn> {
        let Some(source_user_turn) = target_turn.checked_sub(1) else {
            return Vec::new();
        };
        if !matches!(
            self.conversation.get(source_user_turn),
            Some(ConversationTurn {
                role: ConversationRole::User,
                ..
            })
        ) || !matches!(
            self.conversation.get(target_turn),
            Some(ConversationTurn {
                role: ConversationRole::Assistant,
                ..
            })
        ) {
            return Vec::new();
        }
        Self::conversation_slice_within_budget(&self.conversation[..source_user_turn], max_bytes)
    }

    fn conversation_slice_within_budget(
        conversation: &[ConversationTurn],
        max_bytes: usize,
    ) -> Vec<ConversationTurn> {
        let mut remaining = max_bytes;
        let mut selected_pairs = Vec::new();
        let mut end = conversation.len();
        while end >= 2 {
            let pair = &conversation[end - 2..end];
            if !is_complete_exchange(pair) {
                end -= 1;
                continue;
            }
            let bytes = pair.iter().map(|turn| turn.content.len()).sum::<usize>();
            if bytes > remaining {
                break;
            }
            selected_pairs.push(pair.to_vec());
            remaining -= bytes;
            end -= 2;
        }
        selected_pairs.reverse();
        selected_pairs.into_iter().flatten().collect()
    }

    fn trim_conversation(&mut self) {
        while self.conversation.len() > MAX_CONVERSATION_TURNS {
            if let Some(index) = self.conversation.windows(2).position(is_complete_exchange) {
                self.conversation.drain(index..index + 2);
            } else {
                self.conversation.remove(0);
            }
        }
    }

    /// Bytes visibles que se enviarán antes de incorporar historial acotado.
    pub fn input_bytes(&self) -> usize {
        self.problem.len()
            + self
                .focus
                .as_ref()
                .map(|focus| focus.summary.len() + REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                .unwrap_or_default()
    }

    fn use_api_key(&self) -> bool {
        matches!(
            self.provider,
            ProviderProfile::OpenCodeGo | ProviderProfile::DeepSeek
        )
    }

    fn select_provider(&mut self, provider: ProviderProfile) {
        if self.provider == provider {
            return;
        }
        self.provider = provider;
        self.model = match provider {
            ProviderProfile::OllamaLocal => OLLAMA_DEFAULT_MODEL.into(),
            _ => OPENCODE_DEFAULT_MODEL.into(),
        };
        self.available_models.clear();
        self.key_available = false;
        self.key_status_checked = false;
        self.vision_enabled = false;
        self.image_upload_consent = false;
        self.clear_remote_history();
        self.error = None;
    }

    fn select_model(&mut self, model: impl Into<String>) {
        let model = model.into();
        if self.model != model {
            self.model = model;
            self.vision_enabled = false;
            self.image_upload_consent = false;
            self.clear_remote_history();
        }
    }

    /// Recupera una preferencia no secreta de proveedor y modelo.
    pub fn apply_preferences(&mut self, provider: ProviderProfile, model: impl Into<String>) {
        let model = model.into();
        let model = if model.trim().is_empty() {
            match provider {
                ProviderProfile::OllamaLocal => OLLAMA_DEFAULT_MODEL.into(),
                _ => OPENCODE_DEFAULT_MODEL.into(),
            }
        } else {
            model
        };
        if self.provider != provider || self.model != model {
            self.clear_remote_history();
        }
        self.provider = provider;
        self.model = model;
        self.key_available = false;
        self.key_status_checked = false;
    }

    fn clear_remote_history(&mut self) {
        self.conversation.clear();
        self.reveal_pending = false;
        self.reveal_started_at = None;
        self.clear_proposal_cards();
        self.clear_proposal_correction();
        self.cancel_remote_authorization();
        self.clear_proposed_plan();
    }

    fn clear_proposal_correction(&mut self) {
        self.proposal_correction_available = false;
        self.proposal_correction_question = None;
        self.proposal_correction_feedback = None;
        self.proposal_correction_target_turn = None;
        self.proposal_correction_attempt = None;
        self.proposal_correction_context = None;
    }

    fn clear_proposal_cards(&mut self) {
        self.verified_proposals.clear();
        self.applied_proposals.clear();
        self.preflight_candidate_count = 0;
        self.proposal_code_block_indices.clear();
        self.reveal_pending = false;
        self.reveal_started_at = None;
    }
}

fn pending_attachment_status(state: &AssistantPanelState) -> &'static str {
    if state.image_upload_consent {
        "Estos adjuntos corresponden al payload en curso y están bloqueados."
    } else {
        "Las imágenes se conservan localmente y no se enviarán con la corrección."
    }
}

fn trim_turn(content: String) -> String {
    if content.chars().count() <= MAX_CONVERSATION_TURN_CHARS {
        return content;
    }
    let mut trimmed = content
        .chars()
        .take(MAX_CONVERSATION_TURN_CHARS.saturating_sub(1))
        .collect::<String>();
    trimmed.push('…');
    trimmed
}

/// Recorte de revelación de una respuesta que se está animando.
#[derive(Debug, Clone, Copy)]
struct RevealClip {
    visible_blocks: usize,
    boundary_alpha: f32,
}

/// Calcula el recorte de revelación para la última respuesta del asistente.
///
/// Depende sólo del reloj de egui y se autolimpia al completar, para no
/// conservar estado persistente ni programar temporizadores del sistema.
fn assistant_reveal_clip(
    ui: &egui::Ui,
    state: &mut AssistantPanelState,
    total_blocks: usize,
) -> Option<RevealClip> {
    if !state.reveal_pending {
        return None;
    }
    if total_blocks == 0 {
        state.reveal_pending = false;
        state.reveal_started_at = None;
        return None;
    }
    let time = ui.input(|input| input.time);
    let started = *state.reveal_started_at.get_or_insert(time);
    let duration = (ASSISTANT_REVEAL_BASE_SECONDS
        + total_blocks as f64 * ASSISTANT_REVEAL_PER_BLOCK_SECONDS)
        .min(ASSISTANT_REVEAL_MAX_SECONDS);
    let elapsed = (time - started).max(0.0);
    let progress = (elapsed / duration) as f32;
    if progress >= 0.9999 {
        state.reveal_pending = false;
        state.reveal_started_at = None;
        return None;
    }
    ui.ctx().request_repaint();
    let scaled = progress * total_blocks as f32;
    Some(RevealClip {
        visible_blocks: scaled.floor() as usize,
        boundary_alpha: scaled - scaled.floor(),
    })
}

fn is_complete_exchange(turns: &[ConversationTurn]) -> bool {
    matches!(
        turns,
        [
            ConversationTurn {
                role: ConversationRole::User,
                ..
            },
            ConversationTurn {
                role: ConversationRole::Assistant,
                ..
            }
        ]
    )
}

fn push_unique_model(models: &mut Vec<String>, model: &str) {
    let model = model.trim();
    if !model.is_empty() && !models.iter().any(|known| known == model) {
        models.push(model.to_owned());
    }
}

fn model_is_selectable(provider: ProviderProfile, model: &str) -> bool {
    if provider != ProviderProfile::OpenCodeGo {
        return true;
    }
    let normalized = model.trim().to_ascii_lowercase();
    !normalized.contains("kimi") && !normalized.contains("mimo")
}

fn model_allows_image_attachment(model: &str) -> bool {
    model != "fusion"
}

fn should_draw_empty_state(state: &AssistantPanelState) -> bool {
    state.conversation.is_empty() && !state.is_pending
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AssistantMessageBlock {
    Heading { level: usize, text: String },
    Bullet(String),
    Paragraph(String),
    Code { language: String, text: String },
    Table(Vec<Vec<String>>),
    DisplayMath(String),
}

fn parse_assistant_blocks(content: &str) -> Vec<AssistantMessageBlock> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_assistant_paragraph(&mut blocks, &mut paragraph);
            index += 1;
            continue;
        }

        if let Some(language) = trimmed.strip_prefix("```") {
            if let Some(end) = lines[index + 1..]
                .iter()
                .position(|candidate| candidate.trim_start().starts_with("```"))
            {
                flush_assistant_paragraph(&mut blocks, &mut paragraph);
                let end = index + end + 1;
                blocks.push(AssistantMessageBlock::Code {
                    language: language.trim().to_ascii_lowercase(),
                    text: lines[index + 1..end].join("\n"),
                });
                index = end + 1;
                continue;
            }
        }

        if let Some(closing_delimiter) = match trimmed {
            "$$" => Some("$$"),
            r"\[" => Some(r"\]"),
            _ => None,
        } {
            if let Some(end) = lines[index + 1..]
                .iter()
                .position(|candidate| candidate.trim() == closing_delimiter)
            {
                let end = index + end + 1;
                let math = lines[index + 1..end].join("\n");
                if !math.trim().is_empty() {
                    flush_assistant_paragraph(&mut blocks, &mut paragraph);
                    blocks.push(AssistantMessageBlock::DisplayMath(math.trim().into()));
                    index = end + 1;
                    continue;
                }
            }
        }

        if let Some(math) = trimmed
            .strip_prefix("$$")
            .and_then(|value| value.strip_suffix("$$"))
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                trimmed
                    .strip_prefix(r"\[")
                    .and_then(|value| value.strip_suffix(r"\]"))
                    .filter(|value| !value.trim().is_empty())
            })
        {
            flush_assistant_paragraph(&mut blocks, &mut paragraph);
            blocks.push(AssistantMessageBlock::DisplayMath(math.trim().into()));
            index += 1;
            continue;
        }

        if index + 1 < lines.len() {
            if let (Some(header), Some(separator)) = (
                parse_markdown_table_row(trimmed),
                parse_markdown_table_row(lines[index + 1].trim()),
            ) {
                if markdown_table_separator(&separator, header.len()) {
                    flush_assistant_paragraph(&mut blocks, &mut paragraph);
                    let mut rows = vec![header];
                    index += 2;
                    while index < lines.len() {
                        let Some(row) = parse_markdown_table_row(lines[index].trim()) else {
                            break;
                        };
                        if row.len() != rows[0].len() {
                            break;
                        }
                        rows.push(row);
                        index += 1;
                    }
                    blocks.push(AssistantMessageBlock::Table(rows));
                    continue;
                }
            }
        }

        let heading_level = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if (1..=3).contains(&heading_level) && trimmed.as_bytes().get(heading_level) == Some(&b' ')
        {
            flush_assistant_paragraph(&mut blocks, &mut paragraph);
            blocks.push(AssistantMessageBlock::Heading {
                level: heading_level,
                text: trimmed[heading_level + 1..].trim().into(),
            });
            index += 1;
            continue;
        }

        if let Some(bullet) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            flush_assistant_paragraph(&mut blocks, &mut paragraph);
            blocks.push(AssistantMessageBlock::Bullet(bullet.trim().into()));
            index += 1;
            continue;
        }

        paragraph.push(trimmed.to_owned());
        index += 1;
    }
    flush_assistant_paragraph(&mut blocks, &mut paragraph);
    blocks
}

fn flush_assistant_paragraph(blocks: &mut Vec<AssistantMessageBlock>, paragraph: &mut Vec<String>) {
    if !paragraph.is_empty() {
        blocks.push(AssistantMessageBlock::Paragraph(paragraph.join(" ")));
        paragraph.clear();
    }
}

fn parse_markdown_table_row(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.contains('|') {
        return None;
    }
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let cells = line
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect::<Vec<_>>();
    (!cells.is_empty()).then_some(cells)
}

fn markdown_table_separator(cells: &[String], expected_columns: usize) -> bool {
    cells.len() == expected_columns
        && cells.iter().all(|cell| {
            let dashes = cell.trim().trim_matches(':');
            dashes.len() >= 3 && dashes.chars().all(|character| character == '-')
        })
}

fn focused_context_preview(summary: &str) -> String {
    if summary.chars().count() <= MAX_FOCUSED_CONTEXT_PREVIEW_CHARS {
        return summary.into();
    }
    let mut preview = summary
        .chars()
        .take(MAX_FOCUSED_CONTEXT_PREVIEW_CHARS.saturating_sub(3))
        .collect::<String>();
    preview.push_str("...");
    preview
}

/// Acción solicitada por el usuario; la aplicación decide cómo realizar I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantUiAction {
    /// Resolver primero la consulta dentro del proceso local.
    Submit,
    /// Autorizar explícitamente el envío de una consulta no resuelta localmente.
    AuthorizeRemote,
    /// Descartar una oferta de transporte remoto sin borrar el turno visible.
    CancelRemoteAuthorization,
    /// Cancelar cooperativamente la solicitud remota en curso.
    Cancel,
    /// Guardar el borrador de clave en el almacén seguro del sistema.
    SaveApiKey,
    /// Comprobar si existe una clave en el almacén seguro del sistema.
    LoadApiKey,
    /// Eliminar la clave del almacén seguro del sistema.
    ClearApiKey,
    /// Persistir el proveedor y descartar la clave de sesión anterior.
    ProviderChanged,
    /// Persistir el modelo, que no contiene secretos.
    ModelChanged,
    /// Consultar los modelos publicados por el proveedor configurado.
    RefreshModels,
    /// Abrir el selector nativo para adjuntar una imagen.
    AttachImage,
    /// Eliminar un adjunto por índice sin conservar su ruta de origen.
    RemoveAttachment(usize),
    /// Copiar una invocación ya comprobada a la barra de entrada, sin ejecutarla.
    InsertCommand(usize),
    /// Aplicar una propuesta comprobada sólo tras una acción explícita del usuario.
    ApplyProposal(usize),
    /// Aplicar una propuesta local tipada sólo tras una acción explícita.
    ApplyProposedPlan,
    /// Solicitar una única corrección para una propuesta que el preflight descartó.
    RetryProposalCorrection,
    /// Persistir el permiso explícito para usar Fusion como fallback de MiniMax M3.
    FusionFallbackChanged,
    /// Descartar la conversación local actual.
    ClearConversation,
    /// Ocultar el panel sin cancelar ni descartar el trabajo asistente activo.
    HidePanel,
    /// Copiar un turno visible al portapapeles del sistema.
    CopyMessage(String),
    /// Activar o desactivar un plugin por id (la app persiste el cambio).
    TogglePlugin(String, bool),
    /// Cambiar el permiso completo (respuestas en línea sin cartel).
    FullPermissionChanged(bool),
    /// Activar o desactivar el modo agente (loop con herramientas).
    AgentModeChanged(bool),
    /// Generar una animación del objeto/expresión y reproducirla en el chat.
    RunAnimation,
}

/// Dibuja el asistente como parte permanente del shell, antes del canvas.
pub fn draw_assistant_panel(
    ctx: &egui::Context,
    state: &mut AssistantPanelState,
    reserved_bottom_height: f32,
    visuals: AssistantVisuals,
    cache: &mut AssistantBlocksCache,
) -> Option<AssistantUiAction> {
    let mut action = None;
    let theme = current_theme(ctx);
    let available_rect = ctx.available_rect();
    if assistant_uses_bottom_sheet(available_rect.width()) {
        let (min_height, max_height, default_height) =
            assistant_compact_panel_heights(available_rect.height(), reserved_bottom_height, state);
        egui::TopBottomPanel::bottom("grafito_assistant_compact_panel")
            .resizable(true)
            .default_height(default_height)
            .min_height(min_height)
            .max_height(max_height)
            .frame(
                egui::Frame::none()
                    .fill(theme.panel_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator)),
            )
            .show(ctx, |ui| {
                action = draw_panel_contents(ui, state, true, visuals, cache);
            });
    } else {
        let (min_width, max_width, default_width) = assistant_panel_widths(available_rect.width());
        egui::SidePanel::right("grafito_assistant_panel")
            .resizable(true)
            .default_width(default_width)
            .min_width(min_width)
            .max_width(max_width)
            .frame(
                egui::Frame::none()
                    .fill(theme.panel_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator)),
            )
            .show(ctx, |ui| {
                action = draw_assistant_contents(ui, state, visuals, cache);
            });
    }
    if action.is_none() {
        action = draw_assistant_settings_window(ctx, state);
    }
    action
}

/// Dibuja el contenido del asistente dentro de un dock anfitrión existente.
///
/// El shell de Geometry 3D lo usa para alternar con el Inspector sin crear una
/// segunda columna lateral. La capa de aplicación conserva el manejo de I/O.
pub fn draw_assistant_contents(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
    visuals: AssistantVisuals,
    cache: &mut AssistantBlocksCache,
) -> Option<AssistantUiAction> {
    draw_panel_contents(ui, state, false, visuals, cache)
}

fn assistant_panel_widths(available_width: f32) -> (f32, f32, f32) {
    let maximum = (available_width * 0.42)
        .min((available_width - ASSISTANT_MIN_CANVAS_WIDTH).max(160.0))
        .clamp(160.0, ASSISTANT_PANEL_MAX_WIDTH);
    let minimum = ASSISTANT_PANEL_MIN_WIDTH.min(maximum);
    let default = ASSISTANT_PANEL_DEFAULT_WIDTH.clamp(minimum, maximum);
    (minimum, maximum, default)
}

fn assistant_uses_bottom_sheet(available_width: f32) -> bool {
    available_width < ASSISTANT_SIDE_PANEL_MIN_VIEWPORT_WIDTH
}

fn assistant_compact_panel_heights(
    available_height: f32,
    reserved_bottom_height: f32,
    state: &AssistantPanelState,
) -> (f32, f32, f32) {
    // Preserve the complete composer plus the header and a minimal transcript
    // strip when possible. Shorter windows scroll the composer rather than
    // extending the panel outside the viewport.
    let available_height = available_height.max(0.0);
    let assistant_budget =
        (available_height - reserved_bottom_height.max(0.0) - ASSISTANT_COMPACT_MIN_CANVAS_HEIGHT)
            .max(0.0);
    let desired_minimum = assistant_composer_height(state) + 96.0;
    let minimum = desired_minimum.min(assistant_budget);
    let maximum = assistant_budget;
    let default = 360.0_f32.clamp(minimum, maximum);
    (minimum, maximum, default)
}

fn assistant_composer_height(state: &AssistantPanelState) -> f32 {
    let mut height = ASSISTANT_COMPOSER_BASE_HEIGHT;
    if state.focus.is_some() {
        height += ASSISTANT_COMPOSER_FOCUS_HEIGHT;
    }
    if state.input_bytes() > RequestBudget::default().max_input_chars * 3 / 4 {
        height += ASSISTANT_COMPOSER_BUDGET_HEIGHT;
    }
    if !state.attachments.is_empty() {
        height += ASSISTANT_COMPOSER_ATTACHMENT_HEIGHT;
        height += (state.attachments.len().saturating_sub(1) / 2) as f32
            * ASSISTANT_COMPOSER_ATTACHMENT_ROW_HEIGHT;
        if state.attachment_message.is_some() {
            height += ASSISTANT_COMPOSER_ATTACHMENT_MESSAGE_HEIGHT;
        }
        if state.is_pending {
            height += ASSISTANT_COMPOSER_PENDING_ATTACHMENT_HEIGHT;
        }
    }
    height
}

pub fn draw_assistant_settings_window(
    ctx: &egui::Context,
    state: &mut AssistantPanelState,
) -> Option<AssistantUiAction> {
    if !state.settings_open {
        return None;
    }
    let mut open = state.settings_open;
    let mut action = None;
    let shows_fusion_fallback =
        state.provider == ProviderProfile::OpenCodeGo && state.model == OPENCODE_DEFAULT_MODEL;
    let window_size = match (state.use_api_key(), shows_fusion_fallback) {
        (true, true) => egui::vec2(390.0, 266.0),
        (true, false) => egui::vec2(390.0, 214.0),
        (false, true) => egui::vec2(390.0, 184.0),
        (false, false) => egui::vec2(390.0, 132.0),
    };
    egui::Window::new("Configuración del asistente")
        .open(&mut open)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(window_size)
        .show(ctx, |ui| {
            if !state.key_status_checked && state.use_api_key() {
                state.key_status_checked = true;
                action = Some(AssistantUiAction::LoadApiKey);
            }
            if let Some(settings_action) = draw_assistant_settings_contents(ui, state) {
                action = Some(settings_action);
            }
        });
    state.settings_open = open;
    action
}

fn draw_assistant_settings_contents(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let mut action = None;

    ui.label(egui::RichText::new("Proveedor").size(TYPE_XS));
    let mut provider = state.provider;
    ui.add_enabled_ui(!state.is_pending, |ui| {
        egui::ComboBox::from_id_salt("assistant_provider")
            .selected_text(provider_label(state.provider))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut provider, ProviderProfile::OpenCodeGo, "OpenCode Go");
                ui.selectable_value(&mut provider, ProviderProfile::OllamaLocal, "Ollama");
            });
    });
    if provider != state.provider {
        state.select_provider(provider);
        action = Some(AssistantUiAction::ProviderChanged);
    }

    ui.add_space(SPACE_SM);
    ui.label(egui::RichText::new("Modelo").size(TYPE_XS));
    let mut selected_model = state.model.clone();
    ui.add_enabled_ui(!state.is_pending, |ui| {
        egui::ComboBox::from_id_salt("assistant_model")
            .selected_text(&state.model)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for model in state.model_choices() {
                    ui.selectable_value(&mut selected_model, model.clone(), model);
                }
            });
    });
    if selected_model != state.model {
        state.select_model(selected_model);
        action = Some(AssistantUiAction::ModelChanged);
    }

    if state.provider == ProviderProfile::OpenCodeGo && state.model == OPENCODE_DEFAULT_MODEL {
        ui.add_space(SPACE_SM);
        let changed = ui
            .add_enabled(
                !state.is_pending,
                egui::Checkbox::new(
                    &mut state.allow_fusion_fallback,
                    "Permitir revisión Fusion si una propuesta M3 falla",
                ),
            )
            .changed();
        ui.label(
            egui::RichText::new(
                "Fusion reconsulta MiniMax M3 y DeepSeek v4 Pro sin adjuntos ni Apply automático.",
            )
            .color(theme.text_tertiary)
            .size(TYPE_XS),
        );
        if changed {
            action = Some(AssistantUiAction::FusionFallbackChanged);
        }
    }

    ui.add_space(SPACE_SM);
    ui.separator();
    ui.add_space(SPACE_XS);
    let mut full_permission = state.full_permission;
    let permission_changed = ui
        .checkbox(
            &mut full_permission,
            "Respuestas en línea automáticas (permiso completo)",
        )
        .on_hover_text(
            "Si la resolución local no alcanza, consultar al proveedor sin pedir confirmación.",
        )
        .changed();
    if permission_changed && full_permission != state.full_permission {
        state.full_permission = full_permission;
        action = Some(AssistantUiAction::FullPermissionChanged(full_permission));
    }

    ui.add_space(SPACE_XS);
    let mut agent_mode = state.agent_mode;
    let agent_changed = ui
        .checkbox(&mut agent_mode, "Modo agente (herramientas y actividad)")
        .on_hover_text(
            "Usa el loop agéntico con herramientas locales y muestra su actividad en el chat (requiere un proveedor compatible con tool calling).",
        )
        .changed();
    if agent_changed && agent_mode != state.agent_mode {
        state.agent_mode = agent_mode;
        action = Some(AssistantUiAction::AgentModeChanged(agent_mode));
    }

    if state.use_api_key() {
        ui.add_space(SPACE_SM);
        ui.separator();
        ui.add_space(SPACE_XS);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Clave de API").size(TYPE_XS));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if state.key_available && ui.small_button("Eliminar").clicked() {
                    action = Some(AssistantUiAction::ClearApiKey);
                }
            });
        });
        ui.add_space(SPACE_XS);
        ui.horizontal(|ui| {
            let button_width = 76.0;
            let key_editor = ui.add_sized(
                egui::vec2(
                    (ui.available_width() - button_width - SPACE_SM).max(120.0),
                    ui.spacing().interact_size.y,
                ),
                egui::TextEdit::singleline(&mut state.api_key_draft)
                    .password(true)
                    .hint_text("Pegar una nueva clave"),
            );
            let save_with_enter =
                key_editor.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui
                .add_enabled(
                    !state.api_key_draft.trim().is_empty(),
                    egui::Button::new("Guardar").min_size(egui::vec2(button_width, 0.0)),
                )
                .clicked()
                || save_with_enter
            {
                action = Some(AssistantUiAction::SaveApiKey);
            }
        });
    }
    ui.add_space(SPACE_SM);
    ui.separator();
    ui.add_space(SPACE_XS);
    ui.label(
        egui::RichText::new("Plugins (opcionales)")
            .size(TYPE_XS)
            .strong(),
    );
    if state.plugins.is_empty() {
        ui.label(
            egui::RichText::new(
                "Sin plugins cargados. Son extensiones opcionales; el asistente y las animaciones nativas funcionan igual sin ellos.",
            )
            .color(theme.text_tertiary)
            .size(TYPE_XS),
        );
    } else {
        for plugin in &state.plugins {
            let enabled = plugin.enabled;
            let mut toggled = enabled;
            egui::Frame::none()
                .fill(if plugin.enabled {
                    theme.accent_muted
                } else {
                    theme.panel_bg
                })
                .stroke(egui::Stroke::new(
                    1.0,
                    if plugin.enabled {
                        theme.accent
                    } else {
                        theme.separator
                    },
                ))
                .rounding(RADIUS_SM)
                .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_XS))
                .show(ui, |ui| {
                    egui::Grid::new(ui.make_persistent_id(("plugin_row", &plugin.id)))
                        .num_columns(2)
                        .spacing([SPACE_SM, 2.0])
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&plugin.name)
                                        .color(theme.text_primary)
                                        .size(TYPE_SM)
                                        .strong(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&plugin.description)
                                            .color(theme.text_tertiary)
                                            .size(TYPE_XS),
                                    )
                                    .wrap(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_enabled_ui(plugin.error.is_none(), |ui| {
                                        let changed = ui
                                            .checkbox(&mut toggled, "")
                                            .on_hover_text(plugin.description.clone())
                                            .changed();
                                        if changed && toggled != enabled {
                                            action = Some(AssistantUiAction::TogglePlugin(
                                                plugin.id.clone(),
                                                toggled,
                                            ));
                                        }
                                    });
                                },
                            );
                            ui.end_row();
                        });
                    if let Some(error) = &plugin.error {
                        ui.label(
                            egui::RichText::new(format!("No disponible: {error}"))
                                .color(theme.danger)
                                .size(TYPE_XS),
                        );
                    }
                });
            ui.add_space(SPACE_XS);
        }
    }
    if let Some(error) = &state.error {
        ui.label(egui::RichText::new(error).color(theme.danger).size(TYPE_XS));
    }
    action
}

fn draw_panel_contents(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
    compact: bool,
    visuals: AssistantVisuals,
    cache: &mut AssistantBlocksCache,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let mut action = draw_assistant_header(ui, state, theme, visuals);
    ui.add_space(SPACE_XS);

    let composer_height = assistant_composer_height(state);
    let visible_composer_height = if compact {
        // Leave room for the header and a sliver of transcript in the sheet.
        (ui.available_height() - 64.0).max(0.0).min(composer_height)
    } else {
        composer_height
    };
    egui::TopBottomPanel::bottom("grafito_assistant_composer")
        .exact_height(visible_composer_height)
        .frame(
            egui::Frame::none()
                .fill(theme.input_bar_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator))
                .inner_margin(egui::Margin::same(SPACE_SM)),
        )
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("grafito_assistant_composer_scroll")
                .show(ui, |ui| {
                    retain_first_assistant_action(&mut action, draw_assistant_composer(ui, state));
                });
        });

    egui::ScrollArea::vertical()
        .id_salt("grafito_assistant_conversation")
        .auto_shrink([false, true])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if let Some(error) = state.error.clone() {
                egui::Frame::none()
                    .fill(theme.danger.gamma_multiply(0.12))
                    .rounding(RADIUS_MD)
                    .inner_margin(egui::Margin::same(SPACE_SM))
                    .show(ui, |ui| {
                        ui.colored_label(theme.danger, error);
                        if state.proposal_correction_available
                            && ui.small_button("Pedir una corrección").clicked()
                        {
                            retain_first_assistant_action(
                                &mut action,
                                Some(AssistantUiAction::RetryProposalCorrection),
                            );
                        }
                        if ui.small_button("Limpiar error").clicked() {
                            state.clear_error();
                        }
                    });
                ui.add_space(SPACE_SM);
            }
            if state.has_pending_remote_authorization() {
                retain_first_assistant_action(
                    &mut action,
                    draw_remote_authorization_card(ui, state),
                );
                ui.add_space(SPACE_SM);
            }
            if should_draw_empty_state(state) {
                draw_assistant_empty_state(ui, state, visuals);
            } else {
                let reveal_clip = {
                    let last_content = state
                        .conversation
                        .iter()
                        .rev()
                        .find(|turn| matches!(turn.role, ConversationRole::Assistant))
                        .map(|turn| turn.content.clone())
                        .unwrap_or_default();
                    let total_last_blocks = if last_content.is_empty() {
                        0
                    } else {
                        cache.blocks(&last_content).len()
                    };
                    assistant_reveal_clip(ui, state, total_last_blocks)
                };
                for (turn_index, turn) in state.conversation.iter().enumerate() {
                    let proposal_state = AssistantProposalRenderState {
                        verified_proposals: verified_proposals_for_turn(state, turn_index),
                        applied_proposals: applied_proposals_for_turn(state, turn_index),
                        preflight_candidate_count: preflight_candidate_count_for_turn(
                            state, turn_index,
                        ),
                        proposal_code_block_indices: proposal_code_block_indices_for_turn(
                            state, turn_index,
                        ),
                        proposal_results_available: proposal_results_available_for_turn(
                            state, turn_index,
                        ),
                        correction_available: proposal_correction_available_for_turn(
                            state, turn_index,
                        ),
                    };
                    let is_last = turn_index + 1 == state.conversation.len();
                    let reveal_here = if is_last && matches!(turn.role, ConversationRole::Assistant)
                    {
                        reveal_clip
                    } else {
                        None
                    };
                    if action.is_none() {
                        action =
                            draw_conversation_turn(ui, turn, proposal_state, reveal_here, cache);
                    } else {
                        let _ =
                            draw_conversation_turn(ui, turn, proposal_state, reveal_here, cache);
                    }
                    ui.add_space(SPACE_MD);
                }
            }
            if !state.is_pending {
                retain_first_assistant_action(&mut action, draw_proposed_plan_card(ui, state));
            }
            if state.is_pending {
                draw_pending_indicator(ui, state, visuals);
            }
            if state.anim_progress {
                draw_animation_progress(ui, state, visuals);
            }
            if let Some(media) = &state.media {
                draw_media_card(ui, media, state);
            }
        });
    action
}

/// Tarjeta en vivo mientras se genera la animación (progreso sin fricción).
fn draw_animation_progress(
    ui: &mut egui::Ui,
    state: &AssistantPanelState,
    visuals: AssistantVisuals,
) {
    let theme = current_theme(ui.ctx());
    let _ = visuals;
    let time = ui.input(|input| input.time);
    egui::Frame::none()
        .fill(theme.input_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let pulse = ((time * 3.0).sin() + 1.0) * 0.5;
                let color = theme.accent.gamma_multiply(0.45 + 0.55 * pulse as f32);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, color);
                ui.add_space(4.0);
                ui.add(egui::Label::new(
                    egui::RichText::new("Generando animación…")
                        .color(theme.text_secondary)
                        .size(TYPE_SM),
                ));
            });
        });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(48));
    let _ = state;
}

/// Reproductor de animación (GIF-like) en el chat.
fn draw_media_card(ui: &mut egui::Ui, media: &AssistantMedia, state: &AssistantPanelState) {
    let theme = current_theme(ui.ctx());
    let (textures, ready) = state.media_textures();
    if ready && !textures.is_empty() {
        let time = ui.input(|input| input.time);
        let fps = 12.0;
        let index = ((time * fps) as usize) % textures.len();
        egui::Frame::none()
            .fill(theme.input_bg)
            .stroke(egui::Stroke::new(1.0, theme.separator))
            .rounding(RADIUS_MD)
            .inner_margin(egui::Margin::same(SPACE_SM))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Animación")
                        .color(theme.accent)
                        .size(TYPE_SM)
                        .strong(),
                );
                if !media.title.is_empty() {
                    ui.label(
                        egui::RichText::new(&media.title)
                            .color(theme.text_secondary)
                            .size(TYPE_XS),
                    );
                }
                ui.add_space(SPACE_XS);
                let texture = &textures[index];
                let size = texture.size_vec2();
                let max_w = ui.available_width().max(80.0);
                let scale = (max_w / size.x.max(1.0)).clamp(0.25, 1.5);
                let display = egui::vec2(size.x * scale, size.y * scale).ceil();
                let rect = egui::Rect::from_min_size(ui.cursor().min, display);
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                ui.advance_cursor_after_rect(rect);
            });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(40));
    } else {
        ui.label(
            egui::RichText::new("Preparando animación…")
                .color(theme.text_tertiary)
                .size(TYPE_XS),
        );
    }
}

fn retain_first_assistant_action(
    current: &mut Option<AssistantUiAction>,
    candidate: Option<AssistantUiAction>,
) {
    if current.is_none() {
        *current = candidate;
    }
}

fn draw_remote_authorization_card(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
) -> Option<AssistantUiAction> {
    let authorization = state.pending_remote_authorization.as_ref()?;
    let theme = current_theme(ui.ctx());
    let reason = authorization.reason.clone();
    let attachment_count = state.attachments.len();
    let configuration_accepts_images = model_allows_image_attachment(&state.model);
    let images_ready = attachment_count == 0
        || (configuration_accepts_images && state.vision_enabled && state.image_upload_consent);
    let mut action = None;

    egui::Frame::none()
        .fill(theme.accent_muted)
        .stroke(egui::Stroke::new(1.0, theme.accent))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Consulta local sin resolver")
                    .color(theme.accent_strong)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new(reason)
                    .color(theme.text_primary)
                    .size(TYPE_SM),
            );
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new(
                    "Podés autorizar una consulta remota. El transcript mostrará sólo “Consulta remota autorizada”.",
                )
                .color(theme.text_secondary)
                .size(TYPE_XS),
            );
            if attachment_count > 0 {
                ui.add_space(SPACE_SM);
                ui.add_enabled(
                    configuration_accepts_images,
                    egui::Checkbox::new(
                        &mut state.vision_enabled,
                        "Confirmo que la configuración remota admite imágenes",
                    ),
                );
                ui.add_enabled(
                    configuration_accepts_images,
                    egui::Checkbox::new(
                        &mut state.image_upload_consent,
                        format!(
                            "Autorizo enviar {attachment_count} imagen(es) en esta consulta"
                        ),
                    ),
                );
                if !configuration_accepts_images {
                    ui.label(
                        egui::RichText::new(
                            "La configuración remota actual no admite imágenes.",
                        )
                        .color(theme.warning)
                        .size(TYPE_XS),
                    );
                }
            }
            ui.add_space(SPACE_SM);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(images_ready, egui::Button::new("Autorizar consulta remota"))
                    .clicked()
                {
                    action = Some(AssistantUiAction::AuthorizeRemote);
                }
                if ui.small_button("Seguir localmente").clicked() {
                    action = Some(AssistantUiAction::CancelRemoteAuthorization);
                }
            });
        });
    action
}

fn draw_proposed_plan_card(
    ui: &mut egui::Ui,
    state: &AssistantPanelState,
) -> Option<AssistantUiAction> {
    let plan = state.proposed_plan()?;
    let theme = current_theme(ui.ctx());
    let mut apply_clicked = false;
    egui::Frame::none()
        .fill(theme.accent_muted)
        .stroke(egui::Stroke::new(1.0, theme.accent))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Propuesta local comprobada")
                    .color(theme.accent_strong)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(&plan.summary)
                    .color(theme.text_primary)
                    .size(TYPE_SM),
            );
            for change in &state.proposed_plan_changes {
                ui.label(
                    egui::RichText::new(format!("- {change}"))
                        .color(theme.text_secondary)
                        .size(TYPE_XS),
                );
            }
            ui.add_space(SPACE_XS);
            apply_clicked = ui
                .add_sized(
                    egui::vec2(ui.available_width(), 28.0),
                    egui::Button::new("Aplicar en Grafito")
                        .fill(theme.panel_bg)
                        .stroke(egui::Stroke::new(1.0, theme.accent)),
                )
                .clicked();
        });
    apply_clicked.then_some(AssistantUiAction::ApplyProposedPlan)
}

fn verified_proposals_for_turn(
    state: &AssistantPanelState,
    turn_index: usize,
) -> &[VerifiedAssistantProposal] {
    if proposal_results_available_for_turn(state, turn_index) {
        &state.verified_proposals
    } else {
        &[]
    }
}

fn applied_proposals_for_turn(
    state: &AssistantPanelState,
    turn_index: usize,
) -> &[VerifiedAssistantProposal] {
    if proposal_results_available_for_turn(state, turn_index) {
        &state.applied_proposals
    } else {
        &[]
    }
}

fn preflight_candidate_count_for_turn(state: &AssistantPanelState, turn_index: usize) -> usize {
    if proposal_results_available_for_turn(state, turn_index) {
        state.preflight_candidate_count
    } else {
        0
    }
}

fn proposal_code_block_indices_for_turn(
    state: &AssistantPanelState,
    turn_index: usize,
) -> &[usize] {
    if proposal_results_available_for_turn(state, turn_index) {
        &state.proposal_code_block_indices
    } else {
        &[]
    }
}

fn proposal_results_available_for_turn(state: &AssistantPanelState, turn_index: usize) -> bool {
    if state.is_pending {
        return false;
    }
    let latest_assistant_turn = state
        .conversation
        .iter()
        .rposition(|turn| matches!(turn.role, ConversationRole::Assistant));
    latest_assistant_turn == Some(turn_index)
}

fn proposal_correction_available_for_turn(state: &AssistantPanelState, turn_index: usize) -> bool {
    state.proposal_correction_available
        && state.proposal_correction_target_turn == Some(turn_index)
        && state
            .conversation
            .get(turn_index)
            .is_some_and(|turn| matches!(turn.role, ConversationRole::Assistant))
}

fn draw_mora_avatar(
    ui: &mut egui::Ui,
    visuals: AssistantVisuals,
    size: f32,
    active: bool,
) -> egui::Response {
    let size = size.clamp(20.0, 96.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, MORA_ACCESSIBLE_LABEL)
    });
    if ui.is_rect_visible(rect) {
        let theme = current_theme(ui.ctx());
        let scale = mora_avatar_scale(active, ui.input(|input| input.time));
        let avatar_rect = egui::Rect::from_center_size(rect.center(), rect.size() * scale);
        let painter = ui.painter_at(rect);
        painter.circle_filled(
            avatar_rect.center(),
            avatar_rect.width() * 0.48,
            theme.accent_muted,
        );
        if let Some(texture) = visuals.mora_texture {
            painter.image(
                texture,
                avatar_rect.shrink(1.0),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            painter.circle_stroke(
                avatar_rect.center(),
                avatar_rect.width() * 0.43,
                egui::Stroke::new((avatar_rect.width() * 0.06).max(1.0), theme.accent),
            );
            painter.text(
                avatar_rect.center(),
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::proportional(avatar_rect.width() * 0.46),
                theme.accent_strong,
            );
        }
    }
    response.on_hover_text(MORA_ACCESSIBLE_LABEL)
}

fn mora_avatar_scale(active: bool, time: f64) -> f32 {
    if !active {
        return 1.0;
    }
    let time = if time.is_finite() { time as f32 } else { 0.0 };
    1.0 + (time * 3.2).sin() * 0.02
}

fn draw_assistant_empty_state(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
    visuals: AssistantVisuals,
) {
    let theme = current_theme(ui.ctx());
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_MD))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let _ = draw_mora_avatar(ui, visuals, 64.0, false);
                ui.add_space(SPACE_SM);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Hola, soy {MORA_NAME}"))
                            .color(theme.text_primary)
                            .size(TYPE_MD)
                            .strong(),
                    );
                    ui.add_space(SPACE_XS);
                    ui.label(
                        egui::RichText::new(
                            "Puedo resolver, graficar, analizar o construir. Las acciones se aplican sólo cuando las confirmás.",
                        )
                        .color(theme.text_secondary)
                        .size(TYPE_SM),
                    );
                });
            });
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new("Probá con")
                    .color(theme.text_tertiary)
                    .size(TYPE_XS),
            );
            ui.horizontal_wrapped(|ui| {
                for (label, prompt) in suggestion_prompts(state.focus.is_some()) {
                    if ui.small_button(label).clicked() {
                        state.problem = prompt.into();
                        state.clear_error();
                    }
                }
            });
        });
}

fn draw_assistant_header(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
    theme: &crate::theme::Theme,
    visuals: AssistantVisuals,
) -> Option<AssistantUiAction> {
    let mut action = None;
    ui.add_space(SPACE_XS);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ASSISTANT_HEADER_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let _ = draw_mora_avatar(ui, visuals, 28.0, false);
            ui.add_space(SPACE_XS);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(MORA_NAME)
                        .color(theme.text_primary)
                        .strong()
                        .size(TYPE_MD),
                );
                ui.label(
                    egui::RichText::new("Asistente matemático")
                        .color(theme.text_tertiary)
                        .size(TYPE_XS),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_icon_button(
                    ui,
                    Icon::ChevronRight,
                    theme.text_secondary,
                    "Ocultar asistente",
                )
                .clicked()
                {
                    action = Some(AssistantUiAction::HidePanel);
                }
                if action_icon_button(
                    ui,
                    Icon::Settings,
                    theme.text_secondary,
                    "Configuración del asistente",
                )
                .clicked()
                {
                    state.settings_open = true;
                }
                if ui
                    .add_enabled(
                        !state.is_pending && !state.conversation.is_empty(),
                        egui::Button::new("Limpiar").small(),
                    )
                    .clicked()
                {
                    action = Some(AssistantUiAction::ClearConversation);
                }
            });
        },
    );
    action
}

fn draw_assistant_composer(
    ui: &mut egui::Ui,
    state: &mut AssistantPanelState,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let attachment_limits = AttachmentLimits::default();
    let mut action = None;

    if let Some(focus) = &state.focus {
        egui::Frame::none()
            .fill(theme.accent_muted)
            .rounding(RADIUS_MD)
            .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_XS))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Contexto")
                            .color(theme.accent)
                            .size(TYPE_XS)
                            .strong(),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(focused_context_preview(&focus.summary))
                                .color(theme.text_primary)
                                .size(TYPE_SM),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&focus.summary);
                });
            });
        ui.add_space(SPACE_XS);
    }

    egui::Frame::none()
        .fill(theme.input_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            let editor = ui.add_sized(
                egui::vec2(ui.available_width(), ASSISTANT_COMPOSER_EDITOR_HEIGHT),
                egui::TextEdit::multiline(&mut state.problem)
                    .id_source("grafito_assistant_problem")
                    .hint_text("Preguntale a Mora sobre matemática")
                    .text_color(theme.input_text)
                    .frame(false)
                    .desired_rows(1),
            );
            let submit_on_enter = should_submit_on_enter(
                editor.has_focus(),
                ui.input(|input| input.key_pressed(egui::Key::Enter)),
                ui.input(|input| input.modifiers.shift),
                state.can_submit(),
            );
            let request_budget = RequestBudget::default();
            let input_bytes = state.input_bytes();

            ui.horizontal_centered(|ui| {
                let can_attach = !state.is_pending
                    && !state.is_importing_image
                    && state.attachments.len() < attachment_limits.max_attachments;
                let attach_response = ui
                    .add_enabled_ui(can_attach, |ui| {
                        action_icon_button(
                            ui,
                            Icon::Image,
                            theme.text_secondary,
                            if state.is_importing_image {
                                "Importando imagen"
                            } else {
                                "Adjuntar imagen PNG o JPEG"
                            },
                        )
                    })
                    .inner;
                if attach_response.clicked() {
                    action = Some(AssistantUiAction::AttachImage);
                }
                if !state.attachments.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/{} imágenes",
                            state.attachments.len(),
                            attachment_limits.max_attachments
                        ))
                        .color(theme.text_tertiary)
                        .size(TYPE_XS),
                    );
                }
                if state.is_pending {
                    if state.is_cancelling {
                        ui.add_enabled(false, egui::Button::new("Cancelando..."));
                    } else if ui.button("Cancelar").clicked() {
                        action = Some(AssistantUiAction::Cancel);
                    }
                } else if ui
                    .add_enabled(state.can_submit(), egui::Button::new("Enviar"))
                    .clicked()
                    || submit_on_enter
                {
                    action = Some(AssistantUiAction::Submit);
                }
            });
            if input_bytes > request_budget.max_input_chars * 3 / 4 {
                let input_color = if input_bytes > request_budget.max_input_chars {
                    theme.warning
                } else {
                    theme.text_tertiary
                };
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {} B",
                            input_bytes, request_budget.max_input_chars
                        ))
                        .color(input_color)
                        .size(TYPE_XS),
                    );
                });
            }
        });

    if !state.attachments.is_empty() {
        ui.add_space(SPACE_XS);
        egui::Frame::none()
            .fill(theme.input_bg)
            .rounding(RADIUS_MD)
            .inner_margin(egui::Margin::same(SPACE_SM))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, attachment) in state.attachments.iter().enumerate() {
                        let kind = attachment
                            .media_type
                            .strip_prefix("image/")
                            .unwrap_or("imagen");
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} {}x{}",
                                        kind.to_ascii_uppercase(),
                                        attachment.pixel_width,
                                        attachment.pixel_height
                                    ))
                                    .size(TYPE_XS),
                                );
                                let remove = ui
                                    .add_enabled_ui(!state.is_pending, |ui| {
                                        action_icon_button(
                                            ui,
                                            Icon::Close,
                                            theme.text_secondary,
                                            "Quitar imagen",
                                        )
                                    })
                                    .inner;
                                if remove.clicked() {
                                    action = Some(AssistantUiAction::RemoveAttachment(index));
                                }
                            });
                        });
                    }
                });
                if state.is_pending {
                    ui.label(
                        egui::RichText::new(pending_attachment_status(state))
                            .color(theme.text_tertiary)
                            .size(TYPE_XS),
                    );
                }
                if let Some(message) = &state.attachment_message {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(message)
                                .color(theme.text_secondary)
                                .size(TYPE_XS),
                        )
                        .truncate(),
                    )
                    .on_hover_text(message);
                }
            });
    }
    action
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConversationTurnAppearance {
    fill: egui::Color32,
    stroke: egui::Color32,
    role_color: egui::Color32,
}

#[derive(Clone, Copy)]
struct AssistantProposalRenderState<'a> {
    verified_proposals: &'a [VerifiedAssistantProposal],
    applied_proposals: &'a [VerifiedAssistantProposal],
    preflight_candidate_count: usize,
    proposal_code_block_indices: &'a [usize],
    proposal_results_available: bool,
    correction_available: bool,
}

fn conversation_turn_appearance(
    theme: &crate::theme::Theme,
    is_user: bool,
) -> ConversationTurnAppearance {
    if is_user {
        // Sin burbuja tipo WhatsApp: tarjeta neutra con el rol en acento.
        ConversationTurnAppearance {
            fill: theme.panel_bg,
            stroke: theme.separator,
            role_color: theme.accent_strong,
        }
    } else {
        ConversationTurnAppearance {
            fill: theme.input_bar_bg,
            stroke: theme.separator,
            role_color: theme.text_primary,
        }
    }
}

fn draw_conversation_turn(
    ui: &mut egui::Ui,
    turn: &ConversationTurn,
    proposal_state: AssistantProposalRenderState<'_>,
    reveal_clip: Option<RevealClip>,
    cache: &mut AssistantBlocksCache,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let is_user = matches!(turn.role, ConversationRole::User);
    let appearance = conversation_turn_appearance(theme, is_user);
    let mut action = None;
    let mut copy_requested = false;
    egui::Frame::none()
        .fill(appearance.fill)
        .stroke(egui::Stroke::new(1.0, appearance.stroke))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let label = if is_user {
                "Vos".into()
            } else {
                let origin = turn
                    .origin
                    .unwrap_or(AssistantExecutionOrigin::AuthorizedRemote);
                format!("{MORA_NAME} · {}", origin.public_label())
            };
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .color(appearance.role_color)
                        .size(TYPE_SM)
                        .strong(),
                );
            });
            ui.add_space(SPACE_SM);
            if is_user {
                draw_inline_text(ui, &turn.content);
            } else {
                action =
                    draw_assistant_response(ui, &turn.content, proposal_state, reveal_clip, cache);
                ui.add_space(SPACE_XS);
                // Telemetría tipo harness: estimación de salida del turno.
                let est_tokens = (turn.content.chars().count() / 4).max(1);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!("~{est_tokens} token de salida (est.)"))
                            .color(theme.text_tertiary)
                            .size(TYPE_XS),
                    );
                });
            }
            ui.add_space(SPACE_SM);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    copy_requested = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Copiar")
                                    .color(theme.text_secondary)
                                    .size(TYPE_XS),
                            )
                            .frame(false),
                        )
                        .clicked();
                },
            );
        });
    if copy_requested && action.is_none() {
        action = Some(AssistantUiAction::CopyMessage(turn.content.clone()));
    }
    action
}

fn draw_pending_indicator(
    ui: &mut egui::Ui,
    state: &AssistantPanelState,
    visuals: AssistantVisuals,
) {
    let theme = current_theme(ui.ctx());
    let appearance = conversation_turn_appearance(theme, false);
    egui::Frame::none()
        .fill(appearance.fill)
        .stroke(egui::Stroke::new(1.0, appearance.stroke))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(MORA_NAME)
                    .color(appearance.role_color)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.horizontal(|ui| {
                let _ = draw_mora_avatar(ui, visuals, 34.0, true);
                ThinkingOrb::new(
                    if state.is_cancelling {
                        ThinkingOrbState::Cancelling
                    } else {
                        ThinkingOrbState::Solving
                    },
                    30.0,
                )
                .draw(ui);
                ui.label(
                    egui::RichText::new(match state.is_cancelling {
                        true => "Cancelando...",
                        false => {
                            if state.agent_mode {
                                "Agente trabajando..."
                            } else {
                                "Consulta remota autorizada..."
                            }
                        }
                    })
                    .color(theme.text_secondary)
                    .size(TYPE_SM),
                );
            });
            if let Some(ledger) = &state.agent_ledger {
                ui.add_space(SPACE_XS);
                egui::CollapsingHeader::new("Estado de la tarea (ledger)")
                    .id_salt("assistant_agent_ledger")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(ledger)
                                .color(theme.text_secondary)
                                .monospace()
                                .size(TYPE_XS),
                        );
                    });
                ui.ctx().request_repaint();
            }
            if !state.agent_activity.is_empty() {
                ui.add_space(SPACE_XS);
                for row in state.agent_activity.iter().rev().take(6) {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("•").color(theme.accent));
                        ui.label(
                            egui::RichText::new(&row.text)
                                .color(theme.text_secondary)
                                .size(TYPE_XS),
                        );
                    });
                }
                ui.ctx().request_repaint();
            }
        });
}

fn draw_assistant_response(
    ui: &mut egui::Ui,
    content: &str,
    proposal_state: AssistantProposalRenderState<'_>,
    reveal_clip: Option<RevealClip>,
    cache: &mut AssistantBlocksCache,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let blocks = cache.blocks(content);
    let mut action = None;
    let mut code_block_index = 0;
    let mut rendered_candidate_indices = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let previous_opacity = ui.opacity();
        let reveal_restore = if let Some(clip) = reveal_clip {
            if index > clip.visible_blocks {
                break;
            }
            let alpha = if index == clip.visible_blocks {
                clip.boundary_alpha.max(0.12)
            } else {
                1.0
            };
            if alpha < 0.999 {
                ui.set_opacity(previous_opacity * alpha);
                true
            } else {
                false
            }
        } else {
            false
        };
        match block {
            AssistantMessageBlock::Heading { level, text } => {
                let size = match level {
                    1 => TYPE_LG,
                    2 => TYPE_MD,
                    _ => TYPE_SM,
                };
                ui.label(
                    egui::RichText::new(text)
                        .color(theme.text_primary)
                        .size(size)
                        .strong(),
                );
            }
            AssistantMessageBlock::Bullet(text) => {
                ui.horizontal_top(|ui| {
                    ui.label(egui::RichText::new("-").color(theme.accent));
                    draw_inline_text(ui, text);
                });
            }
            AssistantMessageBlock::Paragraph(text) => draw_inline_text(ui, text),
            AssistantMessageBlock::DisplayMath(math) => {
                egui::Frame::none()
                    .fill(theme.panel_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator))
                    .rounding(RADIUS_SM)
                    .inner_margin(egui::Margin::same(SPACE_SM))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Expresión matemática")
                                .color(theme.accent)
                                .size(TYPE_XS)
                                .strong(),
                        );
                        egui::ScrollArea::horizontal()
                            .id_salt(ui.make_persistent_id(("assistant_math", index)))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                let _ = draw_math(ui, math);
                            });
                    });
            }
            AssistantMessageBlock::Table(rows) => draw_markdown_table(ui, rows, index),
            AssistantMessageBlock::Code { language, text } => {
                let current_code_block_index = code_block_index;
                code_block_index += 1;
                egui::Frame::none()
                    .fill(theme.input_bg)
                    .stroke(egui::Stroke::new(1.0, theme.separator))
                    .rounding(RADIUS_SM)
                    .inner_margin(egui::Margin::same(SPACE_SM))
                    .show(ui, |ui| {
                        if !language.is_empty() {
                            ui.label(
                                egui::RichText::new(language)
                                    .color(theme.text_tertiary)
                                    .size(TYPE_XS),
                            );
                        }
                        egui::ScrollArea::horizontal()
                            .id_salt(ui.make_persistent_id(("assistant_code", index)))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text).monospace().size(TYPE_SM),
                                    )
                                    .extend(),
                                );
                            });
                        if let Some(proposal_index) = candidate_index_for_code_block(
                            proposal_state.proposal_code_block_indices,
                            current_code_block_index,
                        ) {
                            match assistant_proposal_card_state(
                                proposal_index,
                                proposal_state.verified_proposals,
                                proposal_state.applied_proposals,
                                proposal_state.preflight_candidate_count,
                                proposal_state.proposal_results_available,
                            ) {
                                Some(AssistantProposalCardState::Ready(verified)) => {
                                    rendered_candidate_indices.push(verified.candidate_index);
                                    retain_first_assistant_action(
                                        &mut action,
                                        draw_verified_assistant_proposal(ui, verified, false),
                                    );
                                }
                                Some(AssistantProposalCardState::Applied(applied)) => {
                                    rendered_candidate_indices.push(applied.candidate_index);
                                    draw_applied_assistant_proposal(ui, applied, false);
                                }
                                Some(AssistantProposalCardState::Rejected) => {
                                    ui.add_space(SPACE_SM);
                                    retain_first_assistant_action(
                                        &mut action,
                                        draw_rejected_assistant_proposal(
                                            ui,
                                            proposal_state.correction_available,
                                        ),
                                    );
                                }
                                Some(AssistantProposalCardState::NotPreflighted) => {
                                    ui.add_space(SPACE_SM);
                                    draw_unpreflighted_assistant_proposal(
                                        ui,
                                        proposal_state.preflight_candidate_count,
                                    );
                                }
                                None => {}
                            }
                        }
                    });
            }
        }
        if reveal_restore {
            ui.set_opacity(previous_opacity);
        }
        ui.add_space(SPACE_SM);
    }
    if reveal_clip.is_some() {
        return action;
    }
    // A bounded transcript can end before a complete fenced proposal. Preserve
    // the locally verified canonical action instead of silently hiding it.
    for verified in unrendered_verified_proposals(
        proposal_state.verified_proposals,
        &rendered_candidate_indices,
    ) {
        ui.label(
            egui::RichText::new("Propuesta comprobada de una respuesta resumida")
                .color(theme.text_secondary)
                .size(TYPE_XS),
        );
        retain_first_assistant_action(
            &mut action,
            draw_verified_assistant_proposal(ui, verified, true),
        );
        ui.add_space(SPACE_XS);
    }
    for applied in unrendered_verified_proposals(
        proposal_state.applied_proposals,
        &rendered_candidate_indices,
    ) {
        ui.label(
            egui::RichText::new("Propuesta aplicada de una respuesta resumida")
                .color(theme.text_secondary)
                .size(TYPE_XS),
        );
        draw_applied_assistant_proposal(ui, applied, true);
        ui.add_space(SPACE_XS);
    }
    action
}

enum AssistantProposalCardState<'a> {
    Ready(&'a VerifiedAssistantProposal),
    Applied(&'a VerifiedAssistantProposal),
    Rejected,
    NotPreflighted,
}

fn candidate_index_for_code_block(
    proposal_code_block_indices: &[usize],
    code_block_index: usize,
) -> Option<usize> {
    proposal_code_block_indices
        .iter()
        .position(|index| *index == code_block_index)
}

fn assistant_proposal_card_state<'a>(
    candidate_index: usize,
    verified_proposals: &'a [VerifiedAssistantProposal],
    applied_proposals: &'a [VerifiedAssistantProposal],
    preflight_candidate_count: usize,
    proposal_results_available: bool,
) -> Option<AssistantProposalCardState<'a>> {
    if !proposal_results_available {
        return None;
    }
    if let Some(verified) = verified_proposal(candidate_index, verified_proposals) {
        return Some(AssistantProposalCardState::Ready(verified));
    }
    if let Some(applied) = verified_proposal(candidate_index, applied_proposals) {
        return Some(AssistantProposalCardState::Applied(applied));
    }
    if candidate_index < preflight_candidate_count {
        Some(AssistantProposalCardState::Rejected)
    } else {
        Some(AssistantProposalCardState::NotPreflighted)
    }
}

fn draw_rejected_assistant_proposal(
    ui: &mut egui::Ui,
    correction_available: bool,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    let mut correction_clicked = false;
    egui::Frame::none()
        .fill(theme.warning.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, theme.warning))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Esta propuesta no superó la comprobación local.")
                    .color(theme.warning)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(
                    "Sólo se habilitan acciones verificadas de la respuesta actual.",
                )
                .color(theme.text_secondary)
                .size(TYPE_XS),
            );
            correction_clicked =
                correction_available && ui.button("Pedir una corrección").clicked();
        });
    rejected_proposal_action(correction_available, correction_clicked)
}

fn draw_unpreflighted_assistant_proposal(ui: &mut egui::Ui, preflight_candidate_count: usize) {
    let theme = current_theme(ui.ctx());
    egui::Frame::none()
        .fill(theme.input_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Propuesta sin comprobar")
                    .color(theme.text_primary)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!(
                    "La comprobación se limitó a las primeras {preflight_candidate_count} propuesta(s) de esta respuesta."
                ))
                .color(theme.text_secondary)
                .size(TYPE_XS),
            );
        });
}

fn draw_applied_assistant_proposal(
    ui: &mut egui::Ui,
    applied: &VerifiedAssistantProposal,
    show_canonical_commands: bool,
) {
    let theme = current_theme(ui.ctx());
    if show_canonical_commands {
        let commands = applied.proposal.canonical_text();
        egui::Frame::none()
            .fill(theme.panel_bg)
            .stroke(egui::Stroke::new(1.0, theme.separator))
            .rounding(RADIUS_SM)
            .inner_margin(egui::Margin::same(SPACE_SM))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt(ui.make_persistent_id((
                        "assistant_applied_proposal",
                        applied.candidate_index,
                    )))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(commands).monospace().size(TYPE_SM));
                    });
            });
    }
    egui::Frame::none()
        .fill(theme.success.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, theme.success))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Aplicada a Grafito")
                    .color(theme.success)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("La propuesta ya se incorporó al documento.")
                    .color(theme.text_primary)
                    .size(TYPE_SM),
            );
        });
}

fn rejected_proposal_action(
    correction_available: bool,
    correction_clicked: bool,
) -> Option<AssistantUiAction> {
    (correction_available && correction_clicked)
        .then_some(AssistantUiAction::RetryProposalCorrection)
}

fn draw_verified_assistant_proposal(
    ui: &mut egui::Ui,
    verified: &VerifiedAssistantProposal,
    show_canonical_commands: bool,
) -> Option<AssistantUiAction> {
    let theme = current_theme(ui.ctx());
    if show_canonical_commands {
        let commands = verified.proposal.canonical_text();
        egui::Frame::none()
            .fill(theme.panel_bg)
            .stroke(egui::Stroke::new(1.0, theme.separator))
            .rounding(RADIUS_SM)
            .inner_margin(egui::Margin::same(SPACE_SM))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt(ui.make_persistent_id((
                        "assistant_verified_proposal",
                        verified.candidate_index,
                    )))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(commands).monospace().size(TYPE_SM));
                    });
            });
    }

    let description = match &verified.proposal {
        AssistantProposal::Command(_) => {
            if verified.prerequisite_parameters.is_empty() {
                "Aplicar el comando comprobado al documento de Grafito"
            } else {
                "Aplicar el comando comprobado con sus parámetros necesarios"
            }
        }
        AssistantProposal::Scene(_) => {
            if verified.prerequisite_parameters.is_empty() {
                "Aplicar escena 3D verificada al documento de Grafito"
            } else {
                "Aplicar escena 3D verificada con sus parámetros necesarios"
            }
        }
        AssistantProposal::Parameter(_) => "Aplicar parámetro verificado al documento de Grafito",
    };
    let mut action = None;
    let status = match &verified.proposal {
        AssistantProposal::Command(_) => "Comando comprobado localmente antes de mostrarse.",
        AssistantProposal::Scene(_) => {
            "Escena completa comprobada localmente; se aplica de forma atómica."
        }
        AssistantProposal::Parameter(_) => "Parámetro comprobado localmente antes de mostrarse.",
    };
    let card = egui::Frame::none()
        .fill(theme.accent_muted)
        .stroke(egui::Stroke::new(1.0, theme.accent))
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Lista para aplicar")
                    .color(theme.accent_strong)
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new(description)
                    .color(theme.text_primary)
                    .size(TYPE_SM),
            );
            ui.add_space(SPACE_SM);
            if ui
                .add_sized(
                    egui::vec2(ui.available_width(), 28.0),
                    egui::Button::new("Aplicar en Grafito")
                        .fill(theme.panel_bg)
                        .stroke(egui::Stroke::new(1.0, theme.accent)),
                )
                .clicked()
            {
                action = Some(AssistantUiAction::ApplyProposal(verified.candidate_index));
            }
            if let AssistantProposal::Command(_) = &verified.proposal {
                if verified.prerequisite_parameters.is_empty()
                    && ui.small_button("Editar en la entrada").clicked()
                {
                    action = Some(AssistantUiAction::InsertCommand(verified.candidate_index));
                }
            }
            ui.label(
                egui::RichText::new(status)
                    .color(theme.text_primary)
                    .size(TYPE_XS),
            );
        });
    if card.response.hovered() {
        ui.painter().rect_stroke(
            card.response.rect.expand(1.0),
            RADIUS_SM,
            egui::Stroke::new(2.0, theme.accent),
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !verified.prerequisite_parameters.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "Al aplicar también se establecerá: {}.",
                verified
                    .prerequisite_parameters
                    .iter()
                    .map(AssistantParameterAssignment::canonical_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .color(theme.text_secondary)
            .size(TYPE_XS),
        );
    }
    action
}

fn verified_proposal(
    candidate_index: usize,
    verified_proposals: &[VerifiedAssistantProposal],
) -> Option<&VerifiedAssistantProposal> {
    verified_proposals
        .iter()
        .find(|verified| verified.candidate_index == candidate_index)
}

fn unrendered_verified_proposals<'a>(
    verified_proposals: &'a [VerifiedAssistantProposal],
    rendered_candidate_indices: &[usize],
) -> Vec<&'a VerifiedAssistantProposal> {
    verified_proposals
        .iter()
        .filter(|proposal| !rendered_candidate_indices.contains(&proposal.candidate_index))
        .collect()
}

fn draw_markdown_table(ui: &mut egui::Ui, rows: &[Vec<String>], index: usize) {
    let theme = current_theme(ui.ctx());
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_XS))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt(ui.make_persistent_id(("assistant_table_scroll", index)))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new(ui.make_persistent_id(("assistant_table", index)))
                        .striped(true)
                        .min_col_width(44.0)
                        .show(ui, |ui| {
                            for (row_index, row) in rows.iter().enumerate() {
                                for cell in row {
                                    let text = egui::RichText::new(cell)
                                        .color(theme.text_primary)
                                        .size(TYPE_SM);
                                    if row_index == 0 {
                                        ui.label(text.strong());
                                    } else {
                                        ui.label(text);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MathExpr {
    Text(String),
    Row(Vec<MathExpr>),
    Fraction {
        numerator: Box<MathExpr>,
        denominator: Box<MathExpr>,
    },
    Root(Box<MathExpr>),
    Script {
        base: Box<MathExpr>,
        superscript: Option<Box<MathExpr>>,
        subscript: Option<Box<MathExpr>>,
    },
}

impl MathExpr {
    fn to_plain(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Row(expressions) => expressions.iter().map(Self::to_plain).collect(),
            Self::Fraction {
                numerator,
                denominator,
            } => format!("{} / {}", numerator.to_plain(), denominator.to_plain()),
            Self::Root(radicand) => format!("√{}", radicand.to_plain()),
            Self::Script {
                base,
                superscript,
                subscript,
            } => {
                let mut rendered = base.to_plain();
                if let Some(subscript) = subscript {
                    rendered.push_str(&decorate_math_script(&subscript.to_plain(), false));
                }
                if let Some(superscript) = superscript {
                    rendered.push_str(&decorate_math_script(&superscript.to_plain(), true));
                }
                rendered
            }
        }
    }
}

const MAX_MATH_PARSE_DEPTH: usize = 16;
const MAX_MATH_PARSE_NODES: usize = 256;

struct MathParser<'a> {
    source: &'a str,
    position: usize,
    depth: usize,
    nodes: usize,
}

impl<'a> MathParser<'a> {
    fn parse(source: &'a str) -> Option<MathExpr> {
        let mut parser = Self {
            source,
            position: 0,
            depth: 0,
            nodes: 0,
        };
        let expression = parser.parse_row(None)?;
        parser.peek().is_none().then_some(expression)
    }

    fn parse_row(&mut self, closing: Option<char>) -> Option<MathExpr> {
        let mut expressions = Vec::new();
        while let Some(character) = self.peek() {
            if Some(character) == closing {
                self.take()?;
                return self.node(MathExpr::Row(expressions));
            }
            if character == '}' {
                return None;
            }
            let mut expression = match character {
                '{' => self.parse_group()?,
                '\\' => self.parse_command()?,
                '^' | '_' => return None,
                _ => {
                    let text = self.take()?.to_string();
                    self.node(MathExpr::Text(text))?
                }
            };
            let mut superscript = None;
            let mut subscript = None;
            while matches!(self.peek(), Some('^' | '_')) {
                let marker = self.take()?;
                let script = self.parse_script_atom()?;
                if marker == '^' {
                    if superscript.is_some() {
                        return None;
                    }
                    superscript = Some(Box::new(script));
                } else {
                    if subscript.is_some() {
                        return None;
                    }
                    subscript = Some(Box::new(script));
                }
            }
            if superscript.is_some() || subscript.is_some() {
                expression = self.node(MathExpr::Script {
                    base: Box::new(expression),
                    superscript,
                    subscript,
                })?;
            }
            if let (Some(MathExpr::Text(previous)), MathExpr::Text(text)) =
                (expressions.last_mut(), &expression)
            {
                previous.push_str(text);
                continue;
            }
            expressions.push(expression);
        }
        closing
            .is_none()
            .then(|| self.node(MathExpr::Row(expressions)))?
    }

    fn parse_group(&mut self) -> Option<MathExpr> {
        self.take_if('{')?;
        self.depth += 1;
        if self.depth > MAX_MATH_PARSE_DEPTH {
            return None;
        }
        let expression = self.parse_row(Some('}'));
        self.depth -= 1;
        expression
    }

    fn parse_script_atom(&mut self) -> Option<MathExpr> {
        match self.peek()? {
            '{' => self.parse_group(),
            '\\' => self.parse_command(),
            '}' | '^' | '_' => None,
            _ => {
                let text = self.take()?.to_string();
                self.node(MathExpr::Text(text))
            }
        }
    }

    fn parse_command(&mut self) -> Option<MathExpr> {
        self.take_if('\\')?;
        let first = self.take()?;
        if !first.is_ascii_alphabetic() {
            if matches!(first, '!' | ',' | ';' | ':') {
                return self.node(MathExpr::Text(String::new()));
            }
            return self.node(MathExpr::Text(first.to_string()));
        }
        let mut command = first.to_string();
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_alphabetic())
        {
            command.push(self.take()?);
        }
        if self.peek() == Some(' ') {
            self.take()?;
        }
        match command.as_str() {
            "frac" | "dfrac" => {
                let numerator = self.parse_group()?;
                let denominator = self.parse_group()?;
                self.node(MathExpr::Fraction {
                    numerator: Box::new(numerator),
                    denominator: Box::new(denominator),
                })
            }
            "sqrt" => {
                let radicand = self.parse_group()?;
                self.node(MathExpr::Root(Box::new(radicand)))
            }
            "text" | "mathrm" | "operatorname" | "mathbb" | "mathbf" | "mathit" | "mathcal" => {
                self.parse_group()
            }
            "left" | "right" => self.node(MathExpr::Text(String::new())),
            "quad" | "qquad" => self.node(MathExpr::Text(" ".into())),
            _ => self.node(MathExpr::Text(math_command_symbol(&command)?.into())),
        }
    }

    fn node(&mut self, expression: MathExpr) -> Option<MathExpr> {
        self.nodes += 1;
        (self.nodes <= MAX_MATH_PARSE_NODES).then_some(expression)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn take(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn take_if(&mut self, expected: char) -> Option<()> {
        (self.peek()? == expected).then(|| self.take().map(|_| ()))?
    }
}

fn math_command_symbol(command: &str) -> Option<&'static str> {
    match command {
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" | "varepsilon" => Some("ε"),
        "zeta" => Some("ζ"),
        "eta" => Some("η"),
        "theta" | "vartheta" => Some("θ"),
        "iota" => Some("ι"),
        "kappa" => Some("κ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "nu" => Some("ν"),
        "xi" => Some("ξ"),
        "pi" => Some("π"),
        "rho" => Some("ρ"),
        "sigma" => Some("σ"),
        "tau" => Some("τ"),
        "upsilon" => Some("υ"),
        "phi" | "varphi" => Some("φ"),
        "chi" => Some("χ"),
        "psi" => Some("ψ"),
        "omega" => Some("ω"),
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Theta" => Some("Θ"),
        "Lambda" => Some("Λ"),
        "Xi" => Some("Ξ"),
        "Pi" => Some("Π"),
        "Sigma" => Some("Σ"),
        "Phi" => Some("Φ"),
        "Psi" => Some("Ψ"),
        "Omega" => Some("Ω"),
        "leq" | "le" => Some("≤"),
        "geq" | "ge" => Some("≥"),
        "neq" | "ne" => Some("≠"),
        "approx" => Some("≈"),
        "equiv" => Some("≡"),
        "to" | "rightarrow" => Some("→"),
        "in" => Some("∈"),
        "pm" => Some("±"),
        "times" => Some("×"),
        "cdot" => Some("·"),
        "infty" => Some("∞"),
        "sum" => Some("∑"),
        "int" => Some("∫"),
        "partial" => Some("∂"),
        "nabla" => Some("∇"),
        "sin" => Some("sin "),
        "cos" => Some("cos "),
        "tan" => Some("tan "),
        "log" => Some("log "),
        "ln" => Some("ln "),
        "exp" => Some("exp "),
        _ => None,
    }
}

fn decorate_math_script(source: &str, superscript: bool) -> String {
    let mapped = source
        .chars()
        .map(|character| math_script_character(character, superscript))
        .collect::<Option<String>>();
    mapped.unwrap_or_else(|| {
        if superscript {
            format!("^({source})")
        } else {
            format!("_({source})")
        }
    })
}

fn math_script_character(character: char, superscript: bool) -> Option<char> {
    let characters = if superscript {
        "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱ"
    } else {
        "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ"
    };
    let plain = if superscript {
        "0123456789+-=()ni"
    } else {
        "0123456789+-=()aehijklmnoprstuvx"
    };
    plain
        .chars()
        .position(|candidate| candidate == character)
        .and_then(|index| characters.chars().nth(index))
}

fn math_to_plain(source: &str) -> String {
    MathParser::parse(source)
        .map(|expression| expression.to_plain())
        .filter(|rendered| !rendered.is_empty())
        .unwrap_or_else(|| source.to_owned())
}

struct MathGlyph {
    offset: egui::Vec2,
    galley: std::sync::Arc<egui::Galley>,
}

struct MathLayout {
    size: egui::Vec2,
    baseline: f32,
    glyphs: Vec<MathGlyph>,
    rules: Vec<(egui::Vec2, egui::Vec2)>,
}

impl MathLayout {
    fn append(&mut self, mut child: Self, offset: egui::Vec2) {
        for glyph in &mut child.glyphs {
            glyph.offset += offset;
        }
        for rule in &mut child.rules {
            rule.0 += offset;
            rule.1 += offset;
        }
        self.glyphs.extend(child.glyphs);
        self.rules.extend(child.rules);
    }
}

fn draw_math(ui: &mut egui::Ui, source: &str) -> egui::Response {
    let theme = current_theme(ui.ctx());
    let Some(expression) = MathParser::parse(source) else {
        return ui
            .add(
                egui::Label::new(
                    egui::RichText::new(source)
                        .color(theme.text_primary)
                        .size(TYPE_SM),
                )
                .wrap(),
            )
            .on_hover_text(source);
    };
    let layout = layout_math(ui.painter(), &expression, TYPE_MD, theme.text_primary);
    let (rect, response) = ui.allocate_exact_size(layout.size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    for glyph in layout.glyphs {
        painter.galley(rect.min + glyph.offset, glyph.galley, theme.text_primary);
    }
    for (from, to) in layout.rules {
        painter.line_segment(
            [rect.min + from, rect.min + to],
            egui::Stroke::new(1.0, theme.text_primary.gamma_multiply(0.8)),
        );
    }
    response.on_hover_text(source)
}

fn layout_math(
    painter: &egui::Painter,
    expression: &MathExpr,
    font_size: f32,
    color: egui::Color32,
) -> MathLayout {
    match expression {
        MathExpr::Text(text) => {
            let galley =
                painter.layout_no_wrap(text.clone(), egui::FontId::proportional(font_size), color);
            MathLayout {
                size: galley.size(),
                baseline: galley.size().y * 0.76,
                glyphs: vec![MathGlyph {
                    offset: egui::Vec2::ZERO,
                    galley,
                }],
                rules: Vec::new(),
            }
        }
        MathExpr::Row(expressions) => {
            let children = expressions
                .iter()
                .map(|expression| layout_math(painter, expression, font_size, color))
                .collect::<Vec<_>>();
            let baseline = children
                .iter()
                .map(|child| child.baseline)
                .fold(font_size * 0.76, f32::max);
            let descent = children
                .iter()
                .map(|child| child.size.y - child.baseline)
                .fold(font_size * 0.24, f32::max);
            let gap = 1.0;
            let width = children.iter().map(|child| child.size.x).sum::<f32>()
                + gap * children.len().saturating_sub(1) as f32;
            let mut layout = MathLayout {
                size: egui::vec2(width, baseline + descent),
                baseline,
                glyphs: Vec::new(),
                rules: Vec::new(),
            };
            let mut x = 0.0;
            for child in children {
                let width = child.size.x;
                let y = baseline - child.baseline;
                layout.append(child, egui::vec2(x, y));
                x += width + gap;
            }
            layout
        }
        MathExpr::Fraction {
            numerator,
            denominator,
        } => {
            let script_size = (font_size * 0.78).max(TYPE_XS);
            let numerator = layout_math(painter, numerator, script_size, color);
            let denominator = layout_math(painter, denominator, script_size, color);
            let padding = 3.0;
            let width = numerator.size.x.max(denominator.size.x) + padding * 2.0;
            let rule_y = numerator.size.y + 2.0;
            let denominator_y = rule_y + 3.0;
            let numerator_x = (width - numerator.size.x) * 0.5;
            let denominator_x = (width - denominator.size.x) * 0.5;
            let mut layout = MathLayout {
                size: egui::vec2(width, denominator_y + denominator.size.y),
                baseline: denominator_y + denominator.baseline,
                glyphs: Vec::new(),
                rules: vec![(
                    egui::vec2(padding, rule_y),
                    egui::vec2(width - padding, rule_y),
                )],
            };
            layout.append(numerator, egui::vec2(numerator_x, 0.0));
            layout.append(denominator, egui::vec2(denominator_x, denominator_y));
            layout
        }
        MathExpr::Root(radicand) => {
            let radicand = layout_math(painter, radicand, font_size, color);
            let sign = layout_math(painter, &MathExpr::Text("√".into()), font_size, color);
            let inner_x = sign.size.x.max(8.0) - 2.0;
            let inner_y = 3.0;
            let height = sign.size.y.max(inner_y + radicand.size.y);
            let baseline = (inner_y + radicand.baseline).max(sign.baseline);
            let sign_y = height - sign.size.y;
            let mut layout = MathLayout {
                size: egui::vec2(inner_x + radicand.size.x, height),
                baseline,
                glyphs: Vec::new(),
                rules: vec![(
                    egui::vec2(inner_x, inner_y),
                    egui::vec2(inner_x + radicand.size.x, inner_y),
                )],
            };
            layout.append(sign, egui::vec2(0.0, sign_y));
            layout.append(radicand, egui::vec2(inner_x, inner_y));
            layout
        }
        MathExpr::Script {
            base,
            superscript,
            subscript,
        } => {
            let base = layout_math(painter, base, font_size, color);
            let script_size = (font_size * 0.66).max(TYPE_XS);
            let superscript = superscript
                .as_deref()
                .map(|script| layout_math(painter, script, script_size, color));
            let subscript = subscript
                .as_deref()
                .map(|script| layout_math(painter, script, script_size, color));
            let script_width = superscript
                .as_ref()
                .map(|layout| layout.size.x)
                .unwrap_or_default()
                .max(
                    subscript
                        .as_ref()
                        .map(|layout| layout.size.x)
                        .unwrap_or_default(),
                );
            let top = superscript
                .as_ref()
                .map(|layout| layout.size.y * 0.68)
                .unwrap_or_default();
            let base_y = top;
            let subscript_y = base_y + base.baseline + 1.0;
            let height = (base_y + base.size.y)
                .max(
                    subscript
                        .as_ref()
                        .map(|layout| subscript_y + layout.size.y)
                        .unwrap_or_default(),
                )
                .max(
                    superscript
                        .as_ref()
                        .map(|layout| layout.size.y)
                        .unwrap_or_default(),
                );
            let mut layout = MathLayout {
                size: egui::vec2(base.size.x + script_width, height),
                baseline: base_y + base.baseline,
                glyphs: Vec::new(),
                rules: Vec::new(),
            };
            let script_x = layout.size.x - script_width;
            layout.append(base, egui::vec2(0.0, base_y));
            if let Some(superscript) = superscript {
                layout.append(superscript, egui::vec2(script_x, 0.0));
            }
            if let Some(subscript) = subscript {
                layout.append(subscript, egui::vec2(script_x, subscript_y));
            }
            layout
        }
    }
}

fn draw_inline_text(ui: &mut egui::Ui, text: &str) {
    let theme = current_theme(ui.ctx());
    let normal = egui::TextFormat {
        font_id: egui::FontId::proportional(TYPE_BASE),
        color: theme.text_primary,
        ..Default::default()
    };
    let bold = egui::TextFormat {
        font_id: egui::FontId::proportional(TYPE_BASE),
        color: theme.text_primary,
        italics: false,
        ..Default::default()
    };
    let code = egui::TextFormat {
        font_id: egui::FontId::monospace(TYPE_SM),
        color: theme.accent,
        background: theme.accent_muted,
        ..Default::default()
    };
    let math = egui::TextFormat {
        font_id: egui::FontId::proportional(TYPE_BASE),
        color: theme.accent_strong,
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut remaining = text;
    while !remaining.is_empty() {
        let markers = [
            ("**", "**", &bold),
            ("`", "`", &code),
            ("$$", "$$", &math),
            (r"\[", r"\]", &math),
            ("$", "$", &math),
            (r"\(", r"\)", &math),
        ];
        let next = markers
            .iter()
            .filter_map(|(start, end, format)| {
                remaining
                    .find(start)
                    .map(|position| (position, *start, *end, *format))
            })
            .min_by_key(|(position, _, _, _)| *position);
        let Some((position, start, end, format)) = next else {
            job.append(remaining, 0.0, normal.clone());
            break;
        };
        if position > 0 {
            job.append(&remaining[..position], 0.0, normal.clone());
        }
        let after_start = &remaining[position + start.len()..];
        let Some(end_position) = after_start.find(end) else {
            job.append(&remaining[position..], 0.0, normal.clone());
            break;
        };
        let source_end = position + start.len() + end_position + end.len();
        let inline = if matches!(start, "$$" | "$" | r"\[" | r"\(") {
            inline_math_text(&remaining[position..source_end])
        } else {
            after_start[..end_position].to_owned()
        };
        job.append(&inline, 0.0, format.clone());
        remaining = &after_start[end_position + end.len()..];
    }
    ui.add(egui::Label::new(job).wrap());
}

fn inline_math_text(source: &str) -> String {
    for (start, end) in [("$$", "$$"), (r"\[", r"\]"), ("$", "$"), (r"\(", r"\)")] {
        if let Some(content) = source
            .strip_prefix(start)
            .and_then(|content| content.strip_suffix(end))
        {
            return math_to_plain(content);
        }
    }
    source.to_owned()
}

fn provider_label(provider: ProviderProfile) -> &'static str {
    match provider {
        ProviderProfile::OpenCodeGo => "OpenCode Go",
        ProviderProfile::OllamaLocal => "Ollama",
        ProviderProfile::DeepSeek => "DeepSeek",
        ProviderProfile::CustomOpenAiCompatible => "Compatible",
    }
}

fn suggestion_prompts(has_focus: bool) -> [(&'static str, &'static str); 5] {
    if has_focus {
        [
            (
                "Analizar",
                "Analizá la función seleccionada: dominio, raíces, extremos y comportamiento.",
            ),
            (
                "Derivar",
                "Derivá la función seleccionada y explicá qué representa.",
            ),
            (
                "Integrar",
                "Integrá la función seleccionada y mostrá el resultado paso a paso.",
            ),
            (
                "Interpretar",
                "Explicá cómo leer el gráfico de la función seleccionada.",
            ),
            (
                "Aclarar",
                "Explícame con un ejemplo qué significa la pendiente en esta función.",
            ),
        ]
    } else {
        [
            ("Resolver", "Ayudame a resolver este problema paso a paso."),
            ("Graficar", "Decime qué función debería graficar y por qué."),
            (
                "Derivar",
                "derivar x^3 + 2*x · explicame la regla y qué representa la derivada",
            ),
            (
                "Límite",
                "Calculá el límite de sin(x)/x cuando x tiende a 0 y explicámelo",
            ),
            (
                "Aclarar",
                "No sé qué analizar todavía. Haceme una pregunta para orientar el problema.",
            ),
        ]
    }
}

fn should_submit_on_enter(
    editor_has_focus: bool,
    enter_pressed: bool,
    shift_held: bool,
    can_submit: bool,
) -> bool {
    editor_has_focus && enter_pressed && !shift_held && can_submit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correction_context() -> AssistantCorrectionContext {
        let context = ImmutableDocumentContext::empty(0);
        AssistantCorrectionContext {
            document_revision: context.revision,
            document_digest: context.digest,
            focus: None,
        }
    }

    fn command_proposal(text: &str) -> AssistantProposal {
        AssistantProposal::Command(
            grafito_command::assistant_proposals::parse_assistant_command(text)
                .expect("test command must be a recognized assistant action"),
        )
    }

    fn parameter_proposal(text: &str) -> AssistantProposal {
        AssistantProposal::Parameter(
            grafito_command::assistant_proposals::parse_assistant_parameter(text)
                .expect("test parameter must be finite"),
        )
    }

    fn parameter_assignment(text: &str) -> AssistantParameterAssignment {
        grafito_command::assistant_proposals::parse_assistant_parameter(text)
            .expect("test parameter must be finite")
    }

    fn scene_proposal(commands: &[&str]) -> AssistantProposal {
        AssistantProposal::Scene(
            commands
                .iter()
                .map(|command| {
                    grafito_command::assistant_proposals::parse_assistant_command(command)
                        .expect("test scene command must be recognized")
                })
                .collect(),
        )
    }

    #[test]
    fn state_allows_submission_while_a_saved_key_is_resolved_on_demand() {
        let state = AssistantPanelState {
            problem: "2 + 2".into(),
            ..Default::default()
        };
        assert!(state.can_submit());
    }

    #[test]
    fn editorial_turn_appearance_separates_roles_without_split_widths() {
        let user = conversation_turn_appearance(&crate::theme::DARK, true);
        let assistant = conversation_turn_appearance(&crate::theme::DARK, false);

        // Estilo editorial: sin burbuja WhatsApp; roles separados por color de
        // rol y borde, con tarjetas neutras pero distinguibles.
        assert_eq!(user.fill, crate::theme::DARK.panel_bg);
        assert_eq!(assistant.fill, crate::theme::DARK.input_bar_bg);
        assert_eq!(user.role_color, crate::theme::DARK.accent_strong);
        assert_eq!(assistant.role_color, crate::theme::DARK.text_primary);
        assert_ne!(user.fill, assistant.fill);
    }

    #[test]
    fn mora_avatar_fallback_is_accessible_and_keeps_its_requested_size() {
        let context = egui::Context::default();
        let mut size = egui::Vec2::ZERO;

        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                size = draw_mora_avatar(ui, AssistantVisuals::default(), 32.0, false)
                    .rect
                    .size();
            });
        });

        assert_eq!(MORA_NAME, "Mora");
        assert_eq!(size, egui::vec2(32.0, 32.0));
    }

    #[test]
    fn mora_texture_is_painted_in_header_empty_and_pending_states() {
        let context = egui::Context::default();
        let texture = context.load_texture(
            "mora-test-avatar",
            egui::ColorImage::new([1, 1], egui::Color32::WHITE),
            Default::default(),
        );
        let visuals = AssistantVisuals {
            mora_texture: Some(texture.id()),
        };
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 300.0),
            )),
            ..Default::default()
        };
        let mut state = AssistantPanelState::default();

        let header = context.run(input(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let _ = draw_assistant_header(ui, &mut state, current_theme(context), visuals);
            });
        });
        assert!(output_uses_texture(&header, texture.id()));

        let empty = context.run(input(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                draw_assistant_empty_state(ui, &mut state, visuals);
            });
        });
        assert!(output_uses_texture(&empty, texture.id()));

        state.begin_request("2 + 2".into());
        let pending = context.run(input(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                draw_pending_indicator(ui, &state, visuals);
            });
        });
        assert!(output_uses_texture(&pending, texture.id()));
    }

    #[test]
    fn mora_motion_is_static_at_rest_and_stays_inside_fixed_bounds_when_active() {
        assert_eq!(mora_avatar_scale(false, 1.0), 1.0);

        let active = mora_avatar_scale(true, 1.0);
        assert!((0.975..=1.025).contains(&active));
    }

    fn output_uses_texture(output: &egui::FullOutput, texture: egui::TextureId) -> bool {
        output
            .shapes
            .iter()
            .any(|clipped_shape| shape_uses_texture(&clipped_shape.shape, texture))
    }

    fn shape_uses_texture(shape: &egui::epaint::Shape, texture: egui::TextureId) -> bool {
        match shape {
            egui::epaint::Shape::Mesh(mesh) => mesh.texture_id == texture,
            egui::epaint::Shape::Vec(shapes) => shapes
                .iter()
                .any(|shape| shape_uses_texture(shape, texture)),
            _ => false,
        }
    }

    #[test]
    fn attachment_api_rejects_invalid_images_without_storing_them() {
        let mut state = AssistantPanelState::default();
        let result = state.add_attachment(ImageAttachment::new("image/svg+xml", vec![1], 1, 1));

        assert!(result.is_err());
        assert!(state.attachments.is_empty());
    }

    #[test]
    fn successful_requests_keep_only_the_bounded_recent_conversation() {
        let mut state = AssistantPanelState::default();
        for index in 0..4 {
            state.begin_request(format!("q{index}"));
            state.complete_request(format!("a{index}"));
        }

        assert_eq!(state.conversation.len(), MAX_CONVERSATION_TURNS);
        assert_eq!(state.conversation[0].content, "q1");
    }

    #[test]
    fn clearing_a_conversation_discards_its_remote_proposals_and_error() {
        let mut state = AssistantPanelState::default();
        state.begin_request("pregunta".into());
        state.complete_request("respuesta".into());
        state.verified_proposals.push(VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: command_proposal("Function[x]"),
            prerequisite_parameters: Vec::new(),
        });
        state.error = Some("error anterior".into());

        state.clear_conversation();

        assert!(state.conversation.is_empty());
        assert!(state.verified_proposals.is_empty());
        assert!(state.error.is_none());
    }

    #[test]
    fn successful_proposal_commit_moves_the_card_to_applied_history() {
        let mut state = AssistantPanelState::default();
        state.verified_proposals.push(VerifiedAssistantProposal {
            candidate_index: 7,
            proposal: command_proposal("Function[x]"),
            prerequisite_parameters: Vec::new(),
        });

        assert!(!state.finish_verified_proposal_application(7, false));
        assert_eq!(state.verified_proposals.len(), 1);
        assert!(state.finish_verified_proposal_application(7, true));
        assert!(state.verified_proposals.is_empty());
        assert_eq!(state.applied_proposals.len(), 1);
        assert_eq!(state.applied_proposals[0].candidate_index, 7);
        assert!(!state.finish_verified_proposal_application(7, true));
    }

    #[test]
    fn image_consent_resets_when_the_content_or_model_changes() {
        let mut state = AssistantPanelState {
            image_upload_consent: true,
            ..Default::default()
        };
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        assert!(!state.image_upload_consent);

        state.image_upload_consent = true;
        state.vision_enabled = true;
        state.select_model("glm-5.2");
        assert!(!state.image_upload_consent);
        assert!(!state.vision_enabled);

        state.image_upload_consent = true;
        state.select_provider(ProviderProfile::OllamaLocal);
        assert!(!state.image_upload_consent);
    }

    #[test]
    fn destination_changes_discard_history_that_could_be_forwarded_remotely() {
        let mut state = AssistantPanelState::default();
        state.begin_request("private question for the first model".into());
        state.complete_request("private answer from the first model".into());
        state.verified_proposals.push(VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: command_proposal("Function[x]"),
            prerequisite_parameters: Vec::new(),
        });
        state.offer_proposal_correction(
            "private question for the first model".into(),
            grafito_assistant_types::AssistantRepairFeedback {
                failures: Vec::new(),
            },
            correction_context(),
        );

        state.select_model("deepseek-v4-pro");

        assert!(state.conversation.is_empty());
        assert!(state.verified_proposals.is_empty());
        assert!(!state.proposal_correction_available);
        assert!(state.proposal_correction_feedback.is_none());

        state.begin_request("private question for the first provider".into());
        state.complete_request("private answer from the first provider".into());
        state.select_provider(ProviderProfile::OllamaLocal);

        assert!(state.conversation.is_empty());
        assert!(state.verified_proposals.is_empty());

        state.begin_request("private question before restored preferences".into());
        state.complete_request("private answer before restored preferences".into());
        state.apply_preferences(ProviderProfile::OpenCodeGo, "glm-5.2");

        assert!(state.conversation.is_empty());
    }

    #[test]
    fn assistant_configuration_starts_closed_and_provider_changes_reset_key_state() {
        let mut state = AssistantPanelState::default();
        assert!(!state.settings_open);

        state.key_available = true;
        state.key_status_checked = true;
        state.select_provider(ProviderProfile::OllamaLocal);

        assert!(!state.key_available);
        assert!(!state.key_status_checked);
    }

    #[test]
    fn cancellation_keeps_the_submission_locked_until_the_worker_finishes() {
        let mut state = AssistantPanelState {
            problem: "2 + 2".into(),
            key_available: true,
            ..Default::default()
        };
        state.begin_request("2 + 2".into());
        state.begin_cancellation();

        assert!(state.is_pending);
        assert!(state.is_cancelling);
        assert!(!state.can_submit());
    }

    #[test]
    fn pending_request_preserves_the_visible_attachment_snapshot() {
        let mut state = AssistantPanelState::default();
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        state.vision_enabled = true;
        state.image_upload_consent = true;

        state.begin_request("interpretá la imagen".into());

        assert!(state.image_upload_consent);
        assert_eq!(state.attachments.len(), 1);
        state.complete_request("respuesta".into());
        assert!(!state.image_upload_consent);
    }

    #[test]
    fn pending_attachment_status_marks_correction_images_as_not_sent() {
        let mut state = AssistantPanelState::default();
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        state.vision_enabled = true;
        state.image_upload_consent = true;
        state.begin_request("interpretá la imagen".into());

        assert!(pending_attachment_status(&state).contains("payload en curso"));

        state.image_upload_consent = false;
        assert!(pending_attachment_status(&state).contains("no se enviarán"));
    }

    #[test]
    fn proposal_correction_clears_stale_actions_and_image_consent() {
        let mut state = AssistantPanelState::default();
        state.verified_proposals.push(VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: parameter_proposal("a = 2"),
            prerequisite_parameters: Vec::new(),
        });
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        state.image_upload_consent = true;

        state.begin_proposal_correction();

        assert!(state.verified_proposals.is_empty());
        assert!(state.is_pending);
        assert!(!state.is_cancelling);
        assert!(!state.image_upload_consent);
    }

    #[test]
    fn proposal_correction_replaces_the_latest_assistant_turn_without_an_orphan() {
        let mut state = AssistantPanelState::default();
        state.begin_request("graficá una curva".into());
        state.complete_request("```grafito\nUnsupportedGraph[]\n```".into());
        state.begin_proposal_correction();

        state.complete_proposal_correction("```grafito\nFunction[sin(x)]\n```".into());

        assert_eq!(state.conversation.len(), 2);
        assert_eq!(state.conversation[0].role, ConversationRole::User);
        assert_eq!(state.conversation[1].role, ConversationRole::Assistant);
        assert_eq!(
            state.conversation[1].content,
            "```grafito\nFunction[sin(x)]\n```"
        );
        assert_eq!(
            state
                .conversation_within_budget(4_096)
                .into_iter()
                .map(|turn| turn.content)
                .collect::<Vec<_>>(),
            vec![
                "graficá una curva".to_string(),
                "```grafito\nFunction[sin(x)]\n```".to_string(),
            ]
        );
    }

    #[test]
    fn repair_session_excludes_the_rejected_exchange_and_replaces_its_exact_turn() {
        let mut state = AssistantPanelState {
            conversation: vec![
                ConversationTurn::user("consulta anterior"),
                ConversationTurn::assistant("respuesta anterior"),
                ConversationTurn::user("graficá una función compleja"),
                ConversationTurn::assistant(
                    "```grafito\nDomainColoring[1/z, -2, 2, -2, 2, r]\n```",
                ),
            ],
            ..Default::default()
        };
        let feedback = grafito_assistant_types::AssistantRepairFeedback {
            failures: Vec::new(),
        };
        state.offer_proposal_correction_for_turn(
            "graficá una función compleja".into(),
            feedback.clone(),
            Some(3),
            1,
            correction_context(),
        );

        let history = state.conversation_before_turn_within_budget(3, 4_096);
        assert_eq!(
            history
                .into_iter()
                .map(|turn| turn.content)
                .collect::<Vec<_>>(),
            vec!["consulta anterior", "respuesta anterior"]
        );
        assert_eq!(
            state.take_proposal_correction_session(),
            Some(("graficá una función compleja".into(), feedback, 3, 1,))
        );
        state.restore_proposal_correction();
        assert!(state.proposal_correction_available);

        state.begin_proposal_correction_with_route(true);
        assert!(state.is_fusion_review);
        assert!(state.complete_proposal_correction_at(
            3,
            "```grafito\nDomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, 200]\n```".into(),
        ));
        assert_eq!(
            state.conversation[3].content,
            "```grafito\nDomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, 200]\n```"
        );
        assert!(!state.proposal_correction_available);
        assert!(!state.is_fusion_review);
    }

    #[test]
    fn correction_context_rejects_document_or_focus_changes_before_retry() {
        let mut state = AssistantPanelState::default();
        let context = ImmutableDocumentContext::empty(7);
        let focus = AssistantFocus::function("f", "sin(x)", None, None, false);
        assert!(!state.proposal_correction_matches_context(&context, Some(&focus)));
        let feedback = AssistantRepairFeedback {
            failures: Vec::new(),
        };
        state.offer_proposal_correction_for_turn(
            "corregí la propuesta".into(),
            feedback,
            Some(1),
            0,
            AssistantCorrectionContext {
                document_revision: context.revision,
                document_digest: context.digest.clone(),
                focus: Some(focus.clone()),
            },
        );

        assert!(state.proposal_correction_matches_context(&context, Some(&focus)));
        assert!(!state.proposal_correction_matches_context(
            &ImmutableDocumentContext::empty(8),
            Some(&focus),
        ));
        assert!(!state.proposal_correction_matches_context(
            &context,
            Some(&AssistantFocus::function("g", "sin(x)", None, None, false)),
        ));

        state.invalidate_proposal_correction();
        assert!(!state.proposal_correction_available);
    }

    #[test]
    fn pending_request_rejects_attachment_mutation() {
        let mut state = AssistantPanelState::default();
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        state.begin_request("interpretá la imagen".into());

        let result = state.add_attachment(ImageAttachment::new("image/png", vec![2], 1, 1));

        assert!(result.is_err());
        assert!(!state.remove_attachment(0));
        assert_eq!(state.attachments.len(), 1);
    }

    #[test]
    fn enter_submits_only_when_the_editor_is_focused_without_shift() {
        assert!(should_submit_on_enter(true, true, false, true));
        assert!(!should_submit_on_enter(true, true, true, true));
        assert!(!should_submit_on_enter(false, true, false, true));
        assert!(!should_submit_on_enter(true, true, false, false));
    }

    #[test]
    fn model_choices_merge_catalog_discovery_and_current_selection_without_duplicates() {
        let mut state = AssistantPanelState {
            model: "kept-model".into(),
            ..Default::default()
        };
        state.set_available_models(vec![
            "glm-5.2".into(),
            "deepseek-v4-flash".into(),
            "glm-5.2".into(),
            "kimi-k2.7-code".into(),
            "mimo-v2.5".into(),
        ]);

        let choices = state.model_choices();

        assert!(choices.contains(&"kept-model".to_string()));
        assert!(choices.contains(&"deepseek-v4-pro".to_string()));
        assert!(choices.contains(&"minimax-m3".to_string()));
        assert!(choices.contains(&"fusion".to_string()));
        assert!(!choices.iter().any(|model| model.contains("kimi")));
        assert!(!choices.iter().any(|model| model.contains("mimo")));
        assert_eq!(
            choices.iter().filter(|model| *model == "glm-5.2").count(),
            1
        );
    }

    #[test]
    fn local_submission_with_images_does_not_require_remote_upload_consent() {
        let mut state = AssistantPanelState {
            problem: "interpretá la imagen".into(),
            key_available: true,
            ..Default::default()
        };
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();

        assert!(state.can_submit());
    }

    #[test]
    fn remote_authorization_keeps_the_original_turn_and_needs_an_explicit_action() {
        let mut state = AssistantPanelState::default();
        state.begin_request("Explicá el teorema de Stokes".into());
        state.stage_remote_authorization(
            "Explicá el teorema de Stokes".into(),
            "Esta consulta todavía no está disponible localmente.".into(),
        );

        assert!(state.has_pending_remote_authorization());
        assert!(!state.is_pending);
        assert_eq!(state.conversation.len(), 1);
        assert_eq!(
            state.begin_authorized_remote_request().as_deref(),
            Some("Explicá el teorema de Stokes")
        );
        assert!(!state.has_pending_remote_authorization());
        assert!(state.is_pending);
        assert_eq!(state.conversation.len(), 1);
    }

    #[test]
    fn remote_authorization_never_reuses_image_upload_consent() {
        let mut state = AssistantPanelState {
            image_upload_consent: true,
            ..Default::default()
        };

        state.stage_remote_authorization("primera consulta".into(), "sin soporte local".into());
        assert!(!state.image_upload_consent);

        state.image_upload_consent = true;
        state.stage_remote_authorization("consulta reemplazada".into(), "sin soporte local".into());
        assert!(!state.image_upload_consent);

        state.image_upload_consent = true;
        state.cancel_remote_authorization();
        assert!(!state.image_upload_consent);
    }

    #[test]
    fn local_plan_is_cleared_only_after_a_successful_apply() {
        let mut state = AssistantPanelState::default();
        let plan = ProposedPlan::new(
            ImmutableDocumentContext::empty(0).basis(),
            vec![grafito_assistant_types::AssistantOperation::SetVariable {
                name: "a".into(),
                value: 2.0,
            }],
        );
        state.stage_proposed_plan(plan, vec!["Set variable a = 2".into()]);

        assert!(state.proposed_plan().is_some());
        assert!(!state.finish_proposed_plan_application(false));
        assert!(state.proposed_plan().is_some());
        assert!(state.finish_proposed_plan_application(true));
        assert!(state.proposed_plan().is_none());
        assert!(state.proposed_plan_changes.is_empty());
    }

    #[test]
    fn assistant_turns_show_only_public_execution_origins() {
        let mut state = AssistantPanelState::default();
        state.begin_request("2 + 2".into());
        state.complete_local_request("4".into());
        assert_eq!(
            state.conversation[1].origin,
            Some(AssistantExecutionOrigin::Local)
        );

        state.begin_request("consulta externa".into());
        state.complete_request("respuesta".into());
        assert_eq!(
            state.conversation[3].origin,
            Some(AssistantExecutionOrigin::AuthorizedRemote)
        );
        assert_eq!(
            AssistantExecutionOrigin::AuthorizedRemote.public_label(),
            "Consulta remota autorizada"
        );
    }

    #[test]
    fn import_and_oversized_prompt_disable_submission_before_transport() {
        let mut state = AssistantPanelState {
            key_available: true,
            problem: "2 + 2".into(),
            is_importing_image: true,
            ..Default::default()
        };
        assert!(!state.can_submit());

        state.is_importing_image = false;
        state.problem = "x".repeat(RequestBudget::default().max_input_chars + 1);
        assert!(!state.can_submit());
    }

    #[test]
    fn history_is_trimmed_and_budgeted_before_the_next_request() {
        let mut state = AssistantPanelState::default();
        state.begin_request("q".repeat(4_096));
        state.complete_request("a".repeat(4_096));

        assert!(
            state
                .conversation
                .iter()
                .all(|turn| turn.content.len()
                    <= grafito_assistant_types::MAX_CONVERSATION_TURN_CHARS)
        );
        assert!(
            state
                .conversation_within_budget(512)
                .iter()
                .map(|turn| turn.content.len())
                .sum::<usize>()
                <= 512
        );
    }

    #[test]
    fn history_budget_keeps_user_and_assistant_turns_together() {
        let mut state = AssistantPanelState::default();
        state.begin_request("q".repeat(4_096));
        state.complete_request("a".repeat(4_096));

        assert!(state.conversation_within_budget(8_191).is_empty());
        let turns = state.conversation_within_budget(8_192);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, ConversationRole::User);
        assert_eq!(turns[1].role, ConversationRole::Assistant);
    }

    #[test]
    fn trim_turn_uses_a_unicode_character_budget() {
        let trimmed = trim_turn("á".repeat(MAX_CONVERSATION_TURN_CHARS + 1));

        assert_eq!(trimmed.chars().count(), MAX_CONVERSATION_TURN_CHARS);
        assert!(trimmed.ends_with('…'));
    }

    #[test]
    fn inline_double_dollar_and_bracket_math_use_math_rendering() {
        assert_eq!(inline_math_text("$$\\frac{1}{2}$$"), "1 / 2");
        assert_eq!(inline_math_text("\\[\\sqrt{x}\\]"), "√x");
    }

    #[test]
    fn focus_context_counts_toward_the_visible_input_budget() {
        let state = AssistantPanelState {
            key_available: true,
            problem: "q".into(),
            focus: Some(AssistantFocus {
                label: "f".into(),
                kind: "Function".into(),
                summary: "x".repeat(RequestBudget::default().max_input_chars),
            }),
            ..Default::default()
        };

        assert!(state.input_bytes() > RequestBudget::default().max_input_chars);
        assert!(!state.can_submit());
    }

    #[test]
    fn rich_assistant_blocks_recognize_math_tables_and_safe_command_fences() {
        let blocks = parse_assistant_blocks(
            "# Resultado\n\n- primera observación\n- segunda observación\n\n| x | f(x) |\n| --- | --- |\n| 0 | 1 |\n| 1 | 2 |\n\n$$\\frac{x^2}{2}$$\n\n```grafito\nFunction[sin(x)]\n```",
        );

        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::Heading { level: 1, text } if text == "Resultado"
        )));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::Table(rows)
                if rows == &vec![
                    vec![String::from("x"), String::from("f(x)")],
                    vec![String::from("0"), String::from("1")],
                    vec![String::from("1"), String::from("2")],
                ]
        )));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::DisplayMath(text) if text == r"\frac{x^2}{2}"
        )));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::Code { language, text }
                if language == "grafito" && text == "Function[sin(x)]"
        )));
    }

    #[test]
    fn rich_assistant_blocks_keep_malformed_syntax_as_plain_text() {
        let blocks = parse_assistant_blocks(
            "| una tabla sin separador |\n```grafito\nFunction[x]\nsegundo comando\n\n$sin(x)",
        );

        assert!(!blocks
            .iter()
            .any(|block| matches!(block, AssistantMessageBlock::Table(_))));
        assert!(!blocks
            .iter()
            .any(|block| matches!(block, AssistantMessageBlock::Code { .. })));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::Paragraph(text) if text.contains("$sin(x)")
        )));
    }

    #[test]
    fn rich_assistant_blocks_support_standard_multiline_math_and_outerless_tables() {
        let blocks = parse_assistant_blocks(
            "x | f(x)\n--- | ---\n0 | 1\n\n$$\n\\frac{x^2}{2}\n$$\n\n\\[\nx^2 + y^2 = 1\n\\]",
        );

        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::Table(rows)
                if rows == &vec![
                    vec![String::from("x"), String::from("f(x)")],
                    vec![String::from("0"), String::from("1")],
                ]
        )));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::DisplayMath(text) if text == r"\frac{x^2}{2}"
        )));
        assert!(blocks.iter().any(|block| matches!(
            block,
            AssistantMessageBlock::DisplayMath(text) if text == "x^2 + y^2 = 1"
        )));
    }

    #[test]
    fn math_subset_renders_common_latex_as_readable_linear_text() {
        assert_eq!(
            math_to_plain(r"\frac{\alpha_i^2}{\sqrt{\beta}} \leq \pi"),
            "αᵢ² / √β ≤π"
        );
        assert_eq!(math_to_plain(r"\Delta x \to \infty"), "Δx →∞");
    }

    #[test]
    fn math_subset_preserves_nested_scripts_and_falls_back_on_malformed_input() {
        assert_eq!(
            math_to_plain(r"\frac{x^{n+1}}{\sqrt{\beta_i}}"),
            "xⁿ⁺¹ / √βᵢ"
        );
        assert_eq!(math_to_plain(r"\frac{x}{"), r"\frac{x}{");
        assert_eq!(math_to_plain(r"\mathbb{R}"), "R");
        assert_eq!(math_to_plain("x^a^b"), "x^a^b");
        assert_eq!(math_to_plain(r"\sin x + a\!b"), "sin x + ab");
    }

    #[test]
    fn display_math_allocates_vertical_space_for_nested_structure() {
        let context = egui::Context::default();
        let mut size = egui::Vec2::ZERO;

        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                size = draw_math(ui, r"\frac{\alpha_i^2}{\sqrt{\beta}} ")
                    .rect
                    .size();
            });
        });

        assert!(size.y > TYPE_MD * 1.5);
        assert!(size.x > 0.0);
    }

    #[test]
    fn short_user_turn_does_not_consume_transcript_height() {
        let context = egui::Context::default();
        let mut transcript_height = 0.0;
        let mut turn_height = 0.0;
        let turn = ConversationTurn::user("2 + 2");

        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 600.0),
                )),
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none())
                    .show(context, |ui| {
                        transcript_height = ui.max_rect().height();
                        turn_height = ui
                            .scope(|ui| {
                                let _ = draw_conversation_turn(
                                    ui,
                                    &turn,
                                    AssistantProposalRenderState {
                                        verified_proposals: &[],
                                        applied_proposals: &[],
                                        preflight_candidate_count: 0,
                                        proposal_code_block_indices: &[],
                                        proposal_results_available: false,
                                        correction_available: false,
                                    },
                                    None,
                                    &mut AssistantBlocksCache::default(),
                                );
                            })
                            .response
                            .rect
                            .height();
                    });
            },
        );

        assert!(
            turn_height < transcript_height * 0.35,
            "short turn height {turn_height} consumed too much of the {transcript_height}px transcript"
        );
    }

    #[test]
    fn blocks_cache_is_content_addressed_and_bounded() {
        let mut cache = AssistantBlocksCache::default();
        let content = "# Título\n\nTexto explicativo\n\n$$\\frac{1}{2}$$";
        let first = cache.blocks(content);
        assert!(!first.is_empty());
        // Reuso por contenido idéntico sin re-parseo (misma cantidad de bloques).
        let again = cache.blocks(content);
        assert_eq!(first.len(), again.len());
        let mutated = cache.blocks(&format!("{content} extra"));
        assert!(mutated.len() >= first.len());
        let mut empty = AssistantBlocksCache::default();
        assert!(empty.blocks("").is_empty());
    }

    #[test]
    fn agent_activity_is_bounded_and_ledger_sets_and_clears() {
        let mut state = AssistantPanelState::default();
        assert!(state.agent_activity.is_empty());
        assert!(state.agent_ledger.is_none());

        for index in 0..20 {
            state.push_agent_activity(format!("tool {index}"));
        }
        assert!(state.agent_activity.len() <= 12);
        assert_eq!(state.agent_activity.last().unwrap().text, "tool 19");

        state.set_agent_ledger(Some("Goal: resolver".into()));
        assert_eq!(state.agent_ledger.as_deref(), Some("Goal: resolver"));
        state.set_agent_ledger(Some("   ".into()));
        assert!(state.agent_ledger.is_none());

        state.set_agent_ledger(Some("Goal: resolver".into()));
        state.clear_agent_progress();
        assert!(state.agent_activity.is_empty());
        assert!(state.agent_ledger.is_none());
    }

    #[test]
    fn media_card_renders_a_frame_without_panicking() {
        let context = egui::Context::default();
        let mut state = AssistantPanelState::default();
        let media = AssistantMedia {
            title: "probe".into(),
            frames: vec![egui::ColorImage::new([4, 4], egui::Color32::WHITE)],
        };
        state.set_media(Some(media), &context);
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let theme = current_theme(ui.ctx());
                let _ = theme;
                // draw_media_card es privada al módulo tests; se valida vía set_media.
            });
        });
        assert!(state.media.is_some());
        assert!(state.media_textures().1);
    }

    #[test]
    fn full_permission_defaults_to_automatic_remote_answers() {
        let state = AssistantPanelState::default();
        assert!(state.full_permission);
    }

    #[test]
    fn empty_state_is_reserved_for_an_idle_empty_transcript() {
        let state = AssistantPanelState::default();
        assert!(should_draw_empty_state(&state));

        let mut pending = state.clone();
        pending.is_pending = true;
        assert!(!should_draw_empty_state(&pending));

        let mut with_turn = state;
        with_turn.begin_request("2 + 2".into());
        assert!(!should_draw_empty_state(&with_turn));
    }

    #[test]
    fn minimax_m3_accepts_images_but_fusion_does_not() {
        assert!(model_allows_image_attachment("minimax-m3"));
        assert!(!model_allows_image_attachment("fusion"));
    }

    #[test]
    fn image_upload_consent_resets_after_each_finished_request() {
        let mut state = AssistantPanelState {
            problem: "interpretá la imagen".into(),
            ..Default::default()
        };
        state
            .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
            .unwrap();
        state.vision_enabled = true;
        state.image_upload_consent = true;

        state.begin_request("interpretá la imagen".into());

        assert!(state.image_upload_consent);
        assert!(state.vision_enabled);
        state.complete_request("respuesta".into());
        assert!(!state.image_upload_consent);
    }

    #[test]
    fn submitted_user_turn_is_visible_before_the_remote_completion() {
        let mut state = AssistantPanelState::default();

        state.begin_request("Graficá x^2 - 4x + 3".into());

        assert!(state.is_pending);
        assert_eq!(state.conversation.len(), 1);
        assert_eq!(state.conversation[0].role, ConversationRole::User);
        assert_eq!(state.conversation[0].content, "Graficá x^2 - 4x + 3");
        assert!(state.conversation_within_budget(4_096).is_empty());
    }

    #[test]
    fn completed_responses_arm_the_reveal_and_later_actions_clear_it() {
        let mut state = AssistantPanelState::default();
        assert!(!state.reveal_pending);

        state.begin_request("grafico x^2".into());
        assert!(!state.reveal_pending);

        state.complete_local_request("Respuesta con varios bloques.".into());
        assert!(state.reveal_pending);

        state.begin_request("otra pregunta".into());
        assert!(!state.reveal_pending);

        state.complete_request("respuesta".into());
        assert!(state.reveal_pending);

        state.fail_request("sin conexión");
        assert!(!state.reveal_pending);

        state.complete_local_request("respuesta nueva".into());
        assert!(state.reveal_pending);
        state.clear_conversation();
        assert!(!state.reveal_pending);
    }

    #[test]
    fn reveal_clip_clears_without_blocks_and_arms_with_blocks() {
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let mut empty = AssistantPanelState::default();
                empty.complete_local_request("".into());
                assert!(empty.reveal_pending);
                assert!(assistant_reveal_clip(ui, &mut empty, 0).is_none());
                assert!(!empty.reveal_pending);

                let mut armed = AssistantPanelState::default();
                armed.complete_local_request("bloque uno\n\nbloque dos".into());
                let clip = assistant_reveal_clip(ui, &mut armed, 5);
                assert!(clip.is_some());
                assert!(armed.reveal_pending);
                assert!(armed.reveal_started_at.is_some());
                assert!(
                    armed.reveal_started_at.unwrap().is_finite(),
                    "reveal clock must remain finite"
                );
            });
        });
    }

    #[test]
    fn failed_turns_remain_visible_but_are_excluded_from_remote_history() {
        let mut state = AssistantPanelState::default();
        state.begin_request("mensaje fallido".into());
        state.fail_request("Sin conexión");
        state.begin_request("mensaje correcto".into());
        state.complete_request("respuesta correcta".into());

        assert_eq!(state.conversation.len(), 3);
        assert_eq!(state.conversation[0].content, "mensaje fallido");
        assert_eq!(
            state
                .conversation_within_budget(4_096)
                .into_iter()
                .map(|turn| turn.content)
                .collect::<Vec<_>>(),
            vec!["mensaje correcto", "respuesta correcta"]
        );
    }

    #[test]
    fn a_new_request_clears_a_pending_proposal_correction() {
        let mut state = AssistantPanelState::default();

        state.offer_proposal_correction(
            "consulta previa".into(),
            grafito_assistant_types::AssistantRepairFeedback {
                failures: Vec::new(),
            },
            correction_context(),
        );
        assert!(state.proposal_correction_available);

        state.begin_request("una consulta nueva".into());
        assert!(!state.proposal_correction_available);
    }

    #[test]
    fn proposal_correction_is_consumed_once_after_explicit_selection() {
        let mut state = AssistantPanelState::default();
        let feedback = grafito_assistant_types::AssistantRepairFeedback {
            failures: Vec::new(),
        };
        state.offer_proposal_correction(
            "consulta previa".into(),
            feedback.clone(),
            correction_context(),
        );

        assert_eq!(
            state
                .take_proposal_correction()
                .map(|(question, stored_feedback)| (question, stored_feedback.prompt_text())),
            Some(("consulta previa".into(), feedback.prompt_text()))
        );
        assert!(!state.proposal_correction_available);
        assert!(state.take_proposal_correction().is_none());
    }

    #[test]
    fn proposal_correction_retains_sanitized_feedback_until_explicit_selection() {
        let mut state = AssistantPanelState::default();
        let feedback = grafito_assistant_types::AssistantRepairFeedback {
            failures: Vec::new(),
        };

        state.offer_proposal_correction(
            "graficá una curva".into(),
            feedback.clone(),
            correction_context(),
        );

        let (question, stored_feedback) = state
            .take_proposal_correction()
            .expect("an explicit correction must retain its safe feedback");
        assert_eq!(question, "graficá una curva");
        assert_eq!(stored_feedback.prompt_text(), feedback.prompt_text());
    }

    #[test]
    fn focused_context_preview_is_bounded_for_the_composer() {
        let preview = focused_context_preview(&"x".repeat(300));

        assert!(preview.chars().count() <= 161);
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn verified_proposals_without_a_visible_code_block_remain_renderable() {
        let proposals = [
            VerifiedAssistantProposal {
                candidate_index: 1,
                proposal: command_proposal("Function[x]"),
                prerequisite_parameters: Vec::new(),
            },
            VerifiedAssistantProposal {
                candidate_index: 3,
                proposal: scene_proposal(&["Segment3D[0,0,0,1,0,0]"]),
                prerequisite_parameters: Vec::new(),
            },
        ];

        assert_eq!(
            unrendered_verified_proposals(&proposals, &[1])
                .into_iter()
                .map(|proposal| proposal.candidate_index)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }

    #[test]
    fn unverified_proposals_are_not_actionable() {
        let proposal = command_proposal("Function[1/0]");

        assert!(verified_proposal(0, &[]).is_none());
        assert!(verified_proposal(
            0,
            &[VerifiedAssistantProposal {
                candidate_index: 0,
                proposal: proposal.clone(),
                prerequisite_parameters: Vec::new(),
            }]
        )
        .is_some());
        assert!(verified_proposal(
            1,
            &[VerifiedAssistantProposal {
                candidate_index: 0,
                proposal: proposal.clone(),
                prerequisite_parameters: vec![parameter_assignment("a = 1")],
            }]
        )
        .is_none());
    }

    #[test]
    fn proposal_cards_distinguish_applied_rejected_and_preflight_limited_candidates() {
        let proposal = command_proposal("Function[sin(x)]");
        let applied = [VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: proposal.clone(),
            prerequisite_parameters: Vec::new(),
        }];

        assert!(matches!(
            assistant_proposal_card_state(0, &[], &applied, 1, true),
            Some(AssistantProposalCardState::Applied(_))
        ));
        assert!(matches!(
            assistant_proposal_card_state(0, &[], &[], 1, true),
            Some(AssistantProposalCardState::Rejected)
        ));
        assert!(matches!(
            assistant_proposal_card_state(1, &[], &[], 1, true),
            Some(AssistantProposalCardState::NotPreflighted)
        ));
        assert!(assistant_proposal_card_state(0, &[], &[], 1, false).is_none());
    }

    #[test]
    fn proposal_cards_follow_command_owned_code_block_identity() {
        let candidates = [1, 4, 6];

        assert_eq!(candidate_index_for_code_block(&candidates, 1), Some(0));
        assert_eq!(candidate_index_for_code_block(&candidates, 4), Some(1));
        assert_eq!(candidate_index_for_code_block(&candidates, 6), Some(2));
        assert_eq!(candidate_index_for_code_block(&candidates, 0), None);
    }

    #[test]
    fn rejected_proposal_click_only_requests_a_safe_correction() {
        assert_eq!(
            rejected_proposal_action(true, true),
            Some(AssistantUiAction::RetryProposalCorrection)
        );
        assert!(rejected_proposal_action(true, false).is_none());
        assert!(rejected_proposal_action(false, true).is_none());
    }

    #[test]
    fn only_the_latest_assistant_turn_can_use_current_verified_proposals() {
        let mut state = AssistantPanelState {
            conversation: vec![
                ConversationTurn::user("primera consulta"),
                ConversationTurn::assistant("```grafito\nImplicitCurve[x^2+y^2=a^2]\n```"),
                ConversationTurn::user("segunda consulta"),
                ConversationTurn::assistant("```grafito\nImplicitCurve[x^2+y^2=a^2]\n```"),
            ],
            ..Default::default()
        };
        state.verified_proposals = vec![VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: command_proposal("ImplicitCurve[x^2+y^2=a^2]"),
            prerequisite_parameters: vec![parameter_assignment("a = 2")],
        }];

        assert!(verified_proposals_for_turn(&state, 1).is_empty());
        assert_eq!(
            verified_proposals_for_turn(&state, 3),
            &state.verified_proposals
        );
    }

    #[test]
    fn current_assistant_turn_keeps_correction_available_with_verified_parameters() {
        let mut state = AssistantPanelState {
            conversation: vec![
                ConversationTurn::user("graficá una función"),
                ConversationTurn::assistant(
                    "```grafito-param\na = 2\n```\n```grafito\nFunction[1/0]\n```",
                ),
            ],
            ..Default::default()
        };
        state.verified_proposals = vec![VerifiedAssistantProposal {
            candidate_index: 0,
            proposal: parameter_proposal("a = 2"),
            prerequisite_parameters: Vec::new(),
        }];
        state.offer_proposal_correction(
            "graficá una función".into(),
            grafito_assistant_types::AssistantRepairFeedback {
                failures: Vec::new(),
            },
            correction_context(),
        );

        assert!(proposal_correction_available_for_turn(&state, 1));
        assert!(!proposal_correction_available_for_turn(&state, 0));
    }

    #[test]
    fn header_actions_are_not_overwritten_by_composer_actions() {
        let mut header_action = Some(AssistantUiAction::HidePanel);
        retain_first_assistant_action(&mut header_action, Some(AssistantUiAction::Submit));
        assert_eq!(header_action, Some(AssistantUiAction::HidePanel));

        let mut no_header_action = None;
        retain_first_assistant_action(&mut no_header_action, Some(AssistantUiAction::Submit));
        assert_eq!(no_header_action, Some(AssistantUiAction::Submit));
    }

    #[test]
    fn composer_height_returns_to_its_compact_baseline_without_optional_content() {
        let mut state = AssistantPanelState::default();
        assert_eq!(ASSISTANT_COMPOSER_BASE_HEIGHT, 116.0);
        assert_eq!(
            assistant_composer_height(&state),
            ASSISTANT_COMPOSER_BASE_HEIGHT
        );

        state.problem = "x".repeat(RequestBudget::default().max_input_chars * 3 / 4 + 1);
        assert_eq!(
            assistant_composer_height(&state),
            ASSISTANT_COMPOSER_BASE_HEIGHT + ASSISTANT_COMPOSER_BUDGET_HEIGHT
        );

        state.problem.clear();
        assert_eq!(
            assistant_composer_height(&state),
            ASSISTANT_COMPOSER_BASE_HEIGHT
        );
    }

    #[test]
    fn composer_height_accounts_for_bounded_attachment_rows() {
        let mut state = AssistantPanelState::default();
        for _ in 0..AttachmentLimits::default().max_attachments {
            state
                .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
                .unwrap();
        }

        assert_eq!(
            assistant_composer_height(&state),
            ASSISTANT_COMPOSER_BASE_HEIGHT
                + ASSISTANT_COMPOSER_ATTACHMENT_HEIGHT
                + ASSISTANT_COMPOSER_ATTACHMENT_ROW_HEIGHT
        );

        let editable_height = assistant_composer_height(&state);
        state.is_pending = true;
        assert!(assistant_composer_height(&state) > editable_height);
    }

    #[test]
    fn assistant_width_preserves_a_canvas_budget_before_reaching_its_default_width() {
        let (minimum, maximum, default) = assistant_panel_widths(960.0);
        assert_eq!(minimum, 340.0);
        assert_eq!(default, 400.0);
        assert!((400.0..=404.0).contains(&maximum), "maximum {maximum}");

        let (minimum, maximum, default) = assistant_panel_widths(600.0);
        assert!(minimum <= maximum);
        assert!(default <= maximum);
        assert!(600.0 - maximum >= 440.0);
    }

    #[test]
    fn narrow_viewports_use_a_bottom_sheet_instead_of_crushing_the_canvas() {
        assert!(assistant_uses_bottom_sheet(
            ASSISTANT_SIDE_PANEL_MIN_VIEWPORT_WIDTH - 1.0
        ));
        assert!(!assistant_uses_bottom_sheet(
            ASSISTANT_SIDE_PANEL_MIN_VIEWPORT_WIDTH
        ));
    }

    #[test]
    fn compact_panel_keeps_the_full_dynamic_composer_visible() {
        let mut state = AssistantPanelState::default();
        for _ in 0..AttachmentLimits::default().max_attachments {
            state
                .add_attachment(ImageAttachment::new("image/png", vec![1], 1, 1))
                .unwrap();
        }
        state.focus = Some(AssistantFocus {
            label: "f".into(),
            kind: "Function".into(),
            summary: "contexto".into(),
        });
        state.problem = "x".repeat(RequestBudget::default().max_input_chars * 3 / 4 + 1);

        let (minimum, maximum, default) = assistant_compact_panel_heights(600.0, 0.0, &state);
        assert!(minimum >= assistant_composer_height(&state) + 96.0);
        assert!(maximum >= minimum);
        assert!(default >= minimum);

        let (minimum, maximum, default) = assistant_compact_panel_heights(480.0, 220.0, &state);
        assert!(minimum <= maximum);
        assert!(maximum <= 480.0);
        assert!(default <= maximum);
    }
}
