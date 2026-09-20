//! Ground truth formal vía kernel Lean externo (sidecar honesto).
//!
//! - `lean_check` verifica SIN guardar; `lean_submit` verifica y guarda la
//!   prueba aceptada (o fallida) en `lab_proofs/` junto al ledger.
//! - Anti-specification-gaming estilo Erdős: denylist de `sorry` / `admit` /
//!   `axiom` como palabra completa fuera de comentarios `--`; los bloques
//!   `/- ... -/` se rechazan de plano (pueden ocultar trampas).
//! - Sin kernel no hay veredicto: error honesto con guía de instalación
//!   (como `FfmpegMissing` / `SolverMissing`). Jamás se inventa PROVED.
//! - `PROVED` = el kernel aceptó el archivo, no verdad matemática absoluta
//!   (se advierte en cada respuesta).
//!
//! Este archivo es solo el núcleo; otro agente hace el wire en
//! `protocol.rs` / `lib.rs`.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// ── Constantes ─────────────────────────────────────────────────────────

/// Tope por campo (`statement`, `proof`): 64 KiB cada uno.
pub const MAX_LEAN_FIELD_BYTES: usize = 64 * 1024;
/// Timeout por defecto del kernel: 120 s.
pub const DEFAULT_LEAN_TIMEOUT_MS: u64 = 120_000;
/// Timeout máximo configurable: 600 s.
pub const MAX_LEAN_TIMEOUT_MS: u64 = 600_000;
/// Salida del kernel capada en la respuesta.
const OUTPUT_CAP_CHARS: usize = 4000;
/// Tokens tramposos (palabra completa, fuera de comentarios).
const DENY_TOKENS: [&str; 3] = ["sorry", "admit", "axiom"];

// ── Defs MCP (las registra otro agente en protocol.rs) ─────────────────

/// `lean_check` (solo lee) + `lean_submit` (persiste lo verificado).
pub fn lean_tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "lean_check",
            "description": "Verifica un enunciado + prueba Lean con el kernel externo (lake/lean). No guarda nada. Rechaza sorry/admit/axiom. PROVED = el kernel aceptó el archivo, no verdad absoluta.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "statement": {"type": "string"},
                    "proof": {"type": "string"}
                },
                "required": ["statement", "proof"]
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "lean_submit",
            "description": "Verifica con el kernel y guarda la prueba en lab_proofs/ por su sha256 (archivo .lean + .result.json + línea JSONL). Rechaza sorry/admit/axiom.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "statement": {"type": "string"},
                    "proof": {"type": "string"},
                    "label": {"type": "string"}
                },
                "required": ["statement", "proof"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
    ]
}

// ── Entradas públicas ──────────────────────────────────────────────────

/// `lean_check(statement, proof)`: denylist + kernel, SIN guardar.
pub fn lean_check(args: &Value) -> Result<Value, String> {
    let statement = get_field(args, "lean_check", "statement")?;
    let proof = get_field(args, "lean_check", "proof")?;
    let (proof_id, outcome) = run_verified("lean_check", &statement, &proof)?;
    Ok(response("lean_check", &proof_id, &outcome))
}

/// `lean_submit(statement, proof, label?)`: verifica y persiste en `lab_proofs/`.
pub fn lean_submit(args: &Value) -> Result<Value, String> {
    let statement = get_field(args, "lean_submit", "statement")?;
    let proof = get_field(args, "lean_submit", "proof")?;
    let label = args
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let (proof_id, outcome) = run_verified("lean_submit", &statement, &proof)?;
    let payload = response("lean_submit", &proof_id, &outcome);
    persist(&proof_id, &label, &statement, &proof, &payload)?;
    Ok(payload)
}

/// Recurso `lean://proofs/{id}`: devuelve lo guardado por `lean_submit`.
pub fn read_lean_proof(proof_id: &str) -> Result<Value, String> {
    if !crate::is_valid_cnf_hash(proof_id) {
        return Err("lean: proof_id inválido (se esperan 64 hex)".into());
    }
    let dir = proof_dir();
    let result_path = dir.join(format!("{proof_id}.result.json"));
    let text = std::fs::read_to_string(&result_path)
        .map_err(|_| "lean: prueba desconocida; generala con lean_submit".to_string())?;
    let result: Value = serde_json::from_str(&text)
        .map_err(|_| "lean: resultado guardado corrupto; reenviá con lean_submit".to_string())?;
    let lean_src =
        std::fs::read_to_string(dir.join(format!("{proof_id}.lean"))).unwrap_or_default();
    Ok(json!({
        "proof_id": proof_id,
        "lean": lean_src,
        "result": result,
    }))
}

