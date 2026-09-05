//! Puente IPC al motor de animaciones externo (ciclo de vida, jobs y presupuestos).
//! Mejoras de auditoria 2026-08-20: Statem AnimJobState, correccion de races/leaks/timeouts.

use crate::protocol::{
    downcast, kinds, localize_worker_error, sanitize_error_code, truncate_worker_message,
    AnimJobId, AnimRequest, AnimResult, RenderProgress, WireMessage, ANIM_PROTOCOL_VERSION,
    MAX_WORKER_MESSAGE_LEN,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

/// Timeout por defecto para completar un job (90 s).
pub const DEFAULT_JOB_TIMEOUT_SECS: u64 = 90;
/// Timeout por defecto para handshake/apagado cooperativo (8 s).
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 8;
/// Tope por defecto de bytes por línea del worker (64 KiB).
pub const DEFAULT_LINE_CAP_BYTES: usize = 64 * 1024;
/// Rango válido para `job_timeout`: 1 s..=600 s.
pub const MIN_JOB_TIMEOUT_SECS: u64 = 1;
pub const MAX_JOB_TIMEOUT_SECS: u64 = 600;
/// Rango válido para `idle_timeout`: 1 s..=60 s.
pub const MIN_IDLE_TIMEOUT_SECS: u64 = 1;
pub const MAX_IDLE_TIMEOUT_SECS: u64 = 60;
/// Rango válido para `line_cap_bytes`: 1 KiB..=1 MiB.
pub const MIN_LINE_CAP_BYTES: usize = 1024;
pub const MAX_LINE_CAP_BYTES: usize = 1024 * 1024;
/// Gracia cooperativa de cancelación: el kill llega antes de 200 ms.
///
/// Se fija en 100 ms para dejar ~100 ms de margen de planificación del SO y
/// cumplir `<200 ms desde el pedido hasta el kill` de forma determinista en
/// test incluso bajo carga de CI.
pub const CANCEL_GRACE: Duration = Duration::from_millis(100);
/// Deadline dura de cancelación exigida (<200 ms).
pub const CANCEL_DEADLINE: Duration = Duration::from_millis(200);

/// Configuracion del proceso del motor de animaciones.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// argv del worker (programa + argumentos).
    pub command: Vec<String>,
    /// Carpeta de trabajo donde el motor debe escribir los artefactos.
    pub working_dir: Option<PathBuf>,
    /// Tiempo de espera para el handshake y el apagado cooperativo.
    pub idle_timeout: Duration,
    /// Tiempo maximo para completar un job.
    pub job_timeout: Duration,
    /// Tope de caracteres por linea de salida del motor.
    pub line_cap_bytes: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            command: vec![
                "python3".to_string(),
                "-m".to_string(),
                "grafito_manim_engine".to_string(),
            ],
            working_dir: None,
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
            job_timeout: Duration::from_secs(DEFAULT_JOB_TIMEOUT_SECS),
            line_cap_bytes: DEFAULT_LINE_CAP_BYTES,
        }
    }
}

impl EngineConfig {
    /// Valida rangos: `job_timeout` 1..=600 s, `idle_timeout` 1..=60 s,
    /// `line_cap` 1 KiB..=1 MiB. Mensajes en español para la UI.
    pub fn validate(&self) -> Result<(), String> {
        let job = self.job_timeout.as_secs();
        if !(MIN_JOB_TIMEOUT_SECS..=MAX_JOB_TIMEOUT_SECS).contains(&job) {
            return Err(format!(
                "job_timeout fuera de rango: {job}s (válido {MIN_JOB_TIMEOUT_SECS}..={MAX_JOB_TIMEOUT_SECS}s)"
            ));
        }
        let idle = self.idle_timeout.as_secs();
        if !(MIN_IDLE_TIMEOUT_SECS..=MAX_IDLE_TIMEOUT_SECS).contains(&idle) {
            return Err(format!(
                "idle_timeout fuera de rango: {idle}s (válido {MIN_IDLE_TIMEOUT_SECS}..={MAX_IDLE_TIMEOUT_SECS}s)"
            ));
        }
        if self.line_cap_bytes < MIN_LINE_CAP_BYTES || self.line_cap_bytes > MAX_LINE_CAP_BYTES {
            return Err(format!(
                "line_cap fuera de rango: {} bytes (válido {MIN_LINE_CAP_BYTES}..={MAX_LINE_CAP_BYTES})",
                self.line_cap_bytes
            ));
        }
        if self.command.is_empty() {
            return Err("comando del motor vacío".to_string());
        }
        Ok(())
    }

    /// Construye la config desde Env con validación de rango.
    ///
    /// Vars: `GRAFITO_ANIM_JOB_TIMEOUT_SECS`, `GRAFITO_ANIM_IDLE_TIMEOUT_SECS`,
    /// `GRAFITO_ANIM_LINE_CAP_BYTES`. Ausentes → defecto (90 s / 8 s / 64 KiB).
    /// Valores no numéricos o fuera de rango → `Err` en español.
    pub fn from_env() -> Result<Self, String> {
        let mut cfg = Self::default();
        if let Ok(raw) = std::env::var("GRAFITO_ANIM_JOB_TIMEOUT_SECS") {
            let secs: u64 = raw.trim().parse().map_err(|_| {
                format!("GRAFITO_ANIM_JOB_TIMEOUT_SECS inválido: {raw:?} (entero en segundos)")
            })?;
            cfg.job_timeout = Duration::from_secs(secs);
        }
        if let Ok(raw) = std::env::var("GRAFITO_ANIM_IDLE_TIMEOUT_SECS") {
            let secs: u64 = raw.trim().parse().map_err(|_| {
                format!("GRAFITO_ANIM_IDLE_TIMEOUT_SECS inválido: {raw:?} (entero en segundos)")
            })?;
            cfg.idle_timeout = Duration::from_secs(secs);
        }
        if let Ok(raw) = std::env::var("GRAFITO_ANIM_LINE_CAP_BYTES") {
            let bytes: usize = raw.trim().parse().map_err(|_| {
                format!("GRAFITO_ANIM_LINE_CAP_BYTES inválido: {raw:?} (entero en bytes)")
            })?;
            cfg.line_cap_bytes = bytes;
        }
        cfg.validate()?;
        Ok(cfg)
    }
}

/// Estado tipado del ciclo de vida de un job (Statem).
/// Cada transicion es verificada; no hay submit sin Ready, no hay fuga sin ShuttingDown.
///
/// ```text
/// Idle -> Spawning -> AwaitingHello{deadline} -> AwaitingPong{deadline} -> Ready
///      -> Running{job_id, deadline, duration} -> Cancelling{job_id} -> ShuttingDown{deadline}
///      -> Completed{media_path} | Failed{code,msg} | TimedOut | Cancelled
/// ```
/// Deadlines absolutas (Instant::now()+timeout) para no derivar con polls.
/// Drop no-bloqueante: kill+try_wait sin wait().
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimJobState {
    Idle,
    Spawning,
    AwaitingHello {
        deadline: Instant,
    },
    AwaitingPong {
        deadline: Instant,
    },
    Ready,
    Running {
        job_id: AnimJobId,
        deadline: Instant,
    },
    Cancelling {
        job_id: AnimJobId,
    },
    ShuttingDown {
        deadline: Instant,
    },
    Completed {
        media_path: PathBuf,
    },
    Failed {
        code: String,
        message: String,
    },
    TimedOut,
    Cancelled,
}

