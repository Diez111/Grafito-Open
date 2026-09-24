//! Enlace Colab Pro para la app: cliente MCP mínimo + pareo + jobs.
//!
//! La app es dueña ÚNICA del proxy `colab-mcp` (un solo pareo; el server
//! acepta un solo cliente). Este módulo NO usa egui: es testeable puro +
//! std. La UI vive en `colab_panel.rs`.
//!
//! Flujo: `request_connect` lanza un worker que hace
//! `initialize → tools/list → tools/call open_colab_browser_connection`
//! (el server abre Chrome con la URL + token; el usuario parea en ≤60 s)
//! y termina con `tools/list` para descubrir las tools del notebook.
//! Todo I/O con timeouts: un server colgado jamás cuelga la UI.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Tool inyectada por `colab-mcp` que abre el browser para parear.
pub const OPEN_TOOL: &str = "open_colab_browser_connection";
/// Versión del protocolo que hablamos.
const MCP_VERSION: &str = "2024-11-05";
/// Espera del pareo en browser (el server da 60 s; margen local).
const PAIR_TIMEOUT: Duration = Duration::from_secs(75);
/// Timeout de llamadas normales.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout de ejecución de jobs en Colab (pueden tardar minutos).
const RUN_TIMEOUT: Duration = Duration::from_secs(600);
/// Timeout del primer `initialize` (cubre descarga de `uvx`).
const INIT_TIMEOUT: Duration = Duration::from_secs(300);
/// Líneas de log retenidas.
const LOG_CAP: usize = 60;
/// Salida recortada (output de notebook puede ser enorme).
pub const OUTPUT_CAP: usize = 4000;

// ── Transporte inyectable (mock portable en tests) ───────────────────

/// Línea JSON-RPC por stdio. El mock de tests lo implementa con canales.
pub trait McpPipe: Send {
    fn send_line(&mut self, line: &str) -> Result<(), String>;
    fn recv_line(&mut self, timeout: Duration) -> Result<String, String>;
}

/// Tubo real sobre un hijo con lector en hilo (el `read` bloqueante jamás
/// frena al llamante más allá del timeout).
pub struct ChildPipe {
    stdin: ChildStdin,
    rx: mpsc::Receiver<Result<String, String>>,
    _child: Child,
}

impl ChildPipe {
    pub fn spawn(bin: &std::path::Path, args: &[&str]) -> Result<Self, String> {
        let mut child = Command::new(bin)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("no se pudo lanzar {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().ok_or("stdin del hijo no disponible")?;
        let stdout = child.stdout.take().ok_or("stdout del hijo no disponible")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let done = match line {
                    Ok(l) => tx.send(Ok(l)).is_err(),
                    Err(e) => tx.send(Err(format!("stdout roto: {e}"))).is_err(),
                };
                if done {
                    break;
                }
            }
            let _ = tx.send(Err("eof del servidor".to_string()));
        });
        Ok(Self {
            stdin,
            rx,
            _child: child,
        })
    }
}

impl McpPipe for ChildPipe {
    fn send_line(&mut self, line: &str) -> Result<(), String> {
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| format!("stdin roto: {e}"))
    }

    fn recv_line(&mut self, timeout: Duration) -> Result<String, String> {
        self.rx
            .recv_timeout(timeout)
            .map_err(|_| "timeout esperando al servidor".to_string())?
    }
}

impl Drop for ChildPipe {
    fn drop(&mut self) {
        let _ = self._child.kill();
        let _ = self._child.wait();
    }
}

// ── Primitivas JSON-RPC ──────────────────────────────────────────────