// ── Validación ─────────────────────────────────────────────────────────

fn get_field(args: &Value, tool: &str, name: &str) -> Result<String, String> {
    let raw = args
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{tool}: '{name}' requerido (string no vacío)"))?;
    if raw.trim().is_empty() {
        return Err(format!("{tool}: '{name}' vacío: mandá el contenido real"));
    }
    if raw.len() > MAX_LEAN_FIELD_BYTES {
        return Err(format!(
            "{tool}: '{name}' excede 64 KiB; partí el desarrollo en lemas"
        ));
    }
    Ok(raw.to_string())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn contains_word(hay: &str, needle: &str) -> bool {
    let h: Vec<char> = hay.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    for i in 0..=h.len() - n.len() {
        if h[i..i + n.len()] == n[..] {
            let before_ok = i
                .checked_sub(1)
                .and_then(|b| h.get(b))
                .is_some_and(|c| is_word_char(*c));
            let after_ok = h.get(i + n.len()).is_some_and(|c| is_word_char(*c));
            if !before_ok && !after_ok {
                return true;
            }
        }
    }
    false
}

/// Corta el comentario de línea Lean (`-- resto ignorado`).
fn strip_line_comment(line: &str) -> &str {
    match line.find("--") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Denylist sobre el fuente combinado (líneas 1-based del `.lean` real).
fn deny_scan(source: &str) -> Result<(), String> {
    if source.contains("/-") || source.contains("-/") {
        return Err("lean: el código trae comentario de bloque `/- ... -/`; no se aceptan porque pueden ocultar `sorry`/`admit`: sacalos y mandá solo código lineal".into());
    }
    for (idx, line) in source.lines().enumerate() {
        let code = strip_line_comment(line);
        for tok in DENY_TOKENS {
            if contains_word(code, tok) {
                return Err(format!(
                    "lean: '{tok}' no permitido en línea {}: mandá una prueba completa, sin sorry/admit/axiom",
                    idx + 1
                ));
            }
        }
    }
    Ok(())
}

// ── Fuente + SHA-lock ──────────────────────────────────────────────────

fn lean_source(statement: &str, proof: &str) -> String {
    format!("{statement}\n{proof}\n")
}

/// `proof_id` = sha256 del statement normalizado (trim).
fn proof_id_for(statement: &str) -> String {
    crate::sha256_hex(statement.trim())
}

// ── Kernel sidecar ─────────────────────────────────────────────────────

fn missing_msg() -> String {
    "solver ausente (lean): instalá elan (https://leanprover.github.io/lean4/doc/setup.html: `elan toolchain install leanprover/lean4:stable`) y dejá `lake`/`lean` en tu PATH, o seteá GRAFITO_LEAN_BIN al binario; Grafito no declara teoremas sin kernel externo".to_string()
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn is_lake_binary(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "lake")
}

/// Resuelve el binario: `GRAFITO_LEAN_BIN` o `lake`/`lean` en PATH.
/// Devuelve (binario, es_lake).
fn lean_binary() -> Result<(PathBuf, bool), String> {
    if let Ok(raw) = std::env::var("GRAFITO_LEAN_BIN") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if is_executable_file(&path) {
                let lake = is_lake_binary(&path);
                return Ok((path, lake));
            }
            return Err(missing_msg());
        }
    }
    if let Some(path) = crate::sat::find_in_path("lake") {
        return Ok((path, true));
    }
    if let Some(path) = crate::sat::find_in_path("lean") {
        return Ok((path, false));
    }
    // Instalación estándar de elan (no siempre está en el PATH heredado).
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for name in ["lake", "lean"] {
            let cand = home.join(".elan/bin").join(name);
            if is_executable_file(&cand) {
                return Ok((cand, name == "lake"));
            }
        }
    }
    Err(missing_msg())
}