impl AnimJobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::TimedOut | Self::Cancelled
        )
    }
    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

// ── Statem v3 extendido (documentado + preparado para migración) ───────────
// El statem v2 actual (AnimJobState) cubre el puente IPC. La extensión v3
// expone progreso fino Queued/Rendering/Exporting/Retrying y duración.
// Si no se migra engine completo, este enum + trait permiten implementar
// un motor v3 sin romper el API v2 — el engine v2 puede mapearse a v3.

// ---------------------------------------------------------------------------
// AnimEngineState v3 — granulometría fina de pipeline
// ---------------------------------------------------------------------------

/// Estado extendido v3 para motores que exponen cola, render y export.
/// Complementa a `AnimJobState`; no reemplaza todavía el Statem IPC.
/// Migración futura: `AnimJobState::Running` ↔ `Rendering` + `Exporting`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimEngineState {
    /// En cola, aún no enviado al worker.
    Queued,
    /// Renderizando frames: `done` de `total` (0..=total).
    Rendering {
        done: u32,
        total: u32,
    },
    /// Exportando al formato pedido (gif/mp4/png).
    Exporting {
        format: String,
    },
    /// Reintentando tras error transitorio: intento N con backoff.
    /// NOTA v3: representación solamente — ningún código construye esta
    /// variante ni reintenta automáticamente (ver doc de `submit`).
    Retrying {
        attempt: u32,
        backoff_ms: u64,
    },
    /// Mapeo 1:1 a terminales de `AnimJobState` (para compat).
    Completed {
        media_path: PathBuf,
    },
    Failed {
        code: String,
        message: String,
    },
    TimedOut,
    Cancelled,
}

impl AnimEngineState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::TimedOut | Self::Cancelled
        )
    }
}

/// Trait mínimo para motores v3 (y para mockear en tests).
/// Especificación pedida: `trait AnimEngine { fn submit(&mut self, params: AnimParams) -> Result<AnimJobId> }`
/// Se nombra `AnimEngineTrait` para evitar colisión con la struct `AnimEngine` existente;
/// alias `AnimEngine` en spec → `AnimEngineTrait` aquí. Migración futura renombrará la struct a `AnimEngineImpl`.
pub trait AnimEngineTrait {
    fn submit(&mut self, params: crate::protocol::AnimParams) -> Result<AnimJobId, String>;
    fn engine_state(&self) -> Option<AnimEngineState>;
    fn cancel_engine(&mut self) -> Result<(), String>;
}

impl AnimEngineTrait for AnimEngine {
    fn submit(&mut self, params: crate::protocol::AnimParams) -> Result<AnimJobId, String> {
        self.submit_params(params)
    }
    fn engine_state(&self) -> Option<AnimEngineState> {
        match self.state() {
            AnimJobState::Idle | AnimJobState::Spawning => Some(AnimEngineState::Queued),
            // Progreso REAL: done/total derivan del último `progress` del worker
            // (percent 0..=100), no de un 0/48 inventado.
            AnimJobState::Running { .. } => {
                let done = self
                    .last_progress
                    .as_ref()
                    .map_or(0, |p| u32::from(p.percent.min(100)));
                Some(AnimEngineState::Rendering { done, total: 100 })
            }
            AnimJobState::ShuttingDown { .. } => Some(AnimEngineState::Exporting {
                format: "gif".into(),
            }),
            AnimJobState::Completed { media_path } => Some(AnimEngineState::Completed {
                media_path: media_path.clone(),
            }),
            AnimJobState::Failed { code, message } => Some(AnimEngineState::Failed {
                code: code.clone(),
                message: message.clone(),
            }),
            AnimJobState::TimedOut => Some(AnimEngineState::TimedOut),
            AnimJobState::Cancelled | AnimJobState::Cancelling { .. } => {
                Some(AnimEngineState::Cancelled)
            }
            _ => None,
        }
    }
    fn cancel_engine(&mut self) -> Result<(), String> {
        self.cancel()
    }
}

/// Evento de un job de render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    Progress(RenderProgress),
    Result(AnimResult),
    Error { code: String, message: String },
}

impl JobEvent {
    /// Fracción REAL 0..1 solo para `Progress` (parseada del worker).
    ///
    /// Devuelve `Some(f)` con `f = percent/100.0` si es progreso, `None` en
    /// otro caso. Nunca inventa valores: si no hay `Progress`, la UI debe
    /// mostrar indeterminado.
    pub fn fraction(&self) -> Option<f32> {
        match self {
            Self::Progress(p) => Some(p.fraction()),
            Self::Result(_) | Self::Error { .. } => None,
        }
    }
    /// Mensaje localizado al español para `Error`; `None` si no es error.
    pub fn localized_error(&self) -> Option<String> {
        match self {
            Self::Error { code, message } => Some(localize_worker_error(code, message)),
            Self::Progress(_) | Self::Result(_) => None,
        }
    }
}

/// Puente hacia un proceso de motor de animaciones ya lanzado.
pub struct AnimEngine {
    child: Option<Child>,
    stdin: ChildStdin,
    events: Receiver<WireMessage>,
    next_job: u64,
    config: EngineConfig,
    diagnostics: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    state: AnimJobState,
    /// Último progreso REAL reportado por el worker (fracción vía `fraction()`).
    last_progress: Option<RenderProgress>,
}

