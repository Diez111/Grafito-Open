//! Orquestación agéntica super compleja para generación de animaciones manim 3b1b.
//!
//! F0: el worker Python está jubilado (`native://`). Este orquestador ya NO
//! finge renders: `tick()` solo avanza plan/escritura con lógica pura y el
//! completado real llega por `apply_job_event` (eventos de un motor externo
//! configurado) o por el render nativo (`anim_native`). El I/O bloqueante
//! vive en `try_render_external_blocking` (llamar en worker thread, nunca en
//! la UI).
//!
//! Integra <https://github.com/3b1b/manim> y <https://github.com/3b1b/videos>
//! mediante `grafito-anim` (motor externo opcional). Arquitectura multi-agente:
//!
//! - Planner: descompone el concepto en escenas
//! - ScriptWriter: genera código manim (Scene, Axes, FunctionGraph)
//! - Renderer: ejecuta el motor externo y produce frames (via grafito-anim Engine)
//! - Reviewer: valida frames y propone correcciones
//! - Orchestrator: coordina el ciclo con estados tipados y budget
//!
//! Todo en Rust puro, testeable headless, sin I/O en UI.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use grafito_assistant_types::{
    ConversationRole, ConversationTurn, TurnMediaRef, MAX_CONVERSATION_TURN_CHARS,
    TURN_MEDIA_MAX_FRAMES, TURN_MEDIA_THUMB_MAX_BYTES,
};
use grafito_ui::assistant::AssistantMedia;

/// Presupuesto para la orquestación.
#[derive(Debug, Clone)]
pub struct OrchestratorBudget {
    pub max_agents: usize,
    pub max_steps: usize,
    pub step_timeout: Duration,
    pub total_timeout: Duration,
}
impl Default for OrchestratorBudget {
    fn default() -> Self {
        Self {
            max_agents: 4,
            max_steps: 8,
            step_timeout: Duration::from_secs(12),
            total_timeout: Duration::from_secs(60),
        }
    }
}

/// Rol de cada agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    Planner,
    ScriptWriter,
    Renderer,
    Reviewer,
}

impl AgentRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planner => "Planner",
            Self::ScriptWriter => "ScriptWriter",
            Self::Renderer => "Renderer",
            Self::Reviewer => "Reviewer",
        }
    }
}

/// Estado tipado del orquestador — hace imposibles los estados inválidos.
///
/// `Reviewing` es real: `apply_job_event(Result)` deja el artefacto en
/// revisión (`frames` + `media_path` sin verificar) y `poll_review`
/// (Reviewer) lo valida (`frames > 0` + `media_path` existente) antes de
/// `Completed`. `tick` jamás completa solo: a lo sumo falla por timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorState {
    Idle,
    Planning {
        started: Instant,
    },
    Writing {
        plan: String,
        started: Instant,
    },
    Rendering {
        script: String,
        started: Instant,
    },
    Reviewing {
        frames: usize,
        media_path: String,
        started: Instant,
    },
    Completed {
        media_path: String,
    },
    Failed {
        reason: String,
    },
    Cancelled,
}

impl OrchestratorState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
    pub fn can_start(&self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
}

/// Actividad de un agente para mostrar en UI.
#[derive(Debug, Clone)]
pub struct AgentActivity {
    pub role: AgentRole,
    pub message: String,
    pub at: Instant,
}

/// Orquestador — coordina 4 agentes con cola y ledger J-Space.
pub struct ManimOrchestrator {
    pub state: OrchestratorState,
    pub budget: OrchestratorBudget,
    pub activities: VecDeque<AgentActivity>,
    pub ledger: Option<String>,
    pub concept: String,
    pub template: String,
    orchestrator_started: Option<Instant>,
    steps_taken: usize,
    /// Fracción REAL 0..1 del último `progress` del worker (`percent/100`).
    ///
    /// Nunca inventada: `0.0` hasta que el worker emite `progress`. La UI debe
    /// mostrar indeterminado mientras sea `0.0` sin eventos previos.
    render_fraction: f32,
    /// Último error tipado del worker `(code, mensaje_localizado)`.
    last_worker_error: Option<(String, String)>,
}

impl Default for ManimOrchestrator {
    fn default() -> Self {
        Self {
            state: OrchestratorState::Idle,
            budget: OrchestratorBudget::default(),
            activities: VecDeque::new(),
            ledger: None,
            concept: String::new(),
            template: "universal".into(),
            orchestrator_started: None,
            steps_taken: 0,
            render_fraction: 0.0,
            last_worker_error: None,
        }
    }
}