/// Timeout desde el entorno (default 120 s, tope 600 s, 0 = sin timeout).
fn lean_timeout_ms() -> u64 {
    std::env::var("GRAFITO_LEAN_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|v| {
            if v == 0 {
                0
            } else {
                v.min(MAX_LEAN_TIMEOUT_MS)
            }
        })
        .unwrap_or(DEFAULT_LEAN_TIMEOUT_MS)
}

struct KernelOutcome {
    status: &'static str,
    output: String,
    elapsed_ms: u64,
}

fn cap_output(text: &str) -> String {
    text.chars().take(OUTPUT_CAP_CHARS).collect()
}

/// Corre el kernel sobre el archivo con polling (patrón `sat.rs`).
fn run_kernel(
    binary: &Path,
    via_lake: bool,
    lean_file: &Path,
    timeout_ms: u64,
) -> Result<KernelOutcome, String> {
    let start = Instant::now();
    let mut cmd = Command::new(binary);
    if via_lake {
        cmd.arg("env").arg("lean");
    }
    let mut child = cmd
        .arg(lean_file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar {}: {e}", binary.display()))?;
    let timeout = if timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms))
    };
    let finished = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) => {
                if timeout.is_some_and(|t| start.elapsed() >= t) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(format!("lean: error esperando al kernel: {e}")),
        }
    };
    let elapsed_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if !finished {
        return Ok(KernelOutcome {
            status: "TIMEOUT",
            output: format!(
                "se agotó el timeout de {timeout_ms} ms; achicá la prueba o subí GRAFITO_LEAN_TIMEOUT_MS (máx {MAX_LEAN_TIMEOUT_MS})"
            ),
            elapsed_ms,
        });
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("lean: error leyendo al kernel: {e}"))?;
    if output.status.success() {
        return Ok(KernelOutcome {
            status: "PROVED",
            output: String::new(),
            elapsed_ms,
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    let message = if combined.is_empty() {
        "el kernel rechazó la prueba sin mensaje (exit != 0)".to_string()
    } else {
        combined
    };
    Ok(KernelOutcome {
        status: "FAILED",
        output: cap_output(&message),
        elapsed_ms,
    })
}

// ── Verificación completa (sin guardar) ────────────────────────────────

/// Valida tamaños, pasa la denylist y corre el kernel sobre un staging
/// temporal (se borra; `lean_check` no persiste nada).
fn run_verified(
    tool: &str,
    statement: &str,
    proof: &str,
) -> Result<(String, KernelOutcome), String> {
    let source = lean_source(statement, proof);
    deny_scan(&source).map_err(|e| format!("{tool}: {e}"))?;
    let (binary, via_lake) = lean_binary().map_err(|e| format!("{tool}: {e}"))?;
    let proof_id = proof_id_for(statement);
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("grafito-{proof_id}.lean"));
    std::fs::write(&tmp, &source)
        .map_err(|e| format!("{tool}: no se pudo staging temporal: {e}"))?;
    let outcome = run_kernel(&binary, via_lake, &tmp, lean_timeout_ms())
        .map_err(|e| format!("{tool}: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    Ok((proof_id, outcome))
}

// ── Respuesta ──────────────────────────────────────────────────────────

fn response(tool: &str, proof_id: &str, outcome: &KernelOutcome) -> Value {
    let ok = outcome.status == "PROVED";
    let mut payload = json!({
        "tool": tool,
        "proof_id": proof_id,
        "status": outcome.status,
        "ok": ok,
        "elapsed_ms": outcome.elapsed_ms,
        "note": "PROVED = el kernel Lean aceptó el archivo; es chequeo mecánico, no verdad matemática absoluta",
    });
    if !ok && !outcome.output.is_empty() {
        payload["errors"] = Value::String(outcome.output.clone());
    }
    payload
}

// ── Persistencia (`lab_proofs/` hermano del ledger) ────────────────────

/// Directorio de pruebas: hermano del ledger (misma lógica que `cnf_dir`).
fn proof_dir() -> PathBuf {
    let cnf = crate::ledger::cnf_dir();
    match cnf.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("lab_proofs"),
        _ => PathBuf::from("lab_proofs"),
    }
}

/// Guarda `.lean` + `.result.json` + línea JSONL (corre tras verificar).
fn persist(
    proof_id: &str,
    label: &str,
    statement: &str,
    proof: &str,
    payload: &Value,
) -> Result<(), String> {
    let dir = proof_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("lean: no se pudo crear {dir:?}: {e}"))?;
    std::fs::write(
        dir.join(format!("{proof_id}.lean")),
        lean_source(statement, proof),
    )
    .map_err(|e| format!("lean: no se pudo guardar la prueba: {e}"))?;
    let text =
        serde_json::to_string(payload).map_err(|e| format!("lean: no se pudo serializar: {e}"))?;
    std::fs::write(dir.join(format!("{proof_id}.result.json")), text)
        .map_err(|e| format!("lean: no se pudo guardar el resultado: {e}"))?;
    let line = json!({
        "proof_id": proof_id,
        "label": label,
        "statement_hash": proof_id,
        "ok": payload.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "ts": crate::ledger::now_epoch_secs(),
    })
    .to_string();
    append_line(&dir.join("lab_proofs.jsonl"), &line)
}

fn append_line(path: &Path, line: &str) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("lean: no se pudo abrir {path:?}: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("lean: no se pudo escribir: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Escribe un stub de toolchain ejecutable (`#!/bin/sh` + cuerpo).
    fn write_stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("grafito-lean-test-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Fija `GRAFITO_LAB_LEDGER` a un temporal y devuelve el dir; el
    /// caller restaura con `restore_env`.
    fn lock_and_isolate(
        name: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        PathBuf,
        Option<String>,
        Option<String>,
    ) {
        let guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_ledger = std::env::var("GRAFITO_LAB_LEDGER").ok();
        let old_bin = std::env::var("GRAFITO_LEAN_BIN").ok();
        let dir = temp_dir(name);
        let ledger = dir.join("lab.jsonl");
        let _ = std::fs::remove_file(&ledger);
        unsafe {
            std::env::set_var("GRAFITO_LAB_LEDGER", ledger.to_string_lossy().to_string());
        }
        (guard, dir, old_ledger, old_bin)
    }

    unsafe fn restore_env(old_ledger: Option<String>, old_bin: Option<String>) {
        match old_ledger {
            Some(v) => std::env::set_var("GRAFITO_LAB_LEDGER", v),
            None => std::env::remove_var("GRAFITO_LAB_LEDGER"),
        }
        match old_bin {
            Some(v) => std::env::set_var("GRAFITO_LEAN_BIN", v),
            None => std::env::remove_var("GRAFITO_LEAN_BIN"),
        }
    }

    #[test]
    fn denylist_reporta_token_y_linea() {
        let err = deny_scan("theorem t : True := by\n  sorry\n").unwrap_err();
        assert!(err.contains("sorry"), "{err}");
        assert!(err.contains("línea 2"), "{err}");
        let err = deny_scan("foo\nbar admit baz\n").unwrap_err();
        assert!(err.contains("admit") && err.contains("línea 2"), "{err}");
        let err = deny_scan("axiom fe : True\n").unwrap_err();
        assert!(err.contains("axiom") && err.contains("línea 1"), "{err}");
    }

    #[test]
    fn comentario_de_linea_con_sorry_pasa_y_subcadenas_no() {
        assert!(deny_scan("theorem t : True := trivial -- sorry viejo\n").is_ok());
        // `sorry` como subcadena no es palabra completa.
        assert!(deny_scan("def sorrys : Nat := 1\n").is_ok());
        assert!(deny_scan("def admission : Nat := 1\n").is_ok());
    }

    #[test]
    fn bloque_comentario_se_rechaza_con_guia() {
        let err = deny_scan("theorem t : True := by\n/- sorry -/\n  trivial\n").unwrap_err();
        assert!(err.contains("/-"), "{err}");
    }

    #[test]
    fn sha_lock_estable_ante_trim() {
        assert_eq!(
            proof_id_for("  theorem t : True  "),
            proof_id_for("theorem t : True")
        );
        assert_eq!(proof_id_for("a").len(), 64);
    }

    #[test]
    fn binario_ausente_da_error_honesto() {
        let (_g, _dir, old_ledger, old_bin) = lock_and_isolate("no-bin");
        unsafe {
            std::env::set_var("GRAFITO_LEAN_BIN", "/nonexistent/lean");
        }
        let r = lean_check(&json!({"statement": "theorem t : True", "proof": "by trivial"}));
        let err = r.unwrap_err();
        assert!(err.contains("solver ausente (lean)"), "{err}");
        assert!(err.contains("elan"), "{err}");
        unsafe { restore_env(old_ledger, old_bin) };
    }

    #[test]
    fn stub_proved_y_failed() {
        let (_g, dir, old_ledger, old_bin) = lock_and_isolate("stubs");
        let ok_stub = write_stub(&dir, "fake-lean", "echo ok");
        unsafe {
            std::env::set_var("GRAFITO_LEAN_BIN", ok_stub.to_string_lossy().to_string());
        }
        let out =
            lean_check(&json!({"statement": "theorem t : True", "proof": "by trivial"})).unwrap();
        assert_eq!(out.get("status").and_then(Value::as_str), Some("PROVED"));
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(true));
        assert!(out
            .get("note")
            .and_then(Value::as_str)
            .is_some_and(|n| n.contains("no verdad")));

        let fail_stub = write_stub(
            &dir,
            "fake-lean-fail",
            "echo 'error: unknown identifier' >&2\nexit 1",
        );
        unsafe {
            std::env::set_var("GRAFITO_LEAN_BIN", fail_stub.to_string_lossy().to_string());
        }
        let out =
            lean_check(&json!({"statement": "theorem t : True", "proof": "by bogus"})).unwrap();
        assert_eq!(out.get("status").and_then(Value::as_str), Some("FAILED"));
        assert_eq!(out.get("ok").and_then(Value::as_bool), Some(false));
        assert!(out
            .get("errors")
            .and_then(Value::as_str)
            .is_some_and(|e| e.contains("unknown identifier")));
        unsafe { restore_env(old_ledger, old_bin) };
    }

    #[test]
    fn submit_guarda_y_recurso_lee() {
        let (_g, dir, old_ledger, old_bin) = lock_and_isolate("submit");
        let ok_stub = write_stub(&dir, "fake-lean", "echo ok");
        unsafe {
            std::env::set_var("GRAFITO_LEAN_BIN", ok_stub.to_string_lossy().to_string());
        }
        let statement = "theorem t : True";
        let out =
            lean_submit(&json!({"statement": statement, "proof": "by trivial", "label": "demo"}))
                .unwrap();
        let proof_id = out
            .get("proof_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        assert_eq!(proof_id, proof_id_for(statement));
        // SHA-lock estable: segundo submit da el mismo id.
        let out2 =
            lean_submit(&json!({"statement": "  theorem t : True  ", "proof": "by trivial"}))
                .unwrap();
        assert_eq!(
            out2.get("proof_id").and_then(Value::as_str),
            Some(proof_id.as_str())
        );

        let proofs = proof_dir();
        assert!(proofs.join(format!("{proof_id}.lean")).is_file());
        assert!(proofs.join(format!("{proof_id}.result.json")).is_file());
        let jsonl = std::fs::read_to_string(proofs.join("lab_proofs.jsonl")).unwrap();
        assert!(
            jsonl.contains(&proof_id) && jsonl.contains("\"label\":\"demo\""),
            "{jsonl}"
        );

        let res = read_lean_proof(&proof_id).unwrap();
        assert_eq!(
            res.get("proof_id").and_then(Value::as_str),
            Some(proof_id.as_str())
        );
        assert!(read_lean_proof("zz").is_err());
        unsafe { restore_env(old_ledger, old_bin) };
    }

    #[test]
    fn defs_traen_dos_tools_con_schema_y_readonly() {
        let defs = lean_tool_defs();
        assert_eq!(defs.len(), 2);
        let names: Vec<&str> = defs
            .iter()
            .filter_map(|d| d.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"lean_check") && names.contains(&"lean_submit"));
        for d in &defs {
            assert!(d.get("inputSchema").is_some());
            assert!(d
                .pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                .is_some());
        }
    }

    #[test]
    fn campos_vacios_y_enormes_se_rechazan() {
        let (_g, _dir, old_ledger, old_bin) = lock_and_isolate("valid");
        assert!(lean_check(&json!({"statement": "  ", "proof": "x"})).is_err());
        assert!(lean_check(&json!({"statement": "x"})).is_err());
        let big = "y".repeat(MAX_LEAN_FIELD_BYTES + 1);
        assert!(lean_check(&json!({"statement": big, "proof": "x"})).is_err());
        unsafe { restore_env(old_ledger, old_bin) };
    }
}