impl AnimEngine {
    /// Lanza el proceso del motor y empieza a leer sus mensajes.
    pub fn spawn(config: EngineConfig) -> Result<Self, String> {
        config.validate()?;
        for arg in &config.command {
            if arg.contains('\0') {
                return Err("el comando del motor contiene byte NUL".into());
            }
        }
        if let Some(dir) = &config.working_dir {
            if dir.as_os_str().is_empty() {
                return Err("working_dir del motor vacío".into());
            }
            if !dir.exists() {
                return Err(format!(
                    "working_dir del motor no existe: {}",
                    dir.display()
                ));
            }
        }
        // Validación temprana: si el binario es una ruta, verificar existencia antes de spawn.
        if config.command[0].contains('/') || config.command[0].contains('\\') {
            let bin = Path::new(&config.command[0]);
            if !bin.exists() {
                return Err(format!(
                    "binario del motor no encontrado: {}",
                    bin.display()
                ));
            }
        }
        let mut command = Command::new(&config.command[0]);
        command
            .args(&config.command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(working_dir) = &config.working_dir {
            command.current_dir(working_dir);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("no se pudo iniciar el motor de animación: {error}"))?;
        // Si take() falla, matamos al hijo para no fugarlo (S1).
        let stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("stdin del motor no disponible".into());
            }
        };
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("stdout del motor no disponible".into());
            }
        };
        let stderr = child.stderr.take();
        let (sender, receiver) = std::sync::mpsc::sync_channel(128);
        let diagnostics = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        spawn_reader(stdout, sender, config.line_cap_bytes);
        if let Some(stderr) = stderr {
            spawn_stderr_drainer(stderr, std::sync::Arc::clone(&diagnostics));
        }
        Ok(Self {
            child: Some(child),
            stdin,
            events: receiver,
            next_job: 0,
            config,
            diagnostics,
            state: AnimJobState::Spawning,
            last_progress: None,
        })
    }

    pub fn state(&self) -> &AnimJobState {
        &self.state
    }

    /// Último progreso REAL del worker, si ya emitió `progress`.
    pub fn last_progress(&self) -> Option<&RenderProgress> {
        self.last_progress.as_ref()
    }

    /// Fracción REAL 0..1 del último `progress` (`percent/100`).
    ///
    /// `0.0` si aún no hay progreso: la UI debe mostrar indeterminado en ese
    /// caso y nunca inventar un % falso.
    pub fn progress_fraction(&self) -> f32 {
        self.last_progress.as_ref().map_or(0.0, |p| p.fraction())
    }

    /// PID del hijo para tests de cancelación (verifican kill <200 ms).
    #[cfg(test)]
    fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Diagnosticos (stderr) recogidos del motor — poison-aware.
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Espera el handshake (hello + pong) antes de enviar jobs.
    /// Usa config.idle_timeout (H1) y no silencia Error (H2).
    pub fn wait_ready(&mut self) -> Result<(), String> {
        self.state = AnimJobState::AwaitingHello {
            deadline: Instant::now() + self.config.idle_timeout,
        };
        let hello_deadline = Instant::now() + self.config.idle_timeout;
        loop {
            if Instant::now() >= hello_deadline {
                self.state = AnimJobState::Failed {
                    code: "handshake_timeout".into(),
                    message: truncate_worker_message("hello no recibido"),
                };
                let _ = self.shutdown();
                return Err(localize_worker_error(
                    "handshake_timeout",
                    "el motor no envió el saludo inicial",
                ));
            }
            let remaining = hello_deadline.saturating_duration_since(Instant::now());
            // polling fino para respetar deadline sin 1s de granularidad
            let poll = remaining.min(Duration::from_millis(250));
            match self.recv_raw(Some(poll)) {
                Ok(Some(WireMessage::Hello {
                    protocol_version, ..
                })) => {
                    if protocol_version != ANIM_PROTOCOL_VERSION {
                        self.state = AnimJobState::Failed {
                            code: "version_mismatch".into(),
                            message: truncate_worker_message(&format!("v{protocol_version}")),
                        };
                        let _ = self.shutdown();
                        return Err(localize_worker_error(
                            "version_mismatch",
                            &format!(
                                "el motor habla v{protocol_version}; Grafito soporta v{ANIM_PROTOCOL_VERSION}"
                            ),
                        ));
                    }
                    break;
                }
                Ok(Some(WireMessage::Error { code, message })) => {
                    let code = sanitize_error_code(&code);
                    let message = truncate_worker_message(&message);
                    self.state = AnimJobState::Failed {
                        code: code.clone(),
                        message: message.clone(),
                    };
                    let _ = self.shutdown();
                    return Err(localize_worker_error(&code, &message));
                }
                Ok(_) => {}
                Err(error) => {
                    self.state = AnimJobState::Failed {
                        code: "handshake_error".into(),
                        message: truncate_worker_message(&error),
                    };
                    return Err(error);
                }
            }
        }
        self.send(&json!({ "type": kinds::PING }))?;
        let pong_timeout = self.config.idle_timeout.min(Duration::from_secs(4));
        let pong_deadline = Instant::now() + pong_timeout;
        self.state = AnimJobState::AwaitingPong {
            deadline: pong_deadline,
        };
        loop {
            if Instant::now() >= pong_deadline {
                self.state = AnimJobState::Failed {
                    code: "handshake_timeout".into(),
                    message: truncate_worker_message("pong no recibido"),
                };
                let _ = self.shutdown();
                return Err(localize_worker_error(
                    "handshake_timeout",
                    "el motor no respondió al ping de salud",
                ));
            }
            let remaining = pong_deadline.saturating_duration_since(Instant::now());
            let poll = remaining.min(Duration::from_millis(250));
            match self.recv_raw(Some(poll)) {
                Ok(Some(WireMessage::Pong)) => {
                    self.state = AnimJobState::Ready;
                    return Ok(());
                }
                Ok(Some(WireMessage::Error { code, message })) => {
                    let code = sanitize_error_code(&code);
                    let message = truncate_worker_message(&message);
                    self.state = AnimJobState::Failed {
                        code: code.clone(),
                        message: message.clone(),
                    };
                    let _ = self.shutdown();
                    return Err(localize_worker_error(&code, &message));
                }
                Ok(_) => {}
                Err(e) => {
                    self.state = AnimJobState::Failed {
                        code: "handshake_error".into(),
                        message: truncate_worker_message(&e),
                    };
                    return Err(e);
                }
            }
        }
    }

    /// Envia un nuevo pedido y devuelve su id. Solo en Ready (SB1).
    /// Propaga `duration_ms` al worker (antes se perdía en `into_request` sin campo).
    ///
    /// NOTA v3 (honesta): NO hay cola FIFO — el engine atiende un solo job por
    /// vez; un segundo `submit` en `Running` se rechaza hasta que el actual
    /// termine (`shutdown` + `spawn`/`wait_ready` de nuevo). `AnimEngineState`
    /// expone `Retrying` solo como representación; no existe loop de reintento
    /// automático. El test `submit_while_running_is_rejected_no_fifo_queue`
    /// pinnea este comportamiento.
    pub fn submit(&mut self, request: AnimRequest) -> Result<AnimJobId, String> {
        if !self.state.can_submit() {
            return Err(format!(
                "no se puede enviar en estado {:?}: llamá wait_ready() primero",
                self.state
            ));
        }
        // Validación temprana: evita enviar canvas >4096 o duration fuera de rango al motor.
        // Antes 8192 pasaba y el motor clampaba silencioso.
        if let Err(e) = request.validate() {
            return Err(format!("petición inválida: {e}"));
        }
        self.next_job = self.next_job.wrapping_add(1);
        let job_id = AnimJobId(format!("job-{}", self.next_job));
        let message = json!({
            "type": kinds::RENDER_REQUEST,
            "job_id": job_id.0,
            "template": request.template,
            "concept": request.concept,
            "params": request.params,
            "spec": request.spec,
            "export": request.export.as_str(),
            "canvas": [request.canvas.0, request.canvas.1],
            "duration_ms": request.duration_ms,
        });
        self.send(&message)?;
        self.state = AnimJobState::Running {
            job_id: job_id.clone(),
            deadline: Instant::now() + self.config.job_timeout,
        };
        Ok(job_id)
    }

    /// Variante type-safe que recibe `AnimParams` (con duration tipada) y valida.
    pub fn submit_params(
        &mut self,
        params: crate::protocol::AnimParams,
    ) -> Result<AnimJobId, String> {
        params
            .validate()
            .map_err(|e| format!("parámetros inválidos: {e}"))?;
        let req = params.into_request();
        self.submit(req)
    }

    /// Cancela el job en curso con deadline cooperativa <200 ms.
    ///
    /// Envía `SHUTDOWN`, espera hasta [`CANCEL_GRACE`] (100 ms) a salida
    /// graciosa y luego hace kill. Sincrónico y acotado: retorna en <200 ms
    /// incluso si el worker ignora el apagado. Transiciona a `Cancelling` y
    /// luego a `Cancelled`.
    pub fn cancel(&mut self) -> Result<(), String> {
        match &self.state {
            AnimJobState::Running { job_id, .. } => {
                let job_id = job_id.clone();
                self.state = AnimJobState::Cancelling { job_id };
                let _ = self.send(&json!({ "type": kinds::SHUTDOWN }));
                kill_child_with_grace(self.child.take(), CANCEL_GRACE);
                self.state = AnimJobState::Cancelled;
                Ok(())
            }
            other => Err(format!(
                "no hay job en curso para cancelar (estado {other:?})"
            )),
        }
    }

    /// Kill cooperativo rápido cuando el estado ya es `Cancelling`/`TimedOut`.
    ///
    /// Usado por `run_job` que fija el estado antes de matar (evita el chequeo
    /// `Running` de [`Self::cancel`]). Envía `SHUTDOWN`, mata en <200 ms y
    /// deja `Cancelled` si venía de `Cancelling`; `TimedOut` se conserva.
    fn cancel_fast_after_state(&mut self) -> Result<(), String> {
        let _ = self.send(&json!({ "type": kinds::SHUTDOWN }));
        kill_child_with_grace(self.child.take(), CANCEL_GRACE);
        if matches!(self.state, AnimJobState::Cancelling { .. }) {
            self.state = AnimJobState::Cancelled;
        }
        Ok(())
    }

    /// Lee el siguiente evento de un job (None cuando no hay mensaje en el timeout).
    ///
    /// Actualiza [`Self::progress_fraction`] con el último `Progress` REAL del
    /// worker (`percent/100.0`). Requiere `&mut` para recordar ese progreso.
    pub fn recv_event(&mut self, timeout: Option<Duration>) -> Result<Option<JobEvent>, String> {
        match self.recv_raw(timeout)? {
            Some(WireMessage::Progress(progress)) => {
                self.last_progress = Some(progress.clone());
                Ok(Some(JobEvent::Progress(progress)))
            }
            Some(WireMessage::Result(result)) => Ok(Some(JobEvent::Result(result))),
            Some(WireMessage::Error { code, message }) => {
                let code = sanitize_error_code(&code);
                let message = truncate_worker_message(&message);
                Ok(Some(JobEvent::Error { code, message }))
            }
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }

    /// Apaga el motor de forma cooperativa sin bloquear el hilo de UI.
    ///
    /// Envía `SHUTDOWN` y delega el `try_wait`/`kill`/`wait` a un hilo en segundo
    /// plano. El llamante sólo debe marcar `ShuttingDown` y pedir `request_repaint`;
    /// no bloquea 8 s en el UI thread. El `Drop` garantiza la limpieza final con
    /// `try_wait` no bloqueante.
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.shutdown_with_grace(self.config.idle_timeout)
    }

    /// Apagado con gracia explícita (para tests y cancelación rápida).
    fn shutdown_with_grace(&mut self, grace: Duration) -> Result<(), String> {
        self.state = AnimJobState::ShuttingDown {
            deadline: Instant::now() + grace,
        };
        let _ = self.send(&json!({ "type": kinds::SHUTDOWN }));
        // Delega la espera bloqueante a un hilo para no congelar la UI.
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                wait_or_kill(&mut child, grace);
                let _ = child.wait();
            });
        }
        Ok(())
    }

    /// Variante explícitamente asíncrona para uso desde UI: idéntica a `shutdown`
    /// pero documenta la intención no bloqueante y puede consultarse vía `state()`.
    pub fn shutdown_async(&mut self) -> Result<(), String> {
        self.shutdown()
    }

    fn send(&mut self, value: &Value) -> Result<(), String> {
        let mut line = value.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|error| format!("falló escribir al stdin del motor: {error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("falló vaciar el stdin del motor: {error}"))
    }

    fn recv_raw(&self, timeout: Option<Duration>) -> Result<Option<WireMessage>, String> {
        let received = match timeout {
            Some(duration) => self.events.recv_timeout(duration),
            None => self
                .events
                .recv()
                .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(message) => Ok(Some(message)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                let diagnostics = self.diagnostics().join("; ");
                let detail = truncate_worker_message(&diagnostics);
                Err(if detail.is_empty() {
                    "el motor se cerró inesperadamente".to_string()
                } else {
                    format!("el motor se cerró inesperadamente: {detail}")
                })
            }
        }
    }
}

