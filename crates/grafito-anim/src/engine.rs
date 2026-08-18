//! Puente IPC al motor de animaciones externo (ciclo de vida, jobs y presupuestos).

use crate::protocol::{
    downcast, kinds, AnimJobId, AnimRequest, AnimResult, RenderProgress, WireMessage,
    ANIM_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// Configuración del proceso del motor de animaciones.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// argv del worker (programa + argumentos).
    pub command: Vec<String>,
    /// Carpeta de trabajo donde el motor debe escribir los artefactos.
    pub working_dir: Option<PathBuf>,
    /// Tiempo de espera para el handshake y el apagado cooperativo.
    pub idle_timeout: Duration,
    /// Tiempo máximo para completar un job.
    pub job_timeout: Duration,
    /// Tope de caracteres por línea de salida del motor.
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

/// Evento de un job de render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    /// Progreso parcial del motor.
    Progress(RenderProgress),
    /// El motor terminó el render.
    Result(AnimResult),
    /// El motor reportó un error.
    Error { code: String, message: String },
}

/// Puente hacia un proceso de motor de animaciones ya lanzado.
pub struct AnimEngine {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<WireMessage>,
    next_job: u64,
    config: EngineConfig,
    diagnostics: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl AnimEngine {
    /// Lanza el proceso del motor y empieza a leer sus mensajes.
    pub fn spawn(config: EngineConfig) -> Result<Self, String> {
        if config.command.is_empty() {
            return Err("animation engine command is empty".into());
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
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "animation engine stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "animation engine stdout is unavailable".to_string())?;
        let stderr = child.stderr.take();
        let (sender, receiver) = std::sync::mpsc::channel();
        let diagnostics = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        spawn_reader(stdout, sender, config.line_cap_bytes);
        if let Some(stderr) = stderr {
            spawn_stderr_drainer(stderr, std::sync::Arc::clone(&diagnostics));
        }
        Ok(Self {
            child,
            stdin,
            events: receiver,
            next_job: 0,
            config,
            diagnostics,
        })
    }

    /// Diagnósticos (stderr) recogidos del motor.
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics.lock().unwrap().clone()
    }

    /// Espera el handshake (hello + pong) antes de enviar jobs.
    pub fn wait_ready(&mut self) -> Result<(), String> {
        let hello_deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if Instant::now() >= hello_deadline {
                return Err("animation engine did not send a hello handshake".into());
            }
            match self.recv_raw(Some(Duration::from_secs(1))) {
                Ok(Some(WireMessage::Hello {
                    protocol_version, ..
                })) => {
                    if protocol_version != ANIM_PROTOCOL_VERSION {
                        return Err(format!(
                            "animation engine speaks protocol v{protocol_version}; this Grafito supports v{ANIM_PROTOCOL_VERSION}"
                        ));
                    }
                    break;
                }
                Ok(_) => {}
                Err(error) => return Err(error),
            }
        }
        self.send(&json!({ "type": kinds::PING }))?;
        let pong_deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if Instant::now() >= pong_deadline {
                return Err("animation engine did not answer the health ping".into());
            }
            if let Some(WireMessage::Pong) = self.recv_raw(Some(Duration::from_secs(1)))? {
                return Ok(());
            }
        }
    }

    /// Envía un nuevo pedido y devuelve su id.
    pub fn submit(&mut self, request: AnimRequest) -> Result<AnimJobId, String> {
        self.next_job += 1;
        let job_id = format!("job-{}", self.next_job);
        let message = json!({
            "type": kinds::RENDER_REQUEST,
            "job_id": job_id,
            "template": request.template,
            "concept": request.concept,
            "params": request.params,
            "spec": request.spec,
            "export": request.export.as_str(),
            "canvas": [request.canvas.0, request.canvas.1],
        });
        self.send(&message)?;
        Ok(job_id)
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

    /// Apaga el motor de forma cooperativa y garantiza su terminación.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let _ = self.send(&json!({ "type": kinds::SHUTDOWN }));
        let deadline = Instant::now()
            .checked_add(self.config.idle_timeout)
            .unwrap_or_else(Instant::now);
        loop {
            if Instant::now() >= deadline {
                break;
            }
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    let _ = self.events.recv_timeout(Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Ejecuta un job de punta a punta contra un motor efímero.
pub fn run_job(
    config: &EngineConfig,
    request: &AnimRequest,
    cancel: Option<&dyn Fn() -> bool>,
    mut on_event: impl FnMut(JobEvent),
) -> Result<AnimResult, String> {
    let mut engine = AnimEngine::spawn(config.clone())?;
    engine.wait_ready()?;
    let _job_id = engine.submit(request.clone())?;
    let started = Instant::now();
    loop {
        if cancel.is_some_and(|cancel| cancel()) {
            let _ = engine.shutdown();
            return Err("animation job was cancelled".into());
        }
        let remaining = config.job_timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            let _ = engine.shutdown();
            return Err("animation job timed out".into());
        }
        match engine.recv_event(Some(remaining)) {
            Ok(Some(JobEvent::Progress(progress))) => on_event(JobEvent::Progress(progress)),
            Ok(Some(JobEvent::Result(result))) => {
                if !validate_media_path(
                    config.working_dir.as_deref().unwrap_or(Path::new(".")),
                    &result.media_path,
                ) {
                    let _ = engine.shutdown();
                    return Err(
                        "animation engine returned a media path outside its working dir".into(),
                    );
                }
                let _ = engine.shutdown();
                return Ok(result);
            }
            Ok(Some(JobEvent::Error { code, message })) => {
                let _ = engine.shutdown();
                return Err(format!("animation engine {code}: {message}"));
            }
            Ok(None) => {}
            Err(error) => {
                let _ = engine.shutdown();
                return Err(error);
            }
        }
    }
}

/// Comprueba que el artefacto devuelto queda dentro del directorio de trabajo.
pub fn validate_media_path(working_dir: &Path, media_path: &str) -> bool {
    let path = Path::new(media_path);
    if path.is_absolute()
        && !path.components().next().is_some_and(|component| {
            matches!(
                component,
                std::path::Component::RootDir | std::path::Component::Prefix(_)
            )
        })
    {
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
    // El archivo puede no existir todavía: se canónica el directorio padre y
    // se comprueba que ese directorio quede dentro del directorio de trabajo.
    let Some(parent) = absolute.parent() else {
        return false;
    };
    let Ok(parent_canonical) = parent.canonicalize() else {
        return false;
    };
    parent_canonical.starts_with(&cwd)
}

fn spawn_reader(stdout: ChildStdout, sender: Sender<WireMessage>, line_cap: usize) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let mut oversized = false;
            let mut done = false;
            while !done {
                let mut chunk = [0_u8; 512];
                match reader.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(count) => {
                        if let Some(newline) = chunk[..count].iter().position(|byte| *byte == b'\n')
                        {
                            line.extend_from_slice(&chunk[..newline]);
                            done = true;
                        } else {
                            line.extend_from_slice(&chunk[..count]);
                            if line.len() > line_cap {
                                oversized = true;
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
            if oversized || line.len() > line_cap {
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
                    let mut guard = shared.lock().unwrap();
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
        let dir = std::env::temp_dir().join(format!("grafito_anim_stub_{}", std::process::id()));
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
        fs::remove_dir_all(&dir).unwrap();
    }
}
