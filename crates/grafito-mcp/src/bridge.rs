//! Puente hacia el cerebro completo de Grafito (sin duplicar lógica).
//!
//! - **Proxy**: las 30 tools puras de `grafito-assistant` (`all_safe_tool_schemas`
//!   menos el harness-2 viejo y `web_search`) se re-exportan tal cual y se
//!   despachan por `SafeGrafitoDispatcher`. Paridad para siempre: si el
//!   asistente suma una tool, el MCP la expone sin tocar este crate.
//! - **`execute_command`**: ejecución REAL multi-step sobre un `Document`
//!   efímero vía `process_input` (el mismo camino del smoke de ejecución).
//!   Cubre los 650 comandos con errores honestos; bloquea el I/O de archivos
//!   (imágenes, sonido, export, grabación) con guía hacia la app.
//!
//! Todo es cerebro puro: sin egui, sin wgpu, sin red.

use grafito_agent::{ToolCall, ToolDispatcher};
use serde_json::{json, Value};

/// Tools del asistente que el MCP NO proxea (las sirve él con versión propia
/// o las excluye a propósito).
fn is_excluded(name: &str) -> bool {
    matches!(
        name,
        // Harness-2 viejo: el MCP sirve su versión extendida (familias,
        // índice espacial, ledger). Mismo nombre, mejor motor.
        "search_topp39" | "export_dimacs"
        // Red opt-in de la app: un server stdio local no hace red en silencio.
        | "web_search"
    )
}

/// Definiciones MCP de las tools proxedas (30: 3 base + 8 pedag + 17 math + 2 harness1).
pub fn proxied_tool_defs() -> Vec<Value> {
    grafito_assistant::agent::all_safe_tool_schemas()
        .iter()
        .filter(|s| !is_excluded(&s.name))
        .map(|s| {
            json!({
                "name": s.name,
                "description": s.description,
                "inputSchema": s.parameters,
                "annotations": {"readOnlyHint": true, "destructiveHint": false},
            })
        })
        .collect()
}

/// ¿`name` es una tool proxeda?
pub fn is_proxied(name: &str) -> bool {
    !is_excluded(name)
        && grafito_assistant::agent::all_safe_tool_schemas()
            .iter()
            .any(|s| s.name == name)
}

/// Despacha una tool proxeda por el dispatcher del asistente.
///
/// `Ok` → JSON (parsea el contenido si es JSON, si no lo envuelve en
/// `{"text": ...}`). `Err` → mensaje honesto del motor (el protocolo lo
/// envuelve con `isError:true`).
pub fn dispatch_proxied(name: &str, args: &Value) -> Result<Value, String> {
    if is_excluded(name) {
        return Err(format!("tool '{name}' la sirve el MCP directamente"));
    }
    if !is_proxied(name) {
        return Err(format!("tool '{name}' desconocida"));
    }
    let call = ToolCall {
        id: format!("mcp-{name}"),
        name: name.to_string(),
        arguments: args.clone(),
    };
    let dispatcher = grafito_assistant::agent::SafeGrafitoDispatcher;
    let result = dispatcher.dispatch(&call);
    if !result.ok {
        return Err(result.content);
    }
    // structuredContent DEBE ser objeto: varios clientes (opencode) validan
    // `record` y rechazan escalares (ej. evaluate_expr → `4`). Todo lo que
    // no sea objeto se envuelve en {"value": …}.
    match serde_json::from_str::<Value>(&result.content) {
        Ok(Value::Object(map)) => Ok(Value::Object(map)),
        Ok(parsed) => Ok(json!({"value": parsed})),
        Err(_) => Ok(json!({"text": result.content})),
    }
}

// ── execute_command ──────────────────────────────────────────────────

/// Pasos máximos por llamada (construcciones multi-objeto sin abuso).
pub const MAX_EXEC_STEPS: usize = 32;
/// Caracteres máximos por paso (paridad con `run_command`, 2000).
pub const MAX_STEP_CHARS: usize = 2000;