/// Espera salida graciosa hasta `grace` y luego hace kill (cooperativo).
fn wait_or_kill(child: &mut Child, grace: Duration) {
    let deadline = Instant::now() + grace;
    loop {
        if Instant::now() >= deadline {
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
}

/// Kill sincrónico acotado a `grace` para cancelación <200 ms.
///
/// Toma el hijo, espera salida graciosa hasta `grace` y luego mata y cosecha.
/// Retorna siempre antes de `grace + ~20 ms` incluso si el worker ignora todo.
fn kill_child_with_grace(child: Option<Child>, grace: Duration) {
    if let Some(mut c) = child {
        wait_or_kill(&mut c, grace);
        let _ = c.kill();
        let _ = c.wait();
    }
}

impl Drop for AnimEngine {
    fn drop(&mut self) {
        // No bloquear el hilo de UI: intenta matar y hace try_wait no bloqueante.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.try_wait();
        }
        // Si aún no terminó, el SO lo cosechará; evitamos bloquear Drop.
    }
}

/// Ejecuta un job de punta a punta contra un motor efimero.
/// Polling de 200ms para honrar cancel (RJ1) y timeout incluye wait_ready via deadline absoluta.
///
/// - Progreso REAL: cada `Progress` del worker se reenvía a `on_event` tal cual
///   (fracción `percent/100.0` vía [`JobEvent::fraction`], sin inventar %).
/// - Errores tipados: `code + mensaje(≤500)` localizados al español.
/// - Cancelación cooperativa con kill <200 ms; timeout con kill rápido.
///
/// Cancelación con `CancellationToken` (ANIM-REVIVE): la firma YA lo soporta —
/// `cancel: Option<&dyn Fn() -> bool>` acepta cualquier señal, incluido el
/// `CancellationToken` de `grafito-assistant` (`is_cancelled()`), sin añadir
/// dependencia a este crate. Patrón para el lead en `grafito-app`:
/// ```ignore
/// let token = grafito_assistant::CancellationToken::default();
/// let cancel = || token.is_cancelled(); // vive lo que dure `run_job`
/// grafito_anim::run_job(&config, &request, Some(&cancel), on_event)?;
/// // El botón Cancelar del panel (`AnimPanelEvent::CancelRequested`) debe
/// // llamar a `token.cancel()`; el loop de aquí lo observa cada ≤200 ms.
/// ```
/// HUECO EXACTO pineado: `grafito-app/src/assistant.rs` hoy llama a `run_job`
/// con `None` (ver `Some(&|| true)` en el test `run_job_honors_cancel_closure`
/// como prueba de que el cableado funciona cuando se pasa). Cambiar ese `None`
/// por el closure de arriba es todo lo que falta (scope del lead, `app.rs` /
/// `assistant.rs` fuera de este módulo).
pub fn run_job(
    config: &EngineConfig,
    request: &AnimRequest,
    cancel: Option<&dyn Fn() -> bool>,
    mut on_event: impl FnMut(JobEvent),
) -> Result<AnimResult, String> {
    // Validación temprana: duration y canvas viajaban perdidos o clampados.
    request
        .validate()
        .map_err(|e| format!("petición inválida: {e}"))?;
    let mut engine = AnimEngine::spawn(config.clone())?;
    engine.wait_ready()?;
    let job_id = engine.submit(request.clone())?;
    let started = Instant::now();
    // deadline absoluta para timeout correcto (RJ2)
    let job_deadline = started + config.job_timeout;
    loop {
        if cancel.is_some_and(|cancel| cancel()) {
            engine.state = AnimJobState::Cancelling {
                job_id: job_id.clone(),
            };
            // Kill cooperativo rápido (<200 ms) en lugar de shutdown de 8 s.
            let _ = engine.cancel_fast_after_state();
            return Err(localize_worker_error("cancelled", ""));
        }
        let now = Instant::now();
        if now >= job_deadline {
            engine.state = AnimJobState::TimedOut;
            let _ = engine.cancel_fast_after_state();
            return Err(localize_worker_error(
                "job_timeout",
                &format!("límite {}s excedido", config.job_timeout.as_secs()),
            ));
        }
        let remaining = job_deadline.saturating_duration_since(now);
        // cap a 200ms para chequear cancel con baja latencia
        let poll = remaining.min(Duration::from_millis(200));
        match engine.recv_event(Some(poll)) {
            Ok(Some(JobEvent::Progress(progress))) => {
                // filtrar por job_id si el motor multiplexa (RJ5)
                if progress.job_id == job_id {
                    on_event(JobEvent::Progress(progress));
                }
            }
            Ok(Some(JobEvent::Result(result))) => {
                if result.job_id != job_id {
                    continue;
                }
                // Asegura que el parent existe antes de validar (evita TOCTOU por subdir inexistente).
                {
                    let working = config.working_dir.as_deref().unwrap_or(Path::new("."));
                    let candidate = Path::new(&result.media_path);
                    let absolute = if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        working.join(candidate)
                    };
                    if let Some(parent) = absolute.parent() {
                        if !parent.exists() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                    }
                }
                if !validate_media_path(
                    config.working_dir.as_deref().unwrap_or(Path::new(".")),
                    &result.media_path,
                ) {
                    engine.state = AnimJobState::Failed {
                        code: "path_escape".into(),
                        message: truncate_worker_message(&result.media_path),
                    };
                    let _ = engine.shutdown();
                    return Err(localize_worker_error(
                        "path_escape",
                        "el artefacto quedó fuera del área de trabajo",
                    ));
                }
                engine.state = AnimJobState::Completed {
                    media_path: PathBuf::from(result.media_path.clone()),
                };
                let _ = engine.shutdown();
                return Ok(result);
            }
            Ok(Some(JobEvent::Error { code, message })) => {
                let code = sanitize_error_code(&code);
                let message = truncate_worker_message(&message);
                debug_assert!(
                    message.chars().count() <= MAX_WORKER_MESSAGE_LEN,
                    "mensaje worker acotado a 500"
                );
                engine.state = AnimJobState::Failed {
                    code: code.clone(),
                    message: message.clone(),
                };
                let _ = engine.shutdown();
                return Err(localize_worker_error(&code, &message));
            }
            Ok(None) => {}
            Err(error) => {
                engine.state = AnimJobState::Failed {
                    code: "engine_exit".into(),
                    message: truncate_worker_message(&error),
                };
                let _ = engine.shutdown();
                return Err(error);
            }
        }
    }
}

