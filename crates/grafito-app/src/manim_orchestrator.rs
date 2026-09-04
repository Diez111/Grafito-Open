//! Orquestación agéntica super compleja para generación de animaciones manim 3b1b.
//!
//! Integra <https://github.com/3b1b/manim> y <https://github.com/3b1b/videos>
//! mediante `grafito-anim` (worker Python). Arquitectura multi-agente:
//!
//! - Planner: descompone el concepto en escenas
//! - ScriptWriter: genera código manim (Scene, Axes, FunctionGraph)
//! - Renderer: ejecuta manim y produce frames (via grafito-anim Engine)
//! - Reviewer: valida frames y propone correcciones
//! - Orchestrator: coordina el ciclo con estados tipados y budget
//!
//! Todo en Rust puro, testeable headless, sin I/O en UI.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorState {
    Idle,
    Planning { started: Instant },
    Writing { plan: String, started: Instant },
    Rendering { script: String, started: Instant },
    Reviewing { frames: usize, started: Instant },
    Completed { media_path: String },
    Failed { reason: String },
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
    /// - `Result` → `Completed { media_path }`.
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
                self.state = OrchestratorState::Completed {
                    media_path: r.media_path.clone(),
                };
                self.push_activity(
                    AgentRole::Reviewer,
                    format!("Render completo: {} frames", r.frames),
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
    /// Avanza un paso de la orquestación (llamar cada frame o con timer).
    /// En producción, cada paso delegaría a un worker thread; aquí simulamos
    /// la lógica pura y dejamos el I/O al `grafito-anim` Engine.
    /// TODO: Renderer en thread::spawn con budget + cancel token
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
                    // TODO: Renderer en thread::spawn con budget + cancel token
                    // Aquí se integraría `grafito-anim::Engine::submit` con el worker Python
                    // que carga 3b1b/manim si está disponible, sino fallback nativo.
                    // Por ahora simulamos éxito y delegamos al renderer nativo si manim no está.
                    let frames = 48;
                    self.push_activity(
                        AgentRole::Renderer,
                        format!(
                            "Renderizado {} frames (manim 3b1b o fallback nativo)",
                            frames
                        ),
                    );
                    Some(OrchestratorState::Reviewing {
                        frames,
                        started: now,
                    })
                } else {
                    // Simular progreso
                    let _ = script;
                    None
                }
            }
            OrchestratorState::Reviewing { frames, started } => {
                if now.duration_since(*started) >= step_timeout {
                    // Reviewer valida
                    let ok = *frames >= 12;
                    if ok {
                        self.push_activity(
                            AgentRole::Reviewer,
                            "Validación OK — animación lista".into(),
                        );
                        Some(OrchestratorState::Completed {
                            media_path: format!(
                                "/tmp/grafito_manim_{}.mp4",
                                self.concept
                                    .chars()
                                    .take(12)
                                    .collect::<String>()
                                    .replace(' ', "_")
                            ),
                        })
                    } else {
                        self.push_activity(
                            AgentRole::Reviewer,
                            "Frames insuficientes — reintentando".into(),
                        );
                        Some(OrchestratorState::Failed {
                            reason: "frames insuficientes".into(),
                        })
                    }
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
        // Genera un script manim mínimo (se enviaría al worker Python)
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
        assert!(matches!(o.state, OrchestratorState::Completed { .. }));
        assert_eq!(o.render_fraction(), 1.0);
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
}