/// Comandos con I/O de archivos/medios: se rechazan con guía hacia la app.
/// Comparación por cabeza del comando en minúsculas.
fn blocked_head(head: &str) -> bool {
    matches!(
        head,
        "setimage"
            | "toolimage"
            | "playsound"
            | "exportimage"
            | "startrecord"
            | "save"
            | "export"
            | "import"
    )
}

/// Cabeza del comando: hasta `[`, `(`, `=` o espacio (para el blocklist).
fn command_head(text: &str) -> String {
    text.chars()
        .take_while(|c| !matches!(c, '[' | '(' | '=' | ' ' | '\t'))
        .collect::<String>()
        .to_lowercase()
}

/// Valida un paso: una línea, ≤2000 chars, sin NUL, sin I/O.
fn validate_step(raw: &str) -> Result<String, String> {
    let texto = raw.trim();
    if texto.is_empty() {
        return Err("execute_command: paso vacío".into());
    }
    if texto.contains('\n') || texto.contains('\r') || texto.contains('\0') {
        return Err("execute_command: cada paso debe ser una sola línea sin NUL".into());
    }
    if texto.chars().count() > MAX_STEP_CHARS {
        return Err(format!(
            "execute_command: paso excede {MAX_STEP_CHARS} caracteres"
        ));
    }
    let head = command_head(texto);
    if blocked_head(&head) {
        return Err(format!(
            "execute_command: '{head}' toca archivos/medios y solo corre en la app (usá Grafito UI); el MCP ejecuta los 650 comandos de cómputo y geometría en memoria"
        ));
    }
    Ok(texto.to_string())
}