fn req(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// Llama un método y devuelve `result` (los errores de tool viajan como
/// `Err` con el texto; notificaciones intermedias se ignoran).
///
/// Tolerante al ruido: `uvx`/wheels imprimen progreso a stdout en la primera
/// corrida; las líneas que no son JSON se saltean hasta el deadline en vez
/// de abortar (ese era el "respuesta no-JSON" que rompía el pareo).
pub fn mcp_call(
    pipe: &mut dyn McpPipe,
    id: u64,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    pipe.send_line(&req(id, method, params))?;
    let deadline = Instant::now() + timeout;
    let mut noise = 0u32;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            if noise > 0 {
                return Err(format!(
                    "timeout en '{method}' tras saltear {noise} líneas de ruido (¿uvx descargando?)"
                ));
            }
            return Err(format!("timeout en '{method}'"));
        }
        let line = pipe.recv_line(left)?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                noise += 1;
                continue; // ruido de arranque (uvx/pip), no es la respuesta
            }
        };
        if msg.get("id") != Some(&json!(id)) {
            continue; // notificación u otro turno
        }
        if let Some(err) = msg.get("error") {
            return Err(format!("error de protocolo: {err}"));
        }
        return msg
            .get("result")
            .cloned()
            .ok_or("respuesta sin result ni error".to_string());
    }
}

/// Inicializa el server (`initialize` + `notifications/initialized`).
pub fn mcp_init(pipe: &mut dyn McpPipe, id: &mut u64, timeout: Duration) -> Result<(), String> {
    *id += 1;
    mcp_call(
        pipe,
        *id,
        "initialize",
        json!({"protocolVersion": MCP_VERSION, "capabilities": {}, "clientInfo": {"name": "grafito", "version": env!("CARGO_PKG_VERSION")}}),
        timeout,
    )?;
    pipe.send_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)?;
    Ok(())
}

/// Lista tools (`tools/list` → Vec<(nombre, descripción, schema)>).
pub fn mcp_list_tools(
    pipe: &mut dyn McpPipe,
    id: &mut u64,
    timeout: Duration,
) -> Result<Vec<ToolInfo>, String> {
    *id += 1;
    let result = mcp_call(pipe, *id, "tools/list", json!({}), timeout)?;
    let arr = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or("tools/list sin tools")?;
    let mut out = Vec::with_capacity(arr.len());
    for t in arr {
        out.push(ToolInfo {
            name: t
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string(),
            description: t
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            input_schema: t.get("inputSchema").cloned().unwrap_or(Value::Null),
        });
    }
    Ok(out)
}

/// Llama una tool (`tools/call` → contenido; `isError` = `Err`).
pub fn mcp_call_tool(
    pipe: &mut dyn McpPipe,
    id: &mut u64,
    name: &str,
    args: Value,
    timeout: Duration,
) -> Result<Value, String> {
    *id += 1;
    let result = mcp_call(
        pipe,
        *id,
        "tools/call",
        json!({"name": name, "arguments": args}),
        timeout,
    )?;
    if result.get("isError") == Some(&json!(true)) {
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("error sin texto");
        return Err(text.to_string());
    }
    Ok(result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| result.get("content").cloned().unwrap_or(Value::Null)))
}

