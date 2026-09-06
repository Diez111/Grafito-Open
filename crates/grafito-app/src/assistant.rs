//! Integración de proveedores del asistente fuera del hilo de interfaz.

use crate::{assistant_credentials, GrafitoApp};
use grafito_assistant::{
    harness, request_remote_models_with_api_key_on_worker,
    request_remote_streaming_with_api_key_on_worker, validate_attachment, CancellationToken,
    ProviderSettings, RemoteCompletion, SocraticGuardContext,
};
use grafito_assistant_types::{
    AssistantFocus, AssistantRepairFailure, AssistantRepairFailureKind, AssistantRepairFeedback,
    AssistantRequest, AssistantResponse, AttachmentLimits, ConversationRole, ConversationTurn,
    ImmutableDocumentContext, LocalAssistantStatus, ProposedPlan, ProviderCapabilities,
    ProviderProfile, MAX_CONVERSATION_TURNS, MAX_CONVERSATION_TURN_CHARS,
    REMOTE_CONTEXT_PROMPT_OVERHEAD_BYTES, REMOTE_FOCUS_PROMPT_OVERHEAD_BYTES,
    REMOTE_PLUGIN_INSTRUCTIONS_OVERHEAD_BYTES, REMOTE_REPAIR_FEEDBACK_PROMPT_OVERHEAD_BYTES,
    REMOTE_TOOL_CATALOG_PROMPT_OVERHEAD_BYTES,
};
use grafito_command::assistant_proposals::{
    assistant_fenced_proposals, execute_assistant_command, execute_assistant_parameter,
    AssistantCommandInvocation, AssistantParameterAssignment, AssistantProposal,
    AssistantProposalRejection, AssistantProposalRejectionKind,
};
use grafito_pedagogy::scaffold::{extract_concept, is_exploratory_request};
use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine, SocraticFsm, Turn};
use grafito_ui::assistant::{
    AssistantCorrectionContext, AssistantPanelState, AssistantUiAction, VerifiedAssistantProposal,
};
use grafito_ui::toast::ToastKind;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};

const MAX_REMOTE_PROPOSAL_PREFLIGHTS: usize = 4;
const MAX_ASSISTANT_PROPOSAL_CORRECTIONS: u8 = 2;
const MAX_ASSISTANT_CORRECTION_SOURCE_BYTES: usize = 2_048;
/// Modelo multimodal/visión (Xiaomi MiMo 2.5-VL); el razonamiento usa
/// DeepSeek Flash por defecto (el más barato y suficiente).
const OPENCODE_VISION_MODEL: &str = "mimo-2.5-vl";
const OPENCODE_FUSION_MODEL: &str = "fusion";
const ASSISTANT_CORRECTION_INSTRUCTION: &str = "\n\nUna propuesta gráfica anterior no superó la verificación local. Conservá la intención de la solicitud y regenerá una respuesta completa y autocontenida con un bloque grafito o un bloque grafito-scene de 2 a 8 comandos ejecutables. Si necesitás un parámetro escalar nuevo, incluí antes un único bloque grafito-param con una asignación finita. Usá exclusivamente la sintaxis exacta del catálogo; no inventes comandos ni emitas acciones de archivo, red, sistema o Script.";

/// Guarda el perfil en background para no bloquear el UI thread (60fps).
fn spawn_profile_save(profile: grafito_profile::StudentProfile, path: PathBuf) {
    // I/O en background thread para no bloquear UI (60fps)
    let _ = std::thread::Builder::new()
        .name("profile-save".into())
        .spawn(move || {
            let _ = std::fs::write(
                path,
                serde_json::to_string_pretty(&profile).unwrap_or_default(),
            );
        });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssistantRemoteRoute {
    SelectedModel,
    FusionFallback,
}

enum LocalAssistantDisposition {
    Solved {
        answer: String,
        plan: Option<ProposedPlan>,
    },
    NeedsRemoteAuthorization(String),
    Rejected(String),
}

fn classify_local_assistant_response(response: AssistantResponse) -> LocalAssistantDisposition {
    match response.status {
        LocalAssistantStatus::Solved => LocalAssistantDisposition::Solved {
            answer: response.answer,
            plan: response.plan,
        },
        LocalAssistantStatus::Unsupported | LocalAssistantStatus::VisionUnavailable => {
            LocalAssistantDisposition::NeedsRemoteAuthorization(response.answer)
        }
        LocalAssistantStatus::Rejected => LocalAssistantDisposition::Rejected(response.answer),
    }
}

fn apply_local_assistant_plan(
    document: &mut grafito_core::Document,
    plan: &ProposedPlan,
    undo_stack: &mut VecDeque<grafito_core::Document>,
    redo_stack: &mut VecDeque<grafito_core::ChangeSet>,
) -> Result<grafito_command::assistant_plan::PlanApplyResult, String> {
    let before = document.clone();
    let result = harness::apply_plan(document, plan)?;
    let outcome =
        grafito_command::commands::CommandOutcome::Message("Propuesta local aplicada.".into());
    crate::app::save_command_snapshot_if_mutated(
        &outcome, before, document, undo_stack, redo_stack,
    );
    Ok(result)
}

#[derive(Default)]
pub(crate) struct AssistantRuntime {
    next_request_id: u64,
    /// Modelo de fallback solo-sesión (p.ej. deepseek tras caída de spark).
    /// No se persiste: la preferencia del usuario queda intacta y el próximo
    /// pedido reintenta el modelo elegido (auto-recupera si el proveedor vuelve).
    fallback_model: Option<String>,
    remote_job: Option<AssistantRemoteJob>,
    proposal_job: Option<AssistantProposalJob>,
    model_job: Option<AssistantModelJob>,
    model_refresh_queued: bool,
    image_job: Option<AssistantImageJob>,
    agent_job: Option<AssistantAgentJob>,
    anim_job: Option<AssistantAnimJob>,
    /// Export a GIF de la card en vuelo (B5): `JoinHandle` de
    /// `spawn_gif_export` que `poll_gif_export_job` drena sin bloquear.
    gif_export_job: Option<GifExportJob>,
    session_api_key: Option<SessionApiKey>,
}

struct SessionApiKey {
    provider: ProviderProfile,
    key: String,
}

impl Drop for SessionApiKey {
    fn drop(&mut self) {
        // Zeroize clave en memoria para no dejar secretos en heap.
        // No usamos crate `zeroize` (no está en Cargo.toml); implementamos manual
        // con volatile write + black_box para evitar optimización.
        let len = self.key.len();
        if len > 0 {
            // Safety: String's allocation es contigua y escribible; sobreescribimos bytes.
            unsafe {
                std::ptr::write_bytes(self.key.as_mut_ptr(), 0u8, len);
            }
        }
        std::hint::black_box(&self.key);
        // Limpia contenido lógico; la memoria ya está zeroizada arriba.
        self.key.clear();
        // Asegura que el compilador no elimine el zeroize.
        std::hint::black_box(&self.key);
        // Si zeroize crate estuviera disponible, usaríamos `self.key.zeroize()`.
    }
}

impl AssistantRuntime {
    /// Modelo que deben traer los resultados en vuelo: el fallback si hay un
    /// reintento activo, si no el configurado por el usuario.
    fn expected_model<'a>(&'a self, current: &'a str) -> &'a str {
        self.fallback_model.as_deref().unwrap_or(current)
    }

    fn key_for(&self, provider: ProviderProfile) -> Option<String> {
        self.session_api_key
            .as_ref()
            .filter(|stored| stored.provider == provider)
            .map(|stored| stored.key.clone())
    }

    fn remember_key(&mut self, provider: ProviderProfile, key: String) {
        // Punto único de saneado: nunca queda en memoria una clave con
        // espacios/saltos pegados al copiar.
        self.session_api_key = Some(SessionApiKey {
            provider,
            key: key.trim().to_owned(),
        });
    }

    fn forget_key(&mut self) {
        self.session_api_key = None;
    }

    fn remote_request_slot_is_free(&self) -> bool {
        self.remote_job.is_none() && self.proposal_job.is_none() && self.agent_job.is_none()
    }

    fn cancel_stale_agent_job(
        &mut self,
        current_provider: ProviderProfile,
        current_model: &str,
    ) -> bool {
        if let Some(job) = self.agent_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                return true;
            }
        }
        false
    }

    fn cancel_stale_remote_job(
        &mut self,
        current_provider: ProviderProfile,
        current_model: &str,
    ) -> bool {
        let mut cancelled = false;
        if let Some(job) = self.remote_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                cancelled = true;
            }
        }
        if let Some(job) = self.proposal_job.as_ref() {
            if !job.cancellation.is_cancelled()
                && !accepts_remote_result(current_provider, current_model, job.provider, &job.model)
            {
                job.cancellation.cancel();
                cancelled = true;
            }
        }
        cancelled
    }

    fn take_finished_remote_job(&mut self) -> Option<FinishedRemoteJob> {
        let result = {
            let job = self.remote_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La consulta del asistente terminó inesperadamente.".into())
                }
            }
        };
        let job = self.remote_job.take()?;
        Some(FinishedRemoteJob {
            id: job.id,
            provider: job.provider,
            model: job.model,
            route: job.route,
            fusion_fallback_allowed: job.fusion_fallback_allowed,
            question: job.question,
            correction_attempt: job.correction_attempt,
            repair_target_turn: job.repair_target_turn,
            document_revision: job.document_revision,
            document_digest: job.document_digest,
            focus: job.focus,
            cancelled: job.cancellation.is_cancelled(),
            result,
            stream_preview_active: job.preview_active,
        })
    }

    /// Drena los deltas de streaming a la burbuja provisional del chat.
    ///
    /// Lee todo lo disponible del canal acotado (128, best-effort) y crea o
    /// actualiza UN turno asistente provisional al final de `conversation`
    /// (se renderiza como burbuja porque el dibujado recorre toda la
    /// conversación también con `is_pending`). El texto visible se capa a
    /// `MAX_CONVERSATION_TURN_CHARS`; el resultado final llega por el canal
    /// de completado con su propio presupuesto. Si la conversación ya está en
    /// `MAX_CONVERSATION_TURNS`, se omite el preview sin perder el final.
    /// Puro UI-state, sin I/O: se llama cada frame desde `poll_assistant_jobs`.
    pub(crate) fn drain_remote_stream_preview(
        &mut self,
        panel: &mut AssistantPanelState,
        ctx: &egui::Context,
    ) -> bool {
        let deltas: Vec<String> = {
            let Some(job) = self.remote_job.as_ref() else {
                return false;
            };
            let Some(stream) = job.stream_rx.as_ref() else {
                return false;
            };
            stream.try_iter().collect()
        };
        if deltas.is_empty() {
            return false;
        }
        let Some(job) = self.remote_job.as_mut() else {
            return false;
        };
        for delta in deltas {
            job.stream_text.push_str(&delta);
        }
        let display: String = job
            .stream_text
            .chars()
            .take(MAX_CONVERSATION_TURN_CHARS)
            .collect();
        if !job.preview_active {
            if panel.conversation.len() >= MAX_CONVERSATION_TURNS {
                return false;
            }
            panel
                .conversation
                .push(ConversationTurn::assistant(display));
            job.preview_active = true;
        } else if let Some(last) = panel.conversation.last_mut() {
            // Invariante: con el slot remoto ocupado nada más empuja turnos,
            // así que el último sigue siendo nuestro provisional.
            if last.role == ConversationRole::Assistant {
                last.content = display;
            }
        }
        ctx.request_repaint();
        true
    }

    fn take_finished_proposal_job(&mut self) -> Option<FinishedProposalJob> {
        let result = {
            let job = self.proposal_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La comprobación local de la propuesta terminó inesperadamente.".into())
                }
            }
        };
        let job = self.proposal_job.take()?;
        Some(FinishedProposalJob {
            id: job.id,
            provider: job.provider,
            model: job.model,
            route: job.route,
            fusion_fallback_allowed: job.fusion_fallback_allowed,
            question: job.question,
            correction_attempt: job.correction_attempt,
            repair_target_turn: job.repair_target_turn,
            document_revision: job.document_revision,
            document_digest: job.document_digest,
            focus: job.focus,
            text: job.text,
            cancelled: job.cancellation.is_cancelled(),
            result,
        })
    }

    fn request_model_refresh(&mut self) -> bool {
        if self.model_job.is_some() {
            self.model_refresh_queued = true;
            false
        } else {
            true
        }
    }

    fn cancel_stale_model_job(&mut self, current_provider: ProviderProfile) -> bool {
        let Some(job) = self.model_job.as_ref() else {
            return false;
        };
        if job.cancellation.is_cancelled() || job.provider == current_provider {
            return false;
        }
        job.cancellation.cancel();
        true
    }

    fn take_finished_model_job(&mut self) -> Option<FinishedModelJob> {
        let result = {
            let job = self.model_job.as_ref()?;
            match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err("La lista de modelos terminó inesperadamente.".into())
                }
            }
        };
        let job = self.model_job.take()?;
        Some(FinishedModelJob {
            id: job.id,
            provider: job.provider,
            cancelled: job.cancellation.is_cancelled(),
            result,
        })
    }

    fn take_queued_model_refresh_if_idle(&mut self) -> bool {
        self.model_job.is_none() && std::mem::take(&mut self.model_refresh_queued)
    }

    /// Cancela todos los jobs del asistente para el botón Cancel.
    ///
    /// - `remote`/`proposal`/`agent`/`model` tienen token: se marcan cancelados
    ///   pero el slot se conserva hasta que `take_finished_*`/`poll_assistant_agent`
    ///   drene el worker (sin huérfanos; ver test `cancelled_remote_job_...`).
    /// - `anim` tiene token (AS4, como `agent`): se señala y se dropea el slot.
    ///   El worker es acotado (render nativo <2 s con chequeo entre frames o
    ///   `job_timeout` 15 s del motor con closure `cancel`) y su `send` falla
    ///   tras el drop, así que el hilo termina solo sin dejar trabajo huérfano.
    ///
    /// Retorna `true` si había algún job en vuelo.
    fn cancel_all_assistant_jobs(&mut self) -> bool {
        let mut cancelled = false;
        if let Some(job) = self.remote_job.as_ref() {
            job.cancellation.cancel();
            cancelled = true;
        }
        if let Some(job) = self.proposal_job.as_ref() {
            job.cancellation.cancel();
            cancelled = true;
        }
        if let Some(job) = self.agent_job.as_ref() {
            job.cancellation.cancel();
            cancelled = true;
        }
        if let Some(job) = self.model_job.as_ref() {
            job.cancellation.cancel();
            cancelled = true;
        }
        if self.cancel_anim_job() {
            cancelled = true;
        }
        cancelled
    }

    /// Cancela solo el job de animación (AS4): señala el token y dropea el slot.
    ///
    /// Headless y sin I/O: el hilo en vuelo observa el token entre frames
    /// (closure de progreso) y descarta el resultado rancio en vez de
    /// publicarlo. Retorna `true` si había job en vuelo.
    pub(crate) fn cancel_anim_job(&mut self) -> bool {
        if let Some(job) = self.anim_job.as_ref() {
            job.cancellation.cancel();
        }
        if self.anim_job.is_some() {
            self.anim_job = None;
            true
        } else {
            false
        }
    }
}

/// Limpia la burbuja provisional del streaming (cancelación o resultado).
///
/// Sólo retira el último turno si es del asistente: con el slot remoto
/// ocupado nada más empuja turnos, así que ese es el provisional creado por
/// `drain_remote_stream_preview`. Si nunca hubo preview, no toca nada y la
/// conversación queda como en el path no-streaming.
fn pop_provisional_stream_turn(panel: &mut AssistantPanelState) {
    if panel
        .conversation
        .last()
        .is_some_and(|turn| turn.role == ConversationRole::Assistant)
    {
        panel.conversation.pop();
    }
}

/// ¿El error remoto es una reparación socrática forzada por el guard?
///
/// El worker retorna el diagnóstico interno tal cual cuando
/// `guard_remote_completion` detecta telling con `attempts<2`. El diagnóstico
/// conserva la jerga `GUARD TELLING` SOLO para detección y logs; el poll JAMÁS
/// lo publica: lo convierte a voz de Mili vía `repair_student_message`
/// (sin `GUARD`/`attempts`/`estado`/`Re-preguntá`). Publicarlo crudo vía
/// `complete_request` fue el bug P0.
fn is_socratic_repair_error(error: &str) -> bool {
    error.starts_with("GUARD TELLING")
}

/// Cuenta respuestas a pregunta heurística previa (no turnos totales).
///
/// Un turno de usuario cuenta solo si el turno asistente inmediato anterior
/// contiene `?` (re-pregunta heurística). El turno actual (último, ya empujado
/// por `begin_request`) se excluye: aún no es respuesta, es la pregunta en
/// curso. Primer turno sin pregunta previa ⇒ 0 y no punitivo (el guard de
/// sesión se bypassea en `session_socratic_guard`; ver `can_reveal_answer`).
/// Puro y determinista, sin `unwrap`, saturante a `u8`.
fn count_heuristic_answers(conversation: &[ConversationTurn]) -> u8 {
    let minus_current = conversation.len().saturating_sub(1);
    let mut count: usize = 0;
    let mut idx = 0;
    while idx < minus_current {
        let is_user = conversation
            .get(idx)
            .map(|turn| turn.role == ConversationRole::User)
            .unwrap_or(false);
        if is_user {
            let prev_is_question = idx
                .checked_sub(1)
                .and_then(|prev| conversation.get(prev))
                .map(|prev| prev.role == ConversationRole::Assistant && prev.content.contains('?'))
                .unwrap_or(false);
            if prev_is_question {
                count = count.saturating_add(1);
            }
        }
        idx += 1;
    }
    count.min(255) as u8
}

/// Construye el guard socrático de sesión para un lanzamiento remoto.
///
/// - `attempts`: respuestas a pregunta heurística previa (ver
///   `count_heuristic_answers`), no turnos totales. El turno actual se excluye.
///   Primer turno sin pregunta previa ⇒ 0 y no punitivo.
/// - `topic`: concepto sanitizado vía `extract_concept` (quita saludos/verbos
///   como `hola`/`haceme`/`dame`, exige min 3 letras y allowlist). Sin tema
///   reconocido → `String::new()` para que el scaffold caiga al fallback fijo
///   `¿qué querés graficar primero: recta, parábola…?` en vez de interpolar el
///   texto crudo (`¿Te imaginás hola...?` era el bug P0).
/// - `level`: `PedagogicalLevel::from_level_value` del nivel del perfil.
/// - `history`: últimos 4 turnos como `Turn` pedagógicos (contenido capado a
///   200 chars; el engine vuelve a acotar al segmentar).
///   Puro y determinista: misma sesión → mismo guard.
fn socratic_guard_context(
    level_value: u32,
    working_topic: Option<&str>,
    question: &str,
    conversation: &[ConversationTurn],
) -> SocraticGuardContext {
    // Sanitiza: memoria primero, luego pregunta actual; sin tema → "" (fallback).
    let sanitized_topic: String = working_topic
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .and_then(extract_concept)
        .or_else(|| extract_concept(question))
        .unwrap_or_default();
    let mut fsm = SocraticFsm::new(sanitized_topic);
    fsm.attempts = count_heuristic_answers(conversation);
    let history: Vec<Turn> = conversation
        .iter()
        .rev()
        .take(4)
        .rev()
        .map(|turn| Turn {
            role: match turn.role {
                ConversationRole::User => "user".into(),
                ConversationRole::Assistant => "assistant".into(),
            },
            content: turn.content.chars().take(200).collect(),
        })
        .collect();
    let scaffold = ScaffoldEngine.scaffold(
        &fsm.topic.clone(),
        PedagogicalLevel::from_level_value(level_value),
        &history,
    );
    SocraticGuardContext { fsm, scaffold }
}

struct AssistantRemoteJob {
    id: u64,
    /// Identidad seleccionada por el usuario, usada para descartar resultados obsoletos.
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    cancellation: CancellationToken,
    receiver: Receiver<Result<RemoteCompletion, String>>,
    /// Deltas de streaming SSE (sólo protocolo Responses; el resto lo deja
    /// desconectado y nunca hay preview). Acotado a 128 (best-effort).
    stream_rx: Option<Receiver<String>>,
    /// Texto acumulado del stream para la burbuja provisional.
    stream_text: String,
    /// Hay un turno provisional al final de `conversation` que debe limpiarse
    /// al terminar/cancelar (ver `pop_provisional_stream_turn`).
    preview_active: bool,
}

struct AssistantProposalJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
    cancellation: CancellationToken,
    receiver: Receiver<Result<RemoteProposalVerification, String>>,
}

struct AssistantRemoteLaunch {
    settings: ProviderSettings,
    request: AssistantRequest,
    api_key: Option<String>,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    /// Guard socrático de sesión: el worker lo aplica sobre el completado
    /// final (streaming y no-streaming). `None` lo desactiva.
    socratic_guard: Option<SocraticGuardContext>,
}

struct AssistantRepairRequest {
    feedback: AssistantRepairFeedback,
    target_turn: usize,
}

struct AssistantProposalLaunch {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
}

struct AssistantModelJob {
    id: u64,
    provider: ProviderProfile,
    cancellation: CancellationToken,
    receiver: Receiver<Result<Vec<String>, String>>,
}

struct AssistantImageJob {
    receiver: Receiver<Result<grafito_assistant_types::ImageAttachment, String>>,
}

/// Mensaje del hilo del agente hacia la UI.
enum AgentChannelMsg {
    Event(grafito_agent::AgentEvent),
    Done(Result<grafito_agent::loop_engine::AgentOutcome, String>),
}

/// Parsea el `args_summary` de un `ToolStarted{ask_user}` a pendiente UI (S2).
///
/// Puro y no bloqueante: `args_summary` es `arguments.to_string()` truncado a
/// 160 chars; para preguntas cortas típicas alcanza. Si viene truncado con
/// `…` se recorta y se intenta igual; si no parsea, `None` honesto (sin
/// inventar pregunta ni opciones). El `call_id` real no viaja en el evento,
/// así que se deriva estable de la pregunta (longitud) sin inventar UUID.
fn parse_agent_ask_user_pending(
    args_summary: &str,
) -> Option<grafito_ui::assistant::PendingClarification> {
    let cleaned = args_summary.trim().trim_end_matches('…').trim();
    if cleaned.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(cleaned).ok()?;
    let question = value.get("question").and_then(serde_json::Value::as_str)?;
    if question.trim().is_empty() {
        return None;
    }
    let options = value
        .get("options")
        .and_then(|options| options.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let call_id = format!("ask_user-{}", question.len());
    grafito_ui::assistant::PendingClarification::try_new(&call_id, question, options).ok()
}

/// Job que genera y carga una animación del motor externo.
///
/// AS4: con `cancellation` como `AssistantAgentJob` (cancel real: descartar
/// señala el token, no solo dropea el receiver). El hilo la chequea entre
/// frames en el closure de progreso (el render nativo no acepta token) y
/// antes/después del transporte; el motor externo la recibe como closure
/// `cancel` en `run_job` (aborto <200 ms).
struct AssistantAnimJob {
    cancellation: CancellationToken,
    receiver: std::sync::mpsc::Receiver<Result<grafito_ui::assistant::AssistantMedia, String>>,
}

/// Export a GIF de la card en vuelo (B5).
///
/// Guarda el `JoinHandle` de `spawn_gif_export` (hilo existente, reusable y
/// probado) para drenarlo sin bloquear en `poll_gif_export_job`.
/// `frame_count` es solo para el mensaje de éxito (cuántos fotogramas
/// viajaron al GIF).
struct GifExportJob {
    handle: std::thread::JoinHandle<Result<std::path::PathBuf, crate::anim_native::GifExportError>>,
    frame_count: usize,
}

/// Job del modo agente (loop con herramientas).
struct AssistantAgentJob {
    provider: ProviderProfile,
    model: String,
    cancellation: grafito_agent::loop_engine::Cancellation,
    receiver: Receiver<AgentChannelMsg>,
    /// Canal lateral S2 (`ask_user` real vía evento): el forwarder de
    /// background reenvía el pendiente como `PendingClarification` sin
    /// bloquear (try_send acotado). La UI lo drena en `sync_assistant_for_frame`
    /// (hilo UI, try_recv) y lo muestra como botones; nunca toca la zona guard.
    clarification_receiver: Receiver<grafito_ui::assistant::PendingClarification>,
}

struct FinishedRemoteJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    cancelled: bool,
    result: Result<RemoteCompletion, String>,
    /// El job dejó una burbuja provisional que el poll debe limpiar antes de
    /// procesar el resultado (éxito, error o cancelación).
    stream_preview_active: bool,
}

struct FinishedProposalJob {
    id: u64,
    provider: ProviderProfile,
    model: String,
    route: AssistantRemoteRoute,
    fusion_fallback_allowed: bool,
    question: String,
    correction_attempt: u8,
    repair_target_turn: Option<usize>,
    document_revision: u64,
    document_digest: String,
    focus: Option<AssistantFocus>,
    text: String,
    cancelled: bool,
    result: Result<RemoteProposalVerification, String>,
}

struct FinishedModelJob {
    id: u64,
    provider: ProviderProfile,
    cancelled: bool,
    result: Result<Vec<String>, String>,
}

impl GrafitoApp {
    fn assistant_visuals(
        &mut self,
        ctx: &egui::Context,
    ) -> grafito_ui::assistant::AssistantVisuals {
        crate::app::load_mora_avatar_texture_once(
            ctx,
            &mut self.mora_texture,
            &mut self.mora_texture_load_attempted,
            include_bytes!("../../../assets/mora.png"),
        );
        grafito_ui::assistant::AssistantVisuals {
            mora_texture: self.mora_texture.as_ref().map(egui::TextureHandle::id),
        }
    }

