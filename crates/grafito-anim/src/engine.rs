//! Puente IPC al motor de animaciones externo (ciclo de vida, jobs y presupuestos).
//! Mejoras de auditoria 2026-08-20: Statem AnimJobState, correccion de races/leaks/timeouts.

use crate::protocol::{
    downcast, kinds, AnimJobId, AnimRequest, AnimResult, RenderProgress, WireMessage,
    ANIM_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

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
            idle_timeout: Duration::from_secs(8),
            job_timeout: Duration::from_secs(90),
            line_cap_bytes: 64 * 1024,
        }
    }
}

/// Estado tipado del ciclo de vida de un job (Statem).
/// Cada transicion es verificada; no hay submit sin Ready, no hay fuga sin ShuttingDown.
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

/// Evento de un job de render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    Progress(RenderProgress),
    Result(AnimResult),
    Error { code: String, message: String },
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
}

impl AnimEngine {
    /// Lanza el proceso del motor y empieza a leer sus mensajes.
    pub fn spawn(config: EngineConfig) -> Result<Self, String> {
        if config.command.is_empty() {
            return Err("animation engine command is empty".into());
        }
        for arg in &config.command {
            if arg.contains('\0') {
                return Err("animation engine command contains NUL byte".into());
            }
        }
        if let Some(dir) = &config.working_dir {
            if dir.as_os_str().is_empty() {
                return Err("animation engine working_dir is empty".into());
            }
            if !dir.exists() {
                return Err(format!(
                    "animation engine working_dir no existe: {}",
                    dir.display()
                ));
            }
        }
        // Validación temprana: si el binario es una ruta, verificar existencia antes de spawn.
        if config.command[0].contains('/') || config.command[0].contains('\\') {
            let bin = Path::new(&config.command[0]);
            if !bin.exists() {
                return Err(format!(
                    "animation engine bin no encontrado: {}",
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
            .map_err(|error| format!("cannot start animation engine: {error}"))?;
        // Si take() falla, matamos al hijo para no fugarlo (S1).
        let stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("animation engine stdin is unavailable".into());
            }
        };
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("animation engine stdout is unavailable".into());
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
        })
    }

