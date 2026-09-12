//! Slice 6/6 del split de god objects: jobs remoto/agente/modelos/propuestas.
//!
//! `AssistantJobsController` es el nuevo hogar de la orquestación de jobs del
//! asistente que vivía en `impl GrafitoApp` dentro de `assistant.rs`. El
//! controlador es stateless: todo estado que toca llega por
//! [`AssistantJobsContext`] como `&mut` por parámetro, nunca como
//! `&mut self`/`&mut GrafitoApp` entero. Los métodos de `GrafitoApp` en
//! `assistant.rs` quedan como shims finos que parten `self` en el contexto y
//! delegan (cero cambio de comportamiento, cero cambio de firmas).
//!
//! Mapeo nombre nuevo (controlador) → nombre viejo (shim en `assistant.rs`):
//! `build_remote_request` → `build_remote_assistant_request`,
//! `start_remote` → `start_remote_assistant_job`,
//! `start_agent` → `start_agent_assistant_job`,
//! `start_remote_proposal` → `start_remote_proposal_verification`,
//! `start_authorized` → `start_authorized_remote_assistant_request`,
//! `start_remote_for` → `start_remote_assistant_for`,
//! `apply_proposed` → `apply_proposed_assistant_plan` (devuelve
//! [`ApplyProposedEffect`)
//! `start_model` → `start_model_request`,
//! `cancel_remote` → `cancel_assistant_request`,
//! `cancel_agent`/`cancel_model`/`cancel_proposal` → granulares por slot
//! (antes inlinedos en `cancel_anim_job`; misma semántica: señalan el token
//! y el slot se conserva hasta el drain, sin huérfanos),
//! `cancel_stale_remote` → `cancel_stale_remote_request`,
//! `cancel_stale_model` → `cancel_stale_model_request`,
//! `poll_agent` → `poll_assistant_agent`, `poll_assistant_jobs` igual.
//!
//! Efectos que NO cruzaron el seam (quedan en el shim con dueño exacto):
//! - `record_step_from_diff` / `set_perspective` /
//!   `ensure_algebra_panel_visible` (vía [`ApplyProposedEffect`]): el shim
//!   los aplica tras delegar. Dueño: `app.rs` / `assistant.rs`.
//! - `exam_blocks` (toast + flag): el shim lo chequea antes de delegar.
//!   Dueño: `app.rs:4780`.
//! - `SessionApiKey` (Drop zeroize): NO se movió ni se clonó; el controlador
//!   solo usa `key_for`/`remember_key`. Dueño: `assistant.rs`.
//! - `AssistantRuntime` + slots: NO se hundieron; el controlador los toca por
//!   métodos/`pub(crate)` ya existentes. Dueño: `assistant.rs`.
//! - `AssistantTurnState`: SÍ se movió acá (tipo puro + `transition_to` +
//!   `derive_from`): la única fuente de verdad del turno es DERIVADA del
//!   runtime + panel, sin campo almacenado que diverja. `GrafitoApp` la lee
//!   vía `assistant_turn_state()` (`assistant.rs`, sin estado propio).
//!   SIN variante `Done`: los terminales siguen siendo `Failed`/`Cancelled`
//!   como asumen `transition_to` + `poll_jobs`.

use crate::assistant::{
    accepts_model_result, accepts_remote_context, accepts_remote_result,
    apply_local_assistant_plan, attachment_error_message, is_session_or_account_error,
    is_socratic_repair_error, parse_agent_ask_user_pending, pop_provisional_stream_turn,
    remote_error_message, send_agent_msg_nonblocking, should_fallback_agent_spark_to_deepseek,
    should_fallback_remote_spark_to_deepseek, socratic_guard_context, AssistantRuntime,
};
use crate::assistant_preflight::{
    assistant_graph_perspective, can_offer_assistant_proposal_correction,
    inspect_remote_proposals_cancellable, RemoteProposalVerification,
};
use crate::{assistant_credentials, Perspective, ViewMode};
use grafito_assistant::{
    rate_limit_cooldown_remaining_secs, rate_limit_paused_message,
    request_remote_models_with_api_key_on_worker, request_remote_streaming_with_api_key_on_worker,
    CancellationToken, ProviderSettings, RemoteCompletion, SocraticGuardContext,
};
use grafito_assistant_types::{
    AssistantFocus, AssistantRepairFeedback, AssistantRequest, AttachmentLimits, ConversationRole,
    ImmutableDocumentContext, ProviderCapabilities, ProviderProfile,
    REMOTE_CONTEXT_PROMPT_OVERHEAD_BYTES, REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES,
    REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES, REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES,
    REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES,
};
use grafito_core::{ChangeSet, Document, ObjectId};
use grafito_geometry::Camera3D;
use grafito_pedagogy::{scaffold::is_exploratory_request, SocraticFsm};
use grafito_profile::StudentProfile;
use grafito_ui::assistant::{AssistantCorrectionContext, AssistantPanelState, MediaExportState};
use grafito_ui::toast::ToastKind;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

/// Ruta del job remoto (movida verbatim desde `assistant.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssistantRemoteRoute {
    SelectedModel,
    FusionFallback,
}

/// Statem del turno del asistente (F4: enum real, antes solo §4.3 en prosa).
///
/// Única fuente de verdad DERIVADA (cerebro-audit 2026-09-10): no hay campo
/// almacenado en `GrafitoApp` ni en `AssistantRuntime` — [`Self::derive_from`]
/// la computa desde los slots reales + flags del panel en cada lectura, así
/// que no puede divergir. Sin `Verifying`: el preflight corre síncrono
/// dentro de `Thinking`. Sin `Done`: el turno exitoso vuelve a `Idle` vía
/// `complete_request`; `transition_to` + `poll_jobs` asumen terminales
/// `Failed`/`Cancelled` (agregar `Done` rompería esa invariante).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum AssistantTurnState {
    #[default]
    Idle,
    Composing,
    Thinking,
    AwaitingAuthorization,
    Animating {
        job_id: String,
    },
    Failed {
        reason: String,
    },
    Cancelled,
}

impl AssistantTurnState {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn state_name(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Composing => "Composing",
            Self::Thinking => "Thinking",
            Self::AwaitingAuthorization => "AwaitingAuthorization",
            Self::Animating { .. } => "Animating",
            Self::Failed { .. } => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Cancelled)
    }

    /// `pub(crate)` (antes privada de `assistant.rs`): los tests del módulo
    /// hermano la ejercen vía re-export. Sin cambios de transiciones.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn transition_to(&mut self, next: Self) -> Result<(), String> {
        let legal = matches!(
            (&*self, &next),
            (Self::Idle, Self::Composing)
                | (Self::Composing, Self::Thinking)
                | (Self::Thinking, Self::AwaitingAuthorization)
                | (Self::Thinking, Self::Animating { .. })
                | (Self::Thinking, Self::Failed { .. })
                | (Self::AwaitingAuthorization, Self::Animating { .. })
                | (Self::AwaitingAuthorization, Self::Thinking)
                | (Self::AwaitingAuthorization, Self::Cancelled)
                | (Self::Animating { .. }, Self::Failed { .. })
                | (Self::Animating { .. }, Self::Cancelled)
                | (Self::Animating { .. }, Self::Idle)
                | (Self::Failed { .. }, Self::Idle)
                | (Self::Cancelled, Self::Idle)
                | (Self::Idle, Self::Idle)
        );
        if legal {
            *self = next;
            Ok(())
        } else {
            Err(format!(
                "transición inválida {} -> {}",
                self.state_name(),
                next.state_name()
            ))
        }
    }

    /// Única fuente de verdad del turno, computada sin estado almacenado.
    ///
    /// Prioridad espeja `AssistantPanelState::lifecycle` (Cancelling >
    /// Animating > Thinking > AwaitingAuthorization > Failed > Composing >
    /// Idle) pero anclada en los slots reales del runtime, no solo en flags:
    /// - `Cancelled`: solo mientras `is_cancelling` (ventana entre el botón
    ///   Cancel y el drain). Tras el drain el panel publica el error vía
    ///   `fail_request` y esto deriva a `Failed`, igual que la Piel.
    /// - `Animating{job_id}`: `anim_job` → `"anim"`, `anim_ia_job` →
    ///   `"anim-ia"`, o solo flag `anim_progress` → `"anim-progress"`. Los
    ///   exports de card (gif/png/mp4/webm) NO mapean: su fuente es
    ///   `MediaExportState`, no el turno de pregunta.
    /// - `Thinking`: slots de consulta (remote/proposal/agent) o `is_pending`
    ///   (cubre la carrera slot-antes-que-flag y viceversa).
    /// - Auxiliares que NO mapean (ver [`Self::is_auxiliary`]): `model_job`
    ///   (lista de modelos), `image_job` (import de imagen), exports de card
    ///   y preview de streaming. Derivan a `Idle`/`Composing`/lo que toque
    ///   sin secuestrar el turno: son trabajo sin pregunta en curso.
    /// - `Failed{reason}`: clona el error visible del panel (ya en español).
    ///
    /// No toca historial/trim/replay/export/polling: lectura pura.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn derive_from(runtime: &AssistantRuntime, panel: &AssistantPanelState) -> Self {
        if panel.is_cancelling {
            return Self::Cancelled;
        }
        if runtime.anim_job.is_some() {
            return Self::Animating {
                job_id: "anim".to_string(),
            };
        }
        if runtime.anim_ia_job.is_some() {
            return Self::Animating {
                job_id: "anim-ia".to_string(),
            };
        }
        if panel.anim_progress {
            return Self::Animating {
                job_id: "anim-progress".to_string(),
            };
        }
        if runtime.remote_job.is_some()
            || runtime.proposal_job.is_some()
            || runtime.agent_job.is_some()
            || panel.is_pending
        {
            return Self::Thinking;
        }
        if panel.has_pending_remote_authorization() {
            return Self::AwaitingAuthorization;
        }
        if let Some(reason) = panel.error.clone() {
            return Self::Failed { reason };
        }
        if !panel.problem.trim().is_empty() {
            return Self::Composing;
        }
        Self::Idle
    }

    /// ¿Hay trabajo auxiliar en vuelo sin turno de pregunta asociado?
    ///
    /// Cubre lo que [`Self::derive_from`] deja fuera a propósito:
    /// - `model_job`: lista de modelos (worker remoto con el mismo
    ///   `RequestBudget` que el chat —`timeout_ms` 60 s, `max_steps` 8—;
    ///   visible en el selector, no en el chat).
    /// - `image_job`: import de imagen (decode local en hilo, sin red;
    ///   visible vía `is_importing_image` + `attachment_message`).
    /// - Exports de card (`MediaExportState::Exporting`): hilos
    ///   `JoinHandle` (gif/png) o ffmpeg-sidecar (mp4/webm) y LaTeX
    ///   (pdf/svg); su progreso vive en el diálogo/card, con `Cancel`
    ///   propio que señala el token y el poll drena honesto.
    /// - Preview de streaming (`remote_job.preview_active`): burbuja
    ///   provisional del turno en curso (ya mapea a `Thinking` por el slot
    ///   remoto; se lista para que el timeout sea visible: el final llega
    ///   por el canal de completado con su propio presupuesto, y el poll lo
    ///   limpia en cada rama terminal vía `pop_provisional_stream_turn`).
    ///
    /// Pura sobre runtime + panel, sin I/O: el llamante la usa para avisos
    /// ("sigo trabajando en X") sin secuestrar el turno derivado.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_auxiliary(runtime: &AssistantRuntime, panel: &AssistantPanelState) -> bool {
        runtime.model_job.is_some()
            || runtime.image_job.is_some()
            || *panel.media_export_state() == MediaExportState::Exporting
            || runtime
                .remote_job
                .as_ref()
                .is_some_and(|job| job.preview_active)
    }
}