/// Comprueba que el artefacto devuelto queda dentro del directorio de trabajo.
/// Corrige rama muerta (V1), NUL (V4) y TOCTOU documentado.
/// TOCTOU documentado: verificar post-open que fd sigue dentro de cwd via /proc/self/fd
pub fn validate_media_path(working_dir: &Path, media_path: &str) -> bool {
    if media_path.contains('\0') {
        return false;
    }
    let path = Path::new(media_path);
    // Rechaza paths vacios o sin parent util
    if media_path.is_empty() {
        return false;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    };
    let Ok(cwd) = working_dir.canonicalize() else {
        return false;
    };
    let Some(parent) = absolute.parent() else {
        return false;
    };
    // Si el parent no existe, intenta crearlo (el caller debería haberlo hecho,
    // pero aquí toleramos races donde el subdir aún no existe).
    let parent_canonical = match parent.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => {
            if !parent.exists() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    log::warn!(
                        "validate_media_path: no se pudo crear parent {}: {err}",
                        parent.display()
                    );
                    return false;
                }
            }
            match parent.canonicalize() {
                Ok(canonical) => canonical,
                Err(err) => {
                    log::warn!(
                        "validate_media_path: canonicalize falló para {}: {err}",
                        parent.display()
                    );
                    return false;
                }
            }
        }
    };
    if !parent_canonical.starts_with(&cwd) {
        return false;
    }
    // TOCTOU documentado: verificar post-open que fd sigue dentro de cwd via /proc/self/fd
    // El caller debe verificar post-open que el fd abierto sigue dentro de cwd.
    true
}