    /// Sincroniza el foco actual y procesa resultados antes de pintar cualquier
    /// host del asistente, incluso cuando su pestaña no es la visible.
    pub(crate) fn sync_assistant_for_frame(&mut self, ctx: &egui::Context) {
        // Sincroniza la memoria del tutor con la tarjeta de progreso del panel.
        self.assistant.tutor_level = self.profile.level;
        self.assistant.tutor_covered = self
            .profile
            .branches
            .iter()
            .filter(|branch| branch.covered)
            .count();
        self.assistant.tutor_total = self.profile.branches.len();
        self.assistant.tutor_next = self
            .profile
            .recommend_next()
            .first()
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.assistant.tutor_streak = self.profile.streak;
        self.assistant.tutor_best_streak = self.profile.best_streak;
        self.assistant.tutor_domain_samples = self.domain_sparkline();
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.assistant.tutor_last_activity = self
            .profile
            .recommend_next()
            .first()
            .and_then(|branch| branch.last_study_epoch)
            .map(|last| grafito_profile::time_ago(last, epoch))
            .unwrap_or_default();
        if !self.assistant.settings_open {
            // Sincronizar memoria larga bidireccionalmente
            let mut needs_save = false;
            if self.assistant.long_memory.facts.len() != self.profile.long_memory.facts.len()
                || self.assistant.long_memory.summary != self.profile.long_memory.summary
                || self.assistant.long_memory.enabled != self.profile.long_memory.enabled
                || self.assistant.long_memory.preferences != self.profile.long_memory.preferences
            {
                for fact in self.assistant.long_memory.facts.clone() {
                    if !self
                        .profile
                        .long_memory
                        .facts
                        .iter()
                        .any(|f| f.text == fact.text)
                    {
                        self.profile.long_memory.facts.push(fact);
                        needs_save = true;
                    }
                }
                // También detectar borrados en UI
                let before = self.profile.long_memory.facts.len();
                self.profile.long_memory.facts.retain(|pf| {
                    self.assistant
                        .long_memory
                        .facts
                        .iter()
                        .any(|af| af.text == pf.text)
                });
                if self.profile.long_memory.facts.len() != before {
                    needs_save = true;
                }
                self.profile.long_memory.summary = self.assistant.long_memory.summary.clone();
                self.profile.long_memory.relationship_stage =
                    self.assistant.long_memory.relationship_stage;
                self.profile.long_memory.enabled = self.assistant.long_memory.enabled;
                self.profile.long_memory.preferences =
                    self.assistant.long_memory.preferences.clone();
                if needs_save
                    || self.assistant.long_memory.summary != self.profile.long_memory.summary
                {
                    spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                }
            }
            // Sincronizar avatar (incluye nuevos campos bg, accent_custom, instrucciones)
            // Pero preservar si el perfil cambió externamente por nivel up (evolución)
            // Si hay diferencia en evolución, merge
            if self.assistant.avatar != self.profile.avatar {
                // Si no hay edición pendiente, clonar perfil → assistant
                self.assistant.avatar = self.profile.avatar.clone();
            }
            self.assistant.user_name = self.profile.display_name().to_owned();
            self.assistant.long_memory = self.profile.long_memory.clone();
            // F5 inline: sincroniza WorkingMemory episódica (si existe campo en AssistantPanelState)
            // Si no existe campo, este bloque es el comentario de integración pedido en la tarea.
            self.assistant.working_memory = self.profile.working_memory.clone();
            self.avatar_draft = self.profile.avatar.clone();
        } else {
            // Ventana abierta: preview persistente + merge no destructivo de progreso externo
            // Sincronizar solo campos de progreso (nivel, branch) va en tutor_*, no en avatar
            // Mantener avatar_draft sincronizado para preview
            self.avatar_draft = self.assistant.avatar.clone();
            // F5 inline: también en modo preview mantener WorkingMemory sincronizada para debug inline
            self.assistant.working_memory = self.profile.working_memory.clone();
            // Si el perfil ganó XP mientras se edita, no sobrescribir avatar; solo actualizar long_memory si es externa (no editar facts manualmente mientras abierta)
            if self.assistant.long_memory.enabled != self.profile.long_memory.enabled {
                // respetar cambio externo si el usuario no tocó el toggle en esta sesión
                // No-op: el toggle de la UI tiene prioridad
            }
        }
        self.poll_assistant_jobs(ctx);
        if let Some(job) = self.assistant_runtime.anim_job.as_mut() {
            match job.receiver.try_recv() {
                Ok(Ok(media)) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        // Cancel real: el worker observó el token y el
                        // resultado es rancio — se descarta sin publicar
                        // (sin media rancia en el turno).
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        // La animación vive DENTRO del turno del chat:
                        // `set_media` la instala para el reproductor del
                        // transcript (`ui/assistant.rs:915`, `draw_media_card`
                        // en el último turno). Sin ventana compañera.
                        self.assistant.set_media(Some(media), ctx);
                        self.notify("Animación lista.", ToastKind::Success);
                    }
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    let was_cancelled =
                        job.cancellation.is_cancelled() || error.to_lowercase().contains("cancel");
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if was_cancelled {
                        self.notify("Generación cancelada.", ToastKind::Info);
                    } else {
                        self.assistant.set_media(None, ctx);
                        let message = format!("No se pudo generar la animación: {error}");
                        self.notify(&message, ToastKind::Error);
                        self.show_assistant_error(message);
                    }
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let was_cancelled = job.cancellation.is_cancelled();
                    self.assistant_runtime.anim_job = None;
                    self.assistant.anim_progress = false;
                    if !was_cancelled {
                        self.assistant.set_media(None, ctx);
                        self.show_assistant_error(
                            "La generación terminó inesperadamente antes de responder.",
                        );
                    }
                    ctx.request_repaint();
                }
            }
        }
        // B5: drena el export a GIF de la card (sin bloquear; solo join si terminó).
        self.poll_gif_export_job(ctx);
        if !self.plugins_loaded {
            self.plugins_loaded = true;
            self.load_assistant_plugins();
        }
        // S2: drena aclaraciones del canal lateral sin bloquear (try_recv).
        self.drain_pending_clarifications();
        self.assistant.focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
    }

    /// Carga una sola vez el registry de plugins y aplica las preferencias del usuario.
    fn load_assistant_plugins(&mut self) {
        let context = plugin_validation_context();
        let config = crate::utils::load_config();
        let mut registry = grafito_plugins::PluginRegistry::load_many(
            &[
                &crate::utils::plugins_dir(),
                &crate::utils::user_data_plugins_dir(),
                &crate::utils::system_plugins_dir(),
            ],
            &context,
        );
        for plugin in &mut registry.plugins {
            let id = plugin.manifest.plugin.id.clone();
            let automatic = plugin.manifest.plugin.activation != "manual";
            plugin.enabled = if config.enabled_plugins.contains(&id) {
                true
            } else if config.disabled_plugins.contains(&id) {
                false
            } else {
                automatic
            };
        }
        self.plugin_registry = Some(registry);
        self.refresh_plugin_snapshot();
    }

    /// Refresca el snapshot mostrado en la ventana de ajustes del asistente.
    fn refresh_plugin_snapshot(&mut self) {
        let Some(registry) = &self.plugin_registry else {
            self.assistant.plugins.clear();
            return;
        };
        self.assistant.plugins = registry
            .plugins
            .iter()
            .map(|plugin| grafito_ui::assistant::PluginRow {
                id: plugin.manifest.plugin.id.clone(),
                name: plugin.manifest.plugin.name.clone(),
                version: plugin.manifest.plugin.version.clone(),
                category: plugin.manifest.plugin.category.clone(),
                description: plugin.manifest.plugin.description.clone(),
                enabled: plugin.enabled,
                error: plugin.error.clone(),
            })
            .collect();
    }

    /// Instrucciones locales de los plugins activos, ajustadas al presupuesto.
    fn plugin_instructions_budgeted(&self) -> String {
        const PLUGIN_INSTRUCTION_CAP_BYTES: usize = 4 * 1024;
        let Some(registry) = &self.plugin_registry else {
            return String::new();
        };
        registry.instructions_bounded(
            grafito_assistant_types::MAX_SYSTEM_INSTRUCTIONS_BYTES
                .min(PLUGIN_INSTRUCTION_CAP_BYTES),
        )
    }

    /// Activa o desactiva un plugin y persiste la preferencia.
    fn toggle_assistant_plugin(&mut self, id: &str, enabled: bool) {
        let Some(registry) = self.plugin_registry.as_mut() else {
            return;
        };
        if !registry.set_enabled(id, enabled) {
            return;
        }
        let mut config = crate::utils::load_config();
        config.enabled_plugins.retain(|existing| existing != id);
        config.disabled_plugins.retain(|existing| existing != id);
        if enabled {
            config.enabled_plugins.push(id.to_string());
        } else {
            config.disabled_plugins.push(id.to_string());
        }
        crate::utils::save_config(&config);
        self.refresh_plugin_snapshot();
    }

    /// Dibuja el asistente como panel independiente fuera del workspace 3D.
    pub(crate) fn draw_assistant(&mut self, ctx: &egui::Context, reserved_bottom_height: f32) {
        self.sync_assistant_for_frame(ctx);
        if !self.assistant_visible {
            self.cancel_stale_model_request();
            return;
        }
        let visuals = self.assistant_visuals(ctx);
        if let Some(action) = grafito_ui::assistant::draw_assistant_panel(
            ctx,
            &mut self.assistant,
            reserved_bottom_height,
            visuals,
            &mut self.assistant_blocks_cache,
        ) {
            self.handle_assistant_action(ctx, action);
        }
        self.cancel_stale_model_request();
    }

    /// Dibuja el asistente dentro del dock de Geometry 3D ya reservado por el
    /// shell. La sincronización de trabajos ocurre antes de dibujar las tabs.
    pub(crate) fn draw_assistant_contents_in_workspace_dock(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) {
        if !self.assistant_visible {
            ui.label(
                egui::RichText::new("El asistente esta oculto.")
                    .color(grafito_ui::theme::current_theme(ctx).text_secondary),
            );
            if ui.button("Mostrar asistente").clicked() {
                self.assistant_visible = true;
            }
            self.cancel_stale_model_request();
            return;
        }
        let visuals = self.assistant_visuals(ctx);

        let mut action = grafito_ui::assistant::draw_assistant_contents(
            ui,
            &mut self.assistant,
            visuals,
            &mut self.assistant_blocks_cache,
        );
        if action.is_none() {
            action =
                grafito_ui::assistant::draw_assistant_settings_window(ctx, &mut self.assistant);
        }
        if let Some(action) = action {
            self.handle_assistant_action(ctx, action);
        }
        self.cancel_stale_model_request();
    }

    pub(crate) fn handle_assistant_action(
        &mut self,
        ctx: &egui::Context,
        action: AssistantUiAction,
    ) {
        // F3.2 PedagogyDispatcher — integración OpenCode Go
        // -------------------------------------------------
        // Las 6 tools pedagógicas (scaffold, generate_exercise, assess_answer,
        // get_curriculum, suggest_next, generate_animation) ya están expuestas al LLM
        // vía `grafito_assistant::default_agent_tools()` → `agent::all_safe_tool_schemas()`
        // y despachadas por `SafeGrafitoDispatcher` / `PedagogyDispatcher` (puras, sin Document).
        // El modo agente (AssistantAgentJob, start_agent_assistant_job) las orquesta vía
        // OpenCode Go (OpenAI-compatible tool calling) sin salir del chat.
        //
        // assistant_tool_catalog (grafito_command) sigue siendo el catálogo de comandos
        // verificados de Grafito; las pedagógicas viajan como OpenAI tools, no como fences.
        //
        // TODO(F3.2-inline): si se quiere invocación directa sin LLM (ej. botón "Andamiar"
        // o comando /scaffold), cablear aquí un match que llame a
        // `grafito_assistant::agent::PedagogyDispatcher.dispatch(&call)` con un ToolCall
        // construido localmente y renderice el ToolResult en el chat. Hoy la orquestación
        // pedagógica pasa exclusivamente por el loop agente cuando `assistant.agent_mode==true`.
        match action {
            AssistantUiAction::Submit => {
                let problem_clone = self.assistant.problem.clone();
                let lower = problem_clone.to_lowercase();
                // La animación la genera EL ASISTENTE y vive DENTRO del turno
                // del chat (transcript con `AssistantMedia`): heurística
                // honesta sobre el texto (`anim_ui`, solo lectura de ui).
                let wants_animation = crate::anim_ui::wants_animation_request(&problem_clone);
                // Cancela animación previa si existe — evita crash al pedir otra cosa tras animación
                // y evita "tomo una ya hecha" (stale derivative). Cancel real:
                // señala el token, el hilo descarta.
                if self.assistant_runtime.cancel_anim_job() {
                    self.assistant.anim_progress = false;
                }
                // Pedido ambiguo: turno honesto sin media ni hilo, jamás inventa.
                if wants_animation {
                    if let Err(guidance) =
                        crate::anim_ui::animation_concept_from_request(&problem_clone)
                    {
                        let question = problem_clone.clone();
                        self.assistant.begin_request(question);
                        self.assistant.problem.clear();
                        let honest = grafito_ui::assistant::humanize_prose_text(&guidance);
                        self.assistant.complete_local_request(honest.clone());
                        self.assistant.set_media(None, ctx);
                        self.notify(honest, ToastKind::Info);
                        ctx.request_repaint();
                        return;
                    }
                }
                // Heurística de memoria: si el usuario pide recordar o expresa preferencia, guardarlo
                if lower.contains("recuerda que")
                    || lower.contains("prefiero")
                    || lower.contains("me gusta")
                    || lower.contains("no me gusta")
                    || lower.contains("soy ")
                {
                    let epoch = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let fact_text = if let Some(pos) = lower.find("recuerda que") {
                        problem_clone[pos + "recuerda que".len()..]
                            .trim()
                            .chars()
                            .take(100)
                            .collect::<String>()
                    } else {
                        problem_clone.chars().take(100).collect::<String>()
                    };
                    if !fact_text.trim().is_empty() {
                        self.profile.remember_fact(&fact_text, epoch, 0.9);
                        self.assistant.long_memory = self.profile.long_memory.clone();
                        // I/O en background thread para no bloquear UI (60fps)
                        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                    }
                }
                self.start_local_assistant_request(ctx);
                if wants_animation {
                    // Concepto ya validado (no ambiguo): plantilla honesta por
                    // concepto + hilo cancelable que instala `AssistantMedia`
                    // DEL TURNO (transcript, auto-preview inline).
                    let concepto = crate::anim_ui::animation_concept_from_request(&problem_clone)
                        .unwrap_or_else(|_| problem_clone.clone());
                    let plantilla =
                        crate::anim_native::detect_template_for_concept(&concepto).to_string();
                    // La prosa del turno referencia la animación con nombres
                    // humanos (mapa `humanize_control_name` de ui, solo
                    // lectura): jamás IDs literales.
                    if let Some(turno) = self.assistant.conversation.last_mut() {
                        if turno.role == ConversationRole::Assistant
                            && !turno.content.contains("deslizador")
                        {
                            turno.content.push_str("\n\n");
                            turno
                                .content
                                .push_str(crate::anim_ui::animation_reference_sentence());
                        }
                    }
                    self.run_assistant_animation_with(ctx, &plantilla, &concepto);
                }
            }
            AssistantUiAction::AuthorizeRemote => {
                self.start_authorized_remote_assistant_request(ctx)
            }
            AssistantUiAction::CancelRemoteAuthorization => {
                self.assistant.cancel_remote_authorization();
            }
            AssistantUiAction::Cancel => self.cancel_assistant_request(),
            AssistantUiAction::SaveApiKey => self.save_assistant_api_key(),
            AssistantUiAction::LoadApiKey => self.load_assistant_api_key(),
            AssistantUiAction::ClearApiKey => self.clear_assistant_api_key(),
            AssistantUiAction::ProviderChanged => {
                self.assistant_runtime.forget_key();
                self.assistant_runtime.fallback_model = None;
                self.cancel_stale_remote_request();
                self.cancel_stale_model_request();
                self.save_app_config();
            }
            AssistantUiAction::ModelChanged => {
                self.assistant_runtime.fallback_model = None;
                self.cancel_stale_remote_request();
                self.save_app_config();
            }
            AssistantUiAction::FusionFallbackChanged => self.save_app_config(),
            AssistantUiAction::RefreshModels => self.start_model_request(ctx),
            AssistantUiAction::AttachImage => self.attach_assistant_image(),
            AssistantUiAction::RemoveAttachment(index) => {
                self.assistant.remove_attachment(index);
            }
            AssistantUiAction::InsertCommand(candidate_index) => {
                let Some(command) = self
                    .assistant
                    .verified_proposals
                    .iter()
                    .find(|proposal| proposal.candidate_index == candidate_index)
                    .and_then(|verified| {
                        verified.prerequisite_parameters.is_empty().then(|| {
                            match &verified.proposal {
                                AssistantProposal::Command(command) => Some(command.clone()),
                                _ => None,
                            }
                        })?
                    })
                else {
                    self.reject_assistant_command();
                    return;
                };
                if let Some(view) = assistant_graph_view(&command) {
                    if let Some(perspective) = assistant_graph_perspective(view, self.current_view)
                    {
                        self.set_perspective(perspective);
                    }
                    self.ensure_algebra_panel_visible();
                }
                self.input_text = command.canonical_text();
                self.command_input_focus_requested = true;
                self.notify(
                    "Comando preparado en la entrada. Revisalo antes de ejecutarlo.",
                    ToastKind::Info,
                );
                ctx.request_repaint();
            }
            AssistantUiAction::ApplyProposal(candidate_index) => {
                let Some(verified) = self
                    .assistant
                    .verified_proposals
                    .iter()
                    .find(|proposal| proposal.candidate_index == candidate_index)
                    .cloned()
                else {
                    self.reject_assistant_command();
                    return;
                };
                // Si es GenerateAnimation, además de aplicar el comando, dispara el motor de animación
                let is_generate_animation = matches!(
                    &verified.proposal,
                    AssistantProposal::Command(cmd) if cmd.canonical_name() == "GenerateAnimation"
                );
                let generate_args = if is_generate_animation {
                    if let AssistantProposal::Command(cmd) = &verified.proposal {
                        let args = cmd.arguments();
                        Some((args.first().cloned(), args.get(1).cloned()))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let committed = match verified.proposal {
                    // Revalidate simple proposals against the live document at the explicit
                    // click, so a valid card never depends on a stale response-time cache.
                    AssistantProposal::Command(command) => self
                        .apply_verified_assistant_graph_command(
                            &command,
                            &verified.prerequisite_parameters,
                        ),
                    AssistantProposal::Scene(commands) => self.apply_verified_assistant_scene(
                        &commands,
                        &verified.prerequisite_parameters,
                    ),
                    AssistantProposal::Parameter(ref assignment) => {
                        if preflight_assistant_parameter(&self.document, assignment).is_ok() {
                            let command = assignment.canonical_text();
                            let outcome = self.execute_command_and_record(&command, self.ui_time);
                            !matches!(outcome, grafito_command::commands::CommandOutcome::Error(_))
                        } else {
                            self.reject_assistant_command();
                            false
                        }
                    }
                };
                self.assistant
                    .finish_verified_proposal_application(candidate_index, committed);
                if committed {
                    self.ensure_algebra_panel_visible();
                }
                if is_generate_animation && committed {
                    if let Some((template_opt, concept_opt)) = generate_args {
                        let template = template_opt
                            .as_deref()
                            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').trim())
                            .filter(|s| !s.is_empty())
                            .unwrap_or("derivative-slope");
                        let concept = concept_opt
                            .as_deref()
                            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').trim())
                            .unwrap_or("");
                        // El agente propuso `generate_animation` (vía
                        // `GenerateAnimation` verificado): el hilo genera e
                        // incrusta como `AssistantMedia` DEL TURNO.
                        if let Some(turno) = self.assistant.conversation.last_mut() {
                            if turno.role == ConversationRole::Assistant
                                && !turno.content.contains("deslizador")
                            {
                                turno.content.push_str("\n\n");
                                turno
                                    .content
                                    .push_str(crate::anim_ui::animation_reference_sentence());
                            }
                        }
                        self.run_assistant_animation_with(ctx, template, concept);
                    } else {
                        self.run_assistant_animation(ctx);
                    }
                }
            }
            AssistantUiAction::ApplyProposedPlan => self.apply_proposed_assistant_plan(),
            AssistantUiAction::RetryProposalCorrection => {
                self.request_assistant_proposal_correction(ctx);
            }
            // S2 `ask_user` real vía evento: la tool devolvió el pendiente, la
            // UI lo mostró como botones y la respuesta vuelve al loop como
            // `function_call_output`/mensaje en un nuevo job (nunca bloquea).
            AssistantUiAction::AskClarification { .. } => {
                // Staging vía `stage_clarification` (el worker ya terminó);
                // esta variante solo existe para contrato/tests, sin I/O.
            }
            AssistantUiAction::AnswerClarification { call_id, answer } => {
                self.answer_pending_clarification(ctx, &call_id, &answer);
            }
            AssistantUiAction::DismissClarification => {
                self.assistant.clear_pending_clarification();
                self.notify(
                    "Aclaración descartada. Escribí la respuesta en el chat si querés.",
                    ToastKind::Info,
                );
                ctx.request_repaint();
            }
            AssistantUiAction::ClearConversation => {
                self.assistant.clear_conversation();
            }
            AssistantUiAction::HidePanel => {
                self.assistant_visible = false;
                if self.perspective == crate::Perspective::Geometry3D {
                    self.workspace_dock_tab = crate::WorkspaceDockTab::Inspector;
                }
                ctx.request_repaint();
            }
            AssistantUiAction::CopyMessage(message) => {
                ctx.copy_text(message);
                self.notify("Mensaje copiado.", ToastKind::Info);
            }
            AssistantUiAction::TogglePlugin(id, enabled) => {
                self.toggle_assistant_plugin(&id, enabled)
            }
            AssistantUiAction::FullPermissionChanged(_) => {
                self.save_app_config();
            }
            AssistantUiAction::AgentModeChanged(_) => {
                self.save_app_config();
            }
            AssistantUiAction::RunAnimation => self.run_assistant_animation(ctx),
            AssistantUiAction::ExportMedia => self.export_assistant_media(ctx),
            AssistantUiAction::AskNextTopic => {
                let memory = self.profile.memory();
                self.assistant.problem = format!(
                    "Soy nivel {} de Grafito. Mi progreso: {memory} ¿Qué debería estudiar a continuación y cómo?",
                    self.profile.level
                );
                self.start_local_assistant_request(ctx);
            }
            AssistantUiAction::LearnCorrect => self.record_learning(true),
            AssistantUiAction::LearnIncorrect => self.record_learning(false),
            AssistantUiAction::RunMiniExam => self.run_mini_exam(ctx),
            AssistantUiAction::OpenMascotConfig => {
                self.assistant.settings_open = true;
                self.assistant.config_tab = 1;
                self.show_mascot_config = false;
                // Sincroniza borrador al abrir para preview fiel
                self.assistant.avatar = self.profile.avatar.clone();
                self.assistant.user_name = self.profile.display_name().to_owned();
                self.assistant.long_memory = self.profile.long_memory.clone();
                self.avatar_draft = self.profile.avatar.clone();
            }
            AssistantUiAction::SaveAvatar => {
                let draft = self.assistant.avatar.clone();
                // Sincronizar preferencias de memoria con avatar
                let mut long_mem = self.assistant.long_memory.clone();
                long_mem.preferences.custom_instructions = draft.custom_instructions.clone();
                long_mem.preferences.language = draft.language.clone();
                // tone viene de mascot personality
                if let Some(m) = draft.mascot.as_ref() {
                    long_mem.preferences.tone = m.personality.label().to_string();
                }
                match draft
                    .validate()
                    .and_then(|_| long_mem.preferences.validate())
                {
                    Ok(()) => {
                        let name = self.assistant.user_name.clone();
                        let name_ref = if name.trim().is_empty() {
                            "Estudiante"
                        } else {
                            name.trim()
                        };
                        match self.profile.set_display_name(name_ref) {
                            Ok(()) => {
                                self.profile.avatar = draft.clone();
                                // Preservar mascota: sincronizar personality si cambió
                                if let Some(mascot) = draft.mascot.clone() {
                                    self.profile.avatar.mascot = Some(mascot.clone());
                                    self.profile.mascot = Some(mascot);
                                } else if self.profile.mascot.is_some() {
                                    // mantener existente
                                }
                                self.profile.long_memory = long_mem.clone();
                                self.avatar_draft = self.profile.avatar.clone();
                                self.assistant.avatar = self.profile.avatar.clone();
                                self.assistant.user_name = self.profile.display_name().to_owned();
                                self.assistant.long_memory = self.profile.long_memory.clone();
                                self.config_name_error = None;
                                spawn_profile_save(
                                    self.profile.clone(),
                                    crate::utils::profile_path(),
                                );
                                self.notify(
                                    "Avatar y memoria guardados",
                                    grafito_ui::toast::ToastKind::Success,
                                );
                                self.assistant.settings_open = false;
                            }
                            Err(err) => {
                                self.config_name_error = Some(err.clone());
                                self.notify(err, grafito_ui::toast::ToastKind::Error);
                            }
                        }
                    }
                    Err(err) => {
                        self.config_name_error = Some(err.clone());
                        self.notify(err, grafito_ui::toast::ToastKind::Error);
                    }
                }
            }
            AssistantUiAction::LiveSaveAvatar => {
                // Guardado live sin cerrar ventana — para cambios de color/forma inmediatos
                let draft = self.assistant.avatar.clone();
                let mut long_mem = self.assistant.long_memory.clone();
                long_mem.preferences.custom_instructions = draft.custom_instructions.clone();
                long_mem.preferences.language = draft.language.clone();
                if let Some(m) = draft.mascot.as_ref() {
                    long_mem.preferences.tone = m.personality.label().to_string();
                }
                if draft.validate().is_ok() && long_mem.preferences.validate().is_ok() {
                    let name = self.assistant.user_name.clone();
                    let name_ref = if name.trim().is_empty() {
                        "Estudiante"
                    } else {
                        name.trim()
                    };
                    if self.profile.set_display_name(name_ref).is_ok() {
                        self.profile.avatar = draft.clone();
                        if let Some(mascot) = draft.mascot.clone() {
                            self.profile.avatar.mascot = Some(mascot.clone());
                            self.profile.mascot = Some(mascot);
                        }
                        self.profile.long_memory = long_mem.clone();
                        self.avatar_draft = self.profile.avatar.clone();
                        // No cerrar ventana, no toast ruidoso
                        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
                    }
                }
            }
            AssistantUiAction::ResetAvatar => {
                self.assistant.avatar = grafito_profile::AvatarConfig::default();
                self.assistant.avatar.display_name = "Estudiante".to_string();
                self.assistant.user_name = "Estudiante".to_string();
                self.avatar_draft = self.assistant.avatar.clone();
                self.config_name_error = None;
                self.notify(
                    "Avatar restablecido — pulsa Guardar para confirmar",
                    grafito_ui::toast::ToastKind::Info,
                );
            }
            AssistantUiAction::ApplyRawCommand(raw) => {
                // Extrae el primer comando grafito limpio hasta el corchete final, sin texto trailing
                // (evita "no se permite texto después del corchete final" cuando el LLM añade "explicacion" tras el comando)
                let mut command_text = raw.trim().to_string();
                if command_text.is_empty() {
                    self.reject_assistant_command();
                    return;
                }
                // Si el bloque contiene múltiples líneas (grafito-scene), intentar quedarnos con todas las líneas que parezcan comando
                // Para bloque simple, quedarnos solo hasta el primer ']' balanceado
                let trimmed = command_text.trim();
                if trimmed.lines().count() == 1 {
                    if let Some(open) = trimmed.find('[') {
                        let mut depth = 0i32;
                        let mut close_idx: Option<usize> = None;
                        for (i, ch) in trimmed[open..].char_indices() {
                            if ch == '[' {
                                depth += 1;
                            } else if ch == ']' {
                                depth -= 1;
                                if depth == 0 {
                                    close_idx = Some(open + i);
                                    break;
                                }
                            }
                        }
                        if let Some(close) = close_idx {
                            // Solo si hay texto no-espacio después del cierre, recortar
                            if !trimmed[close + 1..].trim().is_empty() {
                                // Verificar que el prefijo sea un comando válido antes de recortar
                                let candidate = trimmed[..=close].trim();
                                if grafito_command::assistant_proposals::parse_assistant_command(
                                    candidate,
                                )
                                .is_some()
                                {
                                    command_text = candidate.to_string();
                                }
                            }
                        }
                    }
                } else {
                    // Multi-línea: filtrar solo líneas que son comandos completos
                    let filtered: Vec<String> = trimmed
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| {
                            !l.is_empty()
                                && grafito_command::assistant_proposals::parse_assistant_command(l)
                                    .is_some()
                        })
                        .map(|s| s.to_string())
                        .collect();
                    if !filtered.is_empty() {
                        command_text = filtered.join("\n");
                    }
                }
                // Intenta parsear como comando de grafito para manejar vista 2D/3D/4D
                if let Some(inv) =
                    grafito_command::assistant_proposals::parse_assistant_command(&command_text)
                {
                    if let Some(view) = assistant_graph_view(&inv) {
                        if let Some(perspective) =
                            assistant_graph_perspective(view, self.current_view)
                        {
                            self.set_perspective(perspective);
                        }
                    }
                } else if command_text.lines().count() > 1 {
                    // Para escena multi-línea, inferir vista de la primera línea válida
                    if let Some(first) = command_text.lines().next() {
                        if let Some(inv) =
                            grafito_command::assistant_proposals::parse_assistant_command(
                                first.trim(),
                            )
                        {
                            if let Some(view) = assistant_graph_view(&inv) {
                                if let Some(perspective) =
                                    assistant_graph_perspective(view, self.current_view)
                                {
                                    self.set_perspective(perspective);
                                }
                            }
                        }
                    }
                }
                let outcome = self.execute_command_and_record(&command_text, self.ui_time);
                match &outcome {
                    grafito_command::commands::CommandOutcome::Error(msg) => {
                        // S3: ante fence/propuesta inválida, responde con
                        // `vibecoder_explain` (negocio en rioplatense) + fix
                        // propuesto en 1 click cuando sea seguro (solo
                        // reescritura sintáctica, jamás cambio semántico
                        // silencioso). El fix queda en la entrada para que el
                        // usuario lo revise y lo aplique explícitamente.
                        let (explained, fix) =
                            grafito_assistant::agent::explain_invalid_proposal(&command_text, "");
                        if let Some(fix) = fix {
                            self.input_text = fix.clone();
                            self.command_input_focus_requested = true;
                            self.show_assistant_error(format!(
                                "{} Te propongo un fix sintáctico en la entrada: {} Revisalo y aplicá.",
                                explained.explanation, fix
                            ));
                        } else {
                            self.show_assistant_error(format!("{} ({msg})", explained.explanation));
                        }
                    }
                    _ => {
                        self.ensure_algebra_panel_visible();
                        self.notify("Comando aplicado en Grafito.", ToastKind::Success);
                    }
                }
                ctx.request_repaint();
            }
            AssistantUiAction::ExplainStepwise(topic) => {
                // teaching_ui.start ya inicia el orchestrator con el template del primer paso
                self.teaching_ui.start(&topic);
                self.notify(format!("Enseñanza iniciada: {topic}"), ToastKind::Info);
            }
        }
    }

    fn show_assistant_error(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.assistant.error = Some(error.clone());
        self.notify(error, ToastKind::Error);
    }

    fn reject_assistant_command(&mut self) {
        self.show_assistant_error(
            "La sugerencia remota está incompleta o no es una acción de Grafito permitida.",
        );
    }

    fn fail_assistant_request(&mut self, error: impl Into<String>) {
        let error = error.into();
        let current_model = self.assistant.model.clone();
        let visible_error = remote_error_message(&error, &current_model);
        self.assistant.fail_request(visible_error.clone());
        self.report_assistant_error(visible_error);
    }

    fn fail_assistant_repair_request(&mut self, error: impl Into<String>) {
        self.assistant.restore_proposal_correction();
        self.fail_assistant_request(error);
    }

    fn report_assistant_error(&mut self, error: impl Into<String>) {
        self.show_assistant_error(error);
    }

    fn apply_verified_assistant_graph_command(
        &mut self,
        command: &AssistantCommandInvocation,
        prerequisite_parameters: &[AssistantParameterAssignment],
    ) -> bool {
        let before = self.object_labels_snapshot();
        let before_ids = self
            .document
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let command_text = command.canonical_text();
        match preflight_assistant_graph_command_with_prerequisites(
            &self.document,
            prerequisite_parameters,
            command,
            self.camera,
        ) {
            Ok(preflight) => {
                let view = preflight.view;
                let outcome = commit_assistant_graph_preflight(
                    &mut self.document,
                    &mut self.undo_stack,
                    &mut self.redo_stack,
                    preflight,
                );
                let committed = !matches!(
                    &outcome,
                    grafito_command::commands::CommandOutcome::Error(_)
                );
                self.handle_command_outcome(outcome, self.ui_time, &command_text);
                self.record_step_from_diff(&command_text, &before, true);
                if let Some(perspective) = assistant_graph_perspective(view, self.current_view) {
                    self.set_perspective(perspective);
                }
                if committed {
                    // S1 auto-graficar 1-click: el objeto queda seleccionado y
                    // visible. 2D encuadra con `zoom_to_fit` (el preflight ya
                    // garantizó geometría visible); 3D queda visible con la
                    // cámara actual (el preflight lo verificó) + selección.
                    self.select_new_assistant_objects(&before_ids);
                    if view == grafito_command::assistant_context::AssistantGraphView::TwoD {
                        self.zoom_to_fit();
                    }
                    self.ensure_algebra_panel_visible();
                }
                committed
            }
            Err(error) => {
                self.show_assistant_error(error);
                false
            }
        }
    }

    fn apply_verified_assistant_scene(
        &mut self,
        commands: &[AssistantCommandInvocation],
        prerequisite_parameters: &[AssistantParameterAssignment],
    ) -> bool {
        let before = self.object_labels_snapshot();
        let before_ids = self
            .document
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        match preflight_assistant_scene_with_prerequisites(
            &self.document,
            prerequisite_parameters,
            commands,
            self.camera,
        ) {
            Ok(preflight) => {
                let view = preflight.view;
                let before_document = self.document.clone();
                self.document = preflight.staged;
                crate::app::save_command_snapshot_if_mutated(
                    &preflight.outcome,
                    before_document,
                    &self.document,
                    &mut self.undo_stack,
                    &mut self.redo_stack,
                );
                self.camera = preflight.camera;
                self.handle_command_outcome(
                    preflight.outcome,
                    self.ui_time,
                    "Escena 3D verificada",
                );
                self.record_step_from_diff("Escena 3D verificada", &before, true);
                if let Some(perspective) = assistant_graph_perspective(view, self.current_view) {
                    self.set_perspective(perspective);
                }
                // S1: selección + encuadre (3D ya viene con cámara fitted del
                // preflight; 2D homogénea encuadra con zoom_to_fit).
                self.select_new_assistant_objects(&before_ids);
                if view == grafito_command::assistant_context::AssistantGraphView::TwoD {
                    self.zoom_to_fit();
                }
                self.ensure_algebra_panel_visible();
                self.notify(
                    "Escena verificada aplicada y encuadrada.",
                    ToastKind::Success,
                );
                true
            }
            Err(error) => {
                self.show_assistant_error(error);
                false
            }
        }
    }

    /// Selecciona los objetos nuevos tras un Apply (S1 auto-graficar 1-click).
    ///
    /// Puro en intención (solo `selected_object`, sin Document ni I/O): difiere
    /// `before_ids` del documento actual y fija el máximo (determinista por
    /// `Ord`) como seleccionado para que quede visible en Álgebra y canvas.
    /// Sin nuevos → conserva la selección previa (default honesto, no inventa).
    fn select_new_assistant_objects(
        &mut self,
        before_ids: &std::collections::HashSet<grafito_core::ObjectId>,
    ) {
        let mut newest: Option<grafito_core::ObjectId> = None;
        for (id, _) in self.document.objects_iter() {
            if before_ids.contains(id) {
                continue;
            }
            newest = Some(match newest {
                None => *id,
                Some(current) => current.max(*id),
            });
        }
        if let Some(id) = newest {
            self.selected_object = Some(id);
        }
    }

    /// Retoma una aclaración pendiente (S2) con la respuesta del usuario.
    ///
    /// No bloquea threads: consume el pendiente y lanza un nuevo job local con
    /// la respuesta como `function_call_output`/mensaje para que el loop la
    /// retome. Si el `call_id` no coincide o la respuesta está vacía, se
    /// descarta sin I/O y con aviso honesto en rioplatense.
    fn answer_pending_clarification(&mut self, ctx: &egui::Context, call_id: &str, answer: &str) {
        let pending = self.assistant.pending_clarification().cloned();
        let Some(pending) = pending else {
            self.notify("No hay aclaración pendiente.", ToastKind::Info);
            return;
        };
        if pending.call_id != call_id {
            self.assistant.clear_pending_clarification();
            self.notify(
                "La aclaración quedó obsoleta; volvé a preguntar si la necesitás.",
                ToastKind::Info,
            );
            ctx.request_repaint();
            return;
        }
        let Some(sanitized) = self
            .assistant
            .take_clarification_answer(call_id, answer)
            .filter(|text| !text.trim().is_empty())
        else {
            self.notify(
                "Escribí una respuesta no vacía para continuar.",
                ToastKind::Info,
            );
            return;
        };
        // La respuesta vuelve al loop como mensaje + `function_call_output`
        // (Responses) para trazabilidad; el nuevo job local la retoma sin
        // bloquear la UI (I/O solo en background, como el resto de jobs).
        let output = serde_json::json!({
            "type": "function_call_output",
            "call_id": pending.call_id,
            "output": sanitized,
        });
        self.assistant.problem = format!(
            "Aclaración ({}): {}\nRespuesta: {}",
            pending.question, output, sanitized
        );
        self.notify(
            "Aclaración respondida. Retomo la consulta…",
            ToastKind::Info,
        );
        self.start_local_assistant_request(ctx);
        ctx.request_repaint();
    }

    /// Drena aclaraciones `ask_user` del canal lateral (S2, sin bloquear).
    ///
    /// Hilo UI, `try_recv` acotado (máx 4 por frame): si hay pendiente nuevo y
    /// no hay otro visible, lo estagia para mostrar botones en el turno. Nunca
    /// bloquea threads ni toca la zona guard (el forwarder ya parseó en
    /// background).
    fn drain_pending_clarifications(&mut self) {
        let Some(job) = self.assistant_runtime.agent_job.as_ref() else {
            return;
        };
        for _ in 0..4 {
            match job.clarification_receiver.try_recv() {
                Ok(pending) => {
                    if !self.assistant.has_pending_clarification() {
                        self.assistant.stage_clarification(pending);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    fn save_assistant_api_key(&mut self) {
        let key = std::mem::take(&mut self.assistant.api_key_draft);
        let trimmed = key.trim().to_owned();
        if trimmed.is_empty() {
            self.show_assistant_error("Ingresá una API key antes de guardarla.");
            return;
        }
        match assistant_credentials::store(self.assistant.provider, &trimmed) {
            Ok(()) => {
                // Una relectura posterior del llavero no debe invalidar la
                // consulta durante la misma sesión en que se guardó la clave.
                self.assistant_runtime
                    .remember_key(self.assistant.provider, trimmed);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.assistant_runtime
                    .remember_key(self.assistant.provider, trimmed);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
        }
    }

    fn load_assistant_api_key(&mut self) {
        match assistant_credentials::load(self.assistant.provider) {
            Ok(Some(key)) => {
                self.assistant_runtime
                    .remember_key(self.assistant.provider, key);
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
            }
            Ok(None) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                self.show_assistant_error("No se pudo consultar el llavero del sistema.");
            }
        }
    }

    fn clear_assistant_api_key(&mut self) {
        self.assistant_runtime.forget_key();
        match assistant_credentials::clear(self.assistant.provider) {
            Ok(()) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
            }
            Err(_) => {
                self.show_assistant_error("No se pudo eliminar la clave guardada.");
            }
        }
    }

    fn attach_assistant_image(&mut self) {
        if self.assistant.is_pending || self.assistant_runtime.image_job.is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Imagen", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let (sender, receiver) = sync_channel(1);
        std::thread::spawn(move || {
            let _ = sender.send(load_assistant_attachment(path));
        });
        self.assistant.is_importing_image = true;
        self.assistant.attachment_message = Some("Importando imagen...".into());
        self.assistant_runtime.image_job = Some(AssistantImageJob { receiver });
    }

    fn build_remote_assistant_request(
        &self,
        question: String,
        document_context: ImmutableDocumentContext,
        focus: Option<AssistantFocus>,
        attachments: Vec<grafito_assistant_types::ImageAttachment>,
        image_upload_consent: bool,
        repair: Option<AssistantRepairRequest>,
    ) -> Result<AssistantRequest, String> {
        let (repair_feedback, history_before_turn) = match repair {
            Some(AssistantRepairRequest {
                feedback,
                target_turn,
            }) => (Some(feedback), Some(target_turn)),
            None => (None, None),
        };
        let mut request = AssistantRequest::remote(question.clone(), document_context);
        // Idioma del selector del panel (auto/es/en) → directiva en el system prompt.
        request.language = self.assistant.avatar.language.clone();
        request.focus = focus;
        let plugin_instructions = self.plugin_instructions_budgeted();
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
        system.push_str(&format!(
            "[Perfil del estudiante]\n{}",
            self.profile.memory()
        ));
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
            Some(target_turn) => self
                .assistant
                .conversation_before_turn_within_budget(target_turn, history_budget),
            None => self.assistant.conversation_within_budget(history_budget),
        };
        request.attachments = attachments;
        request.image_upload_consent = image_upload_consent;
        request.repair_feedback = repair_feedback;
        request.validate(&AttachmentLimits::default())?;
        Ok(request)
    }

    fn start_remote_assistant_job(&mut self, ctx: &egui::Context, launch: AssistantRemoteLaunch) {
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
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let id = self.assistant_runtime.next_request_id;
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
        let repaint = ctx.clone();
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
        self.assistant_runtime.remote_job = Some(AssistantRemoteJob {
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
        });
    }

    /// Lanza el modo agente (loop con herramientas seguras) en un hilo y
    /// enruta sus eventos de actividad + resultado hacia la UI.
    fn start_agent_assistant_job(&mut self, ctx: &egui::Context, launch: AssistantRemoteLaunch) {
        let settings = launch.settings;
        let request = launch.request;
        let api_key = launch.api_key;
        let provider = launch.provider;
        let model = launch.model;
        let question = launch.question;
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let _ = self.assistant_runtime.next_request_id;
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
        let repaint = ctx.clone();
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
                        if sender.send(AgentChannelMsg::Event(event)).is_err() {
                            break;
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
            let _ = sender.send(AgentChannelMsg::Done(outcome));
            repaint.request_repaint();
        });
        self.assistant_runtime.agent_job = Some(AssistantAgentJob {
            provider,
            model,
            cancellation,
            receiver,
            clarification_receiver,
        });
    }

    /// Genera una animación didáctica con el motor externo y la reproduce en el chat.
    /// Genera y envía un mini-examen (3 preguntas) de la rama recomendada.
    fn run_mini_exam(&mut self, ctx: &egui::Context) {
        let branch = self
            .profile
            .recommend_next()
            .first()
            .cloned()
            .map(|branch| (branch.id.clone(), branch.name.clone()))
            .or_else(|| {
                self.profile
                    .branches
                    .first()
                    .map(|branch| (branch.id.clone(), branch.name.clone()))
            });
        let (id, name) = branch.unwrap_or_else(|| ("algebra".to_string(), "Álgebra".to_string()));
        let questions = grafito_profile::exam::mini_exam_questions(&id);
        let mut prompt = format!("Tomame un mini-examen de {name} (rama {id}). Preguntas:\n\n");
        for (index, question) in questions.iter().enumerate() {
            prompt.push_str(&format!("{}. {question}\n", index + 1));
        }
        prompt.push_str("\nRespondé una por una y al final corregime cada una.");
        self.assistant.problem = prompt;
        self.start_local_assistant_request(ctx);
    }

    /// Muestras (0..=1) para el sparkline de evolución de dominio.
    fn domain_sparkline(&self) -> Vec<f32> {
        Self::domain_sparkline_from(&self.profile)
    }

    /// Clasifica el contenido de la última explicación en una rama del plan.
    fn learning_branch(&self) -> (&'static str, &'static str) {
        let text = self
            .assistant
            .latest_assistant_text()
            .unwrap_or_default()
            .to_lowercase();
        for (needle, id, name) in [
            ("deriv", "calculus", "Cálculo"),
            ("integral", "calculus", "Cálculo"),
            ("límite", "calculus", "Cálculo"),
            ("ecuación", "algebra", "Álgebra"),
            ("polinom", "algebra", "Álgebra"),
            ("función", "functions", "Funciones"),
            ("gráf", "functions", "Funciones"),
            ("trigonometr", "trigonometry", "Trigonometría"),
            ("geom", "geometry", "Geometría"),
            ("estadíst", "stats", "Estadística"),
            ("complej", "complex", "Complejos"),
            ("fractal", "complex", "Complejos"),
        ] {
            if text.contains(needle) {
                return (id, name);
            }
        }
        ("general", "General")
    }

    /// Muestras 0..=1 del sparkline: histórico de la rama más trabajada,
    /// hacia atrás hasta 14 puntos (función pura y testeable).
    fn domain_sparkline_from(profile: &grafito_profile::StudentProfile) -> Vec<f32> {
        profile
            .branches
            .iter()
            .max_by_key(|branch| branch.domain_history.len())
            .map(|branch| {
                branch
                    .domain_history
                    .iter()
                    .rev()
                    .take(14)
                    .map(|entry| entry.1.clamp(0.0, 1.0))
                    .collect::<Vec<f32>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Registra el feedback del usuario en la memoria del tutor y persiste.
    /// F5 inline: además alimenta WorkingMemory episódica para el tutor socrático.
    fn record_learning(&mut self, correct: bool) {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let (id, name) = self.learning_branch();
        self.profile.record_outcome(id, name, epoch, correct);
        // F5 WorkingMemory hook: cada intento alimenta la RAM episódica.
        // - correct: solo incrementa pasos (misconception vacía)
        // - incorrect: registra la rama como misconception para adaptación socrática
        let misconception = if correct { "" } else { id };
        self.profile.working_memory.record_attempt(misconception);
        self.profile.working_memory.set_topic(name);
        self.profile.working_memory.set_session_epoch(epoch);
        // Sincroniza vista inline del chat
        self.assistant.working_memory = self.profile.working_memory.clone();
        // I/O en background thread para no bloquear UI (60fps)
        spawn_profile_save(self.profile.clone(), crate::utils::profile_path());
        self.notify(
            if correct {
                "¡Bien! Registrado en tu progreso."
            } else {
                "Anotado: reforzamos ese tema."
            },
            ToastKind::Success,
        );
    }

    fn run_assistant_animation(&mut self, ctx: &egui::Context) {
        self.run_assistant_animation_with(ctx, "derivative-slope", "derivada como pendiente");
    }

    /// Exporta la animación visible en la card a GIF (B5, botón Exportar).
    ///
    /// Cablea al `spawn_gif_export` existente (hilo aparte, probado): la UI
    /// solo emitió `ExportMedia`, acá corre el trabajo fuera del draw.
    /// Jamás mudo:
    /// - export en curso → aviso y no se duplica;
    /// - sin animación o sin fotogramas → `Failed` en la card + aviso;
    /// - preflight (`check_gif_export_budget`: 64 frames / 8 Mpx / dims) →
    ///   motivo en la card + aviso;
    /// - el destino es temporal (`temp_dir`, nombre único por instante) y se
    ///   avisa la ruta al terminar; el delay sale de la velocidad de la card
    ///   (`gif_delay_for_rate` sobre `GIF_EXPORT_DELAY_CS`).
    fn export_assistant_media(&mut self, ctx: &egui::Context) {
        use grafito_ui::assistant::MediaExportState;
        if self.assistant_runtime.gif_export_job.is_some() {
            self.notify("Ya se está exportando la animación.", ToastKind::Info);
            return;
        }
        let frames = self
            .assistant
            .media
            .as_ref()
            .map_or_else(Vec::new, |media| media.frames.clone());
        if frames.is_empty() {
            self.assistant.set_media_export(MediaExportState::Failed(
                "todavía no hay fotogramas para exportar".into(),
            ));
            self.notify("No hay animación para exportar.", ToastKind::Error);
            ctx.request_repaint();
            return;
        }
        if let Err(budget) = crate::anim_native::check_gif_export_budget(&frames) {
            let reason = budget.to_string();
            self.assistant
                .set_media_export(MediaExportState::Failed(reason.clone()));
            self.notify(format!("No se pudo exportar: {reason}"), ToastKind::Error);
            ctx.request_repaint();
            return;
        }
        let frame_count = frames.len();
        let delay_cs = crate::anim_native::gif_delay_for_rate(
            crate::anim_native::GIF_EXPORT_DELAY_CS,
            self.assistant.media_playback_rate(),
        );
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("grafito_animacion_{stamp}_{frame_count}.gif"));
        let handle = crate::anim_native::spawn_gif_export(frames, path, delay_cs);
        self.assistant_runtime.gif_export_job = Some(GifExportJob {
            handle,
            frame_count,
        });
        self.assistant.set_media_export(MediaExportState::Exporting);
        ctx.request_repaint();
    }

    /// Drena el export a GIF sin bloquear (B5).
    ///
    /// Solo hace `join` si el hilo terminó (`is_finished`); publica el
    /// resultado en la card + aviso: éxito con ruta (verificando cota 5 MB
    /// post-escritura: si excede, se borra y es error honesto), o motivo del
    /// fallo. Se llama cada frame desde `sync_assistant_for_frame`.
    fn poll_gif_export_job(&mut self, ctx: &egui::Context) {
        use grafito_ui::assistant::MediaExportState;
        let finished = self
            .assistant_runtime
            .gif_export_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished());
        if !finished {
            return;
        }
        let Some(job) = self.assistant_runtime.gif_export_job.take() else {
            return;
        };
        match job.handle.join() {
            Ok(Ok(path)) => {
                let too_big = std::fs::metadata(&path)
                    .map(|metadata| metadata.len() > crate::anim_native::GIF_EXPORT_MAX_FILE_BYTES)
                    .unwrap_or(false);
                if too_big {
                    let _ = std::fs::remove_file(&path);
                    let reason = "el GIF supera 5 MB";
                    self.assistant
                        .set_media_export(MediaExportState::Failed(reason.into()));
                    self.notify(format!("No se pudo exportar: {reason}."), ToastKind::Error);
                } else {
                    self.assistant.set_media_export(MediaExportState::Done);
                    self.notify(
                        format!(
                            "Se exportaron {} fotogramas a {}.",
                            job.frame_count,
                            path.display()
                        ),
                        ToastKind::Success,
                    );
                }
            }
            Ok(Err(error)) => {
                let reason = error.to_string();
                self.assistant
                    .set_media_export(MediaExportState::Failed(reason.clone()));
                self.notify(
                    format!("No se pudo exportar la animación: {reason}."),
                    ToastKind::Error,
                );
            }
            Err(_) => {
                self.assistant.set_media_export(MediaExportState::Failed(
                    "la exportación terminó inesperadamente".into(),
                ));
                self.notify("La exportación terminó inesperadamente.", ToastKind::Error);
            }
        }
        ctx.request_repaint();
    }

    pub(crate) fn run_assistant_animation_with(
        &mut self,
        ctx: &egui::Context,
        template: &str,
        concept: &str,
    ) {
        // Si hay una animación en curso, cancelarla para regenerar la nueva (evita "tomo una ya hecha").
        // Cancel real (AS4): señala el token antes de dropear, el hilo descarta.
        if self.assistant_runtime.cancel_anim_job() {
            self.assistant.anim_progress = false;
        }
        // No destruir texturas durante el draw (evita wgpu panic 'Texture has been destroyed').
        // La media previa se mantiene visible hasta que la nueva la reemplace en sync_assistant_for_frame
        // (inicio del próximo frame). Solo limpiar si es la primera vez o si el usuario lo pidió explícitamente.
        let _ = ctx;
        // Motor externo si está configurado; si no (o si falla), se usa la
        // animación nativa de Rust para que «Animá» siempre produzca algo.
        let engine = self
            .plugin_registry
            .as_ref()
            .and_then(|registry| registry.engines().into_iter().next().cloned());
        let concept = if concept.is_empty() {
            self.assistant
                .focus
                .as_ref()
                .map(|focus| focus.summary.clone())
                .filter(|summary| !summary.is_empty())
                .unwrap_or_else(|| "derivada como pendiente".to_string())
        } else {
            concept.to_string()
        };
        let template = if template.is_empty() {
            "derivative-slope"
        } else {
            template
        };
        let work_dir = std::env::temp_dir().join(format!(
            "grafito_anim_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        // Sin I/O en UI: el workdir lo crea el hilo (`create_dir_all` abajo).
        let canvas = (720, 540);
        let resolution =
            grafito_anim::protocol::Resolution::try_new(canvas.0, canvas.1).unwrap_or_default();
        let duration = grafito_anim::protocol::AnimDuration::try_new(2.0).unwrap_or_default();
        let anim_params = grafito_anim::protocol::AnimParams {
            template: template.to_string(),
            concept: concept.clone(),
            params: std::collections::BTreeMap::new(),
            duration,
            resolution,
            export: grafito_anim::ExportFormat::Gif,
            spec: None,
        };
        let request = anim_params.into_request();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let repaint = ctx.clone();
        let template_owned = template.to_string();
        let concept_owned = concept.clone();
        std::thread::spawn(move || {
            // I/O solo en el hilo (nunca en UI): el workdir se crea aquí.
            let _ = std::fs::create_dir_all(&work_dir);
            if worker_cancellation.is_cancelled() {
                let _ = sender.send(Err(
                    "La generación se canceló antes de completarse.".to_string()
                ));
                repaint.request_repaint();
                return;
            }
            // Nativo cancelable (AS4): paramétrico si hay `ParametricAnim`, si
            // no el clásico. El render no acepta token: el closure de progreso
            // lo chequea entre frames y el hilo descarta el resultado rancio.
            let render_native_cancellable =
                || -> Result<grafito_ui::assistant::AssistantMedia, String> {
                    if let Some(anim) =
                        crate::anim_native::parametric_for_template(&template_owned, &concept_owned)
                    {
                        let mut saw_cancel = false;
                        let rendered = crate::anim_native::render_parametric_frames_with_progress(
                            &anim,
                            &mut |_, _| {
                                if worker_cancellation.is_cancelled() {
                                    saw_cancel = true;
                                }
                            },
                        );
                        if worker_cancellation.is_cancelled() || saw_cancel {
                            return Err(
                                "La generación se canceló antes de completarse.".to_string()
                            );
                        }
                        match rendered {
                            Ok(frames) => {
                                let title = format!("{} (paramétrica)", concept_owned);
                                Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    } else {
                        let mut saw_cancel = false;
                        let frames = crate::anim_native::render_anim_with_progress(
                            &template_owned,
                            &concept_owned,
                            480,
                            360,
                            &std::collections::BTreeMap::new(),
                            &mut |_, _| {
                                if worker_cancellation.is_cancelled() {
                                    saw_cancel = true;
                                }
                            },
                        );
                        if worker_cancellation.is_cancelled() || saw_cancel {
                            return Err(
                                "La generación se canceló antes de completarse.".to_string()
                            );
                        }
                        if frames.is_empty() {
                            return Err("el motor nativo no produjo fotogramas".to_string());
                        }
                        let title = match template_owned.as_str() {
                            "integral-area" => "Integral — área bajo la curva (nativa)".to_string(),
                            "derivative-slope" => "Derivada como pendiente (nativa)".to_string(),
                            "pitagoras" | "pythagoras" => {
                                "Teorema de Pitágoras (nativa)".to_string()
                            }
                            "taylor-series" => "Serie de Taylor (nativa)".to_string(),
                            "conformal-map" => "Mapeo conforme (nativa)".to_string(),
                            "universal" => format!("{} (nativa)", concept_owned),
                            _ => format!("{} (nativa)", concept_owned),
                        };
                        Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                    }
                };
            let result = match engine {
                Some(engine_section) => {
                    let config = grafito_anim::EngineConfig {
                        command: engine_section.command,
                        working_dir: Some(work_dir.clone()),
                        // Timeouts cortos: si el motor no responde, se cae al
                        // generador nativo para que «Animá» nunca se quede colgado.
                        idle_timeout: std::time::Duration::from_secs(2),
                        job_timeout: std::time::Duration::from_secs(15),
                        ..Default::default()
                    };
                    // Cancel real en el motor externo: `run_job` aborta <200 ms.
                    let cancel_flag = &worker_cancellation;
                    match grafito_anim::run_job(
                        &config,
                        &request,
                        Some(&|| cancel_flag.is_cancelled()),
                        |_| {},
                    ) {
                        Ok(result) => match load_gif_frames(&result.media_path) {
                            Ok(frames) if !frames.is_empty() => {
                                if worker_cancellation.is_cancelled() {
                                    Err("La generación se canceló antes de completarse."
                                        .to_string())
                                } else {
                                    let title = match template_owned.as_str() {
                                        "integral-area" => {
                                            "Integral — área bajo la curva".to_string()
                                        }
                                        "derivative-slope" => "Derivada como pendiente".to_string(),
                                        "pitagoras" | "pythagoras" => {
                                            "Teorema de Pitágoras".to_string()
                                        }
                                        "taylor-series" => "Serie de Taylor".to_string(),
                                        "conformal-map" => "Mapeo conforme".to_string(),
                                        _ => concept_owned.clone(),
                                    };
                                    Ok(grafito_ui::assistant::AssistantMedia { title, frames })
                                }
                            }
                            _ => render_native_cancellable(),
                        },
                        Err(error) => {
                            if worker_cancellation.is_cancelled()
                                || error.to_lowercase().contains("cancel")
                            {
                                Err(error)
                            } else {
                                render_native_cancellable()
                            }
                        }
                    }
                }
                None => render_native_cancellable(),
            };
            // Descarte final: si se canceló en el último instante, no se publica rancio.
            let result = if worker_cancellation.is_cancelled() {
                Err("La generación se canceló antes de completarse.".to_string())
            } else {
                result
            };
            let _ = sender.send(result);
            let _ = std::fs::remove_dir_all(&work_dir);
            repaint.request_repaint();
        });
        self.assistant.anim_progress = true;
        self.assistant_runtime.anim_job = Some(AssistantAnimJob {
            cancellation,
            receiver,
        });
        self.notify("Generando animación…", ToastKind::Info);
    }

    fn start_remote_proposal_verification(
        &mut self,
        ctx: &egui::Context,
        launch: AssistantProposalLaunch,
    ) {
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
        let document = self.document.detached_clone_for_staging();
        let camera = self.camera;
        let response_text = text.clone();
        let cancellation = CancellationToken::default();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
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
        self.assistant_runtime.proposal_job = Some(AssistantProposalJob {
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

    fn build_assistant_proposal_correction(
        &mut self,
        question: &str,
        repair_feedback: AssistantRepairFeedback,
        target_turn: usize,
        correction_attempt: u8,
        route: AssistantRemoteRoute,
        fusion_fallback_allowed: bool,
    ) -> Result<AssistantRemoteLaunch, String> {
        if correction_attempt >= MAX_ASSISTANT_PROPOSAL_CORRECTIONS {
            return Err("La corrección remota alcanzó su límite seguro.".into());
        }
        let mut settings = self.assistant_provider_settings()?;
        if route == AssistantRemoteRoute::FusionFallback {
            if !fusion_fallback_allowed
                || settings.profile != ProviderProfile::OpenCodeGo
                || settings.model != OPENCODE_VISION_MODEL
            {
                return Err(
                    "La revisión remota adicional no está autorizada para la configuración actual."
                        .into(),
                );
            }
            settings.model = OPENCODE_FUSION_MODEL.into();
            settings.capabilities.vision = false;
        }
        let api_key = self.assistant_api_key()?;
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let document_revision = document_context.revision;
        let document_digest = document_context.digest.clone();
        let request = self.build_remote_assistant_request(
            assistant_correction_prompt(question),
            document_context,
            focus.clone(),
            Vec::new(),
            false,
            Some(AssistantRepairRequest {
                feedback: repair_feedback,
                target_turn,
            }),
        )?;

        Ok(AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider: self.assistant.provider,
            model: self.assistant.model.clone(),
            route,
            fusion_fallback_allowed,
            question: question.into(),
            document_revision,
            document_digest,
            focus,
            correction_attempt: correction_attempt + 1,
            repair_target_turn: Some(target_turn),
            socratic_guard: self.session_socratic_guard(question),
        })
    }

    pub(crate) fn start_local_assistant_request(&mut self, ctx: &egui::Context) {
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let question = self.assistant.problem.trim().to_owned();
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let mut request = AssistantRequest::local(question.clone(), document_context);
        request.focus = focus;
        request.attachments = self.assistant.attachments.clone();

        self.assistant.begin_request(question.clone());
        self.assistant.problem.clear();
        // harness es síncrono pero O(n) pequeño, medido <5ms — no bloquea UI >10ms.
        // El trabajo pesado (verificación de propuestas remotas) ya corre en
        // worker thread via proposal_job / inspect_remote_proposals_cancellable.
        let local_result = match harness::request(&self.document, &request) {
            Ok(result) => result,
            Err(error) => {
                self.assistant.fail_request(error.clone());
                self.notify(error, ToastKind::Error);
                ctx.request_repaint();
                return;
            }
        };
        let staged_changes = local_result
            .staged_plan
            .map(|staged| staged.preview().changes.clone());
        match classify_local_assistant_response(local_result.response) {
            LocalAssistantDisposition::Solved { answer, plan } => {
                // Prosa con nombres humanos (mapa `humanize_control_name` de
                // ui, solo lectura): jamás IDs literales en el turno.
                let human = grafito_ui::assistant::humanize_prose_text(&answer);
                self.assistant.complete_local_request(human);
                if let Some(plan) = plan {
                    if let Some(changes) = staged_changes {
                        self.assistant.stage_proposed_plan(plan, changes);
                    } else {
                        self.show_assistant_error(
                            "La propuesta local no pudo completar su comprobación headless.",
                        );
                    }
                }
            }
            LocalAssistantDisposition::NeedsRemoteAuthorization(reason) => {
                if self.assistant.full_permission {
                    if self.remote_provider_ready() {
                        self.start_remote_assistant_for(ctx, question, None);
                    } else {
                        let message =
                            "Configurá un proveedor (Ajustes del asistente) para respuestas en línea automáticas.";
                        self.assistant.fail_request(message);
                        self.notify(message, ToastKind::Info);
                    }
                } else {
                    self.assistant.stage_remote_authorization(question, reason);
                }
            }
            LocalAssistantDisposition::Rejected(error) => {
                self.assistant.fail_request(error.clone());
                self.notify(error, ToastKind::Error);
            }
        }
        ctx.request_repaint();
    }

    /// Arranca la consulta remota tras un consentimiento explícito del cartel.
    fn start_authorized_remote_assistant_request(&mut self, ctx: &egui::Context) {
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let Some(question) = self
            .assistant
            .pending_remote_authorization_question()
            .map(str::to_owned)
        else {
            return;
        };
        self.assistant.begin_authorized_remote_request();
        self.start_remote_assistant_for(ctx, question, None);
    }

    /// Guard socrático de la sesión actual para un lanzamiento remoto.
    ///
    /// Fuente: nivel del perfil + tema de `WorkingMemory` + conversación.
    /// Bypass exploratorio (demo, no evaluación) → `None` (guard desactivado):
    /// - pedido demostrativo (`ejemplos`/`probá`/`capacidades`/`graficá` vía
    ///   `is_exploratory_request`), o
    /// - sin `last_concept`/`current_topic` (primer turno sin tema previo), o
    /// - sin pregunta heurística previa en la conversación (`?` asistente).
    ///   Primer turno sin pregunta previa ⇒ no punitivo (ver `can_reveal_answer`
    ///   y `count_heuristic_answers`).
    ///
    /// El modo agente lo ignora porque ya orquesta sus propias tools socráticas.
    fn session_socratic_guard(&self, question: &str) -> Option<SocraticGuardContext> {
        // Es demo, no evaluación: se salta el telling.
        if is_exploratory_request(question) {
            return None;
        }
        let topic = self.profile.working_memory.last_concept.as_deref().or(self
            .profile
            .working_memory
            .current_topic
            .as_deref());
        // Sin concepto previo → demo, no evaluación.
        let topic_trimmed = topic.map(str::trim).filter(|topic| !topic.is_empty())?;
        let _ = topic_trimmed;
        // Sin pregunta heurística previa → primer turno no punitivo.
        let has_prior_question = self
            .assistant
            .conversation
            .iter()
            .any(|turn| turn.role == ConversationRole::Assistant && turn.content.contains('?'));
        if !has_prior_question {
            return None;
        }
        Some(socratic_guard_context(
            self.profile.level,
            topic,
            question,
            &self.assistant.conversation,
        ))
    }

    /// Lanza la consulta remota con la pregunta dada, sin depender del cartel.
    ///
    /// Con permiso completo, el consentimiento de imágenes se otorga automático;
    /// la capacidad de visión del modelo sigue siendo un requisito real.
    /// `model_override` sólo-sesión: no toca la preferencia guardada.
    fn start_remote_assistant_for(
        &mut self,
        ctx: &egui::Context,
        question: String,
        model_override: Option<&str>,
    ) {
        if !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        if !self.assistant.attachments.is_empty() {
            if !self.assistant.vision_enabled {
                self.show_assistant_error(
                    "Confirmá que la configuración remota admite imágenes antes de enviarlas.",
                );
                return;
            }
            if self.assistant.full_permission {
                self.assistant.image_upload_consent = true;
            }
            if !self.assistant.image_upload_consent {
                self.show_assistant_error(
                    "Autorizá el envío de las imágenes antes de realizar la consulta.",
                );
                return;
            }
        }
        let effective_model = model_override
            .unwrap_or(&self.assistant.model)
            .trim()
            .to_owned();
        if effective_model.is_empty() {
            self.show_assistant_error(
                "Completá la configuración avanzada antes de consultar remotamente.",
            );
            return;
        }
        let settings = match self.assistant_provider_settings_for(&effective_model) {
            Ok(settings) => settings,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let api_key = match self.assistant_api_key() {
            Ok(key) => key,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let document_context = grafito_command::assistant_context::document_context(&self.document);
        let focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        let document_revision = document_context.revision;
        let document_digest = document_context.digest.clone();
        let request = match self.build_remote_assistant_request(
            question.clone(),
            document_context,
            focus.clone(),
            self.assistant.attachments.clone(),
            self.assistant.image_upload_consent,
            None,
        ) {
            Ok(request) => request,
            Err(error) => {
                // `build_remote_assistant_request` valida límites (inglés de
                // crates externos) — se envuelve en español antes de mostrar.
                self.show_assistant_error(remote_error_message(&error, &effective_model));
                return;
            }
        };

        // Fallback sólo-sesión: se recuerda para aceptar el resultado en vuelo
        // sin tocar la preferencia guardada del usuario.
        if model_override.is_some() {
            self.assistant_runtime.fallback_model = Some(effective_model.clone());
        } else {
            self.assistant_runtime.fallback_model = None;
        }
        let launch = AssistantRemoteLaunch {
            settings,
            request,
            api_key,
            provider: self.assistant.provider,
            model: effective_model,
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: self.assistant.allow_fusion_fallback,
            question: question.clone(),
            document_revision,
            document_digest,
            focus,
            correction_attempt: 0,
            repair_target_turn: None,
            socratic_guard: self.session_socratic_guard(&question),
        };
        if self.assistant.agent_mode {
            self.start_agent_assistant_job(ctx, launch);
        } else {
            self.start_remote_assistant_job(ctx, launch);
        }
    }

    fn apply_proposed_assistant_plan(&mut self) {
        let Some(plan) = self.assistant.proposed_plan().cloned() else {
            return;
        };
        let before_labels = self.object_labels_snapshot();
        match apply_local_assistant_plan(
            &mut self.document,
            &plan,
            &mut self.undo_stack,
            &mut self.redo_stack,
        ) {
            Ok(result) => {
                self.record_step_from_diff("Propuesta local aplicada", &before_labels, true);
                // Decide perspectiva según el contenido del plan: CreateGraph es 2D,
                // otras operaciones matemáticas también deben quedar visibles en Álgebra.
                let has_graph = plan.operations.iter().any(|operation| operation.is_graph());
                if has_graph {
                    if let Some(perspective) = assistant_graph_perspective(
                        grafito_command::assistant_context::AssistantGraphView::TwoD,
                        self.current_view,
                    ) {
                        self.set_perspective(perspective);
                    }
                }
                self.ensure_algebra_panel_visible();
                self.assistant.finish_proposed_plan_application(true);
                self.notify(
                    format!("Propuesta local aplicada: {}", result.changes.join(", ")),
                    ToastKind::Success,
                );
            }
            Err(error) => {
                self.assistant.clear_proposed_plan();
                self.show_assistant_error(format!(
                    "La propuesta local cambió o dejó de ser válida: {error}"
                ));
            }
        }
    }

    fn request_assistant_proposal_correction(&mut self, ctx: &egui::Context) {
        if self.assistant.is_pending || !self.assistant_runtime.remote_request_slot_is_free() {
            return;
        }
        let current_context = grafito_command::assistant_context::document_context(&self.document);
        let current_focus = grafito_command::assistant_context::selected_function_focus(
            &self.document,
            self.selected_object,
        );
        if !self
            .assistant
            .proposal_correction_matches_context(&current_context, current_focus.as_ref())
        {
            self.assistant.invalidate_proposal_correction();
            self.report_assistant_error(
                "La corrección se descartó porque cambió el documento o el foco seleccionado.",
            );
            return;
        }
        let Some((question, feedback, target_turn, correction_attempt)) =
            self.assistant.take_proposal_correction_session()
        else {
            return;
        };
        let fusion_fallback_allowed = self.assistant.allow_fusion_fallback;
        let route = if can_use_fusion_fallback(
            fusion_fallback_allowed,
            self.assistant.provider,
            &self.assistant.model,
        ) {
            AssistantRemoteRoute::FusionFallback
        } else {
            AssistantRemoteRoute::SelectedModel
        };
        let launch = match self.build_assistant_proposal_correction(
            &question,
            feedback,
            target_turn,
            correction_attempt,
            route,
            fusion_fallback_allowed,
        ) {
            Ok(launch) => launch,
            Err(error) => {
                self.assistant.restore_proposal_correction();
                // build_* mezcla español local e inglés de validadores externos.
                let current_model = self.assistant.model.clone();
                self.report_assistant_error(remote_error_message(&error, &current_model));
                return;
            }
        };

        self.assistant
            .begin_proposal_correction_with_route(route == AssistantRemoteRoute::FusionFallback);
        self.start_remote_assistant_job(ctx, launch);
    }

    fn start_model_request(&mut self, ctx: &egui::Context) {
        if !self.assistant_runtime.request_model_refresh() {
            return;
        }
        let settings = match self.assistant_provider_settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        let api_key = match self.assistant_api_key() {
            Ok(key) => key,
            Err(error) => {
                self.show_assistant_error(error);
                return;
            }
        };
        self.assistant_runtime.next_request_id =
            self.assistant_runtime.next_request_id.wrapping_add(1);
        let id = self.assistant_runtime.next_request_id;
        let provider = self.assistant.provider;
        let cancellation = CancellationToken::default();
        let worker =
            request_remote_models_with_api_key_on_worker(settings, api_key, cancellation.clone());
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
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
        self.assistant_runtime.model_job = Some(AssistantModelJob {
            id,
            provider,
            cancellation,
            receiver,
        });
    }

    fn cancel_assistant_request(&mut self) {
        // Cancela remote+proposal+agent+model+anim (todos, sin else-if: aunque el
        // slot remoto sólo permite un job de consulta a la vez, model/anim son
        // independientes y deben cancelarse también). Los workers con token se
        // drenan en poll (take_finished_*); anim señala su token y dropea el
        // slot (worker acotado con chequeo entre frames).
        let had_anim = self.assistant_runtime.anim_job.is_some();
        if self.assistant_runtime.cancel_all_assistant_jobs() {
            if had_anim {
                self.assistant.anim_progress = false;
            }
            self.begin_cancelling_remote_request();
        }
    }

    fn begin_cancelling_remote_request(&mut self) {
        self.assistant.begin_cancellation();
    }

    fn cancel_stale_remote_request(&mut self) {
        let expected = self
            .assistant_runtime
            .expected_model(&self.assistant.model)
            .to_owned();
        if self
            .assistant_runtime
            .cancel_stale_remote_job(self.assistant.provider, &expected)
            || self
                .assistant_runtime
                .cancel_stale_agent_job(self.assistant.provider, &expected)
        {
            self.begin_cancelling_remote_request();
        }
    }

    fn cancel_stale_model_request(&mut self) {
        if self
            .assistant_runtime
            .cancel_stale_model_job(self.assistant.provider)
        {}
    }

    /// Drena la actividad del modo agente y cierra el turno al terminar.
    fn poll_assistant_agent(&mut self, ctx: &egui::Context) -> bool {
        let Some(job) = self.assistant_runtime.agent_job.as_ref() else {
            return false;
        };
        loop {
            match job.receiver.try_recv() {
                Ok(AgentChannelMsg::Event(event)) => match event {
                    grafito_agent::AgentEvent::ToolStarted { name, .. } => {
                        self.assistant
                            .push_agent_activity(format!("usando {name}…"));
                    }
                    grafito_agent::AgentEvent::ToolFinished { name, ok } => {
                        let marker = if ok { "✓" } else { "✗" };
                        self.assistant
                            .push_agent_activity(format!("{marker} {name}"));
                    }
                    grafito_agent::AgentEvent::Ledger { render } => {
                        self.assistant.set_agent_ledger(Some(render));
                    }
                    grafito_agent::AgentEvent::Finalized { .. } => {}
                },
                Ok(AgentChannelMsg::Done(result)) => {
                    if let Some(job) = self.assistant_runtime.agent_job.take() {
                        let cancelled = job.cancellation.is_cancelled();
                        if cancelled {
                            self.fail_assistant_request(
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
                                    self.assistant.complete_request(outcome.final_text);
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
                                        self.notify(
                                            "Modo agente con Spark aún no soporta herramientas; reintentando con DeepSeek Flash…",
                                            ToastKind::Info,
                                        );
                                        let question = self.assistant.problem.trim().to_owned();
                                        self.start_remote_assistant_for(
                                            ctx,
                                            question,
                                            Some("deepseek-v4-flash"),
                                        );
                                    } else {
                                        self.fail_assistant_request(error);
                                    }
                                }
                            }
                        }
                    }
                    ctx.request_repaint();
                    return true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some(_job) = self.assistant_runtime.agent_job.take() {
                        self.fail_assistant_request(
                            "El agente terminó inesperadamente antes de responder.",
                        );
                    }
                    ctx.request_repaint();
                    return true;
                }
            }
        }
        false
    }

    fn poll_assistant_jobs(&mut self, ctx: &egui::Context) {
        self.poll_assistant_agent(ctx);
        // Drena deltas SSE a la burbuja provisional antes de cosechar el
        // resultado final (el preview se limpia en cada rama terminal).
        self.assistant_runtime
            .drain_remote_stream_preview(&mut self.assistant, ctx);
        if let Some(completion) = self.assistant_runtime.take_finished_remote_job() {
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
                pop_provisional_stream_turn(&mut self.assistant);
            }
            if cancelled
                || !accepts_remote_result(
                    self.assistant.provider,
                    self.assistant_runtime.expected_model(&self.assistant.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    self.fail_assistant_repair_request(
                        "La corrección se canceló antes de recibir una respuesta.",
                    );
                } else {
                    self.fail_assistant_request(
                        "La consulta se canceló antes de recibir una respuesta.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(&self.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    &self.document,
                    self.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    self.fail_assistant_request(
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    self.assistant.invalidate_proposal_correction();
                } else {
                    match result {
                        Ok(completion) => {
                            if completion.truncated {
                                self.notify(
                                    "La respuesta alcanzó el límite de la consulta; pedí que continúe desde el último punto.",
                                    ToastKind::Info,
                                );
                            }
                            let text = completion.text;
                            self.start_remote_proposal_verification(
                                ctx,
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
                                    self.assistant.restore_proposal_correction();
                                }
                                // Jerga solo en logs/eventos internos.
                                eprintln!(
                                    "grafito: socratic repair interno (no al transcript): {error}"
                                );
                                // Convierte a voz de Mili; si no hay guard
                                // (no debería pasar si hubo repair), fallback
                                // humano sin jerga ni eco crudo.
                                let student = self
                                    .session_socratic_guard(&question)
                                    .map(|guard| {
                                        guard.fsm.repair_student_message(&guard.scaffold)
                                    })
                                    .unwrap_or_else(|| {
                                        "Antes de mostrarte la solución, ¿qué forma te imaginás? Contame qué probaste y lo vemos juntos.".to_owned()
                                    });
                                self.assistant.complete_request(student);
                                self.notify(
                                    "El tutor repregunta antes de mostrar la solución directa.",
                                    ToastKind::Info,
                                );
                            } else if should_fallback_remote_spark_to_deepseek(
                                &error,
                                self.assistant.provider,
                                &self.assistant.model,
                                correction_attempt,
                            ) {
                                eprintln!("grafito: session-fallback muse-spark [{error}] -> deepseek-v4-flash + retry (preferencia intacta)");
                                self.notify(
                                    "Muse Spark no respondió, reintentando con DeepSeek Flash; tu modelo sigue siendo Muse Spark.",
                                    ToastKind::Info,
                                );
                                // Reintentar la misma pregunta con el fallback, sin mostrar error
                                self.start_remote_assistant_for(
                                    ctx,
                                    question.clone(),
                                    Some("deepseek-v4-flash"),
                                );
                                return;
                            } else if correction_attempt > 0 {
                                self.fail_assistant_repair_request(error);
                            } else {
                                self.fail_assistant_request(error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = self.assistant_runtime.take_finished_proposal_job() {
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
                    self.assistant.provider,
                    self.assistant_runtime.expected_model(&self.assistant.model),
                    provider,
                    &model,
                )
            {
                if correction_attempt > 0 {
                    self.fail_assistant_repair_request(
                        "La corrección se canceló antes de terminar la comprobación local.",
                    );
                } else {
                    self.fail_assistant_request(
                        "La consulta se canceló antes de terminar la comprobación local.",
                    );
                }
            } else {
                let current_context =
                    grafito_command::assistant_context::document_context(&self.document);
                let current_focus = grafito_command::assistant_context::selected_function_focus(
                    &self.document,
                    self.selected_object,
                );
                if !accepts_remote_context(
                    &current_context,
                    current_focus.as_ref(),
                    document_revision,
                    &document_digest,
                    focus.as_ref(),
                ) {
                    self.fail_assistant_request(
                        "La respuesta quedó obsoleta porque cambió el documento o el foco; no se aceptó ni se verificaron sus propuestas.",
                    );
                    self.assistant.invalidate_proposal_correction();
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
                            self.assistant.set_proposal_preflight_results(
                                proposal_check.verified,
                                proposal_check.candidate_count,
                                proposal_check.candidate_code_block_indices,
                            );
                            if correction_attempt > 0 {
                                let Some(target_turn) = repair_target_turn else {
                                    self.fail_assistant_request(
                                        "La corrección perdió el turno que debía reemplazar.",
                                    );
                                    return;
                                };
                                if !self
                                    .assistant
                                    .complete_proposal_correction_at(target_turn, text.clone())
                                {
                                    self.fail_assistant_request(
                                        "La corrección no pudo reemplazar su respuesta original.",
                                    );
                                    return;
                                }
                            } else {
                                self.assistant.complete_request(text.clone());
                            }
                            if can_offer_correction {
                                if let Some(feedback) = repair_feedback {
                                    let target_turn = repair_target_turn.or_else(|| {
                                        self.assistant.conversation.len().checked_sub(1)
                                    });
                                    self.assistant.offer_proposal_correction_for_turn(
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
                                self.show_assistant_error(error);
                            } else if correction_attempt > 0
                                && proposal_check.verified_action_count == 0
                            {
                                self.show_assistant_error(
                                    "No se obtuvo una propuesta verificable; no hay nada para aplicar.",
                                );
                            }
                        }
                        Err(error) => {
                            if correction_attempt > 0 {
                                self.fail_assistant_repair_request(error);
                            } else {
                                self.fail_assistant_request(error);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = self.assistant_runtime.take_finished_model_job() {
            let _request_id = completion.id;
            let FinishedModelJob {
                provider,
                cancelled,
                result,
                ..
            } = completion;
            if accepts_model_result(self.assistant.provider, provider, cancelled) {
                match result {
                    Ok(models) => {
                        self.assistant.set_available_models(models);
                    }
                    Err(error) => {
                        // La lista de modelos viene del transporte (inglés
                        // crudo posible) — se envuelve en español.
                        let current_model = self.assistant.model.clone();
                        self.show_assistant_error(remote_error_message(&error, &current_model));
                    }
                }
            }
        }
        if self.assistant_runtime.take_queued_model_refresh_if_idle() {
            self.start_model_request(ctx);
        }

        let image = self.assistant_runtime.image_job.as_ref().and_then(|job| {
            match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(
                    "La importación de imagen terminó inesperadamente.".into(),
                )),
            }
        });
        if let Some(result) = image {
            self.assistant_runtime.image_job = None;
            self.assistant.is_importing_image = false;
            match result {
                Ok(attachment) => match self.assistant.add_attachment(attachment) {
                    Ok(()) => {
                        self.assistant.attachment_message =
                            Some("Imagen lista para consultar.".into())
                    }
                    Err(error) => self.show_assistant_error(attachment_error_message(&error)),
                },
                Err(error) => {
                    self.assistant.attachment_message = None;
                    self.show_assistant_error(attachment_error_message(&error));
                }
            }
        }
    }

    fn assistant_provider_settings(&self) -> Result<ProviderSettings, String> {
        self.assistant_provider_settings_for(&self.assistant.model.clone())
    }

    fn assistant_provider_settings_for(&self, model: &str) -> Result<ProviderSettings, String> {
        let model = model.trim();
        if model.is_empty() {
            return Err(
                "Completá la configuración avanzada antes de consultar remotamente.".into(),
            );
        }
        let mut settings = if self.assistant.provider == ProviderProfile::CustomOpenAiCompatible {
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
            ProviderSettings::for_profile(self.assistant.provider, model)
        };
        if self.assistant.vision_enabled {
            let capabilities = ProviderCapabilities {
                vision: true,
                ..settings.capabilities
            };
            settings = settings.with_capabilities(capabilities);
        }
        Ok(settings)
    }

    /// Indica si el proveedor remoto configurado puede responder hoy.
    fn remote_provider_ready(&mut self) -> bool {
        let Ok(settings) = self.assistant_provider_settings() else {
            return false;
        };
        match settings.profile {
            ProviderProfile::OllamaLocal => true,
            _ => self.assistant_api_key().is_ok(),
        }
    }

    fn assistant_api_key(&mut self) -> Result<Option<String>, String> {
        if self.assistant.provider == ProviderProfile::OllamaLocal {
            return Ok(None);
        }
        if let Some(key) = self.assistant_runtime.key_for(self.assistant.provider) {
            return Ok(Some(key));
        }
        // Custom requiere clave propia (assistant-custom); no reutiliza OpenCodeGo.
        // Si no hay clave Custom, se retorna Err abajo vía load(Custom) => None.
        match assistant_credentials::load(self.assistant.provider) {
            Ok(Some(key)) => {
                self.assistant.key_available = true;
                self.assistant.key_status_checked = true;
                self.assistant_runtime
                    .remember_key(self.assistant.provider, key.clone());
                Ok(Some(key))
            }
            Ok(None) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                Err(
                    "Guardá una clave de API en la configuración avanzada antes de consultar."
                        .into(),
                )
            }
            Err(_) => {
                self.assistant.key_available = false;
                self.assistant.key_status_checked = true;
                Err("El llavero del sistema no está disponible para leer la API key.".into())
            }
        }
    }
}

/// Wiring B1 — loop Spark por Responses en paralelo (agente externo).
///
/// El modo agente con Spark hoy falla porque las tools aún no viajan por la
/// Responses API. B1 implementará ese loop en paralelo; cuando su `Done(Ok)`
/// llegue con Spark, el `poll_assistant_agent` lo acepta directo (nunca
/// fallback). Este helper distingue ese éxito del `Err` "Responses API", que
/// sí dispara el fallback sólo-sesión a deepseek sin tocar la preferencia.
fn is_agent_spark_responses_unsupported_error(error: &str) -> bool {
    error.contains("Responses API")
}

/// Fallback agente sólo-sesión: Spark + Responses API no soportado → deepseek.
/// Nunca dispara en `Ok` (sólo se llama en la rama `Err`).
fn should_fallback_agent_spark_to_deepseek(
    error: &str,
    provider: ProviderProfile,
    model: &str,
) -> bool {
    is_agent_spark_responses_unsupported_error(error)
        && provider == ProviderProfile::OpenCodeGo
        && model.contains("muse-spark")
}

/// Fallback chat (no agente) sólo-sesión: Spark 500/timeout → deepseek.
/// La preferencia guardada queda intacta; el próximo pedido reintenta spark.
fn should_fallback_remote_spark_to_deepseek(
    error: &str,
    provider: ProviderProfile,
    current_model: &str,
    correction_attempt: u8,
) -> bool {
    let slow_or_down = error.contains("500") || error.contains("timed out");
    slow_or_down
        && current_model.contains("muse-spark")
        && provider == ProviderProfile::OpenCodeGo
        && correction_attempt == 0
}

/// Sanea errores de adjuntos de crates externos (inglés crudo) a español.
/// Los mensajes ya españoles de `grafito-ui` pasan intactos.
fn attachment_error_message(error: &str) -> String {
    if error.contains("assistant attachment") {
        "La imagen no es válida o supera los límites permitidos.".into()
    } else {
        error.to_owned()
    }
}

fn remote_error_message(error: &str, current_model: &str) -> String {
    eprintln!(
        "grafito: remote_error raw={} model={}",
        error, current_model
    );
    if error.contains("llavero") || error.contains("API key") {
        "No se pudo preparar la consulta remota. Revisá la configuración avanzada.".into()
    } else if error.contains("could not be built") {
        "No se pudo armar la consulta: la API key o el endpoint tienen caracteres inválidos. Reingresá la clave en Configuración avanzada (sin espacios ni saltos de línea).".into()
    } else if error.contains("Responses API") {
        "Este modelo usa la Responses API: el modo agente con herramientas aún no está soportado. Usá el chat simple o cambiá a deepseek-v4-flash.".into()
    } else if error.contains("cancel") {
        "La consulta remota se canceló antes de completarse.".into()
    } else if error.contains("401") || error.contains("403") || error.contains("unauthorized") {
        format!("La clave de API no es válida o expiró: {error}. Revisá la configuración avanzada.")
    } else if error.contains("429") || error.contains("rate limit") || error.contains("RateLimit") {
        // 429 = cuota por minuto del proveedor, no es tu modelo ni tu clave.
        // Va antes de la rama 404/"model" porque el cuerpo del 429 puede
        // nombrar al modelo ("Model X rate limited").
        "Límite de uso del proveedor (429, cuota por minuto). Esperá ~1 minuto y reintentá; no cambies tu modelo ni tu clave.".into()
    } else if error.contains("404") || error.contains("model") {
        format!(
            "El modelo '{}' no está disponible: {error}. Revisá Configuración → Modelo.",
            current_model
        )
    } else if error.contains("timeout") || error.contains("timed out") {
        format!("La conexión tardó demasiado: {error}. Revisá tu conexión.")
    } else if error.contains("500") {
        if current_model.contains("muse-spark") {
            "Muse Spark responde por la Responses API; si ves un 500 es transitorio del proveedor. Probá de nuevo o cambiá a deepseek-v4-flash, qwen3.8-max o kimi-k3 en Configuración → Modelo.".to_string()
        } else {
            format!(
                "Error interno del proveedor (500) con modelo '{}'. Probá de nuevo en unos segundos o cambiá de modelo.",
                current_model
            )
        }
    } else if error.contains("DNS") || error.contains("connect") || error.contains("network") {
        format!("Error de red: {error}. Revisá tu conexión.")
    } else if error.contains("body cap") {
        "La respuesta superó el tope de 256 KiB: pedila por partes (ej: «dame 3 ejemplos»)."
            .to_string()
    } else {
        // Corte por chars, nunca por bytes (el mensaje puede traer multibyte).
        let truncated: String = error.chars().take(120).collect();
        let truncated = if error.chars().count() > 120 {
            format!("{truncated}…")
        } else {
            truncated
        };
        format!("Error: {truncated} — Revisá Configuración → Modelo (actual: {current_model})")
    }
}

fn accepts_model_result(
    current_provider: ProviderProfile,
    result_provider: ProviderProfile,
    cancelled: bool,
) -> bool {
    !cancelled && current_provider == result_provider
}

fn accepts_remote_result(
    current_provider: ProviderProfile,
    current_model: &str,
    result_provider: ProviderProfile,
    result_model: &str,
) -> bool {
    current_provider == result_provider && current_model == result_model
}

fn accepts_remote_context(
    current_context: &ImmutableDocumentContext,
    current_focus: Option<&AssistantFocus>,
    result_revision: u64,
    result_digest: &str,
    result_focus: Option<&AssistantFocus>,
) -> bool {
    current_context.revision == result_revision
        && current_context.digest == result_digest
        && current_focus == result_focus
}

/// Carga los frames de un GIF en ColorImage para reproducirlos en el chat.
///
/// Presupuesto OOM: máximo 64 frames, 8 M píxeles totales, archivo ≤5 MB, por
/// frame ≤2 M píxeles, `checked_mul` para evitar overflow y `try_reserve`
/// antes de cada `push`.
fn load_gif_frames(path: &str) -> Result<Vec<egui::ColorImage>, String> {
    const MAX_GIF_FRAMES: usize = 64;
    const MAX_GIF_TOTAL_PIXELS: usize = 8_000_000;
    const MAX_GIF_FILE_BYTES: u64 = 5 * 1024 * 1024;
    const MAX_FRAME_PIXELS: usize = 2_000_000;
    let file =
        std::fs::File::open(path).map_err(|error| format!("no se pudo abrir el GIF: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("no se pudo leer metadata del GIF: {error}"))?;
    if metadata.len() > MAX_GIF_FILE_BYTES {
        return Err(format!(
            "GIF demasiado grande ({} bytes > 5 MB)",
            metadata.len()
        ));
    }
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options
        .read_info(file)
        .map_err(|error| format!("GIF inválido: {error}"))?;
    let mut frames = Vec::new();
    frames
        .try_reserve(MAX_GIF_FRAMES)
        .map_err(|_| "sin memoria para GIF (reserve)".to_string())?;
    let mut total_pixels: usize = 0;
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|error| format!("GIF corrupto: {error}"))?
    {
        if frames.len() >= MAX_GIF_FRAMES {
            break;
        }
        let width = frame.width as usize;
        let height = frame.height as usize;
        if width == 0 || height == 0 {
            continue;
        }
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| "GIF frame con overflow de píxeles".to_string())?;
        if pixel_count > MAX_FRAME_PIXELS {
            continue;
        }
        let next_total = total_pixels
            .checked_add(pixel_count)
            .ok_or_else(|| "GIF excede presupuesto de píxeles".to_string())?;
        if next_total > MAX_GIF_TOTAL_PIXELS {
            break;
        }
        let required_bytes = pixel_count
            .checked_mul(4)
            .ok_or_else(|| "GIF frame con overflow de bytes".to_string())?;
        let buffer = &frame.buffer;
        if buffer.len() < required_bytes {
            continue;
        }
        total_pixels = next_total;
        frames
            .try_reserve(1)
            .map_err(|_| "sin memoria para frame de GIF".to_string())?;
        let mut image = egui::ColorImage::new([width, height], egui::Color32::TRANSPARENT);
        // ColorImage::new ya reservó `pixel_count` elementos; verificar que no exceda presupuesto
        // y copiar con checked offset.
        for (index, pixel) in image.pixels.iter_mut().enumerate() {
            let offset = index
                .checked_mul(4)
                .ok_or_else(|| "offset overflow en GIF".to_string())?;
            if offset + 3 >= buffer.len() {
                break;
            }
            *pixel = egui::Color32::from_rgba_unmultiplied(
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            );
        }
        frames.push(image);
    }
    Ok(frames)
}

#[cfg(test)]
mod domain_sparkline_tests {
    #[test]
    fn samples_are_bounded_and_normalized() {
        let mut profile = grafito_profile::StudentProfile::new("Spark");
        for index in 0..20 {
            profile.record_outcome("calculus", "Cálculo", index as u64, index % 2 == 0);
        }
        let samples = crate::GrafitoApp::domain_sparkline_from(&profile);
        assert!(!samples.is_empty());
        assert!(samples.len() <= 14, "muestras acotadas");
        assert!(samples.iter().all(|value| (0.0..=1.0).contains(value)));
    }
}

#[cfg(test)]
mod gif_loader_tests {
    // Generation and decode are exercised against a real in-memory GIF.
    #[test]
    fn gif_loader_reads_bounded_rgba_frames() {
        use gif::{Encoder, Frame, Repeat};

        let mut rgba = Vec::new();
        {
            let mut encoder = Encoder::new(&mut rgba, 2, 2, &[]).unwrap();
            encoder.set_repeat(Repeat::Finite(0)).unwrap();
            for frame in [
                vec![
                    255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
                vec![
                    0u8, 0, 255, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 0, 255, 255,
                ],
            ] {
                let mut rgba = frame;
                encoder
                    .write_frame(&Frame::from_rgba_speed(2, 2, rgba.as_mut_slice(), 10))
                    .unwrap();
            }
        }
        let dir = std::env::temp_dir().join(format!("grafito_gif_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.gif");
        std::fs::write(&path, rgba).unwrap();

        let frames = super::load_gif_frames(&path.to_string_lossy()).expect("decode frames");
        assert!(!frames.is_empty());
        for frame in &frames {
            assert_eq!(frame.size, [2, 2]);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

fn plugin_validation_context() -> grafito_plugins::ValidationContext<'static> {
    grafito_plugins::ValidationContext {
        resolvable_command_ids: &|id| grafito_command::command_registry::resolve(id).is_some(),
        known_tools: &["evaluate_expr", "grafito_docs", "ask_user"],
        known_scenes: &[
            "derivative-slope",
            "concept-flow",
            "graph-trace",
            "riemann",
            "fourier_partial",
            "pythagorean",
            "tetrahedron_rotate",
        ],
    }
}

fn assistant_correction_prompt(question: &str) -> String {
    let mut end = question.len().min(MAX_ASSISTANT_CORRECTION_SOURCE_BYTES);
    while end > 0 && !question.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}{}",
        question[..end].trim(),
        ASSISTANT_CORRECTION_INSTRUCTION
    )
}

fn can_offer_assistant_proposal_correction(
    correction_attempt: u8,
    action_candidate_count: usize,
    verified_action_count: usize,
    repair_feedback: Option<&AssistantRepairFeedback>,
) -> bool {
    correction_attempt < MAX_ASSISTANT_PROPOSAL_CORRECTIONS
        && verified_action_count == 0
        && repair_feedback.is_some()
        && (action_candidate_count > 0 || correction_attempt > 0)
}

fn can_use_fusion_fallback(
    fallback_allowed: bool,
    provider: ProviderProfile,
    selected_model: &str,
) -> bool {
    fallback_allowed
        && provider == ProviderProfile::OpenCodeGo
        && selected_model == OPENCODE_VISION_MODEL
}

fn assistant_expected_syntax(command: &str) -> Vec<String> {
    let mut syntaxes = grafito_command::assistant_context::assistant_executable_syntaxes(command);
    if let Some(guidance) =
        grafito_command::assistant_context::assistant_literal_argument_guidance(command)
    {
        syntaxes.push(guidance.into());
    }
    syntaxes
}

fn classify_assistant_preflight_error(error: &str) -> AssistantRepairFailureKind {
    if error.contains("no creó un objeto") {
        AssistantRepairFailureKind::NoNewObject
    } else if error.contains("fuera de la vista gráfica esperada") {
        AssistantRepairFailureKind::WrongRenderSpace
    } else if error.contains("no produjo geometría visible")
        || error.contains("no produjo una flor 3D visible")
    {
        AssistantRepairFailureKind::NotVisible
    } else {
        AssistantRepairFailureKind::CommandRejected
    }
}

fn assistant_repair_failure_for_command(
    command: &AssistantCommandInvocation,
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: command.canonical_name().into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: assistant_expected_syntax(command.canonical_name()),
    }
}

fn assistant_repair_failure_from_rejection(
    rejection: &AssistantProposalRejection,
) -> AssistantRepairFailure {
    let expected_syntax =
        if grafito_command::assistant_context::assistant_graph_capability(&rejection.command)
            .is_some()
        {
            assistant_expected_syntax(&rejection.command)
        } else {
            Vec::new()
        };
    AssistantRepairFailure {
        command: rejection.command.clone(),
        kind: match rejection.kind {
            AssistantProposalRejectionKind::InvalidSyntax => {
                AssistantRepairFailureKind::InvalidSyntax
            }
            AssistantProposalRejectionKind::UnsupportedCommand => {
                AssistantRepairFailureKind::UnsupportedCommand
            }
        },
        expected_syntax,
    }
}

fn assistant_repair_failure_for_scene(
    _commands: &[AssistantCommandInvocation],
    error: &str,
) -> AssistantRepairFailure {
    AssistantRepairFailure {
        command: "Scene".into(),
        kind: classify_assistant_preflight_error(error),
        expected_syntax: Vec::new(),
    }
}

struct AssistantGraphPreflight {
    staged: grafito_core::Document,
    outcome: grafito_command::commands::CommandOutcome,
    view: grafito_command::assistant_context::AssistantGraphView,
}

struct AssistantScenePreflight {
    staged: grafito_core::Document,
    outcome: grafito_command::commands::CommandOutcome,
    camera: grafito_geometry::Camera3D,
    view: grafito_command::assistant_context::AssistantGraphView,
}

struct RemoteProposalVerification {
    verified: Vec<VerifiedAssistantProposal>,
    candidate_count: usize,
    candidate_code_block_indices: Vec<usize>,
    action_candidate_count: usize,
    verified_action_count: usize,
    repair_feedback: Option<AssistantRepairFeedback>,
}

#[cfg(test)]
fn verified_remote_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> Vec<AssistantProposal> {
    inspect_remote_proposals(document, response, camera)
        .verified
        .into_iter()
        .map(|proposal| proposal.proposal)
        .collect()
}

#[cfg(test)]
fn inspect_remote_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> RemoteProposalVerification {
    inspect_remote_proposals_cancellable(
        document,
        response,
        camera,
        &CancellationToken::default(),
        false,
    )
    .expect("an uncancelled local proposal preflight must complete")
}

#[cfg(test)]
fn inspect_remote_action_proposals(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
) -> RemoteProposalVerification {
    inspect_remote_proposals_cancellable(
        document,
        response,
        camera,
        &CancellationToken::default(),
        true,
    )
    .expect("an uncancelled local proposal preflight must complete")
}

fn inspect_remote_proposals_cancellable(
    document: &grafito_core::Document,
    response: &str,
    camera: grafito_geometry::Camera3D,
    cancellation: &CancellationToken,
    requires_action: bool,
) -> Result<RemoteProposalVerification, String> {
    let all_candidates = assistant_fenced_proposals(response);
    let candidate_code_block_indices = all_candidates
        .iter()
        .map(|candidate| candidate.code_block_index)
        .collect::<Vec<_>>();
    let action_candidate_count = all_candidates
        .iter()
        .take(MAX_REMOTE_PROPOSAL_PREFLIGHTS)
        .filter(|candidate| candidate.is_action_candidate())
        .count();
    let candidates = all_candidates
        .into_iter()
        .take(MAX_REMOTE_PROPOSAL_PREFLIGHTS)
        .collect::<Vec<_>>();
    let candidate_count = candidates.len();
    let mut verified = Vec::new();
    let mut repair_failures = Vec::new();
    let mut parameter_context = document.detached_clone_for_staging();
    let mut prerequisite_parameters = Vec::new();
    for (candidate_index, candidate) in candidates.into_iter().enumerate() {
        if cancellation.is_cancelled() {
            return Err("La comprobación local de la propuesta se canceló.".into());
        }
        let Some(proposal) = candidate.proposal else {
            if let Some(rejection) = candidate.rejection {
                repair_failures.push(assistant_repair_failure_from_rejection(&rejection));
            }
            continue;
        };
        let prerequisite_parameters_for_proposal = match &proposal {
            AssistantProposal::Parameter(_) => Vec::new(),
            AssistantProposal::Command(_) | AssistantProposal::Scene(_) => {
                prerequisite_parameters.clone()
            }
        };
        let accepted = match &proposal {
            AssistantProposal::Command(command) => {
                match preflight_assistant_graph_command_with_camera(
                    &parameter_context,
                    command,
                    camera,
                ) {
                    Ok(_) => true,
                    Err(error) => {
                        repair_failures.push(assistant_repair_failure_for_command(command, &error));
                        false
                    }
                }
            }
            AssistantProposal::Scene(commands) => {
                match preflight_assistant_scene(&parameter_context, commands, camera) {
                    Ok(_) => true,
                    Err(error) => {
                        repair_failures.push(assistant_repair_failure_for_scene(commands, &error));
                        false
                    }
                }
            }
            AssistantProposal::Parameter(assignment) => {
                stage_assistant_parameter(&mut parameter_context, assignment).is_ok()
            }
        };
        if cancellation.is_cancelled() {
            return Err("La comprobación local de la propuesta se canceló.".into());
        }
        if accepted {
            if let AssistantProposal::Parameter(assignment) = &proposal {
                prerequisite_parameters.push(assignment.clone());
            }
            verified.push(VerifiedAssistantProposal {
                candidate_index,
                proposal,
                prerequisite_parameters: prerequisite_parameters_for_proposal,
            });
        }
    }
    let verified_action_count = verified
        .iter()
        .filter(|proposal| {
            matches!(
                &proposal.proposal,
                AssistantProposal::Command(_) | AssistantProposal::Scene(_)
            )
        })
        .count();
    if requires_action && action_candidate_count == 0 && verified_action_count == 0 {
        repair_failures.push(AssistantRepairFailure {
            command: "GraphProposal".into(),
            kind: AssistantRepairFailureKind::InvalidSyntax,
            expected_syntax: vec![
                "Emit a grafito or grafito-scene block with executable catalog commands.".into(),
            ],
        });
    }
    Ok(RemoteProposalVerification {
        verified,
        candidate_count,
        candidate_code_block_indices,
        action_candidate_count,
        verified_action_count,
        repair_feedback: (!repair_failures.is_empty()).then_some(AssistantRepairFeedback {
            failures: repair_failures,
        }),
    })
}

fn document_with_assistant_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
) -> Result<grafito_core::Document, String> {
    let mut staged = document.detached_clone_for_staging();
    for assignment in prerequisite_parameters {
        stage_assistant_parameter(&mut staged, assignment)?;
    }
    Ok(staged)
}

fn preflight_assistant_scene_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_scene(&staged, commands, camera)
}

fn preflight_assistant_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    let homogeneous = commands.first().is_some_and(|first| {
        commands
            .iter()
            .all(|command| command.canonical_name() == first.canonical_name())
    });

    if homogeneous {
        preflight_homogeneous_assistant_scene(document, commands, camera)
    } else {
        preflight_assistant_flower_scene(document, commands, camera)
    }
}

fn preflight_homogeneous_assistant_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    if !(2..=8).contains(&commands.len()) {
        return Err("La escena requiere entre 2 y 8 componentes.".into());
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let mut capability: Option<grafito_command::assistant_context::AssistantGraphCapability> = None;

    for command in commands {
        let current_capability = grafito_command::assistant_context::assistant_graph_capability(
            command.canonical_name(),
        )
        .ok_or_else(|| "La escena contiene un comando no permitido.".to_string())?;
        if !assistant_command_is_safe(command) {
            return Err("La escena contiene un comando con argumentos inválidos.".into());
        }
        if let Some(first_capability) = capability {
            if current_capability.canonical != first_capability.canonical {
                return Err("La escena general debe repetir un único tipo de comando.".into());
            }
        } else {
            capability = Some(*current_capability);
        }

        let before_ids = staged
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let outcome = execute_assistant_command(&mut staged, command);
        if let grafito_command::commands::CommandOutcome::Error(error) = outcome {
            return Err(error);
        }
        let created_ids = staged
            .objects_iter()
            .filter_map(|(id, _)| (!before_ids.contains(id)).then_some(*id))
            .collect::<Vec<_>>();
        if created_ids.is_empty() {
            return Err("La escena contiene un comando que no creó un objeto gráfico.".into());
        }
        if created_ids.iter().any(|id| {
            staged
                .get_object(*id)
                .is_none_or(|object| match current_capability.view {
                    grafito_command::assistant_context::AssistantGraphView::TwoD => {
                        object.render_space() != grafito_core::RenderSpace::D2
                    }
                    grafito_command::assistant_context::AssistantGraphView::ThreeD => {
                        object.render_space() != grafito_core::RenderSpace::D3
                    }
                })
        }) {
            return Err("La escena creó un objeto fuera de la vista gráfica esperada.".into());
        }
    }

    let Some(capability) = capability else {
        return Err("La escena vacía no tiene capacidad.".into());
    };
    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let (camera, has_geometry) = match capability.proof {
        grafito_command::assistant_context::AssistantGraphProof::StaticTwoD => {
            let (vertices, indices) = grafito_render::Renderer::build_geometry_static(
                &inspection,
                inspection.view(),
                false,
                false,
            );
            (
                camera,
                static_geometry_intersects_view(&vertices, &indices, inspection.view()),
            )
        }
        grafito_command::assistant_context::AssistantGraphProof::WorldMeshThreeD => {
            let screen_size = inspection.view().screen_size;
            let initial_mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &camera,
                screen_size.x,
                screen_size.y,
            );
            if !initial_mesh.is_complete() || initial_mesh.validate().is_err() {
                return Err("La escena produjo una geometría 3D inválida.".into());
            }
            let fitted_camera = fit_camera_to_world_mesh(&initial_mesh, camera)?;
            let mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &fitted_camera,
                screen_size.x,
                screen_size.y,
            );
            (
                fitted_camera,
                mesh.is_complete()
                    && mesh.validate().is_ok()
                    && world_mesh_intersects_view(
                        &mesh,
                        &fitted_camera,
                        screen_size.x,
                        screen_size.y,
                    ),
            )
        }
        grafito_command::assistant_context::AssistantGraphProof::CpuOverlayThreeD => (
            camera,
            cpu_overlay_intersects_view(&inspection, &camera, inspection.view().screen_size),
        ),
    };
    if !has_geometry {
        return Err("La escena no produjo geometría visible; no se aplicó al documento.".into());
    }

    Ok(AssistantScenePreflight {
        staged,
        outcome: grafito_command::commands::CommandOutcome::Message(format!(
            "Escena verificada: {} componentes.",
            commands.len()
        )),
        camera,
        view: capability.view,
    })
}

fn preflight_assistant_flower_scene(
    document: &grafito_core::Document,
    commands: &[AssistantCommandInvocation],
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantScenePreflight, String> {
    if !(6..=8).contains(&commands.len()) {
        return Err("La escena de flor requiere entre 6 y 8 componentes.".into());
    }

    let mut stem_count = 0;
    let mut center_count = 0;
    let mut petal_count = 0;
    for command in commands {
        let capability = grafito_command::assistant_context::assistant_graph_capability(
            command.canonical_name(),
        )
        .ok_or_else(|| "La escena contiene un comando no permitido.".to_string())?;
        if capability.view != grafito_command::assistant_context::AssistantGraphView::ThreeD
            || !assistant_command_is_safe(command)
        {
            return Err("La escena sólo admite componentes gráficos 3D verificables.".into());
        }
        match command.canonical_name() {
            "Cylinder" | "Cone" | "Curve3D" => stem_count += 1,
            "Sphere" => center_count += 1,
            "Surface3D" => petal_count += 1,
            _ => return Err("La escena de flor sólo admite tallo, centro y pétalos 3D.".into()),
        }
    }
    if stem_count != 1 || center_count != 1 || petal_count < 4 {
        return Err("La escena debe incluir un tallo, un centro y al menos cuatro pétalos.".into());
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let mut petal_index = 0;
    for command in commands {
        let before_ids = staged
            .objects_iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        let outcome = execute_assistant_command(&mut staged, command);
        if let grafito_command::commands::CommandOutcome::Error(error) = outcome {
            return Err(error);
        }
        let created_ids = staged
            .objects_iter()
            .filter_map(|(id, _)| (!before_ids.contains(id)).then_some(*id))
            .collect::<Vec<_>>();
        for id in created_ids {
            if let Some(object) = staged.get_object_mut(id) {
                style_flower_component(object, &mut petal_index);
            }
        }
    }
    if !flower_scene_components_are_connected(&staged, &existing_ids) {
        return Err("La escena de flor debe formar una única figura conectada.".into());
    }

    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let initial_mesh = grafito_render::Renderer::build_3d_world_mesh(
        &inspection,
        &camera,
        inspection.view().screen_size.x,
        inspection.view().screen_size.y,
    );
    if !initial_mesh.is_complete() {
        return Err("La escena produjo una geometría 3D incompleta.".into());
    }
    initial_mesh
        .validate()
        .map_err(|_| "La escena produjo una geometría 3D inválida.".to_string())?;
    let fitted_camera = fit_camera_to_world_mesh(&initial_mesh, camera)?;
    let mesh = grafito_render::Renderer::build_3d_world_mesh(
        &inspection,
        &fitted_camera,
        inspection.view().screen_size.x,
        inspection.view().screen_size.y,
    );
    if !mesh.is_complete()
        || mesh.validate().is_err()
        || !world_mesh_intersects_view(
            &mesh,
            &fitted_camera,
            inspection.view().screen_size.x,
            inspection.view().screen_size.y,
        )
    {
        return Err("La escena no produjo una flor 3D visible.".into());
    }

    Ok(AssistantScenePreflight {
        staged,
        outcome: grafito_command::commands::CommandOutcome::Message(format!(
            "Escena 3D verificada: {} componentes.",
            commands.len()
        )),
        camera: fitted_camera,
        view: grafito_command::assistant_context::AssistantGraphView::ThreeD,
    })
}

fn flower_scene_components_are_connected(
    document: &grafito_core::Document,
    existing_ids: &std::collections::HashSet<grafito_core::ObjectId>,
) -> bool {
    let mut center = None;
    let mut stem_connected = false;
    let mut petals = Vec::new();

    for (id, object) in document.objects_iter() {
        if existing_ids.contains(id) {
            continue;
        }
        match object {
            grafito_core::GeoObject::Sphere3D(sphere) => {
                center = Some((sphere.center, sphere.radius));
            }
            grafito_core::GeoObject::Surface3D(surface) => petals.push(surface),
            _ => {}
        }
    }

    let Some((center, radius)) = center.filter(|(_, radius)| radius.is_finite() && *radius > 0.0)
    else {
        return false;
    };
    let connection_radius = radius + 0.05;

    for (id, object) in document.objects_iter() {
        if existing_ids.contains(id) {
            continue;
        }
        let connects_to_center = match object {
            grafito_core::GeoObject::Cylinder3D(stem) => {
                point_segment_distance(center, stem.base_center, stem.top_center)
                    .is_some_and(|distance| distance <= radius + stem.radius.abs())
            }
            grafito_core::GeoObject::Cone3D(stem) => {
                point_segment_distance(center, stem.base_center, stem.apex)
                    .is_some_and(|distance| distance <= radius + stem.radius.abs())
            }
            grafito_core::GeoObject::ParametricCurve3D(stem) => {
                grafito_core::parametric_sampling::evaluate_parametric_curve_3d(
                    stem,
                    128,
                    &document.variables,
                )
                .into_iter()
                .any(|(x, y, z)| {
                    let point = grafito_geometry::Point3D::new(x, y, z);
                    point.is_finite() && point.distance(&center) <= connection_radius
                })
            }
            _ => false,
        };
        stem_connected |= connects_to_center;
    }

    stem_connected
        && !petals.is_empty()
        && petals.into_iter().all(|petal| {
            grafito_core::parametric_sampling::evaluate_surface_3d(
                petal,
                petal.mesh_res.clamp(8, 32),
                &document.variables,
            )
            .into_iter()
            .flatten()
            .any(|point| point.is_finite() && point.distance(&center) <= connection_radius)
        })
}

fn point_segment_distance(
    point: grafito_geometry::Point3D,
    start: grafito_geometry::Point3D,
    end: grafito_geometry::Point3D,
) -> Option<f64> {
    if !point.is_finite() || !start.is_finite() || !end.is_finite() {
        return None;
    }
    let start = start.to_dvec3();
    let segment = end.to_dvec3() - start;
    let length_squared = segment.length_squared();
    if !length_squared.is_finite() {
        return None;
    }
    if length_squared <= 1.0e-24 {
        return Some(point.to_dvec3().distance(start));
    }
    let parameter = ((point.to_dvec3() - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    let distance = point.to_dvec3().distance(start + parameter * segment);
    distance.is_finite().then_some(distance)
}

fn style_flower_component(object: &mut grafito_core::GeoObject, petal_index: &mut usize) {
    match object {
        grafito_core::GeoObject::Cylinder3D(stem) => {
            stem.label = "Tallo".into();
            stem.color = grafito_geometry::Color::new(0.08, 0.45, 0.16, 1.0);
            stem.fill_color = Some(grafito_geometry::Color::new(0.12, 0.62, 0.24, 1.0));
        }
        grafito_core::GeoObject::Cone3D(stem) => {
            stem.label = "Tallo".into();
            stem.color = grafito_geometry::Color::new(0.08, 0.45, 0.16, 1.0);
            stem.fill_color = Some(grafito_geometry::Color::new(0.12, 0.62, 0.24, 1.0));
        }
        grafito_core::GeoObject::Sphere3D(center) => {
            center.label = "Centro de la flor".into();
            center.color = grafito_geometry::Color::new(0.75, 0.48, 0.02, 1.0);
            center.fill_color = Some(grafito_geometry::Color::new(1.0, 0.76, 0.08, 1.0));
        }
        grafito_core::GeoObject::Surface3D(petal) => {
            *petal_index += 1;
            petal.label = format!("Pétalo {petal_index}");
            petal.solid = true;
            petal.color = grafito_geometry::Color::new(0.9, 0.12, 0.42, 1.0);
            petal.width = 1.25;
        }
        _ => {}
    }
}

fn fit_camera_to_world_mesh(
    mesh: &grafito_render::WorldMesh,
    mut camera: grafito_geometry::Camera3D,
) -> Result<grafito_geometry::Camera3D, String> {
    let points = mesh
        .opaque_vertices
        .iter()
        .chain(&mesh.wire_vertices)
        .map(|vertex| glam::Vec3::from_array(vertex.position))
        .filter(|point| point.is_finite())
        .collect::<Vec<_>>();
    let Some(first) = points.first().copied() else {
        return Err("La escena no tiene vértices finitos para encuadrar.".into());
    };
    let (min, max) = points
        .into_iter()
        .fold((first, first), |(min, max), point| {
            (min.min(point), max.max(point))
        });
    let radius = ((max - min).length() * 0.5).max(0.5);
    let half_fov = (camera.fov.to_radians() * 0.5).clamp(0.1, 1.4);
    let half_horizontal = (half_fov.tan() * camera.aspect.max(0.25)).atan();
    let limiting_half_angle = half_fov.min(half_horizontal).max(0.1);
    let distance = (radius / limiting_half_angle.sin() * 1.35).max(2.0);
    camera.target = (min + max) * 0.5;
    camera.distance = distance;
    camera.near = (distance - radius * 2.5).max(0.01);
    camera.far = (distance + radius * 4.0 + 10.0).max(100.0);
    Ok(camera)
}

/// Ejecuta una propuesta sobre un documento aislado y exige que los objetos
/// nuevos emitan geometría propia, sin contar ejes, grilla ni objetos previos.
#[cfg(test)]
fn preflight_assistant_graph_command(
    document: &grafito_core::Document,
    command: &str,
) -> Result<AssistantGraphPreflight, String> {
    let command = grafito_command::assistant_proposals::parse_assistant_command(command)
        .ok_or_else(|| "La propuesta no es un gráfico verificable por el asistente.".to_string())?;
    preflight_assistant_graph_command_with_camera(
        document,
        &command,
        grafito_geometry::Camera3D::new(4.0 / 3.0),
    )
}

fn preflight_assistant_graph_command_with_camera(
    document: &grafito_core::Document,
    command: &AssistantCommandInvocation,
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantGraphPreflight, String> {
    let capability =
        grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
            .ok_or_else(|| {
                "La propuesta no es un gráfico verificable por el asistente.".to_string()
            })?;
    if !assistant_command_is_safe(command) {
        return Err(
            "La propuesta usa valores literales que no cumplen el catálogo verificable.".into(),
        );
    }

    let existing_ids = document
        .objects_iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    let mut staged = document.detached_clone_for_staging();
    let outcome = execute_assistant_command(&mut staged, command);
    if let grafito_command::commands::CommandOutcome::Error(error) = &outcome {
        return Err(error.clone());
    }

    let created_ids = staged
        .objects_iter()
        .filter_map(|(id, _)| (!existing_ids.contains(id)).then_some(*id))
        .collect::<Vec<_>>();
    if created_ids.is_empty() {
        return Err("La propuesta no creó un objeto gráfico nuevo.".into());
    }

    if created_ids.iter().any(|id| {
        staged
            .get_object(*id)
            .is_none_or(|object| match capability.view {
                grafito_command::assistant_context::AssistantGraphView::TwoD => {
                    object.render_space() != grafito_core::RenderSpace::D2
                }
                grafito_command::assistant_context::AssistantGraphView::ThreeD => {
                    object.render_space() != grafito_core::RenderSpace::D3
                }
            })
    }) {
        return Err("La propuesta creó un objeto fuera de la vista gráfica esperada.".into());
    }

    let mut inspection = staged.detached_clone_for_staging();
    for id in existing_ids {
        if let Some(object) = inspection.get_object_mut(id) {
            object.set_visible(false);
        }
    }
    let has_geometry = match capability.proof {
        grafito_command::assistant_context::AssistantGraphProof::StaticTwoD => {
            let (vertices, indices) = grafito_render::Renderer::build_geometry_static(
                &inspection,
                inspection.view(),
                false,
                false,
            );
            static_geometry_intersects_view(&vertices, &indices, inspection.view())
        }
        grafito_command::assistant_context::AssistantGraphProof::WorldMeshThreeD => {
            let screen_size = inspection.view().screen_size;
            let mesh = grafito_render::Renderer::build_3d_world_mesh(
                &inspection,
                &camera,
                screen_size.x,
                screen_size.y,
            );
            mesh.is_complete()
                && mesh.validate().is_ok()
                && world_mesh_intersects_view(&mesh, &camera, screen_size.x, screen_size.y)
        }
        grafito_command::assistant_context::AssistantGraphProof::CpuOverlayThreeD => {
            cpu_overlay_intersects_view(&inspection, &camera, inspection.view().screen_size)
        }
    };
    if !has_geometry {
        return Err("La propuesta no produjo geometría visible; no se aplicó al documento.".into());
    }

    Ok(AssistantGraphPreflight {
        staged,
        outcome,
        view: capability.view,
    })
}

fn preflight_assistant_graph_command_with_prerequisites(
    document: &grafito_core::Document,
    prerequisite_parameters: &[AssistantParameterAssignment],
    command: &AssistantCommandInvocation,
    camera: grafito_geometry::Camera3D,
) -> Result<AssistantGraphPreflight, String> {
    let staged = document_with_assistant_prerequisites(document, prerequisite_parameters)?;
    preflight_assistant_graph_command_with_camera(&staged, command, camera)
}

fn preflight_assistant_parameter(
    document: &grafito_core::Document,
    assignment: &AssistantParameterAssignment,
) -> Result<(), String> {
    let mut staged = document.detached_clone_for_staging();
    stage_assistant_parameter(&mut staged, assignment)
}

fn stage_assistant_parameter(
    document: &mut grafito_core::Document,
    assignment: &AssistantParameterAssignment,
) -> Result<(), String> {
    if let grafito_command::commands::CommandOutcome::Error(error) =
        execute_assistant_parameter(document, assignment)
    {
        return Err(error);
    }
    (document.get_variable(assignment.name()) == Some(assignment.value()))
        .then_some(())
        .ok_or_else(|| "La propuesta no actualizó el parámetro esperado.".into())
}

fn commit_assistant_graph_preflight(
    document: &mut grafito_core::Document,
    undo_stack: &mut VecDeque<grafito_core::Document>,
    redo_stack: &mut VecDeque<grafito_core::ChangeSet>,
    preflight: AssistantGraphPreflight,
) -> grafito_command::commands::CommandOutcome {
    let before = document.clone();
    *document = preflight.staged;
    crate::app::save_command_snapshot_if_mutated(
        &preflight.outcome,
        before,
        document,
        undo_stack,
        redo_stack,
    );
    preflight.outcome
}

fn assistant_command_is_safe(command: &AssistantCommandInvocation) -> bool {
    grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
        .is_some()
        && grafito_command::assistant_context::assistant_command_has_literal_safe_form(
            command.canonical_name(),
            command.arguments().len(),
        )
        && grafito_command::assistant_context::assistant_command_has_literal_safe_arguments(
            command.canonical_name(),
            command.arguments(),
        )
}

fn assistant_graph_view(
    command: &AssistantCommandInvocation,
) -> Option<grafito_command::assistant_context::AssistantGraphView> {
    grafito_command::assistant_context::assistant_graph_capability(command.canonical_name())
        .map(|capability| capability.view)
}

#[cfg(test)]
fn validate_assistant_command(candidate: &str) -> Option<String> {
    grafito_command::assistant_proposals::parse_assistant_command(candidate)
        .map(|command| command.canonical_text())
}

fn point_is_in_view(x: f32, y: f32, width: f32, height: f32) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && x <= width && y >= 0.0 && y <= height
}

fn static_geometry_intersects_view(
    vertices: &[grafito_render::Vertex],
    indices: &[u32],
    view: &grafito_geometry::ViewTransform,
) -> bool {
    !indices.is_empty()
        && vertices.iter().any(|vertex| {
            point_is_in_view(
                vertex.position[0],
                vertex.position[1],
                view.screen_size.x,
                view.screen_size.y,
            )
        })
}

fn world_mesh_intersects_view(
    mesh: &grafito_render::WorldMesh,
    camera: &grafito_geometry::Camera3D,
    width: f32,
    height: f32,
) -> bool {
    (!mesh.opaque_indices.is_empty() || !mesh.wire_indices.is_empty())
        && mesh
            .opaque_vertices
            .iter()
            .chain(&mesh.wire_vertices)
            .any(|vertex| {
                camera
                    .project(
                        &grafito_geometry::Point3D::new(
                            vertex.position[0] as f64,
                            vertex.position[1] as f64,
                            vertex.position[2] as f64,
                        ),
                        width,
                        height,
                    )
                    .is_some_and(|(x, y)| point_is_in_view(x, y, width, height))
            })
}

fn cpu_overlay_intersects_view(
    document: &grafito_core::Document,
    camera: &grafito_geometry::Camera3D,
    screen_size: glam::Vec2,
) -> bool {
    document.objects_iter().any(|(_, object)| match object {
        grafito_core::GeoObject::Point3D(point) => camera
            .project(&point.position, screen_size.x, screen_size.y)
            .is_some_and(|(x, y)| point_is_in_view(x, y, screen_size.x, screen_size.y)),
        grafito_core::GeoObject::HyperSurface4D(surface) => {
            surface
                .params
                .first()
                .is_some_and(|scale| scale.is_finite() && *scale > 0.0)
                && camera
                    .project(
                        &grafito_geometry::Point3D::new(0.0, 0.0, 0.0),
                        screen_size.x,
                        screen_size.y,
                    )
                    .is_some_and(|(x, y)| point_is_in_view(x, y, screen_size.x, screen_size.y))
        }
        _ => false,
    })
}

fn assistant_graph_perspective(
    view: grafito_command::assistant_context::AssistantGraphView,
    current_view: crate::ViewMode,
) -> Option<crate::Perspective> {
    match (view, current_view) {
        (grafito_command::assistant_context::AssistantGraphView::TwoD, crate::ViewMode::D3) => {
            Some(crate::Perspective::Geometry2D)
        }
        (grafito_command::assistant_context::AssistantGraphView::ThreeD, crate::ViewMode::D2) => {
            Some(crate::Perspective::Geometry3D)
        }
        _ => None,
    }
}

impl GrafitoApp {
    /// Asegura que el panel de Álgebra quede visible después de un Apply exitoso.
    fn ensure_algebra_panel_visible(&mut self) {
        self.left_drawer_open = true;
        self.compact_drawer_open = false;
        self.sidebar_tab = crate::LeftPanelContent::Algebra.default_sidebar_tab();
    }
}

fn load_assistant_attachment(
    path: PathBuf,
) -> Result<grafito_assistant_types::ImageAttachment, String> {
    let limits = AttachmentLimits::default();
    let file = File::open(path).map_err(|_| "No se pudo leer la imagen.".to_string())?;
    let bytes = read_bounded_attachment(file, limits.max_bytes)?;
    let format =
        image::guess_format(&bytes).map_err(|_| "La imagen debe ser PNG o JPEG.".to_string())?;
    let media_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err("La imagen debe ser PNG o JPEG.".into()),
    };
    let reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "No se pudieron leer las dimensiones de la imagen.".to_string())?;
    let attachment =
        grafito_assistant_types::ImageAttachment::new(media_type, bytes, width, height);
    validate_attachment(&attachment, &limits)?;
    Ok(attachment)
}

fn read_bounded_attachment(reader: impl Read, max_bytes: usize) -> Result<Vec<u8>, String> {
    let maximum = max_bytes
        .checked_add(1)
        .ok_or_else(|| "El límite de imagen no es válido.".to_string())?;
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
    reader
        .take(maximum as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "No se pudo leer la imagen.".to_string())?;
    if bytes.len() > max_bytes {
        return Err("La imagen supera el límite de tamaño permitido.".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        accepts_model_result, accepts_remote_context, accepts_remote_result,
        apply_local_assistant_plan, assistant_graph_perspective, attachment_error_message,
        can_offer_assistant_proposal_correction, classify_local_assistant_response,
        commit_assistant_graph_preflight, inspect_remote_action_proposals,
        inspect_remote_proposals, inspect_remote_proposals_cancellable,
        is_agent_spark_responses_unsupported_error, is_socratic_repair_error,
        pop_provisional_stream_turn, preflight_assistant_flower_scene,
        preflight_assistant_graph_command, preflight_assistant_graph_command_with_prerequisites,
        preflight_assistant_parameter, preflight_assistant_scene, read_bounded_attachment,
        remote_error_message, should_fallback_agent_spark_to_deepseek,
        should_fallback_remote_spark_to_deepseek, socratic_guard_context,
        stage_assistant_parameter, validate_assistant_command, verified_remote_proposals,
        AgentChannelMsg, AssistantAgentJob, AssistantAnimJob, AssistantCommandInvocation,
        AssistantModelJob, AssistantParameterAssignment, AssistantProposalJob, AssistantRemoteJob,
        AssistantRemoteRoute, AssistantRuntime, GifExportJob, LocalAssistantDisposition,
        RemoteProposalVerification,
    };
    use grafito_assistant::{solve_local, CancellationToken, RemoteCompletion};
    use grafito_assistant_types::{
        AssistantFocus, AssistantOperation, AssistantRepairFailure, AssistantRepairFailureKind,
        AssistantRepairFeedback, AssistantRequest, ConversationRole, ConversationTurn,
        ImmutableDocumentContext, ProposedPlan, ProviderProfile,
    };
    use grafito_command::commands::CommandOutcome;
    use grafito_core::{Document, GeoObject};
    use grafito_geometry::ViewTransform;
    use grafito_pedagogy::scaffold::{extract_concept, is_exploratory_request};
    use grafito_pedagogy::{PedagogicalLevel, ScaffoldEngine, SocraticFsm};
    use grafito_ui::assistant::{
        AssistantPanelState, AssistantProposal, VerifiedAssistantProposal,
    };
    use std::collections::VecDeque;
    use std::{io::Cursor, sync::mpsc::sync_channel};

    fn command_proposal(text: &str) -> AssistantProposal {
        AssistantProposal::Command(
            grafito_command::assistant_proposals::parse_assistant_command(text)
                .expect("test command must be recognized by the assistant contract"),
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

    fn assistant_commands(commands: &[String]) -> Vec<AssistantCommandInvocation> {
        commands
            .iter()
            .map(|command| {
                grafito_command::assistant_proposals::parse_assistant_command(command)
                    .expect("test command must be recognized by the assistant contract")
            })
            .collect()
    }

    fn scene_proposal(commands: &[&str]) -> AssistantProposal {
        AssistantProposal::Scene(
            commands
                .iter()
                .map(|command| {
                    grafito_command::assistant_proposals::parse_assistant_command(command)
                        .expect("test scene command must be recognized by the assistant contract")
                })
                .collect(),
        )
    }

    #[test]
    fn assistant_defaults_to_the_opencode_go_provider() {
        let state = AssistantPanelState::default();

        assert_eq!(state.provider, ProviderProfile::OpenCodeGo);
        assert_eq!(state.model, "deepseek-v4-flash");
        assert!(!state.allow_fusion_fallback);
    }

    #[test]
    fn local_arithmetic_is_classified_without_a_remote_request() {
        let response = solve_local(&AssistantRequest::local(
            "2 + 2",
            ImmutableDocumentContext::empty(0),
        ));

        assert!(matches!(
            classify_local_assistant_response(response),
            LocalAssistantDisposition::Solved { plan: None, .. }
        ));
    }

    #[test]
    fn unsupported_local_work_requires_explicit_remote_authorization() {
        let response = solve_local(&AssistantRequest::local(
            "Explicá el teorema de Stokes",
            ImmutableDocumentContext::empty(0),
        ));

        assert!(matches!(
            classify_local_assistant_response(response),
            LocalAssistantDisposition::NeedsRemoteAuthorization(_)
        ));
    }

    #[test]
    fn applying_a_local_plan_records_exactly_one_undo_snapshot() {
        let mut document = Document::new();
        let plan = ProposedPlan::new(
            grafito_command::assistant_context::document_context(&document).basis(),
            vec![
                AssistantOperation::SetVariable {
                    name: "a".into(),
                    value: 2.0,
                },
                AssistantOperation::CreateGraph {
                    expression: "x".into(),
                    variable: "x".into(),
                    domain_min: -2.0,
                    domain_max: 2.0,
                },
            ],
        );
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        let result =
            apply_local_assistant_plan(&mut document, &plan, &mut undo_stack, &mut redo_stack)
                .expect("a valid local plan must apply atomically");

        assert_eq!(result.changes.len(), 2);
        assert_eq!(document.get_variable("a"), Some(2.0));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn stale_local_plan_leaves_document_and_history_unchanged() {
        let mut document = Document::new();
        let plan = ProposedPlan::new(
            grafito_command::assistant_context::document_context(&document).basis(),
            vec![AssistantOperation::SetVariable {
                name: "a".into(),
                value: 2.0,
            }],
        );
        document.set_variable("changed".into(), 1.0);
        let before = serde_json::to_value(&document).expect("document serializes");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        assert!(
            apply_local_assistant_plan(&mut document, &plan, &mut undo_stack, &mut redo_stack,)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(&document).expect("document serializes"),
            before
        );
        assert!(undo_stack.is_empty());
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn parameter_proposals_accept_only_finite_numeric_assignments() {
        let assignment = grafito_command::assistant_proposals::parse_assistant_parameter("a = 2.5")
            .expect("finite parameter assignment");
        assert_eq!(assignment.name(), "a");
        assert_eq!(assignment.value(), 2.5);
        assert!(
            grafito_command::assistant_proposals::parse_assistant_parameter("a = NaN").is_none()
        );
        assert!(
            grafito_command::assistant_proposals::parse_assistant_parameter("a = 1; Save[]")
                .is_none()
        );

        let document = Document::new();
        assert!(preflight_assistant_parameter(&document, &assignment).is_ok());
    }

    #[test]
    fn parameter_proposals_recompute_spreadsheet_dependents() {
        let mut document = Document::new();
        document
            .try_set_variable("a".into(), 1.0)
            .expect("seed ordinary variable");
        document
            .set_spreadsheet_cell(0, 0, "a".into())
            .expect("seed spreadsheet cell");
        document
            .recompute_spreadsheet_variables()
            .expect("spreadsheet cell resolves");
        let assignment = parameter_assignment("a = 2");

        stage_assistant_parameter(&mut document, &assignment).expect("assistant parameter applies");
        assert_eq!(document.get_variable("A1"), Some(2.0));
    }

    #[test]
    fn ordered_parameter_proposals_enable_a_dependent_graph_without_mutating_live_state() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command_text = "ImplicitCurve[(x^2 + y^2 - a^2)^3 - x^2*y^3 = 0]";
        let response = format!("```grafito-param\na = 1\n```\n\n```grafito\n{command_text}\n```");

        let check = inspect_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(
            check.verified,
            vec![
                VerifiedAssistantProposal {
                    candidate_index: 0,
                    proposal: parameter_proposal("a = 1"),
                    prerequisite_parameters: Vec::new(),
                },
                VerifiedAssistantProposal {
                    candidate_index: 1,
                    proposal: command_proposal(command_text),
                    prerequisite_parameters: vec![parameter_assignment("a = 1")],
                },
            ]
        );
        assert_eq!(document.get_variable("a"), None);
        assert_eq!(document.object_count(), 0);
        assert!(preflight_assistant_graph_command(&document, command_text).is_err());

        let graph = &check.verified[1];
        let AssistantProposal::Command(command) = &graph.proposal else {
            panic!("the second proposal must be a graph command");
        };
        let preflight = preflight_assistant_graph_command_with_prerequisites(
            &document,
            &graph.prerequisite_parameters,
            command,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .expect("the explicit graph apply must recreate its verified parameter context");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();
        let outcome = commit_assistant_graph_preflight(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            preflight,
        );

        assert!(matches!(outcome, CommandOutcome::Message(_)));
        assert_eq!(document.get_variable("a"), Some(1.0));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn verified_parameter_does_not_suppress_one_explicit_graph_correction() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let check = inspect_remote_proposals(
            &document,
            "```grafito-param\na = 1\n```\n\n```grafito\nFunction[1/0]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.verified.len(), 1);
        assert_eq!(check.verified_action_count, 0);
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn rejected_proposals_produce_sanitized_repair_feedback() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response =
            "```grafito\nPolyhedron[NumericArray[{{0,0,0}}],NumericArray[{{0,1,2}}]]\n```";

        let check = inspect_remote_proposals(
            &document,
            response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert!(check.verified.is_empty());
        assert_eq!(check.action_candidate_count, 1);
        let feedback = check
            .repair_feedback
            .expect("rejected action has safe feedback");
        assert_eq!(feedback.failures.len(), 1);
        assert_eq!(feedback.failures[0].command, "Polyhedron");
        assert_eq!(
            feedback.failures[0].kind,
            AssistantRepairFailureKind::UnsupportedCommand
        );
        assert!(feedback.failures[0].expected_syntax.is_empty());
        assert!(!feedback.prompt_text().contains("NumericArray"));
    }

    #[test]
    fn stale_model_results_are_not_accepted_after_a_provider_switch() {
        assert!(accepts_model_result(
            ProviderProfile::OpenCodeGo,
            ProviderProfile::OpenCodeGo,
            false,
        ));
        assert!(!accepts_model_result(
            ProviderProfile::OllamaLocal,
            ProviderProfile::OpenCodeGo,
            false,
        ));
        assert!(!accepts_model_result(
            ProviderProfile::OpenCodeGo,
            ProviderProfile::OpenCodeGo,
            true,
        ));
    }

    #[test]
    fn remote_results_are_bound_to_the_model_that_started_them() {
        assert!(accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro"
        ));
        assert!(!accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-pro",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash"
        ));
    }

    #[test]
    fn remote_results_require_the_exact_document_and_focus_snapshot() {
        let context = ImmutableDocumentContext::from_variables(7, [("a".to_string(), 1.0)]);
        let focus = AssistantFocus::function("f", "x^2", None, None, false);

        assert!(accepts_remote_context(
            &context,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let mut changed_revision = context.clone();
        changed_revision.revision = 8;
        assert!(!accepts_remote_context(
            &changed_revision,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let changed_document =
            ImmutableDocumentContext::from_variables(7, [("a".to_string(), 2.0)]);
        assert!(!accepts_remote_context(
            &changed_document,
            Some(&focus),
            7,
            &context.digest,
            Some(&focus),
        ));

        let changed_focus = AssistantFocus::function("g", "x^3", None, None, false);
        assert!(!accepts_remote_context(
            &context,
            Some(&changed_focus),
            7,
            &context.digest,
            Some(&focus),
        ));
    }

    #[test]
    fn session_key_is_available_only_to_its_original_provider() {
        let mut runtime = AssistantRuntime::default();
        runtime.remember_key(ProviderProfile::OpenCodeGo, "session-key".into());

        assert_eq!(
            runtime.key_for(ProviderProfile::OpenCodeGo).as_deref(),
            Some("session-key")
        );
        assert!(runtime.key_for(ProviderProfile::DeepSeek).is_none());
    }

    #[test]
    fn session_fallback_model_overrides_expected_without_touching_preference() {
        let mut runtime = AssistantRuntime::default();
        // Sin fallback: se espera el modelo configurado.
        assert_eq!(
            runtime.expected_model("muse-spark-1.3-contributor"),
            "muse-spark-1.3-contributor"
        );
        // Con fallback activo: el resultado en vuelo trae el fallback...
        runtime.fallback_model = Some("deepseek-v4-flash".into());
        assert_eq!(
            runtime.expected_model("muse-spark-1.3-contributor"),
            "deepseek-v4-flash"
        );
        assert!(accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            runtime.expected_model("muse-spark-1.3-contributor"),
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
        ));
        // ...y el viejo modelo ya no se acepta mientras dura el fallback.
        assert!(!accepts_remote_result(
            ProviderProfile::OpenCodeGo,
            runtime.expected_model("muse-spark-1.3-contributor"),
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
    }

    #[test]
    fn error_429_reports_quota_not_model_or_key() {
        // 429 = cuota por minuto, no modelo ni clave: el mensaje no debe
        // mandar a reconfigurar nada.
        let quota = remote_error_message(
            "assistant agent returned HTTP 429: Model muse-spark-1.3-contributor rate limited",
            "muse-spark-1.3-contributor",
        );
        assert!(quota.contains("429"), "menciona el código: {quota}");
        assert!(
            quota.contains("Esperá"),
            "pide esperar, no reconfigurar: {quota}"
        );
        assert!(
            !quota.contains("Revisá Configuración → Modelo"),
            "no culpa al modelo: {quota}"
        );
    }

    #[test]
    fn agent_spark_success_never_falls_back_only_responses_error_does() {
        // Wiring B1: `Done(Ok)` con Spark se acepta directo — el fallback sólo
        // vive en la rama `Err` ("Responses API") y lo decide este helper.
        assert!(!is_agent_spark_responses_unsupported_error("ok"));
        assert!(!is_agent_spark_responses_unsupported_error("HTTP 500"));
        assert!(is_agent_spark_responses_unsupported_error(
            "agent tools are not supported via Responses API, use chat"
        ));
        assert!(should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
        // No dispara con otro modelo, otro proveedor u otro error.
        assert!(!should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
        ));
        assert!(!should_fallback_agent_spark_to_deepseek(
            "agent tools are not supported via Responses API",
            ProviderProfile::DeepSeek,
            "muse-spark-1.3-contributor",
        ));
        assert!(!should_fallback_agent_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
        ));
    }

    #[test]
    fn remote_spark_fallback_only_on_500_or_timeout_without_correction() {
        assert!(should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        assert!(should_fallback_remote_spark_to_deepseek(
            "request timed out after 60s",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
        // Con corrección en curso, otro modelo/proveedor u otro error: no.
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            1,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::OpenCodeGo,
            "deepseek-v4-flash",
            0,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 500 internal error",
            ProviderProfile::DeepSeek,
            "muse-spark-1.3-contributor",
            0,
        ));
        assert!(!should_fallback_remote_spark_to_deepseek(
            "HTTP 401 unauthorized",
            ProviderProfile::OpenCodeGo,
            "muse-spark-1.3-contributor",
            0,
        ));
    }

    #[test]
    fn session_key_remember_trims_pasted_whitespace() {
        let mut runtime = AssistantRuntime::default();
        runtime.remember_key(ProviderProfile::OpenCodeGo, "  sk-grafito-123\n".into());
        assert_eq!(
            runtime.key_for(ProviderProfile::OpenCodeGo).as_deref(),
            Some("sk-grafito-123")
        );
        runtime.forget_key();
        assert!(runtime.key_for(ProviderProfile::OpenCodeGo).is_none());
    }

    #[test]
    fn remote_errors_for_custom_and_validators_are_spanish() {
        // Error de validador Custom (inglés crudo) → español sin exponerlo.
        let custom = remote_error_message(
            "custom API key reference is invalid or reserved for a named provider",
            "modelo-custom",
        );
        assert!(
            custom.contains("No se pudo preparar") || custom.contains("Revisá"),
            "custom en español: {custom}"
        );
        // Identificador de modelo inválido → rama de modelo en español.
        let model_id = remote_error_message("remote model identifier is invalid", "x");
        assert!(
            model_id.contains("no está disponible"),
            "modelo en español: {model_id}"
        );
        // Endpoint Custom sin HTTPS → envoltorio español.
        let https = remote_error_message("custom OpenAI-compatible endpoints must use HTTPS", "x");
        assert!(
            https.contains("Revisá Configuración"),
            "endpoint en español: {https}"
        );
        // Responses API → mensaje específico en español.
        let responses = remote_error_message(
            "agent tools are not supported via Responses API",
            "muse-spark-1.3-contributor",
        );
        assert!(
            responses.contains("Responses API") && responses.contains("deepseek-v4-flash"),
            "responses en español: {responses}"
        );
    }

    #[test]
    fn attachment_errors_are_spanish_not_raw_english() {
        assert_eq!(
            attachment_error_message("assistant attachment byte limit exceeded"),
            "La imagen no es válida o supera los límites permitidos."
        );
        // Los mensajes ya españoles de grafito-ui pasan intactos.
        assert_eq!(
            attachment_error_message("se alcanzó el límite de adjuntos configurado"),
            "se alcanzó el límite de adjuntos configurado"
        );
    }

    #[test]
    fn cancel_all_jobs_marks_tokens_and_drops_anim_without_orphans() {
        let mut runtime = AssistantRuntime::default();
        let remote_cancel = CancellationToken::default();
        let (remote_tx, remote_rx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: remote_cancel.clone(),
            receiver: remote_rx,
            stream_rx: None,
            stream_text: String::new(),
            preview_active: false,
        });
        let proposal_cancel = CancellationToken::default();
        let (proposal_tx, proposal_rx) =
            sync_channel::<Result<RemoteProposalVerification, String>>(1);
        runtime.proposal_job = Some(AssistantProposalJob {
            id: 2,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "q".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            text: "t".into(),
            cancellation: proposal_cancel.clone(),
            receiver: proposal_rx,
        });
        let agent_cancel = grafito_agent::loop_engine::Cancellation::default();
        let (agent_tx, agent_rx) = sync_channel::<AgentChannelMsg>(128);
        let (_, clarification_rx) = sync_channel::<grafito_ui::assistant::PendingClarification>(4);
        runtime.agent_job = Some(AssistantAgentJob {
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            cancellation: agent_cancel.clone(),
            receiver: agent_rx,
            clarification_receiver: clarification_rx,
        });
        let model_cancel = CancellationToken::default();
        let (model_tx, model_rx) = sync_channel::<Result<Vec<String>, String>>(1);
        runtime.model_job = Some(AssistantModelJob {
            id: 3,
            provider: ProviderProfile::OpenCodeGo,
            cancellation: model_cancel.clone(),
            receiver: model_rx,
        });
        let (_anim_tx, anim_rx) =
            sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        let anim_cancel = CancellationToken::default();
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: anim_cancel.clone(),
            receiver: anim_rx,
        });

        assert!(runtime.cancel_all_assistant_jobs());
        assert!(remote_cancel.is_cancelled());
        assert!(proposal_cancel.is_cancelled());
        assert!(agent_cancel.is_cancelled());
        assert!(model_cancel.is_cancelled());
        // Anim AS4: señala el token como el resto y dropea el slot de inmediato.
        assert!(anim_cancel.is_cancelled());
        assert!(runtime.anim_job.is_none());
        // Los jobs con token conservan el slot hasta el drain (sin huérfanos).
        assert!(!runtime.remote_request_slot_is_free());

        // Drenar remote + proposal vía take_finished_* (marca cancelled).
        remote_tx
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        let finished_remote = runtime.take_finished_remote_job().unwrap();
        assert!(finished_remote.cancelled);
        proposal_tx.send(Err("cancelled".into())).unwrap();
        let finished_proposal = runtime.take_finished_proposal_job().unwrap();
        assert!(finished_proposal.cancelled);
        // El canal del agente sigue vivo hasta que el poll lo drene.
        agent_tx
            .send(AgentChannelMsg::Done(Err("cancelled".into())))
            .unwrap();
        let agent_msg = runtime
            .agent_job
            .as_ref()
            .expect("agent slot held until poll drain")
            .receiver
            .try_recv()
            .expect("agent channel must not be orphaned");
        assert!(matches!(agent_msg, AgentChannelMsg::Done(_)));
        // Model se drena vía take_finished_model_job.
        model_tx.send(Err("cancelled".into())).unwrap();
        assert!(runtime.take_finished_model_job().is_some());
        // Simula el drain del poll del agente: libera el slot remoto.
        runtime.agent_job = None;
        assert!(runtime.remote_request_slot_is_free());
        // Sin jobs, cancelar es no-op.
        assert!(!runtime.cancel_all_assistant_jobs());
    }

    #[test]
    fn anim_cancel_signals_token_instead_of_only_dropping_receiver() {
        // AS4 cancel real: descartar señala el token (el hilo lo chequea entre
        // frames en el closure de progreso). Headless, sin ventana ni render.
        let mut runtime = AssistantRuntime::default();
        assert!(!runtime.cancel_anim_job(), "sin job es no-op");
        let anim_cancel = CancellationToken::default();
        let (_anim_tx, anim_rx) =
            sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: anim_cancel.clone(),
            receiver: anim_rx,
        });
        assert!(!anim_cancel.is_cancelled());
        assert!(runtime.cancel_anim_job());
        assert!(
            anim_cancel.is_cancelled(),
            "descartar debe señalar el token"
        );
        assert!(runtime.anim_job.is_none());
        assert!(!runtime.cancel_anim_job(), "doble cancel es no-op");
    }

    #[test]
    fn anim_progress_closure_observes_token_between_frames() {
        // El render nativo no acepta token: el hilo lo chequea en el closure
        // de progreso. Este test pineado verifica el contrato sin render
        // pesado: el closure ve el token entre frames.
        let cancel = CancellationToken::default();
        let worker = cancel.clone();
        let mut saw: Vec<(usize, usize)> = Vec::new();
        let mut on_frame = |done: usize, total: usize| {
            saw.push((done, total));
            assert!(
                !worker.is_cancelled(),
                "sin cancel el closure no debe ver token"
            );
        };
        for frame in 1..=3 {
            on_frame(frame, 3);
        }
        assert_eq!(saw, vec![(1, 3), (2, 3), (3, 3)]);
        cancel.cancel();
        assert!(worker.is_cancelled(), "cancel debe verse entre frames");
    }

    #[test]
    fn pedido_con_animacion_instala_media_en_el_turno_con_prosa_humana() {
        // Pedido-con-animación → media instalada en el turno, sin ventana.
        // Headless: render clásico tiny (64x48, 48 frames) + transcript.
        use std::collections::BTreeMap;
        let pedido = "explica la derivada con animación";
        assert!(crate::anim_ui::wants_animation_request(pedido));
        let concepto = crate::anim_ui::animation_concept_from_request(pedido)
            .expect("con concepto debe validar");
        let plantilla = crate::anim_native::detect_template_for_concept(&concepto);
        assert_eq!(plantilla, "derivative-slope");
        let frames = crate::anim_native::render_anim_with_progress(
            plantilla,
            &concepto,
            64,
            48,
            &BTreeMap::new(),
            &mut |_, _| {},
        );
        assert!(!frames.is_empty(), "el nativo debe producir frames");
        let ctx = egui::Context::default();
        let mut panel = AssistantPanelState::default();
        panel.begin_request(pedido.to_string());
        let base = grafito_ui::assistant::humanize_prose_text("La derivada es la pendiente.");
        let mut prosa = base;
        prosa.push_str(
            "

",
        );
        prosa.push_str(crate::anim_ui::animation_reference_sentence());
        panel.complete_local_request(prosa.clone());
        let media = grafito_ui::assistant::AssistantMedia {
            title: format!("{concepto} (nativa)"),
            frames,
        };
        panel.set_media(Some(media), &ctx);
        // Media instalada para el reproductor del transcript (último turno).
        assert!(panel.media.is_some(), "media debe vivir en el turno");
        assert!(!panel.media.as_ref().expect("media").frames.is_empty());
        let ultimo = panel.conversation.last().expect("turno asistente");
        assert_eq!(ultimo.role, ConversationRole::Assistant);
        assert!(
            ultimo.content.contains("deslizador"),
            "prosa: {}",
            ultimo.content
        );
        assert!(ultimo.content.contains("reproducir"));
        for id in ["PlayPause", "Slider", "Button", "Tangent", "Select", "Play"] {
            assert!(!ultimo.content.contains(id), "prosa sin {id}");
            assert!(
                !panel.media.as_ref().expect("media").title.contains(id),
                "título sin {id}"
            );
        }
    }

    #[test]
    fn pedido_ambiguo_error_honesto_sin_media_ni_invento() {
        // Pedido ambiguo → error honesto sin media, jamás inventa frames.
        let ctx = egui::Context::default();
        for ambiguo in ["animalo", "con animación", "explica con animación"] {
            assert!(
                crate::anim_ui::wants_animation_request(ambiguo),
                "{ambiguo:?} pide animación"
            );
            let err = crate::anim_ui::animation_concept_from_request(ambiguo)
                .expect_err("ambiguo debe fallar honesto");
            assert!(
                err.contains("qué animar") || err.contains("vacío"),
                "guía útil: {err}"
            );
            assert!(err.contains("por ejemplo"), "dice qué pedir: {err}");
            let mut panel = AssistantPanelState::default();
            panel.begin_request(ambiguo.to_string());
            let honesto = grafito_ui::assistant::humanize_prose_text(&err);
            panel.complete_local_request(honesto.clone());
            panel.set_media(None, &ctx);
            assert!(panel.media.is_none(), "sin media rancia en {ambiguo:?}");
            let ultimo = panel.conversation.last().expect("turno guía");
            assert!(ultimo.content.contains("por ejemplo"));
        }
        // Sin gatillo no es animación (no debe disparar hilo).
        assert!(!crate::anim_ui::wants_animation_request("derivá x^2"));
        assert!(crate::anim_ui::animation_concept_from_request("derivá x^2").is_err());
    }

    #[test]
    fn export_sin_media_falla_honesto_sin_hilo() {
        // Sin animación: nada se spawnea, la card muestra el motivo y hay aviso.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        app.export_assistant_media(&ctx);
        assert!(app.assistant_runtime.gif_export_job.is_none());
        assert!(
            matches!(
                app.assistant.media_export_state(),
                grafito_ui::assistant::MediaExportState::Failed(_)
            ),
            "sin frames el error es honesto, jamás mudo"
        );
    }

    #[test]
    fn export_no_duplica_job_en_vuelo() {
        // Slot ocupado: avisa y no pisa el hilo existente.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        app.assistant_runtime.gif_export_job = Some(GifExportJob {
            handle: std::thread::spawn(|| Ok(std::path::PathBuf::from("ocupado"))),
            frame_count: 1,
        });
        app.export_assistant_media(&ctx);
        assert!(
            app.assistant_runtime.gif_export_job.is_some(),
            "no pisa el job en vuelo"
        );
        // Limpia sin colgar (el hilo ya terminó).
        let job = app
            .assistant_runtime
            .gif_export_job
            .take()
            .expect("job en vuelo");
        let _ = job.handle.join();
    }

    #[test]
    fn export_con_frames_corre_en_hilo_y_publica_exito_con_ruta() {
        // Cableado real: `spawn_gif_export` escribe un GIF válido y el poll
        // publica `Done` (ruta avisada por el aviso). Limpia su temporal.
        // El prefijo `grafito_animacion_` solo lo crea este export.
        let mut app = crate::app::dummy_grafito_app();
        let ctx = egui::Context::default();
        let frames = vec![egui::ColorImage::new([8, 8], egui::Color32::RED); 3];
        app.assistant.set_media(
            Some(grafito_ui::assistant::AssistantMedia {
                title: "prueba".into(),
                frames,
            }),
            &ctx,
        );
        app.export_assistant_media(&ctx);
        assert!(app.assistant_runtime.gif_export_job.is_some());
        assert_eq!(
            *app.assistant.media_export_state(),
            grafito_ui::assistant::MediaExportState::Exporting
        );
        for _ in 0..200 {
            app.poll_gif_export_job(&ctx);
            if app.assistant_runtime.gif_export_job.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            app.assistant_runtime.gif_export_job.is_none(),
            "el hilo debe terminar"
        );
        assert_eq!(
            *app.assistant.media_export_state(),
            grafito_ui::assistant::MediaExportState::Done
        );
        // El GIF existe y es real; se borra para no ensuciar el temporal.
        let mut cleaned = 0;
        let entries = std::fs::read_dir(std::env::temp_dir()).expect("temporal legible");
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_ours = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("grafito_animacion_"));
            if !is_ours {
                continue;
            }
            let bytes = std::fs::read(&path).expect("GIF exportado legible");
            assert_eq!(&bytes[0..6], b"GIF89a", "GIF real con cabecera");
            std::fs::remove_file(&path).expect("limpia su temporal");
            cleaned += 1;
        }
        assert_eq!(cleaned, 1, "un solo GIF de este export");
    }

    #[test]
    fn cancel_a_mitad_senala_token_y_descarta_media_rancia() {
        // Cancel a mitad → token señalado y sin media rancia en el turno.
        let mut runtime = AssistantRuntime::default();
        let cancel = CancellationToken::default();
        let (tx, rx) = sync_channel::<Result<grafito_ui::assistant::AssistantMedia, String>>(1);
        runtime.anim_job = Some(AssistantAnimJob {
            cancellation: cancel.clone(),
            receiver: rx,
        });
        assert!(runtime.cancel_anim_job(), "debe haber job");
        assert!(cancel.is_cancelled(), "descartar señala el token");
        assert!(runtime.anim_job.is_none());
        // El hilo en vuelo observa el token entre frames y su envío tardío
        // (Err cancel) no tiene receiver: se descarta sin publicar.
        // El slot ya se dropeó, así que el turno queda sin media rancia.
        drop(tx);
        let ctx = egui::Context::default();
        let mut panel = AssistantPanelState::default();
        panel.set_media(None, &ctx);
        assert!(panel.media.is_none(), "cancel no deja media rancia");
        // El closure de progreso del hilo habría visto el token (contrato).
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn cancelled_remote_job_remains_nonretryable_until_its_joiner_is_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<RemoteCompletion, String>>(1);
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-pro".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "consulta".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 3,
            document_digest: "fnv1a64:request".into(),
            focus: None,
            cancellation: cancellation.clone(),
            receiver,
            stream_rx: None,
            stream_text: String::new(),
            preview_active: false,
        });

        assert!(runtime.cancel_stale_remote_job(ProviderProfile::DeepSeek, "deepseek-chat"));
        assert!(cancellation.is_cancelled());
        assert!(!runtime.remote_request_slot_is_free());

        sender
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        assert!(!runtime.remote_request_slot_is_free());
        let finished = runtime.take_finished_remote_job().unwrap();
        assert_eq!(finished.document_revision, 3);
        assert_eq!(finished.document_digest, "fnv1a64:request");
        assert!(finished.focus.is_none());
        assert!(runtime.remote_request_slot_is_free());
    }

    #[test]
    fn stream_preview_drains_to_provisional_bubble_and_pops_on_finish() {
        let mut runtime = AssistantRuntime::default();
        let (result_tx, result_rx) = sync_channel::<Result<RemoteCompletion, String>>(1);
        let (delta_tx, delta_rx) = sync_channel::<String>(128);
        let cancel = CancellationToken::default();
        runtime.remote_job = Some(AssistantRemoteJob {
            id: 1,
            provider: ProviderProfile::OpenCodeGo,
            model: "muse-spark-1.3-contributor".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "derivá x^2".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 1,
            document_digest: "d".into(),
            focus: None,
            cancellation: cancel.clone(),
            receiver: result_rx,
            stream_rx: Some(delta_rx),
            stream_text: String::new(),
            preview_active: false,
        });
        let mut panel = AssistantPanelState::default();
        panel
            .conversation
            .push(ConversationTurn::user("derivá x^2"));
        let ctx = egui::Context::default();

        // Sin deltas no hay burbuja provisional.
        assert!(!runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 1);

        delta_tx.send("Hola ".into()).unwrap();
        delta_tx.send("mundo".into()).unwrap();
        assert!(runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 2);
        let provisional = panel.conversation.last().unwrap();
        assert_eq!(provisional.role, ConversationRole::Assistant);
        assert_eq!(provisional.content, "Hola mundo");

        // Más deltas actualizan el mismo turno (no duplican burbujas).
        delta_tx.send("!".into()).unwrap();
        assert!(runtime.drain_remote_stream_preview(&mut panel, &ctx));
        assert_eq!(panel.conversation.len(), 2);
        assert_eq!(panel.conversation.last().unwrap().content, "Hola mundo!");

        // Al terminar (cancelado), el poll retira el provisional y queda el
        // turno de usuario, igual que en el path no-streaming.
        cancel.cancel();
        result_tx
            .send(Err("remote assistant request was cancelled".into()))
            .unwrap();
        let finished = runtime.take_finished_remote_job().unwrap();
        assert!(finished.cancelled);
        assert!(finished.stream_preview_active);
        pop_provisional_stream_turn(&mut panel);
        assert_eq!(panel.conversation.len(), 1);
        assert_eq!(
            panel.conversation.last().unwrap().role,
            ConversationRole::User
        );
    }

    #[test]
    fn pop_provisional_stream_turn_never_touches_real_history() {
        let mut panel = AssistantPanelState::default();
        // Sin turnos o con último turno de usuario: no-op.
        pop_provisional_stream_turn(&mut panel);
        assert!(panel.conversation.is_empty());
        panel.conversation.push(ConversationTurn::user("hola"));
        pop_provisional_stream_turn(&mut panel);
        assert_eq!(panel.conversation.len(), 1);
    }

    #[test]
    fn socratic_guard_context_seeds_attempts_from_heuristic_answers() {
        // Fresca: sólo la pregunta actual → attempts=0, tema sanitizado a canónico.
        // `derivá x^2` extrae `derivada` (verbo→sustantivo), jamás el crudo.
        let fresh = vec![ConversationTurn::user("derivá x^2")];
        let guard = socratic_guard_context(8, None, "derivá x^2", &fresh);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "derivada");
        assert!(guard.scaffold.question.contains("derivada"));
        assert!(!guard.scaffold.question.contains("derivá x^2"));

        // Sin tema reconocido → tópico vacío + scaffold fallback (nunca crudo).
        let raw = "hola haceme ejemplos para probar las capacidades de graficacion";
        let demo = vec![ConversationTurn::user(raw)];
        let guard = socratic_guard_context(8, None, raw, &demo);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "");
        assert_eq!(
            guard.scaffold.question,
            grafito_pedagogy::scaffold::NO_CONCEPT_FALLBACK_QUESTION
        );
        assert!(!guard.scaffold.question.contains("hola"));

        // Un intercambio SIN `?` previa → attempts=0 (no es respuesta heurística).
        let one_exchange = vec![
            ConversationTurn::user("primera"),
            ConversationTurn::assistant("re-pregunta"),
            ConversationTurn::user("segunda"),
        ];
        let guard = socratic_guard_context(8, Some("derivada"), "segunda", &one_exchange);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "derivada");

        // Una respuesta a `?` previa → attempts=1 (sigue bloqueado, <2).
        let one_answer = vec![
            ConversationTurn::user("quiero ver derivada"),
            ConversationTurn::assistant("¿qué forma te imaginás?"),
            ConversationTurn::user("mi intento"),
            ConversationTurn::assistant("¿y ahora qué ves?"),
            ConversationTurn::user("segunda"),
        ];
        let guard = socratic_guard_context(8, Some("derivada"), "segunda", &one_answer);
        assert_eq!(guard.fsm.attempts, 1);
        assert_eq!(guard.fsm.topic, "derivada");

        // Dos respuestas a `?` → attempts=2 (reveal permitido).
        let two_answers = vec![
            ConversationTurn::user("init"),
            ConversationTurn::assistant("¿primera pregunta?"),
            ConversationTurn::user("ans1"),
            ConversationTurn::assistant("¿segunda pregunta?"),
            ConversationTurn::user("ans2"),
            ConversationTurn::assistant("¿tercera pregunta?"),
            ConversationTurn::user("tres"),
        ];
        let guard = socratic_guard_context(8, Some("  "), "tres", &two_answers);
        assert_eq!(guard.fsm.attempts, 2);
        // Tema en blanco + pregunta sin tema → tópico vacío (fallback, no crudo).
        assert_eq!(guard.fsm.topic, "");
    }

    #[test]
    fn socratic_repair_error_is_detected_by_prefix() {
        // Diagnóstico interno (solo logs/detección) conserva el prefijo.
        assert!(is_socratic_repair_error(
            "GUARD TELLING — REPARACIÓN SOCRÁTICA OBLIGATORIA (attempts=0 <2, ...)"
        ));
        assert!(!is_socratic_repair_error(
            "remote assistant returned HTTP 500: boom"
        ));
        assert!(!is_socratic_repair_error(
            "remote assistant request was cancelled"
        ));
        // La voz de estudiante (lo que SÍ va al transcript) jamás lleva el prefijo.
        let fsm = SocraticFsm::new("derivada");
        let scaffold =
            ScaffoldEngine.scaffold("derivada", PedagogicalLevel::from_level_value(8), &[]);
        let student = fsm.repair_student_message(&scaffold);
        assert!(!is_socratic_repair_error(&student));
        assert!(!student.contains("GUARD"), "{student}");
    }

    #[test]
    fn socratic_repair_turn_has_no_jargon_or_raw_echo_red_first() {
        // Regresión P0 red-first con el input real que mostró la directiva interna.
        // Falla si el turno contiene jerga o eco degenerado del saludo.
        let raw = "hola haceme ejemplos para probar las capacidades de graficacion";
        // 1. Es exploratorio y sin tema: el extractor y el scaffold no interpolan crudo.
        assert!(is_exploratory_request(raw));
        assert_eq!(extract_concept(raw), None);
        let scaffold = ScaffoldEngine.scaffold(raw, PedagogicalLevel::from_level_value(8), &[]);
        assert_eq!(
            scaffold.question,
            grafito_pedagogy::scaffold::NO_CONCEPT_FALLBACK_QUESTION
        );
        // 2. El guard sanitiza: tópico vacío (fallback), attempts=0 no punitivo.
        let conversation = vec![ConversationTurn::user(raw)];
        let guard = socratic_guard_context(8, None, raw, &conversation);
        assert_eq!(guard.fsm.attempts, 0);
        assert_eq!(guard.fsm.topic, "");
        // 3. La voz de Mili (lo único que va al transcript) no tiene jerga ni eco.
        let fsm = SocraticFsm::new(guard.fsm.topic.clone());
        let turno = fsm.repair_student_message(&guard.scaffold);
        for forbidden in [
            "GUARD",
            "REPARACIÓN",
            "REPARACION",
            "attempts",
            "can_reveal",
            "Re-pregunt",
            "¿Te imaginás hola",
        ] {
            assert!(
                !turno.contains(forbidden),
                "el turno contiene '{forbidden}': '{turno}'"
            );
        }
        assert!(turno.contains("Antes de mostrarte"), "{turno}");
    }

    #[test]
    fn proposal_verification_keeps_the_remote_slot_locked_until_its_worker_is_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<RemoteProposalVerification, String>>(1);
        runtime.proposal_job = Some(AssistantProposalJob {
            id: 2,
            provider: ProviderProfile::OpenCodeGo,
            model: "deepseek-v4-pro".into(),
            route: AssistantRemoteRoute::SelectedModel,
            fusion_fallback_allowed: false,
            question: "dibujá un corazon".into(),
            correction_attempt: 0,
            repair_target_turn: None,
            document_revision: 3,
            document_digest: "fnv1a64:request".into(),
            focus: None,
            text: "respuesta remota".into(),
            cancellation: cancellation.clone(),
            receiver,
        });

        assert!(!runtime.remote_request_slot_is_free());
        assert!(runtime.cancel_stale_remote_job(ProviderProfile::DeepSeek, "deepseek-chat"));
        assert!(cancellation.is_cancelled());

        sender
            .send(Ok(RemoteProposalVerification {
                verified: Vec::new(),
                candidate_count: 1,
                candidate_code_block_indices: vec![0],
                action_candidate_count: 1,
                verified_action_count: 0,
                repair_feedback: None,
            }))
            .unwrap();
        let finished = runtime.take_finished_proposal_job().unwrap();
        assert_eq!(finished.text, "respuesta remota");
        assert!(finished.cancelled);
        assert!(runtime.remote_request_slot_is_free());
    }

    #[test]
    fn cancelled_proposal_verification_stops_before_preflighting_remote_commands() {
        let document = Document::new();
        let cancellation = CancellationToken::default();
        cancellation.cancel();

        let result = inspect_remote_proposals_cancellable(
            &document,
            "```grafito\nFunction[sin(x)]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
            &cancellation,
            false,
        );

        assert!(matches!(result, Err(error) if error.contains("canceló")));
    }

    #[test]
    fn repeated_provider_switches_and_refreshes_keep_one_model_worker_until_reaped() {
        let mut runtime = AssistantRuntime::default();
        let cancellation = CancellationToken::default();
        let (sender, receiver) = sync_channel::<Result<Vec<String>, String>>(1);
        runtime.model_job = Some(AssistantModelJob {
            id: 7,
            provider: ProviderProfile::OpenCodeGo,
            cancellation: cancellation.clone(),
            receiver,
        });

        assert!(runtime.cancel_stale_model_job(ProviderProfile::DeepSeek));
        assert!(cancellation.is_cancelled());
        for provider in [
            ProviderProfile::OllamaLocal,
            ProviderProfile::DeepSeek,
            ProviderProfile::OpenCodeGo,
        ] {
            assert!(!runtime.cancel_stale_model_job(provider));
            assert!(!runtime.request_model_refresh());
            assert_eq!(runtime.model_job.as_ref().map(|job| job.id), Some(7));
        }
        assert!(!runtime.take_queued_model_refresh_if_idle());

        sender
            .send(Err("remote model request was cancelled".into()))
            .unwrap();
        assert!(runtime.take_finished_model_job().is_some());
        assert!(runtime.take_queued_model_refresh_if_idle());
        assert!(!runtime.take_queued_model_refresh_if_idle());
    }

    #[test]
    fn assistant_command_handoff_accepts_only_verified_graph_proposals() {
        assert_eq!(
            validate_assistant_command("Function[sin(x)]").as_deref(),
            Some("Function[sin(x)]")
        );
        assert_eq!(
            validate_assistant_command("DomainColoring[1/z, -2, 2, -2, 2, 160]").as_deref(),
            Some("DomainColoring[1/z, -2, 2, -2, 2, 160]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[x^2+y^2, -2, 2, -2, 2]").as_deref(),
            Some("Surface3D[x^2+y^2, -2, 2, -2, 2]")
        );
        assert_eq!(
            validate_assistant_command("PolarCurve[1, 0, 2*pi]").as_deref(),
            Some("PolarCurve[1, 0, 2*pi]")
        );
        assert_eq!(
            validate_assistant_command("Segment3D[0, 0, 0, 1, 0, 0]").as_deref(),
            Some("Segment3D[0, 0, 0, 1, 0, 0]")
        );
        assert_eq!(
            validate_assistant_command("Tetrahedron[0, 0, 0, 2]").as_deref(),
            Some("Tetrahedron[0, 0, 0, 2]")
        );
        assert_eq!(
            validate_assistant_command("Tesseract4D[]").as_deref(),
            Some("Tesseract4D[]")
        );
        assert_eq!(
            validate_assistant_command("HypercubeND[4, 1, {0,0,0,0,0,0}]").as_deref(),
            Some("HypercubeND[4, 1, {0,0,0,0,0,0}]")
        );
        assert_eq!(
            validate_assistant_command("Aizawa[]").as_deref(),
            Some("Aizawa[]")
        );
        assert_eq!(
            validate_assistant_command("Mandelbrot[]").as_deref(),
            Some("Mandelbrot[]")
        );
        assert_eq!(
            validate_assistant_command("ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
                .as_deref(),
            Some("ImplicitCurve[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]")
        );
        assert_eq!(
            validate_assistant_command("ImplicitCurve[x^2 + y^2, 1, <=]").as_deref(),
            Some("ImplicitCurve[x^2 + y^2, 1, <=]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[(cos(u), sin(u), v), 0, 2*pi, -1, 1]").as_deref(),
            Some("Surface3D[(cos(u), sin(u), v), 0, 2*pi, -1, 1]")
        );
        assert_eq!(
            validate_assistant_command("Surface3D[cos(u), sin(u), v, 0, 2*pi, -1, 1]").as_deref(),
            Some("Surface3D[cos(u), sin(u), v, 0, 2*pi, -1, 1]")
        );
        assert_eq!(
            validate_assistant_command("DomainColoring[1/z]").as_deref(),
            Some("DomainColoring[1/z]")
        );
        assert!(
            validate_assistant_command("DomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, r]")
                .is_none()
        );
        for command in [
            "Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]",
            "Chen[35, 3, 28]",
            "Halvorsen[1.4]",
            "Dadras[3, 2.7, 1.7, 2, 9]",
            "Chua[15.6, 28, -1.143, -0.714]",
            "Hypersphere[]",
        ] {
            assert_eq!(
                validate_assistant_command(command).as_deref(),
                Some(command)
            );
        }
        for command in [
            "Chen[35, 3, 28, 0]",
            "Halvorsen[1.4, 0]",
            "Dadras[3, 2.7, 1.7, 2, 9, 0]",
            "Chua[15.6, 28, -1.143, -0.714, 0]",
            "Hypersphere[1]",
        ] {
            assert!(
                validate_assistant_command(command).is_none(),
                "{command} must be rejected by the assistant arity gate"
            );
        }
        assert!(validate_assistant_command("Function[]").is_none());
        assert!(validate_assistant_command("Analyze[f]").is_none());
        assert!(validate_assistant_command("ParametricCurve2D[cos(t), sin(t), 0]").is_none());
        assert!(validate_assistant_command("Histogram[{1, 2, 3}]").is_none());
        assert!(validate_assistant_command("Line3D[A, B]").is_none());
        assert!(validate_assistant_command("Plane3D[A, B, C]").is_none());
        assert!(validate_assistant_command("Script[Save[]]").is_none());
        assert!(validate_assistant_command("Save[file]").is_none());
        assert!(validate_assistant_command("Import[data.csv]").is_none());
        assert!(validate_assistant_command("Function[x]; Analyze[f]").is_none());
        assert!(validate_assistant_command("Unknown[x]").is_none());
    }

    #[test]
    fn heart_alias_is_preflighted_before_it_becomes_an_assistant_action() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command = "ImplicitRegion[(x^2 + y^2 - 1)^3 - x^2*y^3 = 0]";
        let response = format!("```grafito\n{command}\n```");

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0)
            ),
            vec![command_proposal(command)]
        );
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn tetrahedron_fence_is_verified_without_mutating_the_live_document() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let command = "Tetrahedron[0, 0, 0, 2]";
        let response = format!("```grafito\n{command}\n```");

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0),
            ),
            vec![command_proposal(command)]
        );
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn named_regular_polytope_fences_are_verified_without_mutating_live_document() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "Tesseract4D[1.5,{0.1,-0.2,0.3,-0.4,0.5,-0.6}]",
            "SimplexND[5,1.25,{0.1,-0.2,0.3,-0.4,0.5,-0.6,0.7,-0.8,0.9,-1.0}]",
        ] {
            let response = format!("```grafito\n{command}\n```");
            assert_eq!(
                verified_remote_proposals(
                    &document,
                    &response,
                    grafito_geometry::Camera3D::new(4.0 / 3.0),
                ),
                vec![command_proposal(command)],
                "{command} must be an actionable verified fence"
            );
            assert_eq!(document.object_count(), 0, "preflight must stay isolated");
        }
    }

    #[test]
    fn assistant_repair_allows_two_bounded_passes_after_a_rejected_graph() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "Polyhedron".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };

        assert!(can_offer_assistant_proposal_correction(
            0,
            1,
            0,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            2,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            0,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            1,
            1,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            0,
            0,
            Some(&feedback),
        ));
    }

    #[test]
    fn explicit_assistant_correction_can_repair_an_attachment_bearing_rejection() {
        let feedback = AssistantRepairFeedback {
            failures: vec![AssistantRepairFailure {
                command: "UnsupportedGraph".into(),
                kind: AssistantRepairFailureKind::UnsupportedCommand,
                expected_syntax: Vec::new(),
            }],
        };

        assert!(can_offer_assistant_proposal_correction(
            0,
            1,
            0,
            Some(&feedback),
        ));
        assert!(can_offer_assistant_proposal_correction(
            1,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            2,
            1,
            0,
            Some(&feedback),
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            0,
            0,
            Some(&feedback)
        ));
        assert!(!can_offer_assistant_proposal_correction(
            0,
            1,
            1,
            Some(&feedback)
        ));
    }

    #[test]
    fn rejected_scenes_can_offer_one_explicit_repair() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response = "```grafito-scene\nCylinder[0, 0, 0, 0.16, 3]\nSphere[0, 3, 0, 0.45]\n```";
        let check = inspect_remote_proposals(
            &document,
            response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.candidate_count, 1);
        assert_eq!(check.action_candidate_count, 1);
        assert!(check.verified.is_empty());
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn malformed_scene_fences_can_offer_one_explicit_repair() {
        let document = Document::new();
        let check = inspect_remote_proposals(
            &document,
            "```grafito-scene\nCylinder[0, 0, 0, 0.16, 3]\n```",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.candidate_count, 1);
        assert_eq!(check.action_candidate_count, 1);
        assert!(check.verified.is_empty());
        let feedback = check
            .repair_feedback
            .as_ref()
            .expect("a malformed scene must offer sanitized feedback");
        assert!(matches!(
            feedback.failures.as_slice(),
            [AssistantRepairFailure {
                command,
                kind: AssistantRepairFailureKind::InvalidSyntax,
                ..
            }] if command == "Scene"
        ));
        assert!(can_offer_assistant_proposal_correction(
            0,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn initial_prose_response_is_not_a_proposal_failure() {
        let document = Document::new();
        let check = inspect_remote_proposals(
            &document,
            "La gráfica se puede estudiar con análisis complejo.",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.action_candidate_count, 0);
        assert_eq!(check.verified_action_count, 0);
        assert!(check.repair_feedback.is_none());
    }

    #[test]
    fn prose_only_repair_response_keeps_one_bounded_retry_available() {
        let document = Document::new();
        let check = inspect_remote_action_proposals(
            &document,
            "La gráfica se puede estudiar con análisis complejo.",
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(check.action_candidate_count, 0);
        assert_eq!(check.verified_action_count, 0);
        let feedback = check
            .repair_feedback
            .as_ref()
            .expect("a repair must request an action");
        assert_eq!(feedback.failures.len(), 1);
        assert_eq!(feedback.failures[0].command, "GraphProposal");
        assert_eq!(
            feedback.failures[0].kind,
            AssistantRepairFailureKind::InvalidSyntax
        );
        assert!(can_offer_assistant_proposal_correction(
            1,
            check.action_candidate_count,
            check.verified_action_count,
            check.repair_feedback.as_ref(),
        ));
    }

    #[test]
    fn assistant_graph_preflight_commits_only_drawable_staged_commands() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        let preflight = preflight_assistant_graph_command(&document, "Function[sin(x)/(x^2+1)]")
            .expect("finite function must be drawable");
        assert!(matches!(preflight.outcome, CommandOutcome::Message(_)));
        assert!(preflight
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Function(_))));
        assert_eq!(
            document.object_count(),
            0,
            "preflight must not mutate live state"
        );

        let complex =
            preflight_assistant_graph_command(&document, "DomainColoring[1/z, -2, 2, -2, 2, 160]")
                .expect("domain coloring must be drawable");
        assert!(complex.staged.objects_iter().any(
            |(_, object)| matches!(object, GeoObject::ComplexGrid(grid) if grid.render_mode == 1)
        ));

        let surface =
            preflight_assistant_graph_command(&document, "Surface3D[x^2+y^2, -2, 2, -2, 2]")
                .expect("finite 3d surface must be drawable");
        assert!(surface
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Surface3D(_))));

        let hypercube = preflight_assistant_graph_command(&document, "Hypercube[]")
            .expect("4d CPU projection must be drawable");
        assert!(hypercube
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::HyperSurface4D(_))));

        let mandelbrot = preflight_assistant_graph_command(&document, "Mandelbrot[]")
            .expect("zero-argument Mandelbrot must use its bounded default");
        assert!(mandelbrot
            .staged
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::Fractal2D(_))));

        for command in [
            "Function[1/0]",
            "Function[1000000]",
            "DomainColoring[not valid, -2, 2, -2, 2, 160]",
            "Cube[2000, 0, 0, 1]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_err(),
                "{command} must not be applied"
            );
        }
        assert_eq!(
            document.object_count(),
            0,
            "rejected preflight must remain atomic"
        );
    }

    #[test]
    fn assistant_preflight_accepts_standard_logarithmic_and_exponential_functions() {
        let document = Document::new();

        for command in [
            "Function[log(x)]",
            "Function[exp(x)]",
            "Function[(4/pi)*(sin(x)+sin(3*x)/3+sin(5*x)/5)]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_ok(),
                "{command} should be a verified assistant proposal"
            );
        }
    }

    #[test]
    fn assistant_functions_do_not_autodefine_remote_symbols() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        assert!(preflight_assistant_graph_command(&document, "Function[sin(k*x)/k]").is_err());
        assert_eq!(document.get_variable("k"), None);

        document.set_variable("k".into(), 3.0);
        assert!(preflight_assistant_graph_command(&document, "Function[sin(k*x)/k]").is_ok());
    }

    #[test]
    fn literal_safe_graph_capabilities_preflight_across_every_supported_render_mode() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "ParametricCurve2D[cos(t), sin(t), 0, 6.28]",
            "PolarCurve[1, 0, 6.28]",
            "ImplicitCurve[x^2+y^2=1]",
            "Contour[x^2+y^2, -2, 2, -2, 2, 1]",
            "VectorField2D[-y, x]",
            "PhasePortrait[y, -x]",
            "ComplexGrid[z^2]",
            "HeatMap[x+y, -2, 2, -2, 2, 64]",
            "Quadrants[]",
            "Julia[-0.7, 0.2, 32]",
            "BurningShip[]",
            "Point3D[0, 0, 0]",
            "Segment3D[0, 0, 0, 1, 1, 1]",
            "Line3D[0, 0, 0, 1, 1, 1]",
            "Plane3D[0, 1, 0, 0]",
            "Sphere[0, 0, 0, 1]",
            "Cube[0, 0, 0, 1]",
            "Tetrahedron[0, 0, 0, 1]",
            "Cylinder[0, 0, 0, 1, 2]",
            "Cone[0, 0, 0, 1, 2]",
            "Torus[0, 0, 0, 2, 0.5]",
            "Moebius[2, 0.5]",
            "Curve3D[(cos(t), sin(t), t), 0, 6.28]",
            "ComplexSurface[1/z, -2, 2, -2, 2, 32]",
            "VectorField3D[-y, x, z]",
            "Lorenz[]",
            "Hypersphere[]",
            "Pentachoron4D[]",
            "Tesseract4D[]",
            "SixteenCell4D[]",
            "TwentyFourCell4D[]",
            "OneTwentyCell4D[]",
            "SixHundredCell4D[]",
            "SimplexND[3]",
            "HypercubeND[4]",
            "CrossPolytopeND[5,1,{0.1,-0.2,0.3,-0.4,0.5,-0.6,0.7,-0.8,0.9,-1.0}]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_ok(),
                "{command} must preflight through its declared render route"
            );
        }
    }

    #[test]
    fn data_backed_graph_capabilities_never_preflight_as_assistant_actions() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));

        for command in [
            "Histogram[{1, 2, 3, 4}, 4]",
            "ScatterPlot[{1, 2, 3}, {1, 4, 9}]",
            "BoxPlot[{1, 2, 3, 4, 5}]",
            "LinearRegression[{1, 2, 3}, {1, 4, 9}]",
        ] {
            assert!(
                preflight_assistant_graph_command(&document, command).is_err(),
                "{command} must remain a reference-only assistant form"
            );
        }
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn preflight_commit_updates_the_document_and_undo_history_once() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let preflight = preflight_assistant_graph_command(&document, "Function[sin(x)]")
            .expect("finite function must be drawable");
        let mut undo_stack = VecDeque::new();
        let mut redo_stack = VecDeque::new();

        let outcome = commit_assistant_graph_preflight(
            &mut document,
            &mut undo_stack,
            &mut redo_stack,
            preflight,
        );

        assert!(matches!(outcome, CommandOutcome::Message(_)));
        assert_eq!(document.object_count(), 1);
        assert_eq!(undo_stack.len(), 1);
        assert!(redo_stack.is_empty());
    }

    #[test]
    fn graph_apply_switches_only_between_required_2d_and_3d_views() {
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::ThreeD,
                crate::ViewMode::D2,
            ),
            Some(crate::Perspective::Geometry3D)
        );
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::TwoD,
                crate::ViewMode::D3,
            ),
            Some(crate::Perspective::Geometry2D)
        );
        assert_eq!(
            assistant_graph_perspective(
                grafito_command::assistant_context::AssistantGraphView::TwoD,
                crate::ViewMode::D2,
            ),
            None
        );
    }

    #[test]
    fn s1_apply_muestra_la_vista_esperada_segun_el_objeto() {
        // S1 auto-graficar 1-click: apply → vista esperada (2D o 3D según el
        // objeto; si no se puede determinar, default 2D honesto).
        use grafito_command::assistant_proposals::{
            parse_assistant_command, parse_assistant_parameter, AssistantProposal,
        };
        let two_d =
            AssistantProposal::Command(parse_assistant_command("Function[x]").expect("2D válido"));
        assert_eq!(
            two_d.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::TwoD
        );
        // Desde 3D, un 2D pide cambio a Geometry2D (el Apply lo abre).
        assert_eq!(
            assistant_graph_perspective(two_d.expected_view(), crate::ViewMode::D3),
            Some(crate::Perspective::Geometry2D)
        );
        // Desde 2D, un 2D no pide cambio (ya visible).
        assert_eq!(
            assistant_graph_perspective(two_d.expected_view(), crate::ViewMode::D2),
            None
        );
        let three_d = AssistantProposal::Command(
            parse_assistant_command("Sphere[0, 0, 0, 1]").expect("3D válido"),
        );
        assert_eq!(
            three_d.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::ThreeD
        );
        assert_eq!(
            assistant_graph_perspective(three_d.expected_view(), crate::ViewMode::D2),
            Some(crate::Perspective::Geometry3D)
        );
        // Parámetro sin objeto → default 2D honesto (no inventa 3D).
        let param =
            AssistantProposal::Parameter(parse_assistant_parameter("a = 2.5").expect("parámetro"));
        assert_eq!(
            param.expected_view(),
            grafito_command::assistant_context::AssistantGraphView::TwoD
        );
    }

    #[test]
    fn s2_clarificacion_round_trip_sin_bloquear() {
        // S2 `ask_user` real vía evento: parse del `args_summary` → pendiente
        // → respuesta saneada para el loop como `function_call_output`.
        // Puro y no bloqueante (sin threads, sin Document, sin I/O).
        let pending = super::parse_agent_ask_user_pending(
            r#"{"question":"¿qué valor le doy a x?","options":["0","1"]}"#,
        )
        .expect("pendiente parseable");
        assert_eq!(pending.question, "¿qué valor le doy a x?");
        assert_eq!(pending.options, vec!["0".to_owned(), "1".to_owned()]);
        // Truncado con `…` igual intenta (honesto, sin inventar).
        assert!(super::parse_agent_ask_user_pending("hola").is_none());
        assert!(super::parse_agent_ask_user_pending("").is_none());
        assert!(super::parse_agent_ask_user_pending(r#"{"question":""}"#).is_none());
        // La respuesta vuelve al loop como `function_call_output` (Responses)
        // vía `answer_pending_clarification` (nuevo job, nunca bloquea).
        let output = grafito_agent::tools::ask_user_answer_function_output(&pending.call_id, "1")
            .expect("respuesta no vacía");
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], pending.call_id);
        assert_eq!(output["output"], "1");
    }

    #[test]
    fn s3_entrada_rota_da_explicacion_mas_fix() {
        // S3: fence inválida típica → `vibecoder_explain` + fix sintáctico.
        let (explained, fix) =
            grafito_assistant::agent::explain_invalid_proposal("Function[x", "derivada");
        assert_eq!(
            explained.kind,
            grafito_assistant::agent::VibecoderKind::Syntax
        );
        assert!(explained.explanation.contains("derivada"));
        assert_eq!(fix.as_deref(), Some("Function[x]"));
        // Semántico jamás se reescribe en silencio.
        let (_, fix) = grafito_assistant::agent::explain_invalid_proposal("Script[Save[]]", "");
        assert!(fix.is_none());
    }

    #[test]
    fn flower_scene_is_atomic_drawable_and_fitted_before_commit() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = flower_scene_commands();
        let typed_commands = assistant_commands(&commands);

        let preflight = preflight_assistant_flower_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .expect("a complete flower scene must be drawable");

        assert_eq!(document.object_count(), 0, "staging must stay atomic");
        assert_eq!(preflight.staged.object_count(), commands.len());
        assert!(preflight.camera.distance.is_finite());
        assert!(preflight.camera.distance > 0.0);
        assert!(preflight.staged.objects_iter().any(|(_, object)| {
            matches!(object, GeoObject::Surface3D(surface) if surface.solid)
        }));
    }

    #[test]
    fn incomplete_flower_scene_never_becomes_a_verified_remote_proposal() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let incomplete = [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 3, 0.3*v), 0, 1.5, -1, 1]",
        ]
        .join("\n");
        let response = format!("```grafito-scene\n{incomplete}\n```");

        assert!(verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0)
        )
        .is_empty());
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn incomplete_world_mesh_scene_never_becomes_an_actionable_assistant_card() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = vec!["Lorenz[]".to_string(); 5];
        let typed_commands = assistant_commands(&commands);

        assert!(preflight_assistant_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .is_err());
        assert_eq!(document.object_count(), 0, "preflight must stay isolated");
    }

    #[test]
    fn multiline_grafito_segment3d_scene_is_preflighted_atomically() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let edges = [
            "Segment3D[0,0,0,1,0,0]",
            "Segment3D[1,0,0,0.5,0.8660254037844386,0]",
            "Segment3D[0.5,0.8660254037844386,0,0,0,0]",
            "Segment3D[0,0,0,0.5,0.2886751345948129,0.816496580927726]",
            "Segment3D[1,0,0,0.5,0.2886751345948129,0.816496580927726]",
            "Segment3D[0.5,0.8660254037844386,0,0.5,0.2886751345948129,0.816496580927726]",
        ];
        let response = format!("```grafito\n{}\n```", edges.join("\n"));

        let proposals = verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(proposals, vec![scene_proposal(&edges)]);
        assert_eq!(document.object_count(), 0, "preflight must stay atomic");
    }

    #[test]
    fn labeled_tetrahedron_scene_is_preflighted_as_six_direct_edges() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let edges = [
            "Segment3D[0,0.816496580927726,0,-0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[0,0.816496580927726,0,0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[0,0.816496580927726,0,0,-0.4082482904638631,0.816496580927726]",
            "Segment3D[-0.7071067811865475,-0.4082482904638631,0,0.7071067811865475,-0.4082482904638631,0]",
            "Segment3D[-0.7071067811865475,-0.4082482904638631,0,0,-0.4082482904638631,0.816496580927726]",
            "Segment3D[0.7071067811865475,-0.4082482904638631,0,0,-0.4082482904638631,0.816496580927726]",
        ];
        let response = format!(
            "```grafito-scene\nv0 = Point3D[0,0.816496580927726,0]\nv1 = Point3D[-0.7071067811865475,-0.4082482904638631,0]\nv2 = Point3D[0.7071067811865475,-0.4082482904638631,0]\nv3 = Point3D[0,-0.4082482904638631,0.816496580927726]\n{}\n```",
            edges
                .iter()
                .enumerate()
                .map(|(index, edge)| format!("a{index:02} = {edge}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        assert_eq!(
            verified_remote_proposals(
                &document,
                &response,
                grafito_geometry::Camera3D::new(4.0 / 3.0),
            ),
            vec![scene_proposal(&edges)]
        );
        assert_eq!(document.object_count(), 0, "preflight must stay atomic");
    }

    #[test]
    fn flower_preflight_rejects_axis_swapped_disconnected_petals() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let commands = [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(-u, -0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(0.3*v, u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
            "Surface3D[(-0.3*v, -u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2)), 0, 1.5, -1, 1]",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let typed_commands = assistant_commands(&commands);

        let error = preflight_assistant_flower_scene(
            &document,
            &typed_commands,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        )
        .err()
        .expect("axis-swapped petals must not pass as one connected flower");

        assert!(error.contains("conectada"), "unexpected error: {error}");
        assert_eq!(document.object_count(), 0);
    }

    #[test]
    fn verified_remote_proposals_preflight_only_a_fixed_number_of_fenced_proposals() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let response = std::iter::repeat_n("```grafito\nFunction[sin(x)]\n```", 5)
            .collect::<Vec<_>>()
            .join("\n");

        let proposals = verified_remote_proposals(
            &document,
            &response,
            grafito_geometry::Camera3D::new(4.0 / 3.0),
        );

        assert_eq!(proposals.len(), 4);
        assert_eq!(document.object_count(), 0);
    }

    fn flower_scene_commands() -> Vec<String> {
        [
            "Cylinder[0, 0, 0, 0.16, 3]",
            "Sphere[0, 3, 0, 0.45]",
            "Surface3D[(u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), 0.3*v), 0, 1.5, -1, 1]",
            "Surface3D[(-u, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), -0.3*v), 0, 1.5, -1, 1]",
            "Surface3D[(0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), u), 0, 1.5, -1, 1]",
            "Surface3D[(-0.3*v, 3 + 0.25*(1-(u/1.5)^2)*(1-v^2), -u), 0, 1.5, -1, 1]",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn attachment_reader_stops_before_an_oversized_file_is_fully_buffered() {
        let bytes = read_bounded_attachment(Cursor::new(vec![7; 5]), 4);

        assert!(bytes.is_err());
    }
}