/// Job remoto en vuelo (movido verbatim; campos `pub(crate)` porque
/// `AssistantRuntime` —que vive en `assistant.rs`— los construye y drena).
pub(crate) struct AssistantRemoteJob {
    pub(crate) id: u64,
    /// Identidad seleccionada por el usuario, usada para descartar resultados obsoletos.
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) cancellation: CancellationToken,
    pub(crate) receiver: Receiver<Result<RemoteCompletion, String>>,
    /// Deltas de streaming SSE (sólo protocolo Responses; el resto lo deja
    /// desconectado y nunca hay preview). Acotado a 128 (best-effort).
    pub(crate) stream_rx: Option<Receiver<String>>,
    /// Texto acumulado del stream para la burbuja provisional.
    pub(crate) stream_text: String,
    /// Hay un turno provisional al final de `conversation` que debe limpiarse
    /// al terminar/cancelar (ver `pop_provisional_stream_turn`).
    pub(crate) preview_active: bool,
    /// Instante de arranque del worker (para etapas con timestamp).
    pub(crate) started_at: std::time::Instant,
    /// Instante del primer delta SSE (para `Recibiendo` + aviso lento).
    /// `None` = aún esperando primer token (p.ej. deepseek no-streaming).
    pub(crate) first_delta_at: Option<std::time::Instant>,
}

/// Job de verificación de propuesta (movido verbatim).
pub(crate) struct AssistantProposalJob {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) text: String,
    pub(crate) cancellation: CancellationToken,
    pub(crate) receiver: Receiver<Result<RemoteProposalVerification, String>>,
}

/// Lanzamiento remoto (movido verbatim).
pub(crate) struct AssistantRemoteLaunch {
    pub(crate) settings: ProviderSettings,
    pub(crate) request: AssistantRequest,
    pub(crate) api_key: Option<String>,
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    /// Guard socrático de sesión: el worker lo aplica sobre el completado
    /// final (streaming y no-streaming). `None` lo desactiva.
    pub(crate) socratic_guard: Option<SocraticGuardContext>,
}

/// Reparación socrática pendiente (movida verbatim).
pub(crate) struct AssistantRepairRequest {
    pub(crate) feedback: AssistantRepairFeedback,
    pub(crate) target_turn: usize,
}

/// Lanzamiento de verificación de propuesta (movido verbatim).
pub(crate) struct AssistantProposalLaunch {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) text: String,
}

/// Job de lista de modelos (movido verbatim).
pub(crate) struct AssistantModelJob {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) cancellation: CancellationToken,
    pub(crate) receiver: Receiver<Result<Vec<String>, String>>,
}

/// Job de importación de imagen (movido verbatim).
pub(crate) struct AssistantImageJob {
    pub(crate) receiver: Receiver<Result<grafito_assistant_types::ImageAttachment, String>>,
}

/// Mensaje del hilo del agente hacia la UI (movido verbatim).
pub(crate) enum AgentChannelMsg {
    Event(grafito_agent::AgentEvent),
    Done(Result<grafito_agent::loop_engine::AgentOutcome, String>),
}

/// Job del modo agente, loop con herramientas (movido verbatim).
pub(crate) struct AssistantAgentJob {
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) cancellation: grafito_agent::loop_engine::Cancellation,
    pub(crate) receiver: Receiver<AgentChannelMsg>,
    /// Canal lateral S2 (`ask_user` real vía evento): el forwarder de
    /// background reenvía el pendiente como `PendingClarification` sin
    /// bloquear (try_send acotado). La UI lo drena en `sync_assistant_for_frame`
    /// (hilo UI, try_recv) y lo muestra como botones; nunca toca la zona guard.
    pub(crate) clarification_receiver: Receiver<grafito_ui::assistant::PendingClarification>,
}

/// Resultado drenado del job remoto (movido verbatim).
pub(crate) struct FinishedRemoteJob {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) cancelled: bool,
    pub(crate) result: Result<RemoteCompletion, String>,
    /// El job dejó una burbuja provisional que el poll debe limpiar antes de
    /// procesar el resultado (éxito, error o cancelación).
    pub(crate) stream_preview_active: bool,
}

/// Resultado drenado del job de propuesta (movido verbatim).
pub(crate) struct FinishedProposalJob {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) model: String,
    pub(crate) route: AssistantRemoteRoute,
    pub(crate) fusion_fallback_allowed: bool,
    pub(crate) question: String,
    pub(crate) correction_attempt: u8,
    pub(crate) repair_target_turn: Option<usize>,
    pub(crate) document_revision: u64,
    pub(crate) document_digest: String,
    pub(crate) focus: Option<AssistantFocus>,
    pub(crate) text: String,
    pub(crate) cancelled: bool,
    pub(crate) result: Result<RemoteProposalVerification, String>,
}

/// Resultado drenado del job de modelos (movido verbatim).
pub(crate) struct FinishedModelJob {
    pub(crate) id: u64,
    pub(crate) provider: ProviderProfile,
    pub(crate) cancelled: bool,
    pub(crate) result: Result<Vec<String>, String>,
}

/// Seam explícito del slice 6: todo lo que un job remoto/agente/modelo/
/// propuesta necesita de `GrafitoApp`, como `&mut` por parámetro.
///
/// El shim en `assistant.rs` lo construye partiendo `&mut self` en campos
/// disjuntos (más un closure `notify` fiel a `GrafitoApp::notify`). Nada acá
/// guarda `&mut GrafitoApp` entero.
pub(crate) struct AssistantJobsContext<'a> {
    pub runtime: &'a mut AssistantRuntime,
    pub panel: &'a mut AssistantPanelState,
    pub document: &'a mut Document,
    pub undo_stack: &'a mut VecDeque<Document>,
    pub redo_stack: &'a mut VecDeque<ChangeSet>,
    pub profile: &'a mut StudentProfile,
    pub plugin_registry: &'a Option<grafito_plugins::PluginRegistry>,
    pub selected_object: Option<ObjectId>,
    pub camera: Camera3D,
    pub current_view: ViewMode,
    pub notify: &'a mut dyn FnMut(String, ToastKind),
}

/// Efecto de `apply_proposed` que el shim completa con dueños de `app.rs`.
///
/// El controlador aplica documento + undo + panel + toast; el shim aplica
/// `record_step_from_diff` + `set_perspective` + `ensure_algebra_panel_visible`
/// desde este efecto (sumideros independientes, orden irrelevante).
pub(crate) struct ApplyProposedEffect {
    pub applied: bool,
    pub wants_perspective: Option<Perspective>,
    pub record_action: Option<(String, HashSet<String>)>,
}

impl ApplyProposedEffect {
    fn none() -> Self {
        Self {
            applied: false,
            wants_perspective: None,
            record_action: None,
        }
    }
}

/// Parámetros del request remoto (agrupa los 6 del builder para no exceder
/// el tope de aridad de clippy; punto único construido por el shim y por
/// `start_remote_for`).
pub(crate) struct BuildRemoteParams {
    pub question: String,
    pub document_context: ImmutableDocumentContext,
    pub focus: Option<AssistantFocus>,
    pub attachments: Vec<grafito_assistant_types::ImageAttachment>,
    pub image_upload_consent: bool,
    pub repair: Option<AssistantRepairRequest>,
}

/// Controlador stateless de jobs del asistente (slice 6).
///
/// Toda la lógica vive en funciones asociadas que reciben
/// [`AssistantJobsContext`]; no hay `&mut self` ni estado propio.
pub(crate) struct AssistantJobsController;

impl AssistantJobsController {
    /// Réplica exacta de `GrafitoApp::object_labels_snapshot` (`app.rs:3374`,
    /// intocable): etiquetas no vacías del documento como conjunto.
    fn labels_snapshot(document: &Document) -> HashSet<String> {
        document
            .objects_iter()
            .filter(|(_, o)| !o.label().is_empty())
            .map(|(_, o)| o.label().to_string())
            .collect()
    }

    /// Instrucciones de plugins acotadas (movida verbatim de
    /// `plugin_instructions_budgeted`; lee el registry del contexto).
    fn plugin_instructions(plugin_registry: &Option<grafito_plugins::PluginRegistry>) -> String {
        const PLUGIN_INSTRUCTION_CAP_BYTES: usize = 4 * 1024;
        let Some(registry) = plugin_registry else {
            return String::new();
        };
        registry.instructions_bounded(
            grafito_assistant_types::MAX_SYSTEM_INSTRUCTIONS_BYTES
                .min(PLUGIN_INSTRUCTION_CAP_BYTES),
        )
    }

    pub(crate) fn emit_error(ctx: &mut AssistantJobsContext<'_>, error: impl Into<String>) {
        let error = error.into();
        ctx.panel.error = Some(error.clone());
        (ctx.notify)(error, ToastKind::Error);
    }

    /// Freno 429 lado UI (movido verbatim).
    pub(crate) fn fail_fast_if_rate_limited(ctx: &mut AssistantJobsContext<'_>) -> bool {
        if let Some(remaining) = rate_limit_cooldown_remaining_secs() {
            Self::emit_error(ctx, rate_limit_paused_message(remaining));
            true
        } else {
            false
        }
    }

    fn fail_request(ctx: &mut AssistantJobsContext<'_>, error: impl Into<String>) {
        let error = error.into();
        let current_model = ctx.panel.model.clone();
        let visible_error = remote_error_message(&error, &current_model);
        ctx.panel.fail_request(visible_error.clone());
        Self::report_error(ctx, visible_error);
    }

    fn fail_repair_request(ctx: &mut AssistantJobsContext<'_>, error: impl Into<String>) {
        ctx.panel.restore_proposal_correction();
        Self::fail_request(ctx, error);
    }