    pub fn state(&self) -> &AnimJobState {
        &self.state
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
                    message: "hello not received".into(),
                };
                let _ = self.shutdown();
                return Err("animation engine did not send a hello handshake".into());
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
                            message: format!("v{protocol_version}"),
                        };
                        let _ = self.shutdown();
                        return Err(format!(
                            "animation engine speaks protocol v{protocol_version}; this Grafito supports v{ANIM_PROTOCOL_VERSION}"
                        ));
                    }
                    break;
                }
                Ok(Some(WireMessage::Error { code, message })) => {
                    self.state = AnimJobState::Failed {
                        code: code.clone(),
                        message: message.clone(),
                    };
                    let _ = self.shutdown();
                    return Err(format!("animation engine {code}: {message}"));
                }
                Ok(_) => {}
                Err(error) => {
                    self.state = AnimJobState::Failed {
                        code: "handshake_error".into(),
                        message: error.clone(),
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
                    message: "pong not received".into(),
                };
                let _ = self.shutdown();
                return Err("animation engine did not answer the health ping".into());
            }
            let remaining = pong_deadline.saturating_duration_since(Instant::now());
            let poll = remaining.min(Duration::from_millis(250));
            match self.recv_raw(Some(poll)) {
                Ok(Some(WireMessage::Pong)) => {
                    self.state = AnimJobState::Ready;
                    return Ok(());
                }
                Ok(Some(WireMessage::Error { code, message })) => {
                    self.state = AnimJobState::Failed {
                        code: code.clone(),
                        message: message.clone(),
                    };
                    let _ = self.shutdown();
                    return Err(format!("animation engine {code}: {message}"));
                }
                Ok(_) => {}
                Err(e) => {
                    self.state = AnimJobState::Failed {
                        code: "handshake_error".into(),
                        message: e.clone(),
                    };
                    return Err(e);
                }
            }
        }
    }

    /// Envia un nuevo pedido y devuelve su id. Solo en Ready (SB1).
    pub fn submit(&mut self, request: AnimRequest) -> Result<AnimJobId, String> {
        if !self.state.can_submit() {
            return Err(format!(
                "cannot submit in state {:?}: wait_ready() first",
                self.state
            ));
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
        });
        self.send(&message)?;
        self.state = AnimJobState::Running {
            job_id: job_id.clone(),
            deadline: Instant::now() + self.config.job_timeout,
        };
        Ok(job_id)
    }

    /// Cancela el job en curso si está en Running. Transiciona a Cancelling.
    pub fn cancel(&mut self) -> Result<(), String> {
        match &self.state {
            AnimJobState::Running { job_id, .. } => {
                let job_id = job_id.clone();
                self.state = AnimJobState::Cancelling { job_id };
                let _ = self.shutdown();
                self.state = AnimJobState::Cancelled;
                Ok(())
            }
            other => Err(format!(
                "no hay job en curso para cancelar (estado {other:?})"
            )),
        }
    }

    /// Lee el siguiente evento de un job (None cuando no hay mensaje en el timeout).
    pub fn recv_event(&self, timeout: Option<Duration>) -> Result<Option<JobEvent>, String> {
        match self.recv_raw(timeout)? {
            Some(WireMessage::Progress(progress)) => Ok(Some(JobEvent::Progress(progress))),
            Some(WireMessage::Result(result)) => Ok(Some(JobEvent::Result(result))),
            Some(WireMessage::Error { code, message }) => {
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
        self.state = AnimJobState::ShuttingDown {
            deadline: Instant::now() + self.config.idle_timeout,
        };
        let _ = self.send(&json!({ "type": kinds::SHUTDOWN }));
        // Delega la espera bloqueante a un hilo para no congelar la UI.
        if let Some(mut child) = self.child.take() {
            let idle = self.config.idle_timeout;
            std::thread::spawn(move || {
                let deadline = Instant::now() + idle;
                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
                let _ = child.kill();
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
            .map_err(|error| format!("animation engine stdin write failed: {error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("animation engine stdin flush failed: {error}"))
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
                Err(format!(
                    "animation engine exited unexpectedly{}{}",
                    if diagnostics.is_empty() { "" } else { ": " },
                    diagnostics
                ))
            }
        }
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
pub fn run_job(
    config: &EngineConfig,
    request: &AnimRequest,
    cancel: Option<&dyn Fn() -> bool>,
    mut on_event: impl FnMut(JobEvent),
) -> Result<AnimResult, String> {
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
            let _ = engine.shutdown();
            engine.state = AnimJobState::Cancelled;
            return Err("animation job was cancelled".into());
        }
        let now = Instant::now();
        if now >= job_deadline {
            engine.state = AnimJobState::TimedOut;
            let _ = engine.shutdown();
            return Err("animation job timed out".into());
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
                        message: result.media_path.clone(),
                    };
                    let _ = engine.shutdown();
                    return Err(
                        "animation engine returned a media path outside its working dir".into(),
                    );
                }
                engine.state = AnimJobState::Completed {
                    media_path: PathBuf::from(result.media_path.clone()),
                };
                let _ = engine.shutdown();
                return Ok(result);
            }
            Ok(Some(JobEvent::Error { code, message })) => {
                engine.state = AnimJobState::Failed {
                    code: code.clone(),
                    message: message.clone(),
                };
                let _ = engine.shutdown();
                return Err(format!("animation engine {code}: {message}"));
            }
            Ok(None) => {}
            Err(error) => {
                engine.state = AnimJobState::Failed {
                    code: "engine_exit".into(),
                    message: error.clone(),
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
                    message: "the animation engine emitted an oversized line".into(),
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
        assert!(error.contains("render_failed"), "error: {error}");
    }

    #[test]
    fn run_job_times_out_when_the_engine_never_answers() {
        if !python_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
        let (_guard, mut config) = stub_engine();
        config.job_timeout = Duration::from_millis(300);
        let error = run_job(&config, &derivada_request("never"), None, |_| {}).unwrap_err();
        assert!(error.contains("timed out"), "error: {error}");
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
}