impl ManimOrchestrator {
    #[allow(clippy::field_reassign_with_default)]
    pub fn new(concept: impl Into<String>, template: impl Into<String>) -> Self {
        let mut o = Self::default();
        o.concept = concept.into();
        o.template = template.into();
        o
    }
    pub fn start(&mut self, concept: impl Into<String>, template: impl Into<String>) -> bool {
        if !self.state.can_start() {
            return false;
        }
        self.concept = concept.into();
        self.template = template.into();
        let now = Instant::now();
        self.state = OrchestratorState::Planning { started: now };
        self.orchestrator_started = Some(now);
        self.steps_taken = 0;
        self.render_fraction = 0.0;
        self.last_worker_error = None;
        self.activities.clear();
        self.push_activity(
            AgentRole::Planner,
            format!(
                "Planificando animación para '{}' (template {})",
                self.concept, self.template
            ),
        );
        self.update_ledger();
        true
    }
    pub fn cancel(&mut self) {
        if !self.state.is_terminal() {
            self.state = OrchestratorState::Cancelled;
            self.push_activity(AgentRole::Planner, "Cancelado por el usuario".into());
        }
    }
    /// Fracción REAL 0..1 del último `progress` del worker.
    pub fn render_fraction(&self) -> f32 {
        self.render_fraction
    }
    /// Último error tipado del worker `(código, mensaje en español)`.
    pub fn last_worker_error(&self) -> Option<&(String, String)> {
        self.last_worker_error.as_ref()
    }
    /// Aplica un `percent` REAL del worker (`progress.percent` 0..=100).
    ///
    /// Fija `render_fraction = percent/100.0`. No inventa valores: solo llamar
    /// con datos parseados del worker (`grafito_anim::RenderProgress`).
    pub fn apply_progress(&mut self, percent: u8) {
        self.render_fraction = (f32::from(percent.min(100))) / 100.0;
        self.update_ledger();
    }
    /// Aplica un evento del worker (`Progress`/`Result`/`Error`) al estado.
    ///
    /// - `Progress` → actualiza `render_fraction` sin cambiar de estado.
    /// - `Result` → `Reviewing { frames, media_path }` (Reviewer pendiente;
    ///   `poll_review` valida antes de `Completed`).
    /// - `Error` → `Failed` con mensaje localizado al español (ver
    ///   `grafito_anim::localize_worker_error`), acotado a 500 chars.
    pub fn apply_job_event(&mut self, event: &grafito_anim::JobEvent) {
        match event {
            grafito_anim::JobEvent::Progress(p) => {
                self.apply_progress(p.percent);
                self.push_activity(
                    AgentRole::Renderer,
                    format!("Renderizando: {}% ({})", p.percent.min(100), p.step),
                );
            }
            grafito_anim::JobEvent::Result(r) => {
                self.render_fraction = 1.0;
                self.state = OrchestratorState::Reviewing {
                    frames: r.frames,
                    media_path: r.media_path.clone(),
                    started: Instant::now(),
                };
                self.push_activity(
                    AgentRole::Reviewer,
                    format!(
                        "Render listo: {} frames en revisión ({})",
                        r.frames, r.media_path
                    ),
                );
                self.update_ledger();
            }
            grafito_anim::JobEvent::Error { code, message } => {
                self.fail_with_worker_error(code, message);
            }
        }
    }
    /// Falla con un error tipado del worker, localizado al español.
    pub fn fail_with_worker_error(&mut self, code: &str, message: &str) {
        let localized = grafito_anim::localize_worker_error(code, message);
        let code_sane = grafito_anim::sanitize_error_code(code);
        self.last_worker_error = Some((code_sane, localized.clone()));
        self.state = OrchestratorState::Failed {
            reason: localized.clone(),
        };
        self.push_activity(AgentRole::Reviewer, localized);
        self.update_ledger();
    }
    /// Reviewer puro: valida el artefacto en revisión sin I/O.
    ///
    /// `media_exists` lo aporta el llamador (`Path::exists` en
    /// `poll_review`): así el criterio es testeable headless y este
    /// predicado no toca el filesystem. `Ok` = ruta verificada para
    /// `Completed`; `Err` = motivo honesto para `Failed`.
    pub fn review_artifact(
        frames: usize,
        media_path: &str,
        media_exists: bool,
    ) -> Result<String, String> {
        if frames == 0 {
            return Err("revisión: el motor no devolvió fotogramas".into());
        }
        if media_path.trim().is_empty() {
            return Err("revisión: el motor no devolvió ruta de artefacto".into());
        }
        if !media_exists {
            return Err(format!(
                "revisión: falta el artefacto del motor ({media_path})"
            ));
        }
        Ok(media_path.to_string())
    }