    pub(crate) fn report_error(ctx: &mut AssistantJobsContext<'_>, error: impl Into<String>) {
        Self::emit_error(ctx, error);
    }

    fn begin_cancelling(ctx: &mut AssistantJobsContext<'_>) {
        ctx.panel.begin_cancellation();
    }

    /// Guard socrático de la sesión actual (movido verbatim; ver docs arriba).
    ///
    /// El modo agente lo ignora porque ya orquesta sus propias tools socráticas.
    pub(crate) fn session_guard(
        panel: &AssistantPanelState,
        profile: &StudentProfile,
        question: &str,
    ) -> Option<SocraticGuardContext> {
        // Es demo, no evaluación: se salta el telling.
        if is_exploratory_request(question) {
            return None;
        }
        let topic = profile
            .working_memory
            .last_concept
            .as_deref()
            .or(profile.working_memory.current_topic.as_deref());
        // Sin concepto previo → demo, no evaluación.
        let topic_trimmed = topic.map(str::trim).filter(|topic| !topic.is_empty())?;
        let _ = topic_trimmed;
        // Sin pregunta heurística previa → primer turno no punitivo.
        let has_prior_question = panel
            .conversation
            .iter()
            .any(|turn| turn.role == ConversationRole::Assistant && turn.content.contains('?'));
        if !has_prior_question {
            return None;
        }
        Some(socratic_guard_context(
            profile.level,
            topic,
            question,
            &panel.conversation,
        ))
    }

    pub(crate) fn provider_settings(
        ctx: &mut AssistantJobsContext<'_>,
    ) -> Result<ProviderSettings, String> {
        Self::provider_settings_for(ctx, &ctx.panel.model.clone())
    }

    fn provider_settings_for(
        ctx: &mut AssistantJobsContext<'_>,
        model: &str,
    ) -> Result<ProviderSettings, String> {
        let model = model.trim();
        if model.is_empty() {
            return Err(
                "Completá la configuración avanzada antes de consultar remotamente.".into(),
            );
        }
        let mut settings = if ctx.panel.provider == ProviderProfile::CustomOpenAiCompatible {
            // Custom requiere clave propia (GRAFITO_ASSISTANT_CUSTOM_*_API_KEY) y no reutiliza OpenCodeGo.
            // Si no hay clave Custom configurada, retorna Err en lugar de fallback silencioso.
            ProviderSettings::custom_openai_compatible(
                "https://opencode.ai/zen/go/v1",
                model,
                "GRAFITO_ASSISTANT_CUSTOM_OPENCODE_API_KEY",
            )
            .or_else(|_| {
                ProviderSettings::custom_openai_compatible(
                    "https://opencode.ai/zen/go/v1",
                    model,
                    "GRAFITO_ASSISTANT_CUSTOM_API_KEY",
                )
            })
            .map_err(|_| {
                // Español directo (sin inglés crudo del validador): la clave
                // Custom vive en GRAFITO_ASSISTANT_CUSTOM_*_API_KEY y nunca
                // reutiliza la de OpenCodeGo.
                "El proveedor Custom requiere su propia API key (GRAFITO_ASSISTANT_CUSTOM_*_API_KEY). Revisá la configuración avanzada."
                    .to_string()
            })?
        } else {
            ProviderSettings::for_profile(ctx.panel.provider, model)
        };
        if ctx.panel.vision_enabled {
            let capabilities = ProviderCapabilities {
                vision: true,
                ..settings.capabilities
            };
            settings = settings.with_capabilities(capabilities);
        }
        // Sesión Go (docs Go 2026-09-08): sólo para el gateway Go
        // (`opencode.ai/zen/go`; cubre OpenCodeGo y el Custom que apunta ahí).
        // DeepSeek/Ollama/Custom no-Go traen `None` (sin header). Lazy: se crea
        // en el primer request Go y queda estable hasta Limpiar.
        let is_go = settings.profile == ProviderProfile::OpenCodeGo
            || settings.endpoint.contains("opencode.ai");
        if is_go {
            let session = ctx.runtime.ensure_go_session();
            settings = settings.with_go_session_id(Some(session))?;
        }
        Ok(settings)
    }

    /// Indica si el proveedor remoto configurado puede responder hoy (movido verbatim).
    pub(crate) fn remote_ready(ctx: &mut AssistantJobsContext<'_>) -> bool {
        let Ok(settings) = Self::provider_settings(ctx) else {
            return false;
        };
        match settings.profile {
            ProviderProfile::OllamaLocal => true,
            _ => Self::api_key(ctx).is_ok(),
        }
    }

    pub(crate) fn api_key(ctx: &mut AssistantJobsContext<'_>) -> Result<Option<String>, String> {
        if ctx.panel.provider == ProviderProfile::OllamaLocal {
            return Ok(None);
        }
        if let Some(key) = ctx.runtime.key_for(ctx.panel.provider) {
            // Diagnóstico sin secretos: fuente + largo, jamás el contenido.
            // Sirve para distinguir "clave no guardada" de "proveedor la rechaza".
            eprintln!(
                "grafito: clave {:?} desde sesión en memoria, largo {}",
                ctx.panel.provider,
                key.len()
            );
            return Ok(Some(key));
        }
        // Custom requiere clave propia (assistant-custom); no reutiliza OpenCodeGo.
        // Si no hay clave Custom, se retorna Err abajo vía load(Custom) => None.
        match assistant_credentials::load(ctx.panel.provider) {
            Ok(Some(key)) => {
                ctx.panel.key_available = true;
                ctx.panel.key_status_checked = true;
                eprintln!(
                    "grafito: clave {:?} desde llavero/sistema, largo {}",
                    ctx.panel.provider,
                    key.len()
                );
                ctx.runtime.remember_key(ctx.panel.provider, key.clone());
                Ok(Some(key))
            }
            Ok(None) => {
                ctx.panel.key_available = false;
                ctx.panel.key_status_checked = true;
                Err(
                    "Guardá una clave de API en la configuración avanzada antes de consultar."
                        .into(),
                )
            }
            Err(_) => {
                ctx.panel.key_available = false;
                ctx.panel.key_status_checked = true;
                Err("El llavero del sistema no está disponible para leer la API key.".into())
            }
        }
    }