// ── Algoritmo de pareo (testeable con mock) ──────────────────────────

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Intentos de pareo (cada uno espera 60 s en Chrome; la pestaña sigue
/// abierta entre intentos porque el token es del proceso).
pub const PAIR_ATTEMPTS: u32 = 3;

/// Secuencia completa de pareo sobre un tubo ya abierto. Devuelve las tools
/// descubiertas tras parear (o el error honesto para mostrar).
///
/// `on_attempt` se llama con (intento, total) para pintar progreso en la UI.
pub fn pair_flow(
    pipe: &mut dyn McpPipe,
    on_attempt: &dyn Fn(u32),
) -> Result<Vec<ToolInfo>, String> {
    let mut id = 0u64;
    let before = mcp_list_tools(pipe, &mut id, CALL_TIMEOUT)?;
    if !before.iter().any(|t| t.name == OPEN_TOOL) {
        return Err("el server no expone open_colab_browser_connection".into());
    }
    for attempt in 1..=PAIR_ATTEMPTS {
        on_attempt(attempt);
        let opened = mcp_call_tool(pipe, &mut id, OPEN_TOOL, json!({}), PAIR_TIMEOUT)?;
        let ok = opened
            .get("result")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if ok {
            return mcp_list_tools(pipe, &mut id, CALL_TIMEOUT);
        }
        // `false` = no llegaste a parear en 60 s; la pestaña sigue abierta
        // con el mismo token y el siguiente intento la reutiliza.
    }
    Err(format!(
        "no se completó el pareo en {PAIR_ATTEMPTS} intentos (60 s c/u): revisá que Chrome abra la pestaña de Colab con tu cuenta Pro y elegí entorno con GPU antes de aceptar"
    ))
}

/// Heurística documentada: primer parámetro string cuyo nombre sugiera
/// código (code/script/text/source/input), si no el primer string.
pub fn pick_string_arg(schema: &Value) -> Option<String> {
    let props = schema.get("properties")?.as_object()?;
    let mut first_string: Option<String> = None;
    for (name, def) in props {
        if def.get("type")?.as_str()? != "string" {
            continue;
        }
        if first_string.is_none() {
            first_string = Some(name.clone());
        }
        let low = name.to_lowercase();
        if ["code", "script", "text", "source", "input", "cell"]
            .iter()
            .any(|k| low.contains(k))
        {
            return Some(name.clone());
        }
    }
    first_string
}

/// Binario en PATH (rechaza `/`, `\` y NUL; ejecutable en Unix).
pub fn find_in_path(binary: &str) -> Option<PathBuf> {
    if binary.is_empty() || binary.contains('/') || binary.contains('\\') || binary.contains('\0') {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(binary);
        if let Ok(meta) = std::fs::metadata(&cand) {
            if meta.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(cand);
                    }
                }
                #[cfg(not(unix))]
                {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// `uvx` en PATH.
pub fn find_uvx() -> Option<PathBuf> {
    find_in_path("uvx")
}

// ── Estado para la UI ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColabPhase {
    Idle,
    Busy,
    Ready,
    Failed,
}

pub enum ColabCmd {
    Run { tool: String, args: Value },
    Disconnect,
}

pub enum ColabEvt {
    Log(String),
    Phase(ColabPhase, String),
    Tools(Vec<ToolInfo>),
    Output(String),
}

/// Estado del enlace (vive en `GrafitoApp`; el worker habla por canales).
pub struct ColabLink {
    pub phase: ColabPhase,
    pub status: String,
    pub log: VecDeque<String>,
    pub tools: Vec<ToolInfo>,
    pub executor: Option<String>,
    pub output: String,
    cmd_tx: Option<mpsc::Sender<ColabCmd>>,
    evt_rx: Option<mpsc::Receiver<ColabEvt>>,
}

impl Default for ColabLink {
    fn default() -> Self {
        Self {
            phase: ColabPhase::Idle,
            status: "Desconectado".into(),
            log: VecDeque::new(),
            tools: Vec::new(),
            executor: None,
            output: String::new(),
            cmd_tx: None,
            evt_rx: None,
        }
    }
}

impl ColabLink {
    pub fn is_busy(&self) -> bool {
        self.phase == ColabPhase::Busy
    }

    pub fn push_log(&mut self, line: String) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Pide conexión (no-op si hay trabajo en curso; mata worker previo).
    pub fn request_connect(&mut self) {
        if self.is_busy() {
            return;
        }
        self.shutdown_worker();
        self.phase = ColabPhase::Busy;
        self.status = "Lanzando colab-mcp…".into();
        self.push_log("lanzando colab-mcp (uvx; la primera vez descarga)".into());
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        self.cmd_tx = Some(cmd_tx);
        self.evt_rx = Some(evt_rx);
        std::thread::spawn(move || connect_worker(cmd_rx, evt_tx));
    }

    pub fn request_run(&mut self, tool: String, args: Value) {
        if self.is_busy() {
            return;
        }
        if let Some(tx) = &self.cmd_tx {
            self.phase = ColabPhase::Busy;
            self.status = "Ejecutando en Colab…".into();
            let _ = tx.send(ColabCmd::Run { tool, args });
        }
    }

    pub fn request_disconnect(&mut self) {
        self.shutdown_worker();
        self.phase = ColabPhase::Idle;
        self.status = "Desconectado".into();
        self.tools.clear();
        self.push_log("desconectado (proxy apagado)".into());
    }

    fn shutdown_worker(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(ColabCmd::Disconnect);
        }
        self.evt_rx = None;
    }

    /// Drena eventos del worker. Devuelve `true` si cambió algo visible.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let evts: Vec<ColabEvt> = match &self.evt_rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for evt in evts {
            changed = true;
            match evt {
                ColabEvt::Log(line) => self.push_log(line),
                ColabEvt::Phase(phase, status) => {
                    self.phase = phase;
                    self.status = status;
                    if phase == ColabPhase::Failed {
                        // El worker murió: el tubo murió con él.
                        self.cmd_tx = None;
                        self.evt_rx = None;
                    }
                }
                ColabEvt::Tools(tools) => {
                    // Auto-elegir ejecutor si hay uno obvio y no hay elección.
                    if self.executor.is_none() {
                        self.executor = tools
                            .iter()
                            .find(|t| {
                                let n = t.name.to_lowercase();
                                n.contains("execut") || n.contains("run") || n.contains("code")
                            })
                            .map(|t| t.name.clone());
                    }
                    self.tools = tools;
                }
                ColabEvt::Output(text) => {
                    self.output = text;
                    self.phase = ColabPhase::Ready;
                    self.status = "Listo".into();
                }
            }
        }
        changed
    }
}

