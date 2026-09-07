//! Aplicación de escritorio Grafito — Punto de entrada principal

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::manual_clamp
)]

fn main() {
    install_crash_log_hook();
    if let Err(e) = grafito_app::run_app() {
        log::error!("Failed to run Grafito: {}", e);
        std::process::exit(1);
    }
}

/// Tope del log de crashes (rotación por truncado honesto, sin dependencia externa).
const MAX_CRASH_LOG_BYTES: u64 = 256 * 1024;

/// Tope del mensaje de pánico persistido (corte siempre en boundary UTF-8).
const MAX_CRASH_MESSAGE_BYTES: usize = 1024;

/// Instala el hook de pánico F10-FIX al inicio del entry.
///
/// Antepone al hook default (que sigue imprimiendo en stderr) y anexa
/// timestamp + versión + mensaje + ubicación al log de crashes del user-data
/// dir. El hook jamás paniquea: todo lo fallible va con `let _`/`ok()`, así
/// que un disco lleno o un HOME ausente nunca empeoran el crash original.
fn install_crash_log_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let _ = append_crash_record(info);
    }));
}

/// Formatea una línea de crash (pura y testeable; no hace I/O ni paniquea).
fn format_crash_record(
    timestamp_secs: u64,
    version: &str,
    message: &str,
    location: Option<&str>,
) -> String {
    format!(
        "[{timestamp_secs}] grafito {version} panic: {message} ({})\n",
        location.unwrap_or("ubicación desconocida")
    )
}

/// Extrae el mensaje del payload sin paniquear y acotado a
/// `MAX_CRASH_MESSAGE_BYTES` sin partir un scalar UTF-8.
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let raw: &str = if let Some(text) = info.payload().downcast_ref::<&str>() {
        text
    } else if let Some(text) = info.payload().downcast_ref::<String>() {
        text
    } else {
        return "non-string panic payload".to_owned();
    };
    truncate_message(raw)
}

/// Corta `raw` a `MAX_CRASH_MESSAGE_BYTES` en un boundary UTF-8 válido.
fn truncate_message(raw: &str) -> String {
    if raw.len() <= MAX_CRASH_MESSAGE_BYTES {
        return raw.to_owned();
    }
    let mut end = MAX_CRASH_MESSAGE_BYTES;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncado]", &raw[..end])
}

/// Ruta del log: el repo no usa crate de dirs, así que
/// `~/.local/share/grafito/crashes.log` (XDG user-data). `Err` si no hay HOME.
fn crash_log_path() -> Result<std::path::PathBuf, ()> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or(())?;
    if home.as_os_str().is_empty() {
        return Err(());
    }
    Ok(home.join(".local/share/grafito/crashes.log"))
}

/// Anexa el registro; si el archivo supera `MAX_CRASH_LOG_BYTES` lo trunca
/// (rotación acotada). Todo fallible se ignora con `let _`.
fn append_crash_record(info: &std::panic::PanicHookInfo<'_>) -> Result<(), ()> {
    let timestamp_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let record = format_crash_record(
        timestamp_secs,
        env!("CARGO_PKG_VERSION"),
        &panic_message(info),
        info.location().map(|loc| loc.to_string()).as_deref(),
    );
    let path = crash_log_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let oversized = std::fs::metadata(&path)
        .ok()
        .is_some_and(|meta| meta.len() > MAX_CRASH_LOG_BYTES);
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true);
    if oversized {
        options.truncate(true);
    } else {
        options.append(true);
    }
    let mut file = options.open(&path).map_err(|_| ())?;
    use std::io::Write as _;
    let _ = file.write_all(record.as_bytes());
    Ok(())
}

#[cfg(test)]
mod crash_hook_tests {
    use super::*;

    #[test]
    fn formato_incluye_timestamp_version_mensaje_y_ubicacion() {
        let record = format_crash_record(
            1_728_000_000,
            "1.2.35",
            "explicit panic",
            Some("src/main.rs:10:5"),
        );
        assert!(record.contains("[1728000000]"));
        assert!(record.contains("grafito 1.2.35"));
        assert!(record.contains("explicit panic"));
        assert!(record.contains("src/main.rs:10:5"));
        assert!(record.ends_with('\n'));
        let sin_ubicacion = format_crash_record(0, "0.0.0", "x", None);
        assert!(sin_ubicacion.contains("ubicación desconocida"));
    }

    #[test]
    fn truncado_acota_sin_romper_utf8() {
        assert_eq!(truncate_message("corto"), "corto");
        let emojis = "😀".repeat(500);
        let cortado = truncate_message(&emojis);
        assert!(cortado.ends_with("…[truncado]"));
        assert!(cortado.len() < emojis.len());
        // Prefijo cortado en boundary válido + sufijo ASCII: el total es UTF-8 sano.
        let Some(prefijo) = cortado.strip_suffix("…[truncado]") else {
            panic!("el truncado debe terminar en el sufijo marcado");
        };
        assert!(prefijo.is_char_boundary(prefijo.len()));
        assert!(emojis.starts_with(prefijo));
    }
}