fn spawn_reader(stdout: ChildStdout, sender: SyncSender<WireMessage>, line_cap: usize) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line: Vec<u8> = Vec::new();
            let mut oversized = false;
            let mut done = false;
            while !done {
                let mut chunk = [0_u8; 512];
                match reader.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(count) => {
                        // Si ya es oversized, drenamos sin acumular hasta \n (LC1 fix)
                        if oversized {
                            if let Some(_nl) = chunk[..count].iter().position(|b| *b == b'\n') {
                                done = true;
                                // descartamos resto de linea
                            }
                            continue;
                        }
                        if let Some(newline) = chunk[..count].iter().position(|byte| *byte == b'\n')
                        {
                            // check cap incluyendo lo que viene antes de \n
                            if line.len() + newline > line_cap {
                                oversized = true;
                                // drenar resto de este chunk hasta \n ya hecho
                                done = true;
                                // limpiar lo acumulado para no retener OOM
                                line.clear();
                            } else {
                                line.extend_from_slice(&chunk[..newline]);
                                done = true;
                            }
                        } else {
                            // sin newline en este chunk
                            if line.len() + count > line_cap {
                                oversized = true;
                                line.clear();
                                // seguir drenando sin acumular
                            } else {
                                line.extend_from_slice(&chunk[..count]);
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
            if oversized {
                let _ = sender.send(WireMessage::Error {
                    code: "protocol".into(),
                    message: "línea del motor excede el límite de 64 KiB".into(),
                });
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let parsed: Result<Value, _> = serde_json::from_slice(&line);
            let Ok(value) = parsed else {
                continue;
            };
            if let Some(message) = downcast(&value) {
                // sync_channel puede bloquear: si esta llena, este thread frena al motor (backpressure)
                let _ = sender.send(message);
            }
        }
    });
}

fn spawn_stderr_drainer(
    stderr: std::process::ChildStderr,
    shared: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                    if guard.len() < 64 {
                        guard.push(line.trim_end().to_string());
                    }
                }
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ExportFormat;
    use std::collections::BTreeMap;
    use std::fs;

    const STUB: &str = r#"
import json, sys, os, time
def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()
send({"type":"hello","protocol_version":1,"capabilities":["derivative-slope"]})
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except Exception:
        continue
    t = msg.get("type")
    if t == "ping":
        send({"type":"pong"})
    elif t == "shutdown":
        break
    elif t == "render_request":
        jid = msg["job_id"]
        if msg.get("concept") == "fail":
            send({"type":"error","job_id":jid,"code":"render_failed","message":"boom"})
            continue
        if msg.get("concept") == "never":
            time.sleep(120)
            continue
        send({"type":"progress","job_id":jid,"step":"render","percent":50})
        out_dir = os.getcwd()
        path = os.path.join(out_dir, jid + ".png")
        with open(path, "wb") as fh:
            fh.write(b"\x89PNG\r\n\x1a\n")
        send({"type":"render_result","job_id":jid,"media_path":path,"frames":1,"duration_ms":120})
"#;

    fn python_available() -> bool {
        Command::new("python3")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn stub_engine() -> (TempDirGuard, EngineConfig) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "grafito_anim_stub_{}_{}_{:?}",
            std::process::id(),
            id,
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let stub_path = dir.join("stub_engine.py");
        fs::write(&stub_path, STUB).unwrap();
        let config = EngineConfig {
            command: vec![
                "python3".to_string(),
                "-u".to_string(),
                stub_path.to_string_lossy().to_string(),
            ],
            working_dir: Some(dir.clone()),
            ..Default::default()
        };
        (TempDirGuard(dir), config)
    }

    fn derivada_request(concept: &str) -> AnimRequest {
        AnimRequest {
            template: "derivative-slope".into(),
            concept: concept.to_string(),
            params: BTreeMap::new(),
            spec: None,
            export: ExportFormat::PngSequence,
            canvas: (640, 480),
            duration_ms: 2000,
        }
    }

    #[test]
    fn health_check_and_job_roundtrip_over_an_external_stub() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine();
        let mut engine = AnimEngine::spawn(config.clone()).unwrap();
        engine.wait_ready().unwrap();
        assert!(matches!(engine.state(), AnimJobState::Ready));
        let job_id = engine
            .submit(derivada_request("derivada como pendiente"))
            .unwrap();
        assert!(!job_id.is_empty());
        let result = loop {
            match engine.recv_event(Some(Duration::from_secs(10))).unwrap() {
                Some(JobEvent::Progress(progress)) => {
                    assert_eq!(progress.job_id, job_id);
                }
                Some(JobEvent::Result(anim_result)) => break anim_result,
                Some(JobEvent::Error { .. }) => panic!("stub reported an error"),
                None => {}
            }
        };
        assert!(validate_media_path(
            config.working_dir.as_deref().unwrap(),
            &result.media_path
        ));
        engine.shutdown().unwrap();
    }

    #[test]
    fn run_job_propagates_engine_errors() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine();
        let error = run_job(&config, &derivada_request("fail"), None, |_| {}).unwrap_err();
        // Error tipado localizado al español, sin inglés crudo.
        assert!(
            error.contains("falló el render"),
            "error localizado esperado, got: {error}"
        );
        assert!(
            !error.to_lowercase().contains("animation engine"),
            "sin inglés crudo, got: {error}"
        );
    }

    #[test]
    fn run_job_times_out_when_the_engine_never_answers() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, mut config) = stub_engine();
        // Mínimo validado 1 s (rango 1..=600 s); el stub "never" duerme 120 s.
        config.job_timeout = Duration::from_secs(1);
        let error = run_job(&config, &derivada_request("never"), None, |_| {}).unwrap_err();
        assert!(
            error.contains("tiempo agotado"),
            "timeout localizado esperado, got: {error}"
        );
    }

    #[test]
    fn media_paths_outside_the_working_dir_are_rejected() {
        let dir = std::env::temp_dir().join("grafito_anim_pathguard");
        fs::create_dir_all(&dir).unwrap();
        assert!(validate_media_path(&dir, "out.png"));
        assert!(!validate_media_path(&dir, "../out.png"));
        assert!(!validate_media_path(&dir, "/etc/passwd"));
        // NUL rechazado
        assert!(!validate_media_path(&dir, "out\0.png"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn statem_rejects_submit_before_ready() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine();
        let mut engine = AnimEngine::spawn(config).unwrap();
        let err = engine.submit(derivada_request("x")).unwrap_err();
        assert!(err.contains("wait_ready"), "err: {err}");
        let _ = engine.shutdown();
    }

    // ── T1 Progreso REAL: fracción 0..1 parseada del worker ──────────────
    #[test]
    fn progress_fraction_is_real_not_invented() {
        use crate::protocol::RenderProgress;
        // Unidad pura: percent → fracción, sin inventar.
        for (pct, want) in [(0u8, 0.0f32), (30, 0.3), (50, 0.5), (100, 1.0)] {
            let p = RenderProgress {
                job_id: "job-1".into(),
                step: "render".into(),
                percent: pct,
            };
            assert!(
                (p.fraction() - want).abs() < 1e-6,
                "percent {pct} → {} (esperaba {want})",
                p.fraction()
            );
        }
        // JobEvent::fraction solo Some para Progress.
        let prog = JobEvent::Progress(RenderProgress {
            job_id: "j".into(),
            step: "s".into(),
            percent: 60,
        });
        assert_eq!(prog.fraction(), Some(0.6));
        assert!(prog.localized_error().is_none());
        let res = JobEvent::Result(AnimResult {
            job_id: "j".into(),
            media_path: "/tmp/x.png".into(),
            frames: 1,
            duration_ms: 10,
        });
        assert_eq!(res.fraction(), None);
        let err = JobEvent::Error {
            code: "render_failed".into(),
            message: "boom".into(),
        };
        assert_eq!(err.fraction(), None);
        let loc = err.localized_error().unwrap();
        assert!(loc.contains("falló el render"), "loc: {loc}");
    }

    #[test]
    fn engine_tracks_real_progress_from_stub() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine();
        let mut engine = AnimEngine::spawn(config).unwrap();
        engine.wait_ready().unwrap();
        // Sin progress aún: 0.0 (UI debe mostrar indeterminado, no inventar).
        assert_eq!(engine.progress_fraction(), 0.0);
        let _job = engine.submit(derivada_request("derivada")).unwrap();
        // El stub emite progress 50 → fracción 0.5 real.
        let mut saw = false;
        for _ in 0..100 {
            match engine.recv_event(Some(Duration::from_millis(200))).unwrap() {
                Some(JobEvent::Progress(p)) => {
                    assert_eq!(p.percent, 50);
                    assert!((p.fraction() - 0.5).abs() < 1e-6);
                    assert!((engine.progress_fraction() - 0.5).abs() < 1e-6);
                    // engine_state v3 usa done/total reales (50/100), no 0/48.
                    match engine.engine_state() {
                        Some(AnimEngineState::Rendering { done, total }) => {
                            assert_eq!((done, total), (50, 100));
                        }
                        other => panic!("esperaba Rendering real, got {other:?}"),
                    }
                    saw = true;
                    break;
                }
                Some(JobEvent::Result(_)) => break,
                Some(JobEvent::Error { code, message }) => {
                    panic!("stub error inesperado {code}: {message}")
                }
                None => {}
            }
        }
        assert!(saw, "el stub debe emitir progress 50");
        let _ = engine.shutdown();
    }

    // ── T2 Cancelación cooperativa <200 ms ────────────────────────────────
    const IGNORING_STUB: &str = r#"
import json, sys, time
def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()
send({"type":"hello","protocol_version":1,"capabilities":[]})
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    try: msg=json.loads(line)
    except Exception: continue
    t=msg.get("type")
    if t=="ping":
        send({"type":"pong"})
    elif t=="shutdown":
        continue
    elif t=="render_request":
        time.sleep(30)
"#;

    fn stub_engine_with(source: &str) -> (TempDirGuard, EngineConfig) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER2: AtomicU64 = AtomicU64::new(10_000);
        let id = COUNTER2.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "grafito_anim_stub2_{}_{}_{:?}",
            std::process::id(),
            id,
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let stub_path = dir.join("stub_engine.py");
        fs::write(&stub_path, source).unwrap();
        let config = EngineConfig {
            command: vec![
                "python3".to_string(),
                "-u".to_string(),
                stub_path.to_string_lossy().to_string(),
            ],
            working_dir: Some(dir.clone()),
            ..Default::default()
        };
        (TempDirGuard(dir), config)
    }

    #[test]
    fn cancel_kills_ignoring_worker_within_200ms() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine_with(IGNORING_STUB);
        let mut engine = AnimEngine::spawn(config).unwrap();
        engine.wait_ready().unwrap();
        let _job = engine.submit(derivada_request("nunca termina")).unwrap();
        let pid = engine.child_pid().expect("hijo vivo antes de cancelar");
        let start = Instant::now();
        engine.cancel().unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed < CANCEL_DEADLINE,
            "cancel() debe retornar <200 ms, tardó {elapsed:?}"
        );
        assert!(
            matches!(engine.state(), AnimJobState::Cancelled),
            "estado {:?}",
            engine.state()
        );
        // El proceso debe estar muerto (kill <200 ms aunque ignore SHUTDOWN).
        let proc = PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_millis(500);
        while proc.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!proc.exists(), "el worker {pid} debió morir en <200 ms");
    }

    // ── T3 Timeouts configurables + line_cap ─────────────────────────────
    #[test]
    fn engine_config_validates_ranges() {
        let mut cfg = EngineConfig::default();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.job_timeout, Duration::from_secs(90));
        assert_eq!(cfg.idle_timeout, Duration::from_secs(8));
        assert_eq!(cfg.line_cap_bytes, 64 * 1024);
        cfg.job_timeout = Duration::from_secs(0);
        assert!(cfg.validate().is_err());
        cfg.job_timeout = Duration::from_secs(601);
        assert!(cfg.validate().is_err());
        cfg.job_timeout = Duration::from_secs(90);
        cfg.idle_timeout = Duration::from_secs(0);
        assert!(cfg.validate().is_err());
        cfg.idle_timeout = Duration::from_secs(61);
        assert!(cfg.validate().is_err());
        cfg.idle_timeout = Duration::from_secs(8);
        cfg.line_cap_bytes = 100;
        assert!(cfg.validate().is_err());
        cfg.line_cap_bytes = 2 * 1024 * 1024;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn engine_config_from_env_with_validation() {
        // Limpia por si otro test dejó vars (best-effort; este es el único
        // test que usa GRAFITO_ANIM_*).
        for k in [
            "GRAFITO_ANIM_JOB_TIMEOUT_SECS",
            "GRAFITO_ANIM_IDLE_TIMEOUT_SECS",
            "GRAFITO_ANIM_LINE_CAP_BYTES",
        ] {
            unsafe { std::env::remove_var(k) };
        }
        let def = EngineConfig::from_env().unwrap();
        assert_eq!(def.job_timeout, Duration::from_secs(90));
        unsafe { std::env::set_var("GRAFITO_ANIM_JOB_TIMEOUT_SECS", "30") };
        unsafe { std::env::set_var("GRAFITO_ANIM_IDLE_TIMEOUT_SECS", "5") };
        unsafe { std::env::set_var("GRAFITO_ANIM_LINE_CAP_BYTES", "32768") };
        let custom = EngineConfig::from_env().unwrap();
        assert_eq!(custom.job_timeout, Duration::from_secs(30));
        assert_eq!(custom.idle_timeout, Duration::from_secs(5));
        assert_eq!(custom.line_cap_bytes, 32768);
        // Fuera de rango → Err en español.
        unsafe { std::env::set_var("GRAFITO_ANIM_JOB_TIMEOUT_SECS", "0") };
        let err = EngineConfig::from_env().unwrap_err();
        assert!(err.contains("fuera de rango"), "err: {err}");
        unsafe { std::env::set_var("GRAFITO_ANIM_JOB_TIMEOUT_SECS", "no-num") };
        let err2 = EngineConfig::from_env().unwrap_err();
        assert!(err2.contains("inválido"), "err: {err2}");
        for k in [
            "GRAFITO_ANIM_JOB_TIMEOUT_SECS",
            "GRAFITO_ANIM_IDLE_TIMEOUT_SECS",
            "GRAFITO_ANIM_LINE_CAP_BYTES",
        ] {
            unsafe { std::env::remove_var(k) };
        }
    }

    const GIANT_STUB: &str = r#"