/// Worker persistente: parea y luego atiende `Run` hasta `Disconnect`.
/// El tubo vive con el worker; si el server muere, el próximo `Run` falla
/// honesto y el panel pide re-parear.
fn connect_worker(cmd_rx: mpsc::Receiver<ColabCmd>, evt_tx: mpsc::Sender<ColabEvt>) {
    let log = |s: &str| {
        let _ = evt_tx.send(ColabEvt::Log(s.to_string()));
    };
    // Drena un Disconnect temprano sin bloquear.
    if matches!(cmd_rx.try_recv(), Ok(ColabCmd::Disconnect)) {
        return;
    }
    let uvx = match find_uvx() {
        Some(p) => p,
        None => {
            let _ = evt_tx.send(ColabEvt::Phase(
                ColabPhase::Failed,
                "uvx no está en el PATH".into(),
            ));
            log("instalá uv (https://docs.astral.sh/uv/) y reintentá");
            return;
        }
    };
    let mut pipe = match ChildPipe::spawn(
        &uvx,
        &[
            "--index",
            "https://pypi.org/simple",
            "git+https://github.com/googlecolab/colab-mcp",
        ],
    ) {
        Ok(p) => p,
        Err(e) => {
            let _ = evt_tx.send(ColabEvt::Phase(ColabPhase::Failed, e.clone()));
            log(&e);
            return;
        }
    };
    let mut id = 0u64;
    if let Err(e) = mcp_init(&mut pipe, &mut id, INIT_TIMEOUT) {
        let _ = evt_tx.send(ColabEvt::Phase(ColabPhase::Failed, e.clone()));
        log(&e);
        return;
    }
    log("servidor ColabMCP listo; abriendo Chrome para parear…");
    let evt_tx2 = evt_tx.clone();
    let on_attempt = move |attempt: u32| {
        let _ = evt_tx2.send(ColabEvt::Phase(
            ColabPhase::Busy,
            format!("Esperando pareo en Chrome (intento {attempt}/{PAIR_ATTEMPTS})…"),
        ));
        let _ = evt_tx2.send(ColabEvt::Log(format!(
            "intento {attempt}/{PAIR_ATTEMPTS}: Chrome abierto, pareá con tu Pro (60 s)"
        )));
    };
    match pair_flow(&mut pipe, &on_attempt) {
        Ok(tools) => {
            let n = tools.len();
            let _ = evt_tx.send(ColabEvt::Tools(tools));
            let _ = evt_tx.send(ColabEvt::Phase(
                ColabPhase::Ready,
                format!("Pareado ({n} tools del notebook)"),
            ));
            log("pareo OK: elegí ejecutor y job para correr");
        }
        Err(e) => {
            let _ = evt_tx.send(ColabEvt::Phase(ColabPhase::Failed, e.clone()));
            log(&e);
            return;
        }
    }
    // Bucle de ejecución hasta Disconnect o muerte del server.
    for cmd in cmd_rx {
        match cmd {
            ColabCmd::Disconnect => break,
            ColabCmd::Run { tool, args } => {
                log(&format!("ejecutando {tool} en Colab…"));
                match mcp_call_tool(&mut pipe, &mut id, &tool, args, RUN_TIMEOUT) {
                    Ok(value) => {
                        let mut text = value.to_string();
                        if text.len() > OUTPUT_CAP {
                            text.truncate(OUTPUT_CAP);
                            text.push('…');
                        }
                        let _ = evt_tx.send(ColabEvt::Output(text));
                        log("ejecución terminada");
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ColabEvt::Output(format!("error: {e}")));
                        log(&e);
                    }
                }
            }
        }
    }
}

