#![no_main]
//! A16-11 Fuzz target del wire de animación (JSON v1 sobre stdio).
//!
//! Espejo documentado de dos fuentes (sin nuevas deps: sólo
//! `serde_json` + `libfuzzer_sys`, ya presentes en `fuzz/Cargo.toml`):
//! - `crates/grafito-anim/src/protocol.rs` — `ANIM_PROTOCOL_VERSION = 1`,
//!   `line_cap` 64 KiB, `percent` 0..=100, códigos saneados, mensajes ≤500.
//! - `crates/grafito-anim/engines/python/manim_engine/__main__.py` —
//!   `SAFE_FUNCS`, rechazo de dunder `"__"`, `MAX_NODES = 200`,
//!   `MAX_EXPR_LEN = 500`.
//!
//! Corpus semilla commiteado (`fuzz/corpus/anim_wire/` — G17):
//! `01_z_squared.expr` (`z^2`, var no permitida), `02_exp_x.expr` (`exp(x)`,
//! SAFE_FUNCS ok), `03_dunder_import.expr` (`__import__('os')`, dunder),
//! `04_hello_v1.json` / `05_progress_60.json` / `06_render_result.json` /
//! `07_error.json` / `08_pong.json` (wire v1 + trunc 500), `09_oversize_66k.json`
//! (66 KiB, >line_cap 64 KiB). Registrado en `fuzz/Cargo.toml` como
//! `[[bin]] anim_wire` y en `.github/workflows/fuzz.yml` (weekly, 7 targets).
//! La lógica aquí es total (sin panic salvo OOM del propio fuzzer) y está
//! pineada como golden legible en
//! `crates/grafito-anim/tests/anim_wire_fuzz_corpus.rs`.

use libfuzzer_sys::fuzz_target;

/// Tope de bytes por línea del worker (`engine.rs: DEFAULT_LINE_CAP_BYTES`).
const LINE_CAP_BYTES: usize = 64 * 1024;
/// Longitud máxima de expresión Python (`__main__.py: MAX_EXPR_LEN`).
const MAX_EXPR_LEN: usize = 500;
/// Nodos AST máximos por expresión (`__main__.py: MAX_NODES`).
const MAX_NODES: usize = 200;
/// Funciones permitidas (`__main__.py: SAFE_FUNCS`) + variable `x`.
const SAFE_IDENTS: &[&str] = &[
    "x", "sin", "cos", "tan", "exp", "log", "sqrt", "abs", "pi", "e",
];

/// Espejo de `validate_expr` (forma, no semántica): `true` = debe rechazar.
fn mirror_expr_rejected(src: &str) -> bool {
    if src.is_empty() || src.len() > MAX_EXPR_LEN {
        return true;
    }
    if src.contains("__") {
        return true;
    }
    // Heurística de nodos: cada token alfanumérico ~1 nodo AST.
    let mut nodes = 0usize;
    let mut current = String::new();
    let mut flush = |current: &mut String, nodes: &mut usize| {
        if !current.is_empty() {
            *nodes += 1;
            let is_number = current
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit() || c == '.');
            let allowed = is_number || current == "x" || SAFE_IDENTS.contains(&current.as_str());
            current.clear();
            if !allowed {
                *nodes = MAX_NODES + 1;
            }
        }
    };
    for ch in src.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            current.push(ch);
        } else {
            flush(&mut current, &mut nodes);
            if "+-*/%^(), ".contains(ch) {
                nodes += 1;
            } else {
                // Caracter fuera del alfabeto de expresiones: rechazo.
                return true;
            }
        }
        if nodes > MAX_NODES {
            return true;
        }
    }
    flush(&mut current, &mut nodes);
    nodes == 0 || nodes > MAX_NODES
}

/// Forma mínima del wire v1: `true` = línea que el lector debe aceptar
/// para parseo (el `downcast` estricto decide después por `type`).
fn mirror_wire_shape_ok(value: &serde_json::Value) -> bool {
    let kind = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match kind {
        "hello" => value.get("protocol_version").and_then(|v| v.as_u64()) == Some(1),
        "progress" => value
            .get("percent")
            .and_then(|v| v.as_u64())
            .map_or(false, |p| p <= 100),
        "render_result" => {
            value.get("job_id").and_then(|v| v.as_str()).is_some()
                && value.get("media_path").and_then(|v| v.as_str()).is_some()
        }
        "error" => value.get("message").and_then(|v| v.as_str()).is_some(),
        "pong" => true,
        _ => false,
    }
}

fuzz_target!(|data: &[u8]| {
    // 1) Presupuesto de línea: oversize se drena sin acumular (anti-OOM).
    if data.len() > LINE_CAP_BYTES {
        return;
    }
    // 2a) Si es JSON con forma de wire v1, la forma debe decidirse sin panic.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
        let _ = mirror_wire_shape_ok(&value);
    }
    // 2b) Si es texto de expresión, el espejo decide aceptar/rechazar sin panic.
    let text = String::from_utf8_lossy(data);
    for line in text.lines().take(8) {
        let _ = mirror_expr_rejected(line.trim());
    }
});