    /// Reviewer real: resuelve `Reviewing` contra el filesystem.
    ///
    /// Hace I/O mínimo (`Path::exists`): llamar en worker thread, nunca en
    /// el draw de la UI. `Some` = transición aplicada (`Completed` verificado
    /// o `Failed` honesto); `None` = no estaba en `Reviewing` (no-op).
    pub fn poll_review(&mut self) -> Option<OrchestratorState> {
        let (frames, media_path) = match &self.state {
            OrchestratorState::Reviewing {
                frames, media_path, ..
            } => (*frames, media_path.clone()),
            _ => return None,
        };
        let exists = std::path::Path::new(&media_path).exists();
        match Self::review_artifact(frames, &media_path, exists) {
            Ok(verificada) => {
                self.state = OrchestratorState::Completed {
                    media_path: verificada.clone(),
                };
                self.push_activity(
                    AgentRole::Reviewer,
                    format!("Revisión ok: {frames} frames ({verificada})"),
                );
            }
            Err(motivo) => {
                self.state = OrchestratorState::Failed {
                    reason: motivo.clone(),
                };
                self.push_activity(AgentRole::Reviewer, motivo);
            }
        }
        if self.state.is_terminal() {
            self.orchestrator_started = None;
        }
        self.update_ledger();
        Some(self.state.clone())
    }
    /// Config del engine con overrides por Env (`GRAFITO_ANIM_*`); si el Env es
    /// inválido, devuelve el defecto para no romper la UI (el error se loguea
    /// en el ledger de actividades).
    pub fn engine_config(&mut self) -> grafito_anim::EngineConfig {
        match grafito_anim::EngineConfig::from_env() {
            Ok(cfg) => cfg,
            Err(e) => {
                self.push_activity(
                    AgentRole::Planner,
                    format!("Config Env inválida ({e}); usando defecto 90s/8s/64KiB"),
                );
                grafito_anim::EngineConfig::default()
            }
        }
    }
    /// Render real contra un motor externo (F0: reemplaza el viejo fake de
    /// `frames = 48` + `/tmp/*.mp4`).
    ///
    /// Llama a `grafito_anim::run_job` de verdad (spawn → wait_ready →
    /// submit → recv → shutdown) y aplica el resultado al estado vía
    /// `apply_job_event`. Falla honesto si no hay motor (`native://` por
    /// defecto), si el pedido es inválido o si el worker reporta error.
    /// Bloquea el hilo llamante: usar SOLO en worker thread, nunca en la UI.
    pub fn try_render_external_blocking(
        &mut self,
        request: &grafito_anim::protocol::AnimRequest,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<grafito_anim::protocol::AnimResult, String> {
        request
            .validate()
            .map_err(|e| format!("petición inválida: {e}"))?;
        let config = self.engine_config();
        let mut applied: Vec<grafito_anim::JobEvent> = Vec::new();
        let attempt = grafito_anim::run_job(&config, request, cancel, |ev| {
            applied.push(ev.clone());
        });
        for ev in &applied {
            self.apply_job_event(ev);
        }
        match attempt {
            Ok(result) => {
                self.apply_job_event(&grafito_anim::JobEvent::Result(result.clone()));
                Ok(result)
            }
            Err(causa) => {
                self.fail_with_worker_error("error", &causa);
                Err(causa)
            }
        }
    }
    /// F0: sin fakes. `tick` solo mueve lógica pura (plan/escritura); el
    /// render bloqueante vive en `try_render_external_blocking` (worker
    /// thread), el `Result` entra en `Reviewing` vía `apply_job_event` y el
    /// completado verificado lo da `poll_review`. Si el tick
    /// llega a `Rendering` sin motor en curso, falla honesto en lugar de
    /// inventar frames o paths `/tmp/*.mp4`.
    pub fn tick(&mut self, now: Instant) -> Option<OrchestratorState> {
        // Deadline absoluta del presupuesto total (Instant::now() + total_timeout)
        if let Some(started) = self.orchestrator_started {
            if now.duration_since(started) >= self.budget.total_timeout {
                self.state = OrchestratorState::Failed {
                    reason: "budget total_timeout excedido".into(),
                };
                self.update_ledger();
                return Some(self.state.clone());
            }
            if self.steps_taken >= self.budget.max_steps {
                self.state = OrchestratorState::Failed {
                    reason: "budget max_steps excedido".into(),
                };
                self.update_ledger();
                return Some(self.state.clone());
            }
        }

        let step_timeout = self.budget.step_timeout;
        let next = match &self.state {
            OrchestratorState::Planning { started } => {
                // Usa budget.step_timeout en lugar de Duration hardcoded 400ms
                if now.duration_since(*started) >= step_timeout {
                    let plan = self.plan_for_concept();
                    self.push_activity(AgentRole::ScriptWriter, format!("Plan listo: {}", plan));
                    Some(OrchestratorState::Writing { plan, started: now })
                } else {
                    // step_timeout excedido se maneja arriba como transición, aquí solo avance por progreso
                    None
                }
            }
            OrchestratorState::Writing { plan, started } => {
                if now.duration_since(*started) >= step_timeout {
                    let script = self.script_for_plan(plan);
                    self.push_activity(
                        AgentRole::Renderer,
                        "Script manim generado, enviando a render".into(),
                    );
                    Some(OrchestratorState::Rendering {
                        script,
                        started: now,
                    })
                } else {
                    None
                }
            }
            OrchestratorState::Rendering { script, started } => {
                if now.duration_since(*started) >= step_timeout {
                    // F0: nada fake. El render real corre en worker thread vía
                    // `try_render_external_blocking`; si el tick lo alcanza
                    // sin motor en curso, se falla honesto (el completado real
                    // entra por `apply_job_event`, no por paths inventados).
                    let _ = script;
                    self.push_activity(
                        AgentRole::Renderer,
                        "Sin motor externo en curso: render no disponible en tick (usá el nativo o try_render_external_blocking)".into(),
                    );
                    Some(OrchestratorState::Failed {
                        reason: "motor externo sin job en curso: nada que revisar en tick".into(),
                    })
                } else {
                    // Aún dentro del budget: espera al próximo tick.
                    let _ = script;
                    None
                }
            }
            OrchestratorState::Reviewing {
                frames,
                media_path,
                started,
            } => {
                if now.duration_since(*started) >= step_timeout {
                    // El Reviewer real es `poll_review` (valida frames>0 +
                    // artefacto existente). Si el tick lo alcanza sin
                    // resolución, falla honesto en vez de completar solo.
                    let _ = (frames, media_path);
                    self.push_activity(
                        AgentRole::Reviewer,
                        "Revisión sin resolver en plazo: no se completa sin artefacto verificado"
                            .into(),
                    );
                    Some(OrchestratorState::Failed {
                        reason:
                            "revisión sin resolver: el completado lo da poll_review con artefacto verificado"
                                .into(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(state) = next {
            self.steps_taken += 1;
            self.state = state.clone();
            // Si llegamos a terminal, limpia deadline para próximo start
            if self.state.is_terminal() {
                self.orchestrator_started = None;
            }
            self.update_ledger();
            Some(state)
        } else {
            // Verifica step_timeout como deadline absoluta por estado (para log)
            // Si el step lleva más de step_timeout sin progresar, el próximo tick lo avanzará;
            // aquí solo esperamos.
            None
        }
    }
    fn plan_for_concept(&self) -> String {
        match self.template.as_str() {
            "derivative-slope" => format!(
                "Escenas: 1) curva {} y secante, 2) límite h→0, 3) tangente y derivada",
                self.concept
            ),
            "integral-area" => format!(
                "Escenas: 1) área bajo {}, 2) Riemann, 3) área exacta",
                self.concept
            ),
            "pitagoras" => {
                "Escenas: 1) triángulo rectángulo, 2) cuadrados en catetos, 3) reordenamiento"
                    .into()
            }
            _ => format!("Escenas universales para '{}'", self.concept),
        }
    }
    fn script_for_plan(&self, plan: &str) -> String {
        // Genera un script manim mínimo (se enviaría a un motor externo
        // configurado; F0: sin worker Python empaquetado)
        // Basado en 3b1b/manim: from manim import Scene, Axes, FunctionGraph, MathTex
        format!(
            "from manim import Scene, Axes, FunctionGraph, MathTex\nclass GrafitoScene(Scene):\n    def construct(self):\n        # Plan: {}\n        axes = Axes(x_range=[-3,3], y_range=[-3,3])\n        self.add(axes)\n        # Concepto: {}\n",
            plan, self.concept
        )
    }
    fn push_activity(&mut self, role: AgentRole, message: String) {
        self.activities.push_back(AgentActivity {
            role,
            message,
            at: Instant::now(),
        });
        if self.activities.len() > 12 {
            self.activities.pop_front();
        }
    }
    fn update_ledger(&mut self) {
        let state_label = match &self.state {
            OrchestratorState::Idle => "Idle",
            OrchestratorState::Planning { .. } => "Planning",
            OrchestratorState::Writing { .. } => "Writing",
            OrchestratorState::Rendering { .. } => "Rendering",
            OrchestratorState::Reviewing { .. } => "Reviewing",
            OrchestratorState::Completed { .. } => "Completed",
            OrchestratorState::Failed { .. } => "Failed",
            OrchestratorState::Cancelled => "Cancelled",
        };
        let activities = self
            .activities
            .iter()
            .map(|a| format!("{}: {}", a.role.label(), a.message))
            .collect::<Vec<_>>()
            .join("\n");
        let pct = (self.render_fraction * 100.0).round() as u32;
        self.ledger = Some(format!("Manim Orchestrator\nEstado: {state_label}\nConcepto: {}\nTemplate: {}\nProgreso real: {pct}%\nActividades:\n{activities}", self.concept, self.template));
    }
    pub fn is_busy(&self) -> bool {
        matches!(
            self.state,
            OrchestratorState::Planning { .. }
                | OrchestratorState::Writing { .. }
                | OrchestratorState::Rendering { .. }
                | OrchestratorState::Reviewing { .. }
        )
    }
}

// ── P0-app historial Thumb+Replay (lado app, puro sin `egui::Context`) ─────
// El modelo W1 (`assistant-types`: `ConversationTurn.media`,
// `attach_turn_media`, `trim_conversation`) pega al turno un thumb RGBA de
// 96×96 + coords de replay. La Piel dibuja la mini-card + [Ver de nuevo] →
// `AssistantUiAction::ReplayMedia`; la app resuelve con
// `anim_ui::history_replay_request` y reinyecta por el camino single
// (`run_assistant_animation_with_history`, slot vivo vía `set_media`).
// Todo lo de acá es puro y headless-testeable: el render pesado y el I/O
// siguen en el worker con `CancellationToken`; la UI solo renderiza
// `&Estado`. Presupuestos intactos (MAX_TURNS 6, GIF 64/8M/5MB).

/// Lado del thumb histórico en píxeles (paridad `TURN_MEDIA_THUMB_SIDE_PX`).
pub(crate) const REPLAY_THUMB_SIDE_PX: usize = 96;

/// Coordenadas de replay de un job single (W1).
///
/// Plantilla + concepto EFECTIVAMENTE renderizados (normalizados por el
/// runner, no el pedido crudo): el drain los usa para pegar `TurnMediaRef`
/// al turno recién creado y el replay los reinyecta por el camino single.
/// La playlist multi-step y el replay no traen coords (`None` en el job):
/// la primera no es reinyectable honesta por el camino single, el segundo
/// ya tiene su media en el turno (basta el slot vivo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimHistoryCoords {
    pub template: String,
    pub concept: String,
}

impl AnimHistoryCoords {
    /// Coordena si ambos campos son no vacíos y dentro del tope W1
    /// (`MAX_TURN_MEDIA_FIELD_CHARS`); `None` honesto en caso contrario
    /// (el job completa igual, solo sin historiar).
    pub(crate) fn new(template: String, concept: String) -> Option<Self> {
        if template.trim().is_empty() || concept.trim().is_empty() {
            return None;
        }
        if template.chars().count() > grafito_assistant_types::MAX_TURN_MEDIA_FIELD_CHARS
            || concept.chars().count() > grafito_assistant_types::MAX_TURN_MEDIA_FIELD_CHARS
        {
            return None;
        }
        Some(Self { template, concept })
    }
}

/// Baja el primer frame a RGBA 96×96 por vecino más cercano.
///
/// Puro y acotado (36 KiB fijos): jamás guarda el `Vec<ColorImage>` por
/// turno (48 frames a 480px ~30 MiB = OOM). `None` honesto sin frames o
/// con píxeles inconsistentes.
pub(crate) fn thumb_rgba_96_from_media(media: &AssistantMedia) -> Option<Vec<u8>> {
    let first = media.frames.first()?;
    let [ancho, alto] = first.size;
    let total = ancho.checked_mul(alto)?;
    if ancho == 0 || alto == 0 || first.pixels.len() != total {
        return None;
    }
    let mut salida = Vec::with_capacity(REPLAY_THUMB_SIDE_PX * REPLAY_THUMB_SIDE_PX * 4);
    for fila in 0..REPLAY_THUMB_SIDE_PX {
        let origen_y = fila.saturating_mul(alto) / REPLAY_THUMB_SIDE_PX;
        for columna in 0..REPLAY_THUMB_SIDE_PX {
            let origen_x = columna.saturating_mul(ancho) / REPLAY_THUMB_SIDE_PX;
            let indice = origen_y.saturating_mul(ancho).saturating_add(origen_x);
            let pixel = first
                .pixels
                .get(indice)
                .copied()
                .unwrap_or(egui::Color32::BLACK);
            salida.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
        }
    }
    Some(salida)
}

/// Construye el `TurnMediaRef` de un job recién completado.
///
/// `None` honesto si no hay frames, si exceden `TURN_MEDIA_MAX_FRAMES`
/// (64), si el thumb no es RGBA 96×96 exacto o si algún campo no valida:
/// el job completa igual con el slot vivo, solo sin mini-card.
pub(crate) fn turn_media_for_completed_job(
    media: &AssistantMedia,
    coords: &AnimHistoryCoords,
) -> Option<TurnMediaRef> {
    if media.frames.is_empty() || media.frames.len() > usize::from(TURN_MEDIA_MAX_FRAMES) {
        return None;
    }
    let thumb = thumb_rgba_96_from_media(media)?;
    if thumb.len() != TURN_MEDIA_THUMB_MAX_BYTES {
        return None;
    }
    let frame_count = u8::try_from(media.frames.len()).ok()?;
    let hecha = TurnMediaRef::new(
        media.title.clone(),
        coords.template.clone(),
        coords.concept.clone(),
        thumb,
        frame_count,
    );
    hecha.validate().ok()?;
    Some(hecha)
}

/// ¿El dueño sigue vivo (último turno y del asistente)?
///
/// El slot vivo solo vale si dueño == último: si el usuario ya pidió otra
/// cosa (o hubo reemplazo), el job rancio se descarta sin contaminar.
/// Puro, sin I/O.
pub(crate) fn es_dueno_vivo(conversacion: &[ConversationTurn], dueno: Option<usize>) -> bool {
    let Some(indice) = dueno else {
        return false;
    };
    let Some(ultimo) = conversacion.len().checked_sub(1) else {
        return false;
    };
    indice == ultimo
        && matches!(
            conversacion.get(indice).map(|turno| &turno.role),
            Some(ConversationRole::Assistant)
        )
}

/// Pega la media SOLO al índice dueño si es turno asistente y último.
///
/// Stale (dueño != último, turno usuario, índice inválido) → `false`
/// honesto sin tocar nada: el drain descarta el job rancio. La media viaja
/// dentro del turno, así que el trim la dropea con el par sin reindexado.
pub(crate) fn attach_media_to_owner_turn(
    conversacion: &mut [ConversationTurn],
    dueno: Option<usize>,
    media: TurnMediaRef,
) -> bool {
    let Some(indice) = dueno else {
        return false;
    };
    if !es_dueno_vivo(conversacion, Some(indice)) {
        return false;
    }
    grafito_assistant_types::attach_turn_media(conversacion, indice, media).is_ok()
}

/// Anexa el error del job al turno dueño (si sigue siendo del asistente).
///
/// El drain lo usa para que el fallo quede pegado al pedido que lo causó,
/// no flotando global. Recorta a `MAX_CONVERSATION_TURN_CHARS`. `false`
/// honesto sin dueño válido o turno no-asistente.
pub(crate) fn anexar_error_a_dueno(
    conversacion: &mut [ConversationTurn],
    dueno: Option<usize>,
    error: &str,
) -> bool {
    let Some(indice) = dueno else {
        return false;
    };
    let limpio = error.trim();
    if limpio.is_empty() {
        return false;
    }
    let Some(turno) = conversacion.get_mut(indice) else {
        return false;
    };
    if turno.role != ConversationRole::Assistant {
        return false;
    }
    turno
        .content
        .push_str("\n\nNo se pudo generar la animación: ");
    let resto = MAX_CONVERSATION_TURN_CHARS.saturating_sub(turno.content.len());
    let recorte: String = limpio.chars().take(resto.min(500)).collect();
    turno.content.push_str(&recorte);
    true
}

/// Recorta app-side a `MAX_CONVERSATION_TURNS` dropeando el par completo.
///
/// El thumb viaja dentro del turno (`ConversationTurn.media`), así que se
/// recorta junto al par sin reindexado ni caché propio acá (el de la Piel
/// se retira con gracia en su `trim_conversation`). Defensivo tras cada
/// `attach`: con `len <= 6` es no-op.
pub(crate) fn trim_conversation_dropping_pair_media(conversation: &mut Vec<ConversationTurn>) {
    grafito_assistant_types::trim_conversation(conversation);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orchestrator_starts_and_ticks() {
        let mut o = ManimOrchestrator::new("derivada de x²", "derivative-slope");
        // Ajusta budget para que tick avance rápido en test (step_timeout 400ms)
        o.budget.step_timeout = Duration::from_millis(400);
        o.budget.total_timeout = Duration::from_secs(60);
        assert!(o.start("derivada de x²", "derivative-slope"));
        assert!(matches!(o.state, OrchestratorState::Planning { .. }));
        // tick after planning timeout should advance: usa deadline absoluta Instant::now + step_timeout
        let now = Instant::now() + Duration::from_millis(500);
        let _ = o.tick(now);
        assert!(matches!(o.state, OrchestratorState::Writing { .. }));
    }

    #[test]
    fn orchestrator_respects_budget_total_timeout() {
        let mut o = ManimOrchestrator::new("test", "universal");
        o.budget.step_timeout = Duration::from_millis(200);
        o.budget.total_timeout = Duration::from_millis(300);
        assert!(o.start("test", "universal"));
        let now = Instant::now() + Duration::from_millis(500);
        let state = o.tick(now);
        // total_timeout excedido debe fallar
        assert!(matches!(
            state,
            Some(OrchestratorState::Failed { .. }) | None
        ));
        // Avanza más tiempo para asegurar total_timeout
        let now2 = Instant::now() + Duration::from_millis(1000);
        let _ = o.tick(now2);
        assert!(matches!(o.state, OrchestratorState::Failed { .. }));
    }
    #[test]
    fn orchestrator_plan_for_concept() {
        let o = ManimOrchestrator::new("integral", "integral-area");
        assert!(o.plan_for_concept().contains("integral"));
    }
    #[test]
    fn orchestrator_applies_real_progress_fraction() {
        let mut o = ManimOrchestrator::new("derivada", "derivative-slope");
        assert_eq!(o.render_fraction(), 0.0);
        o.apply_progress(30);
        assert!((o.render_fraction() - 0.3).abs() < f32::EPSILON);
        o.apply_progress(100);
        assert_eq!(o.render_fraction(), 1.0);
        // clamp >100
        o.apply_progress(250);
        assert_eq!(o.render_fraction(), 1.0);
    }
    #[test]
    fn orchestrator_applies_job_events_with_spanish_errors() {
        use grafito_anim::{AnimResult, JobEvent, RenderProgress};
        let mut o = ManimOrchestrator::new("derivada", "derivative-slope");
        o.apply_job_event(&JobEvent::Progress(RenderProgress {
            job_id: "job-1".into(),
            step: "render".into(),
            percent: 60,
        }));
        assert!((o.render_fraction() - 0.6).abs() < f32::EPSILON);
        o.apply_job_event(&JobEvent::Result(AnimResult {
            job_id: "job-1".into(),
            media_path: "/tmp/x.png".into(),
            frames: 12,
            duration_ms: 120,
        }));
        // El `Result` deja en `Reviewing` (no completa solo): el Reviewer
        // valida después vía `poll_review` / `review_artifact`.
        assert!(matches!(
            o.state,
            OrchestratorState::Reviewing { frames: 12, .. }
        ));
        assert_eq!(o.render_fraction(), 1.0);
        // Reviewer puro: frames>0 + ruta existente.
        assert!(ManimOrchestrator::review_artifact(12, "/tmp/x.png", true).is_ok());
        assert!(ManimOrchestrator::review_artifact(0, "/tmp/x.png", true).is_err());
        assert!(ManimOrchestrator::review_artifact(12, "  ", true).is_err());
        assert!(ManimOrchestrator::review_artifact(12, "/tmp/x.png", false).is_err());
        // Error tipado en español, sin inglés crudo
        let mut o2 = ManimOrchestrator::new("x", "universal");
        o2.apply_job_event(&JobEvent::Error {
            code: "render_failed".into(),
            message: "boom".into(),
        });
        match &o2.state {
            OrchestratorState::Failed { reason } => {
                assert!(reason.contains("falló el render"), "reason: {reason}");
                assert!(!reason.to_lowercase().contains("animation engine"));
            }
            other => panic!("esperaba Failed, got {other:?}"),
        }
        assert!(o2.last_worker_error().is_some());
    }
    #[test]
    fn tick_nunca_inventa_frames_ni_paths_falla_honesto() {
        // F0: `tick` no finge renders. Planning → Writing → Rendering →
        // Failed honesto (sin `frames = 48`, sin `/tmp/*.mp4`). El `Result`
        // real entra en `Reviewing` por `apply_job_event` y solo completa
        // con artefacto verificado vía `poll_review`.
        let mut o = ManimOrchestrator::new("derivada de x²", "derivative-slope");
        o.budget.step_timeout = Duration::from_millis(100);
        o.budget.total_timeout = Duration::from_secs(60);
        assert!(o.start("derivada de x²", "derivative-slope"));
        let t1 = Instant::now() + Duration::from_millis(200);
        let _ = o.tick(t1);
        assert!(matches!(o.state, OrchestratorState::Writing { .. }));
        let t2 = Instant::now() + Duration::from_millis(400);
        let _ = o.tick(t2);
        assert!(matches!(o.state, OrchestratorState::Rendering { .. }));
        let t3 = Instant::now() + Duration::from_millis(800);
        let state = o.tick(t3);
        match state {
            Some(OrchestratorState::Failed { reason }) => {
                assert!(
                    !reason.contains("/tmp/") && !reason.contains("48"),
                    "nada fake en el motivo: {reason}"
                );
            }
            other => panic!("Rendering sin motor debe fallar honesto, got {other:?}"),
        }
        // Un `Result` real entra en `Reviewing`; `poll_review` completa solo
        // con artefacto verificado (frames>0 + path existente).
        let mut o2 = ManimOrchestrator::new("derivada", "derivative-slope");
        o2.apply_job_event(&grafito_anim::JobEvent::Result(grafito_anim::AnimResult {
            job_id: "job-1".into(),
            media_path: "/tmp/w/job-1.png".into(),
            frames: 1,
            duration_ms: 120,
        }));
        assert!(matches!(o2.state, OrchestratorState::Reviewing { .. }));
        // Sin artefacto en disco: `Failed` honesto, jamás `Completed` fake.
        let res = o2.poll_review();
        assert!(matches!(res, Some(OrchestratorState::Failed { .. })));
        assert!(matches!(o2.state, OrchestratorState::Failed { .. }));
    }

    #[test]
    fn reviewing_con_artefacto_real_completa_verificado() {
        // Artefacto temporal real: `Result` → `Reviewing` → `poll_review`
        // → `Completed` con la misma ruta. Limpia su temporal.
        let ruta =
            std::env::temp_dir().join(format!("grafito_review_{}_ok.png", std::process::id()));
        std::fs::write(&ruta, [0x89, b'P', b'N', b'G']).expect("temporal escribible");
        let ruta_txt = ruta.to_string_lossy().into_owned();
        let mut o = ManimOrchestrator::new("derivada", "derivative-slope");
        o.apply_job_event(&grafito_anim::JobEvent::Result(grafito_anim::AnimResult {
            job_id: "job-1".into(),
            media_path: ruta_txt.clone(),
            frames: 3,
            duration_ms: 120,
        }));
        assert!(matches!(o.state, OrchestratorState::Reviewing { .. }));
        let res = o.poll_review();
        match res {
            Some(OrchestratorState::Completed { media_path }) => {
                assert_eq!(media_path, ruta_txt);
            }
            other => panic!("revisión con artefacto debe completar, got {other:?}"),
        }
        std::fs::remove_file(&ruta).expect("limpia su temporal");
    }
    #[test]
    fn orchestrator_truncates_long_worker_message() {
        let mut o = ManimOrchestrator::new("x", "universal");
        let long = "e".repeat(2000);
        o.fail_with_worker_error("render_failed", &long);
        match &o.state {
            OrchestratorState::Failed { reason } => {
                // 500 chars + prefijo "falló el render: " → acotado
                assert!(reason.chars().count() <= 520, "len {}", reason.len());
            }
            other => panic!("esperaba Failed, got {other:?}"),
        }
    }

    // ── P0-app historial Thumb+Replay ──────────────────────────────────
    fn media_de_prueba(titulo: &str, frames: usize) -> AssistantMedia {
        AssistantMedia {
            title: titulo.to_string(),
            frames: vec![
                egui::ColorImage::new([64, 48], egui::Color32::from_rgb(10, 20, 30));
                frames
            ],
        }
    }

    fn coords_de_prueba() -> AnimHistoryCoords {
        AnimHistoryCoords::new("derivative-slope".to_string(), "derivada".to_string())
            .expect("coords de prueba válidas")
    }

    #[test]
    fn coords_rechazan_vacias_y_campos_enormes() {
        assert!(AnimHistoryCoords::new(String::new(), "x".into()).is_none());
        assert!(AnimHistoryCoords::new("t".into(), "   ".into()).is_none());
        let enorme = "a".repeat(grafito_assistant_types::MAX_TURN_MEDIA_FIELD_CHARS + 1);
        assert!(AnimHistoryCoords::new(enorme.clone(), "x".into()).is_none());
        assert!(AnimHistoryCoords::new("t".into(), enorme).is_none());
        assert!(coords_de_prueba().template == "derivative-slope");
    }

    #[test]
    fn thumb_baja_a_rgba_96x96_exactos() {
        let media = media_de_prueba("T", 3);
        let thumb = thumb_rgba_96_from_media(&media).expect("thumb válido");
        assert_eq!(thumb.len(), TURN_MEDIA_THUMB_MAX_BYTES);
        // Vecino más cercano sobre uniforme: todo el thumb es el mismo píxel.
        assert_eq!(&thumb[0..4], &[10, 20, 30, 255]);
        assert_eq!(&thumb[thumb.len() - 4..], &[10, 20, 30, 255]);
        // Sin frames o píxeles inconsistentes: None honesto.
        let vacia = AssistantMedia {
            title: "T".into(),
            frames: Vec::new(),
        };
        assert!(thumb_rgba_96_from_media(&vacia).is_none());
        let rota = AssistantMedia {
            title: "T".into(),
            frames: vec![egui::ColorImage {
                size: [4, 4],
                pixels: vec![egui::Color32::BLACK; 3],
            }],
        };
        assert!(thumb_rgba_96_from_media(&rota).is_none());
    }

    #[test]
    fn turn_media_valida_y_rechaza_fuera_de_presupuesto() {
        let coords = coords_de_prueba();
        let hecha = turn_media_for_completed_job(&media_de_prueba("Tangente", 8), &coords)
            .expect("historía válida");
        assert_eq!(hecha.title, "Tangente");
        assert_eq!(hecha.template, "derivative-slope");
        assert_eq!(hecha.frame_count, 8);
        assert!(hecha.validate().is_ok());
        // Sin frames, >64 frames o título vacío: None honesto (slot vivo igual).
        assert!(turn_media_for_completed_job(&media_de_prueba("T", 0), &coords).is_none());
        assert!(turn_media_for_completed_job(&media_de_prueba("T", 65), &coords).is_none());
        assert!(turn_media_for_completed_job(&media_de_prueba("", 8), &coords).is_none());
        assert!(turn_media_for_completed_job(&media_de_prueba("T", 64), &coords).is_some());
    }

    #[test]
    fn attach_solo_al_dueno_vivo_ultimo_asistente() {
        use grafito_assistant_types::ConversationTurn;
        let coords = coords_de_prueba();
        let hecha = turn_media_for_completed_job(&media_de_prueba("T", 2), &coords)
            .expect("historía válida");
        // Vacía o dueño ausente: nada.
        let mut vacia: Vec<ConversationTurn> = Vec::new();
        assert!(!attach_media_to_owner_turn(
            &mut vacia,
            Some(0),
            hecha.clone()
        ));
        assert!(!attach_media_to_owner_turn(&mut vacia, None, hecha.clone()));
        // Último del usuario (dueño apunta al usuario): nada.
        let mut solo_usuario = vec![ConversationTurn::user("hola")];
        assert!(!attach_media_to_owner_turn(
            &mut solo_usuario,
            Some(0),
            hecha.clone()
        ));
        assert!(solo_usuario[0].media.is_none());
        // Dueño válido == último asistente: pega.
        let mut par = vec![
            ConversationTurn::user("derivada"),
            ConversationTurn::assistant("la pendiente"),
        ];
        assert!(attach_media_to_owner_turn(&mut par, Some(1), hecha.clone()));
        assert!(par[0].media.is_none(), "el usuario no lleva media");
        assert!(par[1].media.is_some(), "el dueño sí");
        // Dueño stale (apunta al primero, ya no es último): se descarta.
        let mut otro = vec![
            ConversationTurn::user("vieja"),
            ConversationTurn::assistant("rancia"),
            ConversationTurn::user("nueva"),
            ConversationTurn::assistant("fresca"),
        ];
        assert!(!attach_media_to_owner_turn(
            &mut otro,
            Some(1),
            hecha.clone()
        ));
        assert!(
            otro.iter().all(|t| t.media.is_none()),
            "stale no contamina ningún turno"
        );
        // Dueño fuera de rango: nada.
        assert!(!attach_media_to_owner_turn(&mut otro, Some(99), hecha));
    }

    #[test]
    fn drena_a_con_b_creado_sin_contaminacion() {
        // Regresión T1: el job A se spawneó con dueño=1; antes de drenar, el
        // usuario pidió B (turnos 2,3 nuevos). El drain de A debe descartar:
        // ni slot del dueño viejo ni media en el turno nuevo.
        use grafito_assistant_types::ConversationTurn;
        let coords = coords_de_prueba();
        let media_a =
            turn_media_for_completed_job(&media_de_prueba("A", 2), &coords).expect("media A");
        let mut conversacion = vec![
            ConversationTurn::user("pregunta A"),
            ConversationTurn::assistant("respuesta A"),
        ];
        let dueno_a = Some(1usize);
        assert!(es_dueno_vivo(&conversacion, dueno_a));
        // Llega B: dos turnos nuevos (el dueño de A queda stale).
        conversacion.push(ConversationTurn::user("pregunta B"));
        conversacion.push(ConversationTurn::assistant("respuesta B"));
        assert!(!es_dueno_vivo(&conversacion, dueno_a), "A quedó stale");
        assert!(!attach_media_to_owner_turn(
            &mut conversacion,
            dueno_a,
            media_a
        ));
        assert!(
            conversacion.iter().all(|t| t.media.is_none()),
            "el drain rancio de A no pega en B ni revive A"
        );
        // El dueño nuevo sí vive.
        assert!(es_dueno_vivo(&conversacion, Some(3)));
    }

    #[test]
    fn error_se_anexa_solo_al_dueno_asistente() {
        use grafito_assistant_types::ConversationTurn;
        let mut par = vec![
            ConversationTurn::user("derivada"),
            ConversationTurn::assistant("la pendiente"),
        ];
        assert!(anexar_error_a_dueno(&mut par, Some(1), "motor caído"));
        assert!(
            par[1]
                .content
                .contains("No se pudo generar la animación: motor caído"),
            "prosa: {}",
            par[1].content
        );
        // Dueño usuario / ausente / vacío / fuera de rango: false.
        assert!(!anexar_error_a_dueno(&mut par, Some(0), "x"));
        assert!(!anexar_error_a_dueno(&mut par, None, "x"));
        assert!(!anexar_error_a_dueno(&mut par, Some(1), "   "));
        assert!(!anexar_error_a_dueno(&mut par, Some(9), "x"));
        assert!(!par[0].content.contains("No se pudo"));
    }

    #[test]
    fn trim_app_side_dropea_el_par_con_su_thumb() {
        use grafito_assistant_types::{ConversationTurn, MAX_CONVERSATION_TURNS};
        let coords = coords_de_prueba();
        let mut conversacion: Vec<ConversationTurn> = Vec::new();
        for i in 0..MAX_CONVERSATION_TURNS + 2 {
            conversacion.push(ConversationTurn::user(format!("q{i}")));
            conversacion.push(ConversationTurn::assistant(format!("r{i}")));
        }
        let vieja = turn_media_for_completed_job(&media_de_prueba("Vieja", 2), &coords)
            .expect("media vieja");
        let nueva = turn_media_for_completed_job(&media_de_prueba("Nueva", 2), &coords)
            .expect("media nueva");
        grafito_assistant_types::attach_turn_media(&mut conversacion, 1, vieja)
            .expect("attach viejo");
        let ultimo = conversacion.len() - 1;
        grafito_assistant_types::attach_turn_media(&mut conversacion, ultimo, nueva)
            .expect("attach nuevo");
        trim_conversation_dropping_pair_media(&mut conversacion);
        assert_eq!(conversacion.len(), MAX_CONVERSATION_TURNS);
        // El par más viejo (con su thumb) salió; la media nueva sigue viva.
        assert!(
            !conversacion
                .iter()
                .any(|t| t.media.as_ref().is_some_and(|m| m.title == "Vieja")),
            "el thumb viejo se recorta con el par"
        );
        assert!(
            conversacion
                .last()
                .and_then(|t| t.media.clone())
                .is_some_and(|m| m.title == "Nueva"),
            "el thumb nuevo sobrevive"
        );
    }
}