// ── Jobs del lab (lee `lab_jobs/` sin depender de grafito-mcp) ───────

/// Directorio de manifiestos: hermano del ledger o XDG (misma regla).
pub fn lab_jobs_dir() -> PathBuf {
    if let Ok(raw) = std::env::var("GRAFITO_LAB_LEDGER") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if let Some(parent) = p.parent() {
                return parent.join("lab_jobs");
            }
        }
    }
    let mut base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let mut h = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            if h.as_os_str().is_empty() {
                h = PathBuf::from(".");
            }
            h.push(".local/share");
            h
        });
    base.push("grafito");
    base.push("lab_jobs");
    base
}

#[derive(Debug, Clone)]
pub struct JobMeta {
    pub job_id: String,
    pub kind: String,
}

/// Manifiestos `*.json` ordenados (solo ids hex de 64).
pub fn list_lab_jobs() -> Vec<JobMeta> {
    let dir = lab_jobs_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let kind = match std::fs::read_to_string(entry.path()) {
            Err(_) => "? (manifiesto ilegible)".to_string(),
            Ok(texto) => match serde_json::from_str::<Value>(&texto) {
                Err(_) => "? (json roto)".to_string(),
                Ok(valor) => valor
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| "? (sin kind)".to_string()),
            },
        };
        out.push(JobMeta {
            job_id: id.to_string(),
            kind,
        });
    }
    out.sort_by(|a, b| a.job_id.cmp(&b.job_id));
    out
}

/// Script `.py` del job (capado para preview).
pub fn read_job_script(job_id: &str, cap: usize) -> Result<String, String> {
    if job_id.len() != 64 || !job_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("job_id inválido".into());
    }
    let text = std::fs::read_to_string(lab_jobs_dir().join(format!("{job_id}.py")))
        .map_err(|_| "script no encontrado (¿job de otra máquina?)".to_string())?;
    Ok(if text.len() > cap {
        format!("{}…", &text[..cap.min(text.len())])
    } else {
        text
    })
}