import json, sys, os
def send(o):
    sys.stdout.write(json.dumps(o) + "\n")
    sys.stdout.flush()
send({"type":"hello","protocol_version":1,"capabilities":[]})
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    try: msg=json.loads(line)
    except Exception: continue
    t=msg.get("type")
    if t=="ping":
        send({"type":"pong"})
    elif t=="shutdown":
        break
    elif t=="render_request":
        jid=msg["job_id"]
        sys.stdout.write("A"*(100*1024) + "\n")
        sys.stdout.flush()
        send({"type":"progress","job_id":jid,"step":"render","percent":10})
        out=os.path.join(os.getcwd(), jid+".png")
        open(out,"wb").write(b"\x89PNG\r\n\x1a\n")
        send({"type":"render_result","job_id":jid,"media_path":out,"frames":1,"duration_ms":10})
"#;

    #[test]
    fn line_cap_rejects_giant_line_as_protocol_error() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, mut config) = stub_engine_with(GIANT_STUB);
        config.line_cap_bytes = 64 * 1024;
        let mut engine = AnimEngine::spawn(config).unwrap();
        engine.wait_ready().unwrap();
        let _job = engine.submit(derivada_request("gigante")).unwrap();
        let mut saw_protocol_error = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match engine.recv_event(Some(Duration::from_millis(300))).unwrap() {
                Some(JobEvent::Error { code, message }) if code == "protocol" => {
                    assert!(
                        message.contains("64 KiB") || message.contains("límite"),
                        "msg: {message}"
                    );
                    saw_protocol_error = true;
                    break;
                }
                Some(JobEvent::Progress(_)) => {
                    // El progress puede llegar antes si el reader reordena;
                    // seguir esperando el error de la línea gigante.
                    continue;
                }
                Some(JobEvent::Result(_)) => break,
                Some(JobEvent::Error { .. }) => continue,
                None => {}
            }
        }
        assert!(
            saw_protocol_error,
            "línea de 100 KiB debe producir Error{{code: protocol}} con line_cap 64 KiB"
        );
        let _ = engine.shutdown();
    }

    // ── T4 Errores tipados acotados ──────────────────────────────────────
    #[test]
    fn worker_error_truncates_to_500_and_localizes() {
        use crate::protocol::{WorkerError, MAX_WORKER_MESSAGE_LEN};
        let long = "x".repeat(2000);
        let e = WorkerError::try_new("render_failed", long);
        assert!(e.message.chars().count() <= MAX_WORKER_MESSAGE_LEN);
        assert_eq!(e.message.chars().count(), 500);
        let loc = e.localized();
        assert!(loc.contains("falló el render"), "loc: {loc}");
        assert!(!loc.to_lowercase().contains("animation engine"));
        // Código inválido → "error" + español.
        let e2 = WorkerError::try_new("bad code!!", "boom");
        assert_eq!(e2.code, "error");
        assert!(e2.localized().contains("error del motor"));
        // Wire: try_downcast trunca mensaje gigante del worker.
        let big = "y".repeat(900);
        let v = serde_json::json!({"type":"error","code":"render_failed","message":big});
        match crate::protocol::try_downcast(&v).unwrap() {
            WireMessage::Error { message, .. } => {
                assert!(message.chars().count() <= 500);
            }
            other => panic!("esperaba Error, got {other:?}"),
        }
    }

    // ── T5 Sandbox Python: 2 tests de escape ─────────────────────────────
    fn python_engine_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("engines/python")
    }

    fn run_sandbox_check(script: &str) -> Result<String, String> {
        let out = Command::new("python3")
            .arg("-c")
            .arg(script)
            .env(
                "PYTHONPATH",
                python_engine_dir().to_string_lossy().to_string(),
            )
            .output()
            .map_err(|e| format!("no se pudo lanzar python3: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err(format!(
                "sandbox check falló: {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    }

    #[test]
    fn sandbox_rejects_code_injection_import_dunder() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        // Cubre: import, dunder (__import__/__class__), Attribute, Call no
        // permitida y Subscript. El sandbox debe rechazar TODO lo listado y
        // aceptar solo "x**2 + sin(x)".
        let script = r#"
import sys
from manim_engine.__main__ import validate_expr
bad = [
    "__import__('os').system('echo pwned')",
    "import os",
    "x.__class__.__bases__[0]",
    "open('/etc/passwd').read()",
    "eval('1+1')",
    "getattr(x, 'real')",
    "x[0]",
    "(lambda x: x)(1)",
    "sin(__import__('os').name)",
]
for expr in bad:
    try:
        validate_expr(expr)
    except ValueError:
        continue
    print(f"ESCAPE NO BLOQUEADO: {expr!r}")
    sys.exit(1)
# Expresión legítima debe pasar
assert validate_expr("x**2 + sin(x)") == "x**2 + sin(x)"
# MAX_EXPR_LEN=500: 501 chars debe fallar
try:
    validate_expr("x+" + "1"*600)
    print("ESCAPE LONGITUD NO BLOQUEADA")
    sys.exit(1)
except ValueError:
    pass
print("sandbox code-injection OK")
"#;
        run_sandbox_check(script).unwrap();
    }

    #[test]
    fn sandbox_rejects_path_traversal_and_symlink_escape() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        // Cubre: path traversal vía job_id ("../escape", "/abs", "a/b") y
        // symlink escape (job-1.png → /etc/passwd debe rechazarse por
        // resolve()+relative_to). También MAX_NODES y export inválido.
        let script = r#"
import pathlib, sys, tempfile, os
from manim_engine.__main__ import safe_path, validate_expr
with tempfile.TemporaryDirectory() as td:
    wd = pathlib.Path(td)
    # 1) job_id con traversal debe fallar (JOB_RE ^[A-Za-z0-9_-]{1,64}$)
    for evil in ["../escape", "/abs", "a/b", "", "x"*65, "a;b", "a b"]:
        try:
            safe_path(wd, evil, "png")
        except ValueError:
            continue
        print(f"TRAVERSAL NO BLOQUEADO: {evil!r}")
        sys.exit(1)
    # 2) export inválido debe fallar
    try:
        safe_path(wd, "job-1", "exe")
        print("EXPORT NO BLOQUEADO")
        sys.exit(1)
    except ValueError:
        pass
    # 3) caso legítimo pasa y queda dentro del workdir
    p = safe_path(wd, "job-1", "png")
    assert str(p).startswith(str(wd.resolve())), p
    # 4) symlink escape: pre-crear job-1.png -> /etc/passwd debe rechazarse
    link = wd / "job-1.png"
    try:
        link.symlink_to("/etc/passwd")
    except (OSError, NotImplementedError) as e:
        print(f"sin symlink en este FS, salto parcial OK ({e})")
    else:
        try:
            safe_path(wd, "job-1", "png")
            print("SYMLINK ESCAPE NO BLOQUEADO")
            sys.exit(1)
        except ValueError:
            pass
    # 5) MAX_NODES=200: expresión gigante debe fallar
    try:
        validate_expr("+".join(["x"]*500))
        print("MAX_NODES NO BLOQUEADO")
        sys.exit(1)
    except ValueError:
        pass
print("sandbox path-traversal OK")
"#;
        run_sandbox_check(script).unwrap();
    }

    // ── v3: sin cola FIFO (single-flight) ───────────────────────────────
    #[test]
    fn submit_while_running_is_rejected_no_fifo_queue() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, config) = stub_engine();
        let mut engine = AnimEngine::spawn(config).unwrap();
        engine.wait_ready().unwrap();
        let _first = engine.submit(derivada_request("derivada")).unwrap();
        // Segundo submit con el primero en curso: rechazado, no encolado.
        let err = engine.submit(derivada_request("otra")).unwrap_err();
        assert!(
            err.contains("wait_ready"),
            "sin cola FIFO: segundo submit rechazado, got: {err}"
        );
        let _ = engine.shutdown();
    }

    // ── v4: cancel cableado hasta run_job (ANIM-REVIVE) ───────────────────
    #[test]
    fn run_job_honors_cancel_closure_like_cancellation_token() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        // La firma YA soporta tokens: cualquier `&dyn Fn() -> bool` (p. ej.
        // `|| token.is_cancelled()` de `grafito-assistant::CancellationToken`)
        // cancela el loop con kill <200 ms y error en español.
        let (_guard, config) = stub_engine();
        let started = Instant::now();
        let error = run_job(
            &config,
            &derivada_request("derivada"),
            Some(&|| true),
            |_| {},
        )
        .unwrap_err();
        assert!(
            error.contains("cancelado"),
            "cancel localizado esperado, got: {error}"
        );
        assert!(
            started.elapsed() < config.job_timeout,
            "cancel debe atajar antes del timeout de 90 s"
        );
        // Sin señal (None) el stub completa: el hueco está en el caller que
        // pasa None, no en esta firma.
        let (_guard2, config2) = stub_engine();
        let mut saw_progress = false;
        let ok = run_job(&config2, &derivada_request("derivada"), None, |ev| {
            if matches!(ev, JobEvent::Progress(_)) {
                saw_progress = true;
            }
        });
        assert!(ok.is_ok(), "sin cancel el job completa");
        assert!(saw_progress, "el stub emite progress real");
    }
}
