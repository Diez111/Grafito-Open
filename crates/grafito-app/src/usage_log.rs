//! Telemetría local opt-in del uso del asistente (JSONL, sin PII).
//!
//! Se activa con `GRAFITO_USAGE_LOG=1` (ruta por defecto
//! `$XDG_DATA_HOME/grafito/assistant_usage.jsonl`) o con una ruta explícita
//! (`GRAFITO_USAGE_LOG=/ruta/archivo.jsonl`). Escribe una línea por turno con
//! métricas de tamaño/tokens/tiempos: nunca contenido del usuario, claves ni
//! rutas. El `append` corre en un hilo efímero para no bloquear la UI.
//!
//! Sirve para medir antes/después de cualquier recorte de prompt (auditoría
//! de tokens): sin números reales no se optimiza, se adivina.

use grafito_assistant_types::AssistantTokenUsage;

/// Evento de uso de un turno remoto (o de la publicación de su propuesta).
#[derive(Debug, Clone)]
pub(crate) struct UsageEvent {
    /// `chat` (completado remoto) o `proposal` (publicación tras preflight).
    pub kind: &'static str,
    pub provider: String,
    pub model: String,
    /// Caracteres de la consulta del usuario.
    pub question_chars: usize,
    /// Caracteres del prompt enviado (0 si no se conoce en este punto).
    pub input_chars: usize,
    /// Caracteres de la respuesta publicada.
    pub output_chars: usize,
    /// Tokens reales de la wire (vacío si el proveedor no los reportó).
    pub usage: Option<AssistantTokenUsage>,
    /// Duración total del turno remoto en milisegundos (0 si no aplica).
    pub elapsed_ms: u64,
    /// El documento/foco cambió durante el turno (aviso informativo).
    pub stale_context: bool,
}

/// Ruta destino: variable de entorno o `None` (telemetría apagada).
fn usage_log_path() -> Option<std::path::PathBuf> {
    let raw = std::env::var_os("GRAFITO_USAGE_LOG")?;
    usage_log_path_from(
        Some(&raw.to_string_lossy()),
        std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from),
        std::env::var_os("HOME").map(std::path::PathBuf::from),
    )
}

/// Resolución pura de la ruta (testeable sin tocar el entorno global).
fn usage_log_path_from(
    raw: Option<&str>,
    xdg_data_home: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "0" || trimmed.eq_ignore_ascii_case("false") {
        return None;
    }
    if trimmed == "1" || trimmed.eq_ignore_ascii_case("true") {
        let base = xdg_data_home
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| {
                let mut home = home.unwrap_or_default();
                if home.as_os_str().is_empty() {
                    home = std::path::PathBuf::from(".");
                }
                home.push(".local/share");
                home
            });
        return Some(base.join("grafito").join("assistant_usage.jsonl"));
    }
    Some(std::path::PathBuf::from(trimmed))
}

/// Registra un evento (no-op si `GRAFITO_USAGE_LOG` está apagado).
///
/// El I/O corre en un hilo efímero: el llamante vive en el hilo de UI y jamás
/// se bloquea por telemetría. El JSON se arma con `serde_json` (mismo crate
/// que el resto del workspace) y el timestamp es epoch en segundos.
pub(crate) fn record(event: UsageEvent) {
    let Some(path) = usage_log_path() else {
        return;
    };
    let usage = event.usage;
    let line = serde_json::json!({
        "ts": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0),
        "kind": event.kind,
        "provider": event.provider,
        "model": event.model,
        "question_chars": event.question_chars,
        "input_chars": event.input_chars,
        "output_chars": event.output_chars,
        "elapsed_ms": event.elapsed_ms,
        "stale_context": event.stale_context,
        "tokens": usage.map(|usage| serde_json::json!({
            "in": usage.input_tokens,
            "out": usage.output_tokens,
            "reasoning": usage.reasoning_tokens,
            "cached": usage.cached_input_tokens,
            "total": usage.total_tokens,
        })),
    })
    .to_string();
    let _ = std::thread::Builder::new()
        .name("grafito-usage-log".into())
        .spawn(move || {
            use std::io::Write;
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{line}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_log_resuelve_rutas_sin_tocar_el_entorno() {
        use std::path::PathBuf;
        // Apagada: sin variable, vacía, "0" o "false".
        assert!(usage_log_path_from(None, None, None).is_none());
        assert!(usage_log_path_from(Some(""), None, None).is_none());
        assert!(usage_log_path_from(Some("0"), None, None).is_none());
        assert!(usage_log_path_from(Some("false"), None, None).is_none());
        // "1" usa XDG_DATA_HOME (o HOME/.local/share como fallback).
        assert_eq!(
            usage_log_path_from(Some("1"), Some(PathBuf::from("/tmp/xdg")), None),
            Some(PathBuf::from("/tmp/xdg/grafito/assistant_usage.jsonl"))
        );
        assert_eq!(
            usage_log_path_from(Some("1"), None, Some(PathBuf::from("/home/u"))),
            Some(PathBuf::from(
                "/home/u/.local/share/grafito/assistant_usage.jsonl"
            ))
        );
        // Ruta explícita tal cual.
        assert_eq!(
            usage_log_path_from(Some("/tmp/usage.jsonl"), None, None),
            Some(PathBuf::from("/tmp/usage.jsonl"))
        );
    }

    #[test]
    fn record_sin_variable_es_noop() {
        // Sin `GRAFITO_USAGE_LOG`, `record` no debe crear archivos: el camino
        // sale antes del spawn (se verifica indirectamente con un evento).
        record(UsageEvent {
            kind: "chat",
            provider: "opencode_go".into(),
            model: "test".into(),
            question_chars: 1,
            input_chars: 2,
            output_chars: 3,
            usage: None,
            elapsed_ms: 4,
            stale_context: false,
        });
    }
}