/// Llamada única a una tool de un binario MCP por stdio (para
/// `import_colab_result` contra el `grafito-mcp` instalado, sin depender
/// del crate). Hace `initialize → tools/call → drop` (el hijo muere ahí).
pub fn mcp_call_once(
    bin: &std::path::Path,
    extra_args: &[&str],
    tool: &str,
    tool_args: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let mut pipe = ChildPipe::spawn(bin, extra_args)?;
    let mut id = 0u64;
    mcp_init(&mut pipe, &mut id, timeout)?;
    mcp_call_tool(&mut pipe, &mut id, tool, tool_args, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock portable: responde guiones sin I/O real.
    struct ScriptPipe {
        tx: mpsc::Sender<String>,
        rx: mpsc::Receiver<String>,
    }

    struct ScriptPeer {
        tx: mpsc::Sender<String>,
        rx: mpsc::Receiver<String>,
        paired: Arc<Mutex<bool>>,
        /// Cuántas aperturas fallan antes de parear (reintentos del cliente).
        fail_open_times: Arc<Mutex<u32>>,
        /// Líneas de ruido (uvx) antes de la primera respuesta.
        noise: usize,
    }

    fn script_pair(fail_open_times: u32, noise: usize) -> (ScriptPipe, ScriptPeer) {
        let (a_tx, a_rx) = mpsc::channel();
        let (b_tx, b_rx) = mpsc::channel();
        (
            ScriptPipe { tx: a_tx, rx: b_rx },
            ScriptPeer {
                tx: b_tx,
                rx: a_rx,
                paired: Arc::new(Mutex::new(false)),
                fail_open_times: Arc::new(Mutex::new(fail_open_times)),
                noise,
            },
        )
    }

    impl McpPipe for ScriptPipe {
        fn send_line(&mut self, line: &str) -> Result<(), String> {
            self.tx.send(line.to_string()).map_err(|e| e.to_string())
        }
        fn recv_line(&mut self, timeout: Duration) -> Result<String, String> {
            self.rx
                .recv_timeout(timeout)
                .map_err(|_| "timeout".to_string())
        }
    }

    /// El peer corre en el hilo del test: responde como colab-mcp real.
    fn drive_peer(peer: ScriptPeer) {
        let mut first = true;
        while let Ok(line) = peer.rx.recv() {
            let msg: Value = serde_json::from_str(&line).unwrap();
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
            if method.starts_with("notifications/") {
                continue;
            }
            if first {
                first = false;
                for _ in 0..peer.noise {
                    // Ruido tipo uvx (progreso a stdout): el cliente lo saltea.
                    if peer.tx.send("  Downloading pair… 42%".to_string()).is_err() {
                        return;
                    }
                }
            }
            let resp = match method {
                "initialize" => {
                    json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {"listChanged": true}}, "serverInfo": {"name": "ColabMCP", "version": "0"}}})
                }
                "tools/list" => {
                    let paired = *peer.paired.lock().unwrap();
                    let tools = if paired {
                        vec![
                            json!({"name": "nb_execute", "description": "run cell", "inputSchema": {"type": "object", "properties": {"code": {"type": "string"}}}}),
                        ]
                    } else {
                        vec![
                            json!({"name": OPEN_TOOL, "description": "pair", "inputSchema": {"type": "object", "properties": {}}}),
                        ]
                    };
                    json!({"jsonrpc": "2.0", "id": id, "result": {"tools": tools}})
                }
                "tools/call" => {
                    let name = msg
                        .get("params")
                        .and_then(|p| p.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if name == OPEN_TOOL {
                        let mut fails = peer.fail_open_times.lock().unwrap();
                        if *fails > 0 {
                            *fails -= 1;
                            json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "false"}], "structuredContent": {"result": false}}})
                        } else {
                            *peer.paired.lock().unwrap() = true;
                            json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "true"}], "structuredContent": {"result": true}}})
                        }
                    } else {
                        json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "salida"}], "structuredContent": {"out": 1}}})
                    }
                }
                _ => {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "nope"}})
                }
            };
            if peer.tx.send(resp.to_string()).is_err() {
                break;
            }
        }
    }

    #[test]
    fn pareo_ok_descubre_tools() {
        let (mut pipe, peer) = script_pair(0, 0);
        let handle = std::thread::spawn(move || drive_peer(peer));
        let mut id = 0u64;
        mcp_init(&mut pipe, &mut id, Duration::from_secs(5)).unwrap();
        let attempts = Arc::new(Mutex::new(0u32));
        let attempts2 = attempts.clone();
        let tools = pair_flow(&mut pipe, &|_| {
            *attempts2.lock().unwrap() += 1;
        })
        .unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "nb_execute");
        assert_eq!(
            pick_string_arg(&tools[0].input_schema).as_deref(),
            Some("code")
        );
        assert_eq!(*attempts.lock().unwrap(), 1);
        drop(pipe);
        handle.join().unwrap();
    }

    #[test]
    fn pareo_reintenta_y_parea_al_tercero() {
        let (mut pipe, peer) = script_pair(2, 0);
        let handle = std::thread::spawn(move || drive_peer(peer));
        let attempts = Arc::new(Mutex::new(0u32));
        let attempts2 = attempts.clone();
        let tools = pair_flow(&mut pipe, &|_| {
            *attempts2.lock().unwrap() += 1;
        })
        .unwrap();
        assert_eq!(tools[0].name, "nb_execute");
        assert_eq!(*attempts.lock().unwrap(), 3);
        drop(pipe);
        handle.join().unwrap();
    }

    #[test]
    fn pareo_agotado_es_honesto() {
        let (mut pipe, peer) = script_pair(99, 0);
        let handle = std::thread::spawn(move || drive_peer(peer));
        let err = pair_flow(&mut pipe, &|_| {}).unwrap_err();
        assert!(err.contains("3 intentos"), "{err}");
        drop(pipe);
        handle.join().unwrap();
    }

    #[test]
    fn ruido_uvx_no_rompe_init() {
        let (mut pipe, peer) = script_pair(0, 3);
        let handle = std::thread::spawn(move || drive_peer(peer));
        let mut id = 0u64;
        // 3 líneas de ruido antes del initialize: se saltean, no es error.
        mcp_init(&mut pipe, &mut id, Duration::from_secs(5)).unwrap();
        drop(pipe);
        handle.join().unwrap();
    }

    #[test]
    fn timeout_sin_respuesta() {
        struct Mute;
        impl McpPipe for Mute {
            fn send_line(&mut self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn recv_line(&mut self, _: Duration) -> Result<String, String> {
                Err("timeout".to_string())
            }
        }
        let mut mute = Mute;
        let err = mcp_call(
            &mut mute,
            1,
            "tools/list",
            json!({}),
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert!(err.contains("timeout"), "{err}");
    }

    #[test]
    fn pick_arg_prefiere_code_y_find_rechaza_slash() {
        let schema = json!({"type": "object", "properties": {"zzz": {"type": "string"}, "code": {"type": "string"}, "n": {"type": "number"}}});
        assert_eq!(pick_string_arg(&schema).as_deref(), Some("code"));
        assert!(pick_string_arg(&json!({})).is_none());
        assert!(find_in_path("a/b").is_none());
        assert!(find_in_path("").is_none());
    }

    #[test]
    fn jobs_dirs_y_scripts_con_cap() {
        let dir = lab_jobs_dir();
        assert!(dir.to_string_lossy().contains("lab_jobs"));
        assert!(read_job_script("xyz", 100).is_err());
        assert!(read_job_script(&"a".repeat(64), 100).is_err());
    }

    #[test]
    fn jobs_kind_distinguido_no_traga_contexto() {
        // FIX 4: el kind distingue ilegible/roto/sin-kind en vez de "?" mudo.
        let base = std::env::temp_dir().join(format!(
            "grafito-labkind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let jobs = base.join("lab_jobs");
        std::fs::create_dir_all(&jobs).unwrap();
        let bueno = "a".repeat(64);
        let roto = "b".repeat(64);
        let sinkind = "c".repeat(64);
        std::fs::write(
            jobs.join(format!("{bueno}.json")),
            r#"{"kind": "integral"}"#,
        )
        .unwrap();
        std::fs::write(jobs.join(format!("{roto}.json")), "{no json").unwrap();
        std::fs::write(jobs.join(format!("{sinkind}.json")), r#"{"otro": 1}"#).unwrap();
        let ledger = base.join("lab_colab.jsonl");
        std::fs::write(&ledger, "").unwrap();
        let previo = std::env::var_os("GRAFITO_LAB_LEDGER");
        std::env::set_var("GRAFITO_LAB_LEDGER", &ledger);
        let lista = list_lab_jobs();
        if let Some(v) = previo {
            std::env::set_var("GRAFITO_LAB_LEDGER", v);
        } else {
            std::env::remove_var("GRAFITO_LAB_LEDGER");
        }
        let kind_de = |id: &str| {
            lista
                .iter()
                .find(|j| j.job_id == id)
                .map(|j| j.kind.clone())
                .unwrap_or_default()
        };
        assert_eq!(kind_de(&bueno), "integral");
        assert!(
            kind_de(&roto).contains("roto"),
            "json roto debe decirlo: {}",
            kind_de(&roto)
        );
        assert!(
            kind_de(&sinkind).contains("sin kind"),
            "sin kind debe decirlo: {}",
            kind_de(&sinkind)
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
