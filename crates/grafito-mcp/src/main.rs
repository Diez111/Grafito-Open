//! Binario `grafito-mcp`: servidor MCP por stdio (una línea JSON por mensaje).
//!
//! - stdin → stdout: JSON-RPC 2.0. stderr: logs mínimos (jamás protocolo).
//! - Línea > 64 MiB se rechaza con `-32700` (anti-OOM de wire).
//! - Cada respuesta termina en `\n` + flush.

use grafito_mcp::{protocol, LabLimits};
use std::io::{BufRead, Write};

/// Límite de una línea del wire (64 MiB, paridad con el puente de animación).
const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

fn main() {
    let limits = LabLimits::from_env();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out_lock = stdout.lock();
    let mut line = String::new();
    let mut reader = stdin.lock();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("[grafito-mcp] stdin: {e}");
                break;
            }
        }
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            let resp = protocol::protocol_error(
                serde_json::Value::Null,
                -32700,
                "línea demasiado larga (límite 64 MiB)".into(),
            );
            if write_response(&mut out_lock, &resp).is_err() {
                break;
            }
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = protocol::protocol_error(
                    serde_json::Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                );
                if write_response(&mut out_lock, &resp).is_err() {
                    break;
                }
                continue;
            }
        };
        // Id para errores de request inválido (si hay).
        let id = msg.get("id").cloned().unwrap_or(serde_json::Value::Null);
        if msg.get("method").and_then(|m| m.as_str()).is_none() {
            let resp =
                protocol::protocol_error(id, -32600, "request inválido: falta 'method'".into());
            if write_response(&mut out_lock, &resp).is_err() {
                break;
            }
            continue;
        }
        if let Some(resp) = protocol::dispatch(&msg, &limits) {
            if write_response(&mut out_lock, &resp).is_err() {
                break;
            }
        }
    }
}

fn write_response(
    stdout: &mut std::io::StdoutLock<'_>,
    resp: &serde_json::Value,
) -> Result<(), std::io::Error> {
    let mut text = resp.to_string();
    text.push('\n');
    stdout.write_all(text.as_bytes())?;
    stdout.flush()
}