    /// Construye el request remoto con presupuestos (movido verbatim de
    /// `build_remote_assistant_request`; `plugin_instructions` lo precomputa
    /// el llamante desde el registry).
    pub(crate) fn build_remote_request(
        panel: &AssistantPanelState,
        profile: &StudentProfile,
        plugin_instructions: String,
        params: BuildRemoteParams,
    ) -> Result<AssistantRequest, String> {
        let BuildRemoteParams {
            question,
            document_context,
            focus,
            attachments,
            image_upload_consent,
            repair,
        } = params;
        let (repair_feedback, history_before_turn) = match repair {
            Some(AssistantRepairRequest {
                feedback,
                target_turn,
            }) => (Some(feedback), Some(target_turn)),
            None => (None, None),
        };
        let mut request = AssistantRequest::remote(question.clone(), document_context);
        // Idioma del selector del panel (auto/es/en) → directiva en el system prompt.
        request.language = panel.avatar.language.clone();
        request.focus = focus;
        let _plugin_instruction_bytes = if plugin_instructions.is_empty() {
            0
        } else {
            plugin_instructions
                .len()
                .saturating_add(REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES)
        };
        // Memoria del tutor: el perfil del estudiante entra en el contexto de
        // cada turno para que Mora adapte la pedagogía (ADR-0001).
        let mut system = plugin_instructions;
        if !system.is_empty() {
            system.push_str("\n\n");
        }
        system.push_str(&format!("[Perfil del estudiante]\n{}", profile.memory()));
        request.system_instructions = system;
        let focus_bytes = request
            .focus
            .as_ref()
            .map(|focus| focus.summary.len())
            .unwrap_or_default();
        let context_bytes =
            if request.context.objects.is_empty() && request.context.variables.is_empty() {
                0
            } else {
                let mut len = REMOTE_CONTEXT_PROMPT_OVERHEAD_BYTES;
                len += 30; // "Objetos visibles:\n" etc.
                for (k, v) in &request.context.variables {
                    len += k.len() + format!("{v}").len() + 4;
                }
                for obj in &request.context.objects {
                    len += obj.label.len()
                        + obj.kind.len()
                        + obj.fingerprint.chars().take(120).collect::<String>().len()
                        + 10;
                }
                len += 120; // instrucción Taylor
                len
            };
        let repair_feedback_bytes = repair_feedback
            .as_ref()
            .map(|feedback| {
                feedback
                    .prompt_text()
                    .len()
                    .saturating_add(REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES)
            })
            .unwrap_or_default();
        // Con 1M de presupuesto, el catálogo puede ser grande pero lo acotamos a 32k para no saturar
        let system_bytes = request.system_instructions.len()
            + if request.system_instructions.is_empty() {
                0
            } else {
                REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES
            };
        let transcription_bytes = request.transcription.text.len()
            + request
                .attachments
                .iter()
                .map(|a| a.transcription.text.len())
                .sum::<usize>();
        // Presupuesto equitativo: garantiza catálogo útil incluso con historia larga
        let raw_catalog = request
            .budget
            .max_input_chars
            .saturating_sub(question.len())
            .saturating_sub(focus_bytes)
            .saturating_sub(
                request
                    .focus
                    .as_ref()
                    .map(|_| REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                    .unwrap_or_default(),
            )
            .saturating_sub(context_bytes)
            .saturating_sub(REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES)
            .saturating_sub(repair_feedback_bytes)
            .saturating_sub(system_bytes)
            .saturating_sub(transcription_bytes);
        // Garantiza mínimo 1K para herramientas relevantes (evita catálogo vacío que deja al LLM ciego)
        let catalog_budget = raw_catalog.clamp(1024, 32_000);
        request.tool_catalog =
            grafito_command::assistant_context::assistant_tool_catalog(&question, catalog_budget);
        let catalog_overhead = if request.tool_catalog.is_empty() {
            0
        } else {
            REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES
        };
        let history_budget = request
            .budget
            .max_input_chars
            .saturating_sub(question.len())
            .saturating_sub(focus_bytes)
            .saturating_sub(
                request
                    .focus
                    .as_ref()
                    .map(|_| REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES)
                    .unwrap_or_default(),
            )
            .saturating_sub(context_bytes)
            .saturating_sub(request.tool_catalog.len())
            .saturating_sub(catalog_overhead)
            .saturating_sub(repair_feedback_bytes)
            .saturating_sub(system_bytes)
            .saturating_sub(transcription_bytes);
        request.conversation = match history_before_turn {
            Some(target_turn) => {
                panel.conversation_before_turn_within_budget(target_turn, history_budget)
            }
            None => panel.conversation_within_budget(history_budget),
        };
        request.attachments = attachments;
        request.image_upload_consent = image_upload_consent;
        request.repair_feedback = repair_feedback;
        request.validate(&AttachmentLimits::default())?;
        Ok(request)
    }

    /// Lanza el job remoto con streaming (movido verbatim de
    /// `start_remote_assistant_job`; el lockdown de examen lo aplica el shim).
    pub(crate) fn start_remote(
        ctx: &mut AssistantJobsContext<'_>,
        egui_ctx: &egui::Context,
        launch: AssistantRemoteLaunch,
    ) {
        // B2-LOW: sin doble-spawn — pisar el slot huérfana el job anterior
        // (su worker sigue vivo pero el poll ya no lo drena). El path normal
        // lo gatea `remote_request_slot_is_free`; esto es el cinturón.
        debug_assert!(
            ctx.runtime.remote_job.is_none(),
            "start_remote con slot remoto ocupado"
        );
        if ctx.runtime.remote_job.is_some() {
            return;
        }
        // D2 lockdown: en examen no sale nada a internet (chequeado en el shim).
        let AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            document_revision,
            document_digest,
            focus,
            correction_attempt,
            repair_target_turn,
            socratic_guard,
        } = launch;
        ctx.runtime.next_request_id = ctx.runtime.next_request_id.wrapping_add(1);
        let id = ctx.runtime.next_request_id;
        let cancellation = CancellationToken::default();
        // Canal acotado de deltas SSE (128, best-effort): el worker de
        // streaming lo alimenta y `poll_assistant_jobs` lo drena a la burbuja
        // provisional. Protocolos no-streaming lo dejan desconectado.
        let (delta_tx, stream_rx) = sync_channel::<String>(128);
        let worker = request_remote_streaming_with_api_key_on_worker(
            settings,
            request,
            api_key,
            cancellation.clone(),
            delta_tx,
            socratic_guard,
        );
        let (sender, receiver) = sync_channel(1);
        let repaint = egui_ctx.clone();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            // Si hay cancelación pendiente, da una ventana breve para que el worker
            // termine cooperativamente antes de bloquear el hilo forwarder (no UI).
            if cancellation_for_thread.is_cancelled() {
                for _ in 0..20 {
                    if worker.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let result = worker.join().unwrap_or_else(|_| {
                Err("La consulta del asistente terminó inesperadamente.".into())
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        ctx.runtime.remote_job = Some(AssistantRemoteJob {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            cancellation,
            receiver,
            stream_rx: Some(stream_rx),
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        });
        // Etapa inicial visible de inmediato (sin esperar al primer poll):
        // `Autorizada` con 0s, la Piel sólo renderiza el texto.
        ctx.panel
            .set_remote_stage(grafito_ui::assistant::RemoteStage::Autorizada, 0);
    }

    /// Lanza el modo agente, loop con herramientas seguras, en un hilo y
    /// enruta sus eventos de actividad + resultado hacia la UI
    /// (movido verbatim de `start_agent_assistant_job`).
    fn start_agent(
        ctx: &mut AssistantJobsContext<'_>,
        egui_ctx: &egui::Context,
        launch: AssistantRemoteLaunch,
    ) {
        // B2-LOW: sin doble-spawn (idem `start_remote`: el slot agente
        // huérfano deja un loop con tools sin drenar).
        debug_assert!(
            ctx.runtime.agent_job.is_none(),
            "start_agent con slot agente ocupado"
        );
        if ctx.runtime.agent_job.is_some() {
            return;
        }
        // D2 lockdown: en examen no sale nada a internet (chequeado en el shim).
        let settings = launch.settings;
        let request = launch.request;
        let api_key = launch.api_key;
        let provider = launch.provider;
        let model = launch.model;
        let question = launch.question;
        ctx.runtime.next_request_id = ctx.runtime.next_request_id.wrapping_add(1);
        let _ = ctx.runtime.next_request_id;
        let cancellation = grafito_agent::loop_engine::Cancellation::default();
        let system = grafito_assistant::assistant_system_prompt(&request);
        let prompt = grafito_assistant::assistant_remote_prompt(&request)
            .unwrap_or_else(|_| question.clone());
        let mut user_messages: Vec<serde_json::Value> = Vec::new();
        for turn in &request.conversation {
            let role = match turn.role {
                grafito_assistant_types::ConversationRole::User => "user",
                grafito_assistant_types::ConversationRole::Assistant => "assistant",
            };
            user_messages.push(serde_json::json!({"role": role, "content": turn.content}));
        }
        user_messages.push(serde_json::json!({"role": "user", "content": prompt}));
        let tools = grafito_assistant::default_agent_tools();
        let budget = grafito_agent::loop_engine::AgentBudget::default();
        let goal = question
            .chars()
            .take(grafito_agent::ledger::MAX_LEDGER_GOAL_CHARS)
            .collect::<String>();
        let ledger = if grafito_agent::router::classify_band(&question)
            == grafito_agent::router::TaskBand::LongRunning
        {
            Some(grafito_agent::ledger::JSpaceLedger::with_task(
                goal,
                "Analizar, verificar con tools y cerrar",
            ))
        } else {
            None
        };
        let (outcome_handle, event_receiver) =
            grafito_assistant::agent::request_agent_on_worker_with_ledger(
                settings,
                api_key,
                system,
                user_messages,
                tools,
                budget,
                ledger,
                cancellation.clone(),
            );
        let (sender, receiver) = std::sync::mpsc::sync_channel(128);
        // S2: canal lateral de aclaraciones (cap 4, try_send sin bloquear).
        let (clarification_sender, clarification_receiver) = std::sync::mpsc::sync_channel(4);
        let repaint = egui_ctx.clone();
        let cancellation_forwarder = cancellation.clone();
        std::thread::spawn(move || {
            // Drena eventos con timeout para respetar cancelación y evitar bloqueo
            // indefinido en `iter()` si el agente se cuelga.
            loop {
                if cancellation_forwarder.is_cancelled() {
                    break;
                }
                match event_receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(event) => {
                        // S2 `ask_user` real vía evento: reenvía el pendiente al
                        // canal lateral sin bloquear (try_send) para que la UI
                        // lo muestre como botones. Nunca bloquea threads.
                        if let grafito_agent::AgentEvent::ToolStarted { name, args_summary } =
                            &event
                        {
                            if name == "ask_user" {
                                if let Some(pending) = parse_agent_ask_user_pending(args_summary) {
                                    let _ = clarification_sender.try_send(pending);
                                }
                            }
                        }
                        // R2-V1: `try_send` + `CancellationToken` (jamás `send` bloqueante).
                        // Lleno → se descarta el evento (best-effort); desconectado/cancelado → corta.
                        match send_agent_msg_nonblocking(
                            &sender,
                            AgentChannelMsg::Event(event),
                            &cancellation_forwarder,
                        ) {
                            Some(_) => {}
                            None => break,
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // Si hay cancelación, espera brevemente a que el worker termine
            // cooperativamente antes de bloquear el hilo forwarder (no UI).
            if cancellation_forwarder.is_cancelled() {
                for _ in 0..20 {
                    if outcome_handle.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let outcome = outcome_handle
                .join()
                .unwrap_or_else(|_| Err("El agente terminó inesperadamente.".to_string()));
            // R2-V1: `Done` con `try_send` + reintento acotado (jamás `send` bloqueante).
            // Si el buffer sigue lleno tras 200 ms o hay cancelación/desconexión,
            // se descarta para que el `join` sea <1s (la UI ve `Disconnected` honesto).
            let mut pending = Some(AgentChannelMsg::Done(outcome));
            for _ in 0..20 {
                if cancellation_forwarder.is_cancelled() {
                    break;
                }
                let Some(msg) = pending.take() else {
                    break;
                };
                match sender.try_send(msg) {
                    Ok(()) => break,
                    Err(std::sync::mpsc::TrySendError::Full(returned)) => {
                        pending = Some(returned);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
            repaint.request_repaint();
        });
        ctx.runtime.agent_job = Some(AssistantAgentJob {
            provider,
            model,
            cancellation,
            receiver,
            clarification_receiver,
        });
    }

    /// Verificación local de propuestas remotas en worker (movida verbatim de
    /// `start_remote_proposal_verification`; el lockdown lo aplica el shim).
    fn start_remote_proposal(
        ctx: &mut AssistantJobsContext<'_>,
        egui_ctx: &egui::Context,
        launch: AssistantProposalLaunch,
    ) {
        // B2-LOW: sin doble-spawn (idem `start_remote`: la verificación
        // huérfana pierde su `repair_target_turn`).
        debug_assert!(
            ctx.runtime.proposal_job.is_none(),
            "start_remote_proposal con slot propuesta ocupado"
        );
        if ctx.runtime.proposal_job.is_some() {
            return;
        }
        // D2 lockdown: en examen no sale nada a internet (chequeado en el shim).
        let AssistantProposalLaunch {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            text,
        } = launch;
        let document = ctx.document.detached_clone_for_staging();
        let camera = ctx.camera;
        let response_text = text.clone();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = sync_channel(1);
        let repaint = egui_ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inspect_remote_proposals_cancellable(
                    &document,
                    &text,
                    camera,
                    &worker_cancellation,
                    correction_attempt > 0,
                )
            }))
            .unwrap_or_else(|_| {
                Err("La comprobación local de la propuesta falló inesperadamente.".into())
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        ctx.runtime.proposal_job = Some(AssistantProposalJob {
            id,
            provider,
            model,
            route,
            fusion_fallback_allowed,
            question,
            correction_attempt,
            repair_target_turn,
            document_revision,
            document_digest,
            focus,
            text: response_text,
            cancellation,
            receiver,
        });
    }

    /// Arranca la consulta remota tras consentimiento explícito del cartel
    /// (movida verbatim de `start_authorized_remote_assistant_request`).
    pub(crate) fn start_authorized(ctx: &mut AssistantJobsContext<'_>, egui_ctx: &egui::Context) {
        if ctx.panel.is_pending || !ctx.runtime.remote_request_slot_is_free() {
            return;
        }
        let Some(question) = ctx
            .panel
            .pending_remote_authorization_question()
            .map(str::to_owned)
        else {
            return;
        };
        ctx.panel.begin_authorized_remote_request();
        Self::start_remote_for(ctx, egui_ctx, question, None);
    }

    /// Lanza la consulta remota con la pregunta dada, sin depender del cartel
    /// (movida verbatim de `start_remote_assistant_for`).
    ///
    /// Con permiso completo, el consentimiento de imágenes se otorga automático;
    /// la capacidad de visión del modelo sigue siendo un requisito real.
    /// `model_override` sólo-sesión: no toca la preferencia guardada.
    pub(crate) fn start_remote_for(
        ctx: &mut AssistantJobsContext<'_>,
        egui_ctx: &egui::Context,
        question: String,
        model_override: Option<&str>,
    ) {
        // Freno 429: en pausa se avisa con cuenta regresiva sin tocar la red.
        // Cubre chat simple, modo agente y el fallback sólo-sesión (que
        // también pasa por acá): ningún reintento automático quema cuota.
        if Self::fail_fast_if_rate_limited(ctx) {
            return;
        }
        if !ctx.runtime.remote_request_slot_is_free() {
            return;
        }
        if !ctx.panel.attachments.is_empty() {
            if !ctx.panel.vision_enabled {
                Self::emit_error(
                    ctx,
                    "Confirmá que la configuración remota admite imágenes antes de enviarlas.",
                );
                return;
            }
            if ctx.panel.full_permission {
                ctx.panel.image_upload_consent = true;
            }
            if !ctx.panel.image_upload_consent {
                Self::emit_error(
                    ctx,
                    "Autorizá el envío de las imágenes antes de realizar la consulta.",
                );
                return;
            }
        }
        let effective_model = model_override.unwrap_or(&ctx.panel.model).trim().to_owned();
        if effective_model.is_empty() {
            Self::emit_error(
                ctx,
                "Completá la configuración avanzada antes de consultar remotamente.",
            );
            return;
        }
        let settings = match Self::provider_settings_for(ctx, &effective_model) {
            Ok(settings) => settings,
            Err(error) => {
                Self::emit_error(ctx, error);
                return;
            }
        };
        let api_key = match Self::api_key(ctx) {
            Ok(key) => key,
            Err(error) => {
                Self::emit_error(ctx, error);
                return;
            }
        };
        let document_context = grafito_command::assistant_context::document_context(ctx.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            ctx.document,
            ctx.selected_object,
        );
        let document_revision = document_context.revision;
        let document_digest = document_context.digest.clone();
        let request = match Self::build_remote_request(
            ctx.panel,
            ctx.profile,
            Self::plugin_instructions(ctx.plugin_registry),
            BuildRemoteParams {
                question: question.clone(),
                document_context,
                focus: focus.clone(),
                attachments: ctx.panel.attachments.clone(),
                image_upload_consent: ctx.panel.image_upload_consent,
                repair: None,
            },
        ) {
            Ok(request) => request,
            Err(error) => {
                // `build_remote_request` valida límites (inglés de
                // crates externos) — se envuelve en español antes de mostrar.
                Self::emit_error(ctx, remote_error_message(&error, &effective_model));
                return;
            }
        };

        // Fallback sólo-sesión: se recuerda para aceptar el resultado en vuelo
        // sin tocar la preferencia guardada del usuario.
        if model_override.is_some() {
            ctx.runtime.fallback_model = Some(effective_model.clone());
        } else {
            ctx.runtime.fallback_model = None;
        }
        let launch = AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider: ctx.panel.provider,
            model: effective_model,
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: ctx.panel.allow_fusion_fallback,
            question: question.clone(),
            document_revision,
            document_digest,
            focus,
            correction_attempt: 0,
            repair_target_turn: None,
            socratic_guard: Self::session_guard(ctx.panel, ctx.profile, &question),
        };
        if ctx.panel.agent_mode {
            Self::start_agent(ctx, egui_ctx, launch);
        } else {
            Self::start_remote(ctx, egui_ctx, launch);
        }
    }

    /// Aplica el plan propuesto (movida verbatim de
    /// `apply_proposed_assistant_plan` salvo los efectos de `app.rs`, que
    /// viajan en [`ApplyProposedEffect`] para el shim).
    pub(crate) fn apply_proposed(ctx: &mut AssistantJobsContext<'_>) -> ApplyProposedEffect {
        let Some(plan) = ctx.panel.proposed_plan().cloned() else {
            return ApplyProposedEffect::none();
        };
        let before_labels = Self::labels_snapshot(ctx.document);
        match apply_local_assistant_plan(ctx.document, &plan, ctx.undo_stack, ctx.redo_stack) {
            Ok(result) => {
                // Decide perspectiva según el contenido del plan: CreateGraph es 2D,
                // otras operaciones matemáticas también deben quedar visibles en Álgebra.
                let has_graph = plan.operations.iter().any(|operation| operation.is_graph());
                let wants_perspective = if has_graph {
                    assistant_graph_perspective(
                        grafito_command::assistant_context::AssistantGraphView::TwoD,
                        ctx.current_view,
                    )
                } else {
                    None
                };
                let summary = format!("Propuesta local aplicada: {}", result.changes.join(", "));
                ctx.panel.finish_proposed_plan_application(true);
                (ctx.notify)(summary, ToastKind::Success);
                ApplyProposedEffect {
                    applied: true,
                    wants_perspective,
                    record_action: Some(("Propuesta local aplicada".to_string(), before_labels)),
                }
            }
            Err(error) => {
                ctx.panel.clear_proposed_plan();
                Self::emit_error(
                    ctx,
                    format!("La propuesta local cambió o dejó de ser válida: {error}"),
                );
                ApplyProposedEffect::none()
            }
        }
    }

    /// Lista de modelos con freno 429 (movida verbatim de `start_model_request`).
    pub(crate) fn start_model(ctx: &mut AssistantJobsContext<'_>, egui_ctx: &egui::Context) {
        // B2-LOW: sin doble-spawn — la lista huérfana deja un GET sin drenar
        // (el path normal lo gatea `request_model_refresh`, que además encola;
        // esto es el cinturón para llamadas directas).
        debug_assert!(
            ctx.runtime.model_job.is_none(),
            "start_model con slot modelos ocupado"
        );
        if ctx.runtime.model_job.is_some() {
            return;
        }
        // La lista de modelos también respeta la pausa (es un GET al mismo
        // proveedor). Con cache fresco el worker ni sale a la red.
        if Self::fail_fast_if_rate_limited(ctx) {
            return;
        }
        if !ctx.runtime.request_model_refresh() {
            return;
        }
        let settings = match Self::provider_settings(ctx) {
            Ok(settings) => settings,
            Err(error) => {
                Self::emit_error(ctx, error);
                return;
            }
        };
        let api_key = match Self::api_key(ctx) {
            Ok(key) => key,
            Err(error) => {
                Self::emit_error(ctx, error);
                return;
            }
        };
        ctx.runtime.next_request_id = ctx.runtime.next_request_id.wrapping_add(1);
        let id = ctx.runtime.next_request_id;
        let provider = ctx.panel.provider;
        let cancellation = CancellationToken::default();
        let worker =
            request_remote_models_with_api_key_on_worker(settings, api_key, cancellation.clone());
        let (sender, receiver) = sync_channel(1);
        let repaint = egui_ctx.clone();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            if cancellation_for_thread.is_cancelled() {
                for _ in 0..20 {
                    if worker.is_finished() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let result = worker
                .join()
                .unwrap_or_else(|_| Err("La lista de modelos terminó inesperadamente.".into()));
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        ctx.runtime.model_job = Some(AssistantModelJob {
            id,
            provider,
            cancellation,
            receiver,
        });
    }

    /// Señala cancelación del slot remoto (granular; el slot se conserva hasta
    /// el drain, igual que `cancel_anim_job`).
    fn cancel_remote_slot(ctx: &mut AssistantJobsContext<'_>) -> bool {
        if let Some(job) = ctx.runtime.remote_job.as_ref() {
            job.cancellation.cancel();
            true
        } else {
            false
        }
    }

    /// Señala cancelación del slot de propuesta (granular, idem).
    fn cancel_proposal(ctx: &mut AssistantJobsContext<'_>) -> bool {
        if let Some(job) = ctx.runtime.proposal_job.as_ref() {
            job.cancellation.cancel();
            true
        } else {
            false
        }
    }

    /// Señala cancelación del slot agente (granular, idem).
    fn cancel_agent(ctx: &mut AssistantJobsContext<'_>) -> bool {
        if let Some(job) = ctx.runtime.agent_job.as_ref() {
            job.cancellation.cancel();
            true
        } else {
            false
        }
    }

    /// Señala cancelación del slot de modelos (granular, idem).
    fn cancel_model(ctx: &mut AssistantJobsContext<'_>) -> bool {
        if let Some(job) = ctx.runtime.model_job.as_ref() {
            job.cancellation.cancel();
            true
        } else {
            false
        }
    }

    /// Cancela el turno en vuelo + resetea la card de export si el cancel
    /// soltó su hilo (movida verbatim de `cancela_turno_anim`: misma
    /// disyunción que `cancel_anim_job` —cuatro slots + media— más el reset
    /// `Exporting → Idle` solo si HABÍA export y ningún slot sigue en vuelo).
    ///
    /// B2-MED: el reset mira TODOS los formatos vía
    /// `any_export_in_flight` (clásico + LaTeX pdf/svg), no solo el GIF: con
    /// solo `gif.is_none()` un png/mp4/webm/pdf/svg en vuelo dejaba la card
    /// colgada en `Exporting` (el reaper entierra el archivo y el poll ya no
    /// drena nada).
    ///
    /// B2-MED: limpia `fallback_model` (solo-sesión): el turno cancelado no
    /// deja un reintento pendiente; el próximo pedido reintenta el modelo
    /// elegido por el usuario (auto-recupera si el proveedor vuelve). El
    /// drain del slot cancelado ya publica `fail_request` sin importar el
    /// modelo esperado, así que limpiarlo acá no cambia el drain.
    pub(crate) fn cancel_turno_anim(ctx: &mut AssistantJobsContext<'_>) -> bool {
        let habia_export = ctx.runtime.any_export_in_flight();
        let remote = Self::cancel_remote_slot(ctx);
        let proposal = Self::cancel_proposal(ctx);
        let agent = Self::cancel_agent(ctx);
        let model = Self::cancel_model(ctx);
        let media = ctx.runtime.media.cancel_media_jobs();
        // LaTeX (pdf/svg) vive en `assistant.rs` fuera del controller: se
        // señala acá vía el puente para no dejarlo huérfano (el poll de
        // `assistant.rs` drena honesto con su slot conservado). Hunk exacto
        // para el dueño de `assistant.rs` en el reporte B2.
        let latex = ctx.runtime.signal_latex_exports_cancel();
        let hubo = remote || proposal || agent || model || media || latex;
        ctx.runtime.fallback_model = None;
        if habia_export && !ctx.runtime.any_export_in_flight() {
            ctx.panel.set_media_export(MediaExportState::Idle);
        }
        hubo
    }

    /// Cancel total del botón Cancel (movida verbatim de `cancel_assistant_request`).
    ///
    /// B2-MED: `cancel_turno_anim` ya limpia `fallback_model` (el turno
    /// cancelado no deja reintento pendiente); este path lo hereda.
    pub(crate) fn cancel_remote(ctx: &mut AssistantJobsContext<'_>) {
        // Cancela remote+proposal+agent+model+anim+export (todos, sin else-if:
        // aunque el slot remoto sólo permite un job de consulta a la vez,
        // model/anim/export son independientes y deben cancelarse también).
        // Los workers con token se drenan en poll (take_finished_*); anim
        // señala su token y dropea el slot (worker acotado con chequeo entre
        // frames); el export suelto lo entierra el reaper y la card vuelve
        // a `Idle` (ver `cancel_turno_anim`).
        let had_anim = ctx.runtime.anim_job.is_some();
        if Self::cancel_turno_anim(ctx) {
            if had_anim {
                ctx.panel.anim_progress = false;
            }
            Self::begin_cancelling(ctx);
        }
    }

    /// Descarta jobs rancios ante cambio de proveedor/modelo (movida verbatim
    /// de `cancel_stale_remote_request`).
    pub(crate) fn cancel_stale_remote(ctx: &mut AssistantJobsContext<'_>) {
        let expected = ctx.runtime.expected_model(&ctx.panel.model).to_owned();
        if ctx
            .runtime
            .cancel_stale_remote_job(ctx.panel.provider, &expected)
            || ctx
                .runtime
                .cancel_stale_agent_job(ctx.panel.provider, &expected)
        {
            Self::begin_cancelling(ctx);
        }
    }

    /// Descarta el job de modelos rancio (movida verbatim de
    /// `cancel_stale_model_request`).
    pub(crate) fn cancel_stale_model(ctx: &mut AssistantJobsContext<'_>) {
        if ctx.runtime.cancel_stale_model_job(ctx.panel.provider) {}
    }

    /// Drena la actividad del modo agente y cierra el turno al terminar
    /// (movida verbatim de `poll_assistant_agent`).
    fn poll_agent(ctx: &mut AssistantJobsContext<'_>, egui_ctx: &egui::Context) -> bool {
        let Some(job) = ctx.runtime.agent_job.as_ref() else {
            return false;
        };
        loop {
            match job.receiver.try_recv() {
                Ok(AgentChannelMsg::Event(event)) => match event {
                    grafito_agent::AgentEvent::ToolStarted { name, .. } => {
                        ctx.panel.push_agent_activity(format!("usando {name}…"));
                    }
                    grafito_agent::AgentEvent::ToolFinished { name, ok } => {
                        let marker = if ok { "✓" } else { "✗" };
                        ctx.panel.push_agent_activity(format!("{marker} {name}"));
                    }
                    grafito_agent::AgentEvent::Ledger { render } => {
                        ctx.panel.set_agent_ledger(Some(render));
                    }
                    grafito_agent::AgentEvent::Finalized { .. } => {}
                },
                Ok(AgentChannelMsg::Done(result)) => {
                    if let Some(job) = ctx.runtime.agent_job.take() {
                        let cancelled = job.cancellation.is_cancelled();
                        if cancelled {
                            Self::fail_request(
                                ctx,
                                "La consulta agente se canceló antes de obtener una respuesta.",
                            );
                        } else {
                            match result {
                                Ok(outcome) => {
                                    // Wiring B1 (loop Spark por Responses en paralelo):
                                    // un `Done(Ok)` con Spark se acepta directo y NUNCA
                                    // dispara el fallback a deepseek. El fallback sólo
                                    // vive en la rama `Err` vía
                                    // `should_fallback_agent_spark_to_deepseek`.
                                    ctx.panel.complete_request(outcome.final_text);
                                }
                                Err(error) => {
                                    // Modo agente + Spark: las tools aún no viajan por Responses API.
                                    // Fallback sólo-sesión a deepseek (chat-compatible), preferencia intacta.
                                    // Si B1 ya cerró el loop por Responses, este error deja
                                    // de ocurrir y el `Ok` de arriba gana sin fallback.
                                    if should_fallback_agent_spark_to_deepseek(
                                        &error,
                                        job.provider,
                                        &job.model,
                                    ) {
                                        eprintln!("grafito: session-fallback agent spark -> deepseek-v4-flash (preferencia intacta)");
                                        (ctx.notify)(
                                            "Modo agente con Spark aún no soporta herramientas; reintentando con DeepSeek Flash…".to_string(),
                                            ToastKind::Info,
                                        );
                                        let question = ctx.panel.problem.trim().to_owned();
                                        Self::start_remote_for(
                                            ctx,
                                            egui_ctx,
                                            question,
                                            Some("deepseek-v4-flash"),
                                        );
                                    } else {
                                        Self::fail_request(ctx, error);
                                    }
                                }
                            }
                        }
                    }
                    egui_ctx.request_repaint();
                    return true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some(_job) = ctx.runtime.agent_job.take() {
                        Self::fail_request(
                            ctx,
                            "El agente terminó inesperadamente antes de responder.",
                        );
                    }
                    egui_ctx.request_repaint();
                    return true;
                }
            }
        }
        false
    }

    /// Dispatcher de jobs del asistente (movido verbatim de `poll_assistant_jobs`).
    pub(crate) fn poll_assistant_jobs(
        ctx: &mut AssistantJobsContext<'_>,
        egui_ctx: &egui::Context,
    ) {
        Self::poll_agent(ctx, egui_ctx);
        // Drena deltas SSE a la burbuja provisional antes de cosechar el
        // resultado final (el preview se limpia en cada rama terminal).
        ctx.runtime.drain_remote_stream_preview(ctx.panel, egui_ctx);
        if let Some(completion) = ctx.runtime.take_finished_remote_job() {
            let FinishedRemoteJob {
                id,
                provider,
                model,
                route,
                fusion_fallback_allowed,
                question,
                correction_attempt,
                repair_target_turn,
                document_revision,
                document_digest,
                focus,
                cancelled,
                result,
                stream_preview_active,
                ..
            } = completion;
            if stream_preview_active {
                pop_provisional_stream_turn(ctx.panel);
            }
            if cancelled
                || !accepts_remote_result(
                    ctx.panel.provider,
                    ctx.runtime.expected_model(&ctx.panel.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    Self::fail_repair_request(
                        ctx,
                        "La corrección se canceló antes de recibir una respuesta.",
                    );
                } else {
                    Self::fail_request(
                        ctx,
                        "La consulta se canceló antes de recibir una respuesta.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(ctx.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    ctx.document,
                    ctx.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    Self::fail_request(
                        ctx,
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    ctx.panel.invalidate_proposal_correction();
                } else {
                    match result {
                        Ok(completion) => {
                            if completion.truncated {
                                (ctx.notify)(
                                    "La respuesta alcanzó el límite de la consulta; pedí que continúe desde el último punto.".to_string(),
                                    ToastKind::Info,
                                );
                            }
                            let text = completion.text;
                            // Guard socrático post-respuesta: un pedido
                            // exploratorio ("mostrame un ejemplo…") cuya
                            // respuesta trae matemática ($..$, `=` numérico)
                            // es telling igual y exige repair ANTES de
                            // publicar (cierra el bypass demo, ver
                            // `SocraticFsm::requires_repair_despite_exploratory`).
                            // NO toca el guard pre-respuesta del worker.
                            if SocraticFsm::requires_repair_despite_exploratory(
                                &question,
                                SocraticFsm::response_brings_math(&text),
                            ) {
                                if correction_attempt > 0 {
                                    ctx.panel.restore_proposal_correction();
                                }
                                eprintln!(
                                    "grafito: socratic repair post-respuesta (no al transcript)"
                                );
                                let student = Self::session_guard(ctx.panel, ctx.profile, &question)
                                    .map(|guard| {
                                        guard.fsm.repair_student_message(&guard.scaffold)
                                    })
                                    .unwrap_or_else(|| {
                                        "Antes de mostrarte la solución, ¿qué forma te imaginás? Contame qué probaste y lo vemos juntos.".to_owned()
                                    });
                                ctx.panel.complete_request(student);
                                (ctx.notify)(
                                    "El tutor repregunta antes de mostrar la solución directa."
                                        .to_string(),
                                    ToastKind::Info,
                                );
                            } else {
                                Self::start_remote_proposal(
                                    ctx,
                                    egui_ctx,
                                    AssistantProposalLaunch {
                                        id,
                                        provider,
                                        model,
                                        route,
                                        fusion_fallback_allowed,
                                        question,
                                        correction_attempt,
                                        repair_target_turn,
                                        document_revision,
                                        document_digest,
                                        focus,
                                        text,
                                    },
                                );
                            }
                        }
                        Err(error) => {
                            // Reparación socrática: el guard detectó telling
                            // con attempts<2 en el worker (streaming o no).
                            // El `error` es diagnóstico interno con jerga
                            // (`GUARD TELLING...`, solo para detección y logs):
                            // JAMÁS se publica crudo vía `complete_request`
                            // (ese fue el bug P0). Se convierte a voz de Mili
                            // vía `repair_student_message` (sin `GUARD`/
                            // `attempts`/`estado`/`Re-preguntá`). No hay
                            // fallback de modelo ni cartel de error aquí.
                            if is_socratic_repair_error(&error) {
                                if correction_attempt > 0 {
                                    ctx.panel.restore_proposal_correction();
                                }
                                // Jerga solo en logs/eventos internos.
                                eprintln!(
                                    "grafito: socratic repair interno (no al transcript): {error}"
                                );
                                // Convierte a voz de Mili; si no hay guard
                                // (no debería pasar si hubo repair), fallback
                                // humano sin jerga ni eco crudo.
                                let student = Self::session_guard(ctx.panel, ctx.profile, &question)
                                    .map(|guard| {
                                        guard.fsm.repair_student_message(&guard.scaffold)
                                    })
                                    .unwrap_or_else(|| {
                                        "Antes de mostrarte la solución, ¿qué forma te imaginás? Contame qué probaste y lo vemos juntos.".to_owned()
                                    });
                                ctx.panel.complete_request(student);
                                (ctx.notify)(
                                    "El tutor repregunta antes de mostrar la solución directa."
                                        .to_string(),
                                    ToastKind::Info,
                                );
                            } else if should_fallback_remote_spark_to_deepseek(
                                // OJO bucle: acá va el modelo INTENTADO (`model`
                                // del job), no la preferencia. Con la preferencia
                                // (siempre spark), el fallo del reintento en
                                // deepseek re-disparaba el fallback al infinito.
                                &error,
                                provider,
                                &model,
                                correction_attempt,
                            ) {
                                if is_session_or_account_error(&error) {
                                    eprintln!("grafito: session-fallback muse-spark 400-sesion [{error}] -> deepseek-v4-flash + retry (preferencia intacta)");
                                    (ctx.notify)(
                                        "Muse Spark rechazó la sesión Go (el header viaja solo; reintentá en un rato y si persiste verificá tu región o re-conectá tu clave Go). Sigo con DeepSeek Flash sin cambiar tu modelo.".to_string(),
                                        ToastKind::Info,
                                    );
                                } else {
                                    eprintln!("grafito: session-fallback muse-spark [{error}] -> deepseek-v4-flash + retry (preferencia intacta)");
                                    (ctx.notify)(
                                        "Muse Spark no respondió, reintentando con DeepSeek Flash; tu modelo sigue siendo Muse Spark.".to_string(),
                                        ToastKind::Info,
                                    );
                                }
                                // Reintentar la misma pregunta con el fallback, sin mostrar error
                                Self::start_remote_for(
                                    ctx,
                                    egui_ctx,
                                    question.clone(),
                                    Some("deepseek-v4-flash"),
                                );
                                return;
                            } else if correction_attempt > 0 {
                                Self::fail_repair_request(ctx, error);
                            } else {
                                Self::fail_request(ctx, error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = ctx.runtime.take_finished_proposal_job() {
            let FinishedProposalJob {
                id: _request_id,
                provider,
                model,
                route: _route,
                fusion_fallback_allowed: _fusion_fallback_allowed,
                question,
                correction_attempt,
                repair_target_turn,
                document_revision,
                document_digest,
                focus,
                text,
                cancelled,
                result,
                ..
            } = completion;
            if cancelled
                || !accepts_remote_result(
                    ctx.panel.provider,
                    ctx.runtime.expected_model(&ctx.panel.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    Self::fail_repair_request(
                        ctx,
                        "La corrección se canceló antes de terminar la comprobación local.",
                    );
                } else {
                    Self::fail_request(
                        ctx,
                        "La consulta se canceló antes de terminar la comprobación local.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(ctx.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    ctx.document,
                    ctx.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    Self::fail_request(
                        ctx,
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    ctx.panel.invalidate_proposal_correction();
                } else {
                    match result {
                        Ok(proposal_check) => {
                            let repair_feedback = proposal_check.repair_feedback.clone();
                            let rejected_count = proposal_check
                                .candidate_count
                                .saturating_sub(proposal_check.verified.len());
                            let can_offer_correction = can_offer_assistant_proposal_correction(
                                correction_attempt,
                                proposal_check.action_candidate_count,
                                proposal_check.verified_action_count,
                                repair_feedback.as_ref(),
                            );
                            ctx.panel.set_proposal_preflight_results(
                                proposal_check.verified,
                                proposal_check.candidate_count,
                                proposal_check.candidate_code_block_indices,
                            );
                            if correction_attempt > 0 {
                                let Some(target_turn) = repair_target_turn else {
                                    Self::fail_request(
                                        ctx,
                                        "La corrección perdió el turno que debía reemplazar.",
                                    );
                                    return;
                                };
                                if !ctx
                                    .panel
                                    .complete_proposal_correction_at(target_turn, text.clone())
                                {
                                    Self::fail_request(
                                        ctx,
                                        "La corrección no pudo reemplazar su respuesta original.",
                                    );
                                    return;
                                }
                            } else {
                                ctx.panel.complete_request(text.clone());
                            }
                            if can_offer_correction {
                                if let Some(feedback) = repair_feedback {
                                    let target_turn = repair_target_turn
                                        .or_else(|| ctx.panel.conversation.len().checked_sub(1));
                                    ctx.panel.offer_proposal_correction_for_turn(
                                        question.clone(),
                                        feedback,
                                        target_turn,
                                        correction_attempt,
                                        AssistantCorrectionContext {
                                            document_revision,
                                            document_digest: document_digest.clone(),
                                            focus: focus.clone(),
                                        },
                                    );
                                }
                            }
                            if rejected_count > 0 {
                                let error = if proposal_check.verified_action_count == 0 {
                                    format!(
                                        "No se obtuvo una propuesta verificable; se descartaron {rejected_count} propuesta(s) localmente."
                                    )
                                } else {
                                    format!(
                                        "Se descartaron {rejected_count} propuesta(s) que no superaron la comprobación local."
                                    )
                                };
                                Self::emit_error(ctx, error);
                            } else if correction_attempt > 0
                                && proposal_check.verified_action_count == 0
                            {
                                Self::emit_error(
                                    ctx,
                                    "No se obtuvo una propuesta verificable; no hay nada para aplicar.",
                                );
                            }
                        }
                        Err(error) => {
                            if correction_attempt > 0 {
                                Self::fail_repair_request(ctx, error);
                            } else {
                                Self::fail_request(ctx, error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = ctx.runtime.take_finished_model_job() {
            let _request_id = completion.id;
            let FinishedModelJob {
                provider,
                cancelled,
                result,
                ..
            } = completion;
            if accepts_model_result(ctx.panel.provider, provider, cancelled) {
                match result {
                    Ok(models) => {
                        ctx.panel.set_available_models(models);
                    }
                    Err(error) => {
                        // La lista de modelos viene del transporte (inglés
                        // crudo posible) — se envuelve en español.
                        let current_model = ctx.panel.model.clone();
                        Self::emit_error(ctx, remote_error_message(&error, &current_model));
                    }
                }
            }
        }
        if ctx.runtime.take_queued_model_refresh_if_idle() {
            Self::start_model(ctx, egui_ctx);
        }

        let image = ctx
            .runtime
            .image_job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(
                    "La importación de imagen terminó inesperadamente.".into(),
                )),
            });
        if let Some(result) = image {
            ctx.runtime.image_job = None;
            ctx.panel.is_importing_image = false;
            match result {
                Ok(attachment) => match ctx.panel.add_attachment(attachment) {
                    Ok(()) => {
                        ctx.panel.attachment_message = Some("Imagen lista para consultar.".into())
                    }
                    Err(error) => Self::emit_error(ctx, attachment_error_message(&error)),
                },
                Err(error) => {
                    ctx.panel.attachment_message = None;
                    Self::emit_error(ctx, attachment_error_message(&error));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant_media::AssistantAnimJob;
    use grafito_assistant_types::ProviderProfile;

    fn dummy_remote_job() -> AssistantRemoteJob {
        let (_, receiver) = sync_channel(1);
        let (_, stream_rx) = sync_channel::<String>(128);
        AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".to_string(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".to_string(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 0,
            document_digest: String::new(),
            focus: None,
            cancellation: CancellationToken::default(),
            receiver,
            stream_rx: Some(stream_rx),
            stream_text: String::new(),
            preview_active: false,
            started_at: std::time::Instant::now(),
            first_delta_at: None,
        }
    }

    fn dummy_anim_job() -> AssistantAnimJob {
        let (_, receiver) = sync_channel(1);
        AssistantAnimJob {
            cancellation: CancellationToken::default(),
            receiver,
            history: None,
        }
    }

    fn derive(runtime: &AssistantRuntime, panel: &AssistantPanelState) -> AssistantTurnState {
        AssistantTurnState::derive_from(runtime, panel)
    }

    #[test]
    fn turn_derivado_idle_por_defecto() {
        let runtime = AssistantRuntime::default();
        let panel = AssistantPanelState::default();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Idle);
    }

    #[test]
    fn turn_derivado_composing_desde_problema() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.problem = "derivada de x^2".to_string();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Composing);
        // Solo espacios no es componer.
        panel.problem = "   ".to_string();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Idle);
    }

    #[test]
    fn turn_derivado_awaiting_authorization() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.stage_remote_authorization("¿derivo?".to_string(), "red".to_string());
        assert_eq!(
            derive(&runtime, &panel),
            AssistantTurnState::AwaitingAuthorization
        );
    }

    #[test]
    fn turn_derivado_thinking_desde_flag_y_desde_slot() {
        let mut runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.is_pending = true;
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
        panel.is_pending = false;
        runtime.remote_job = Some(dummy_remote_job());
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
    }

    #[test]
    fn turn_derivado_animating_con_job_id_estable() {
        let mut runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.anim_progress = true;
        assert_eq!(
            derive(&runtime, &panel),
            AssistantTurnState::Animating {
                job_id: "anim-progress".to_string(),
            }
        );
        panel.anim_progress = false;
        runtime.media.anim_job = Some(dummy_anim_job());
        assert_eq!(
            derive(&runtime, &panel),
            AssistantTurnState::Animating {
                job_id: "anim".to_string(),
            }
        );
    }

    #[test]
    fn turn_derivado_failed_conserva_reason_y_terminal() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.fail_request("Se cortó la conexión.");
        let turn = derive(&runtime, &panel);
        assert_eq!(
            turn,
            AssistantTurnState::Failed {
                reason: "Se cortó la conexión.".to_string(),
            }
        );
        assert!(turn.is_terminal());
        assert_eq!(turn.state_name(), "Failed");
    }

    #[test]
    fn turn_derivado_cancelled_domina_y_drain_deriva_a_failed() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.is_pending = true;
        panel.begin_cancellation();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Cancelled);
        // El drain publica el error (misma semántica que la Piel): el turno
        // cancelado y drenado se lee como Failed, no como Cancelled.
        panel.fail_request("La consulta se canceló antes de recibir una respuesta.");
        assert!(derive(&runtime, &panel).is_terminal());
        assert_eq!(derive(&runtime, &panel).state_name(), "Failed");
    }

    #[test]
    fn turn_derivado_respeta_prioridad_cancelling_sobre_todo() {
        let mut runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.problem = "algo".to_string();
        panel.fail_request("viejo");
        runtime.remote_job = Some(dummy_remote_job());
        runtime.media.anim_job = Some(dummy_anim_job());
        panel.begin_cancellation();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Cancelled);
    }

    #[test]
    fn turn_derivado_thinking_tapa_auth_y_failed() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.stage_remote_authorization("q".to_string(), "red".to_string());
        panel.error = Some("viejo".to_string());
        panel.is_pending = true;
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
    }

    #[test]
    fn turn_derivado_complete_request_vuelve_a_idle() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.problem = "2+2".to_string();
        panel.is_pending = true;
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
        panel.complete_request("4".to_string());
        // `complete_request` no borra el composer: el turno vuelve a
        // Composing si el usuario ya escribe lo siguiente, si no a Idle.
        panel.problem.clear();
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Idle);
    }

    #[test]
    fn turn_derivado_no_toca_historial_ni_export() {
        let runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        panel.complete_request("hola".to_string());
        assert_eq!(panel.conversation.len(), 1);
        let before = panel.conversation.len();
        let _ = derive(&runtime, &panel);
        assert_eq!(panel.conversation.len(), before);
    }

    #[test]
    fn grafito_app_expone_el_turno_derivado() {
        let mut app = crate::app::dummy_grafito_app();
        assert_eq!(
            app.assistant_turn_state(),
            AssistantTurnState::Idle,
            "dummy arranca Idle"
        );
        app.assistant.problem = "área del círculo".to_string();
        assert_eq!(app.assistant_turn_state(), AssistantTurnState::Composing);
        app.assistant.is_pending = true;
        assert_eq!(app.assistant_turn_state(), AssistantTurnState::Thinking);
    }

    // ── Harness B2: contexto mínimo sin GrafitoApp ──

    fn with_test_ctx(f: impl FnOnce(&mut AssistantJobsContext<'_>, &egui::Context)) {
        let mut runtime = AssistantRuntime::default();
        let mut panel = AssistantPanelState::default();
        let mut document = Document::default();
        let mut undo_stack: VecDeque<Document> = VecDeque::new();
        let mut redo_stack: VecDeque<ChangeSet> = VecDeque::new();
        let mut profile = StudentProfile::default();
        let registry: Option<grafito_plugins::PluginRegistry> = None;
        let mut notify = |_: String, _: ToastKind| {};
        let egui_ctx = egui::Context::default();
        let mut ctx = AssistantJobsContext {
            runtime: &mut runtime,
            panel: &mut panel,
            document: &mut document,
            undo_stack: &mut undo_stack,
            redo_stack: &mut redo_stack,
            profile: &mut profile,
            plugin_registry: &registry,
            selected_object: None,
            camera: Camera3D::default(),
            current_view: ViewMode::D2,
            notify: &mut notify,
        };
        f(&mut ctx, &egui_ctx);
    }

    fn dummy_model_job() -> AssistantModelJob {
        let (_, receiver) = sync_channel(1);
        AssistantModelJob {
            id: 7,
            provider: ProviderProfile::OpenCodeGo,
            cancellation: CancellationToken::default(),
            receiver,
        }
    }

    fn dummy_image_job() -> AssistantImageJob {
        let (_, receiver) = sync_channel(1);
        AssistantImageJob { receiver }
    }

    fn dummy_agent_job() -> AssistantAgentJob {
        let (_, receiver) = sync_channel(128);
        let (_, clarification_receiver) = sync_channel(4);
        AssistantAgentJob {
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".to_string(),
            cancellation: grafito_agent::loop_engine::Cancellation::default(),
            receiver,
            clarification_receiver,
        }
    }

    fn dummy_proposal_job() -> AssistantProposalJob {
        let (_, receiver) = sync_channel(1);
        AssistantProposalJob {
            id: 9,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".to_string(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".to_string(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 0,
            document_digest: String::new(),
            focus: None,
            text: "Plot[x]".to_string(),
            cancellation: CancellationToken::default(),
            receiver,
        }
    }

    fn dummy_png_export_job() -> crate::assistant_media::PngDirExportJob {
        let path = std::env::temp_dir().join("grafito-test-png-cancel");
        let finished = path.clone();
        let handle =
            std::thread::spawn(move || Ok::<_, crate::anim_native::PngDirExportError>(finished));
        crate::assistant_media::PngDirExportJob {
            handle,
            frame_count: 1,
            cancel: CancellationToken::default(),
            path,
        }
    }

    fn dummy_remote_launch() -> AssistantRemoteLaunch {
        AssistantRemoteLaunch {
            settings: ProviderSettings::for_profile(
                ProviderProfile::OpenCodeGo,
                "deepseek-v4-flash",
            ),
            request: AssistantRequest::local("q", ImmutableDocumentContext::empty(1)),
            api_key: None,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-flash".to_string(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".to_string(),
            document_revision: 0,
            document_digest: String::new(),
            focus: None,
            correction_attempt: 0,
            repair_target_turn: None,
            socratic_guard: None,
        }
    }

    // ── B2-MED #3: reset Exporting→Idle con cualquier formato ──

    #[test]
    fn cancel_turno_anim_resetea_card_con_png_en_vuelo() {
        with_test_ctx(|ctx, _| {
            ctx.runtime.png_export_job = Some(dummy_png_export_job());
            ctx.panel.set_media_export(MediaExportState::Exporting);
            assert!(AssistantJobsController::cancel_turno_anim(ctx));
            assert!(ctx.runtime.png_export_job.is_none());
            assert_eq!(
                *ctx.panel.media_export_state(),
                MediaExportState::Idle,
                "png en vuelo también resetea la card (antes solo el gif)"
            );
        });
    }

    #[test]
    fn cancel_turno_anim_sin_nada_es_noop_y_no_toca_card() {
        with_test_ctx(|ctx, _| {
            ctx.panel.set_media_export(MediaExportState::Exporting);
            assert!(!AssistantJobsController::cancel_turno_anim(ctx));
            // Sin export en vuelo no hay reset: el estado lo drena el poll.
            assert_eq!(*ctx.panel.media_export_state(), MediaExportState::Exporting);
        });
    }

    // ── B2-MED #4: fallback_model se limpia en el cancel ──

    #[test]
    fn cancel_turno_anim_limpia_fallback_model() {
        with_test_ctx(|ctx, _| {
            ctx.runtime.fallback_model = Some("deepseek-v4-flash".to_string());
            ctx.runtime.remote_job = Some(dummy_remote_job());
            assert!(AssistantJobsController::cancel_turno_anim(ctx));
            assert_eq!(ctx.runtime.fallback_model, None);
        });
    }

    #[test]
    fn cancel_remote_limpia_fallback_model() {
        with_test_ctx(|ctx, _| {
            ctx.runtime.fallback_model = Some("deepseek-v4-flash".to_string());
            ctx.runtime.model_job = Some(dummy_model_job());
            AssistantJobsController::cancel_remote(ctx);
            assert_eq!(ctx.runtime.fallback_model, None);
        });
    }

    // ── B2-LOW #6: sin doble-spawn ──

    // B2-LOW: sin doble-spawn — en dev el `debug_assert!` grita (loud,
    // pineado por `should_panic` solo con `debug_assertions`); en
    // release/bench (`debug_assertions` OFF) el early-return conserva el
    // slot para su drain (pineado por los asserts de abajo). Entre ambos
    // cubren todos los perfiles.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "start_remote con slot remoto ocupado")
    )]
    fn start_remote_no_pisa_slot_ocupado() {
        with_test_ctx(|ctx, egui_ctx| {
            ctx.runtime.remote_job = Some(dummy_remote_job());
            AssistantJobsController::start_remote(ctx, egui_ctx, dummy_remote_launch());
            assert_eq!(
                ctx.runtime.remote_job.as_ref().expect("slot conservado").id,
                1,
                "el job anterior sigue vivo para su drain"
            );
        });
    }

    // B2-LOW: idem `start_remote` (loud en dev via `cfg_attr`, early-return
    // en release pineado por asserts).
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "start_agent con slot agente ocupado")
    )]
    fn start_agent_no_pisa_slot_ocupado() {
        with_test_ctx(|ctx, egui_ctx| {
            ctx.runtime.agent_job = Some(dummy_agent_job());
            AssistantJobsController::start_agent(ctx, egui_ctx, dummy_remote_launch());
            assert_eq!(
                ctx.runtime
                    .agent_job
                    .as_ref()
                    .expect("slot conservado")
                    .model,
                "deepseek-v4-flash"
            );
        });
    }

    // B2-LOW: idem `start_remote` (loud en dev via `cfg_attr`, early-return
    // en release pineado por asserts).
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "start_remote_proposal con slot propuesta ocupado")
    )]
    fn start_remote_proposal_no_pisa_slot_ocupado() {
        with_test_ctx(|ctx, egui_ctx| {
            ctx.runtime.proposal_job = Some(dummy_proposal_job());
            let launch = AssistantProposalLaunch {
                id: 99,
                provider: ProviderProfile::OpenCodeGo,
                model: "otro".to_string(),
                route: AssistantRemoteRoute::SelectedModel,
                fusion_fallback_allowed: false,
                question: "q".to_string(),
                correction_attempt: 0,
                repair_target_turn: None,
                document_revision: 0,
                document_digest: String::new(),
                focus: None,
                text: "Plot[y]".to_string(),
            };
            AssistantJobsController::start_remote_proposal(ctx, egui_ctx, launch);
            assert_eq!(
                ctx.runtime
                    .proposal_job
                    .as_ref()
                    .expect("slot conservado")
                    .id,
                9
            );
        });
    }

    // B2-LOW: idem `start_remote` (loud en dev via `cfg_attr`, early-return
    // en release pineado por asserts).
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "start_model con slot modelos ocupado")
    )]
    fn start_model_no_pisa_slot_ocupado() {
        with_test_ctx(|ctx, egui_ctx| {
            ctx.runtime.model_job = Some(dummy_model_job());
            AssistantJobsController::start_model(ctx, egui_ctx);
            assert_eq!(
                ctx.runtime.model_job.as_ref().expect("slot conservado").id,
                7
            );
        });
    }

    // ── B2-MED #5: auxiliares documentados, sin secuestrar el turno ──

    #[test]
    fn auxiliares_no_derivan_thinking_pero_son_visibles() {
        let mut runtime = AssistantRuntime::default();
        let panel = AssistantPanelState::default();
        assert!(!AssistantTurnState::is_auxiliary(&runtime, &panel));

        runtime.model_job = Some(dummy_model_job());
        assert!(AssistantTurnState::is_auxiliary(&runtime, &panel));
        assert_eq!(
            derive(&runtime, &panel),
            AssistantTurnState::Idle,
            "la lista de modelos no secuestra el turno"
        );

        runtime.model_job = None;
        runtime.image_job = Some(dummy_image_job());
        assert!(AssistantTurnState::is_auxiliary(&runtime, &panel));
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Idle);

        runtime.image_job = None;
        let mut panel = AssistantPanelState::default();
        panel.set_media_export(MediaExportState::Exporting);
        assert!(AssistantTurnState::is_auxiliary(&runtime, &panel));
        assert_eq!(
            derive(&runtime, &panel),
            AssistantTurnState::Idle,
            "el export vive en la card, no en el turno"
        );

        // Preview de streaming: auxiliar visible pero el turno ya es Thinking
        // por el slot remoto (no por el preview).
        runtime.remote_job = Some(dummy_remote_job());
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
        runtime
            .remote_job
            .as_mut()
            .expect("slot remoto")
            .preview_active = true;
        assert!(AssistantTurnState::is_auxiliary(&runtime, &panel));
        assert_eq!(derive(&runtime, &panel), AssistantTurnState::Thinking);
    }
}