fn cap_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// `execute_command(command? | steps?)`: ejecución real sobre doc efímero.
///
/// Acepta un comando solo (`{"command": "Punto[(1,2)]"}`) o una secuencia
/// (`{"steps": ["A = (1, 2)", "Recta[A, B]"]}`) que comparte el mismo
/// documento efímero. Devuelve por paso `{status, detail}` más inventario
/// final (etiquetas ordenadas). Los errores son los honestos del motor
/// (incluida la lista pinneada de no-implementados permanentes).
pub fn execute_command(args: &Value) -> Result<Value, String> {
    let mut steps: Vec<String> = Vec::new();
    if let Some(arr) = args.get("steps").and_then(Value::as_array) {
        if arr.is_empty() || arr.len() > MAX_EXEC_STEPS {
            return Err(format!(
                "execute_command: 'steps' fuera de [1, {MAX_EXEC_STEPS}]"
            ));
        }
        for (i, item) in arr.iter().enumerate() {
            let raw = item
                .as_str()
                .ok_or(format!("execute_command: steps[{i}] debe ser string"))?;
            steps.push(validate_step(raw).map_err(|e| format!("steps[{i}]: {e}"))?);
        }
    } else if let Some(single) = args.get("command").and_then(Value::as_str) {
        steps.push(validate_step(single)?);
    } else {
        return Err("execute_command: pasá 'command' (string) o 'steps' ([string, ...])".into());
    }

    let mut document = grafito_core::Document::new();
    let mut outcomes = Vec::with_capacity(steps.len());
    let mut had_error = false;
    for (i, step) in steps.iter().enumerate() {
        let mut input = step.clone();
        // Barrera anti-caída: el smoke pineó que los 650 ejemplos mínimos no
        // panickean, pero un input arbitrario podría tocar un camino no
        // cubierto. El pánico se captura y se vuelve error honesto; como el
        // doc puede quedar inconsistente, se saltean los pasos restantes en
        // vez de seguir sobre estado corrupto. `AssertUnwindSafe` vale porque
        // el doc es efímero y se descarta tras la llamada (nada compartido).
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            grafito_command::commands::process_input(&mut document, &mut input)
        }));
        let (status, detail, stop) = match caught {
            Ok(grafito_command::commands::CommandOutcome::Ok) => ("ok", String::new(), false),
            Ok(grafito_command::commands::CommandOutcome::Message(m)) => ("message", m, false),
            Ok(grafito_command::commands::CommandOutcome::Error(e)) => {
                had_error = true;
                ("error", e, false)
            }
            Err(_) => {
                had_error = true;
                (
                    "panic",
                    "el motor panickeó con este input (bug a reportar: ningún ejemplo mínimo lo hace según el smoke); pasos restantes salteados"
                        .to_string(),
                    true,
                )
            }
        };
        outcomes.push(json!({
            "i": i,
            "input": cap_chars(step, 256),
            "status": status,
            "detail": cap_chars(&detail, 500),
        }));
        if stop {
            for (j, rest) in steps.iter().enumerate().skip(i + 1) {
                outcomes.push(json!({
                    "i": j,
                    "input": cap_chars(rest, 256),
                    "status": "skipped",
                    "detail": "salteado por pánico en un paso previo",
                }));
            }
            break;
        }
    }
    let mut labels: Vec<String> = document
        .objects_iter_sorted()
        .map(|(_, obj)| obj.label().to_string())
        .take(257)
        .collect();
    let truncated = labels.len() > 256;
    labels.truncate(256);
    let note = if had_error {
        "algún paso falló con error honesto del motor; los pasos son secuenciales sobre el mismo doc efímero"
    } else {
        "ejecución real en memoria sobre doc efímero; nada persiste salvo que cites los pasos"
    };
    Ok(json!({
        "tool": "execute_command",
        "ok": !had_error,
        "steps": outcomes,
        "objects": labels.len(),
        "labels": labels,
        "labels_truncated": truncated,
        "note": note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_cubre_30_tools_y_excluye_2_mas_web() {
        let defs = proxied_tool_defs();
        let names: Vec<&str> = defs
            .iter()
            .filter_map(|d| d.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(defs.len(), 30, "nombres: {names:?}");
        assert!(!names.contains(&"search_topp39"));
        assert!(!names.contains(&"export_dimacs"));
        assert!(!names.contains(&"web_search"));
        for must in [
            "evaluate_expr",
            "grafito_docs",
            "ask_user",
            "diff",
            "integrate",
            "run_command",
            "scaffold",
            "fourier",
            "nth_derivative",
            "partial",
            "lambert_w",
        ] {
            assert!(names.contains(&must), "falta {must} en {names:?}");
        }
    }

    #[test]
    fn evaluate_proxeda_computa() {
        let out = dispatch_proxied("evaluate_expr", &json!({"expression": "2+2"})).unwrap();
        // structuredContent siempre objeto (clientes validan `record`).
        assert!(out.is_object(), "debe ser objeto: {out}");
        assert!(out.to_string().contains('4'), "evaluate 2+2 dio: {out}");
        let d = dispatch_proxied("diff", &json!({"expression": "x^2"})).unwrap();
        assert!(d.to_string().contains('x'), "diff x^2 dio: {d}");
        assert!(dispatch_proxied("evaluate_expr", &json!({})).is_err());
    }

    #[test]
    fn execute_punto_y_bloqueo_io() {
        let ok = execute_command(&json!({"command": "Punto[(1, 2)]"})).unwrap();
        assert_eq!(ok["ok"], json!(true));
        assert!(ok["objects"].as_u64().unwrap() >= 1);
        let blocked = execute_command(&json!({"command": "ExportImage[/tmp/x.png]"})).unwrap_err();
        assert!(blocked.contains("archivos"), "{blocked}");
        let multi = execute_command(&json!({"steps": ["A = (1, 2)", "B = (3, 4)"]})).unwrap();
        assert_eq!(multi["steps"].as_array().unwrap().len(), 2);
    }
}
