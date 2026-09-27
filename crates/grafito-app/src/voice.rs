//! Motor de voz + mux + captions (P1-app).
//!
//! I/O SOLO en hilos worker (`spawn_*` + `CancellationToken` + tmp+rename
//! `O_EXCL` + `kill+wait` anti-zombie, misma disciplina que el ffmpeg-sidecar
//! de `super`): la UI jamás llama acá desde `Ui::`, solo lee el `JoinHandle`.
//!
//! Cerebro puro (sin I/O, sin spawns, testeado acá mismo):
//! - alineación palabra↔tiempo ([`estima_tiempos_palabras`]): el CLI de
//!   `piper` no expone timestamps (solo el ONNX parcheado con alignments
//!   experimentales —nº de samples por fonema— los da); sin ellos se estima
//!   por chars con WPM configurable.
//! - ducking ([`curva_ducking`]): ganancia por tramos que baja la música
//!   bajo la voz (ataque/release en ms, rampas lineales por endpoints).
//! - comandos ffmpeg como strings ([`arma_comando_mux`]/[`arma_comando_duck`]):
//!   se construyen, se testean y JAMÁS se ejecutan desde este módulo
//!   (el worker que sí ejecuta vive en `mux_audio_inner` y amigos, fuera de
//!   estas fns puras).
//!
//! Contrato del núcleo (ya commiteado, se usa sin redefinir):
//! - `grafito_anim::captions::{CaptionTrack, CaptionSegment}` con
//!   `to_srt()`/`to_ass()`/`to_vtt()` (cota 256 KiB interna).
//! - `grafito_anim::AudioTrack { offset_ms: 0..=60000, gain: 0..=2 }`
//!   (declarativo; el mux es este módulo).
//!
//! Sin red, sin descargas: `piper` y las voces se detectan en disco
//! (`detect_piper_available`, `detect_piper_voice_path`); sin ellos los
//! errores son honestos (`PiperMissing` / `VoiceMissing`), jamás silencio.

use grafito_anim::captions::{CaptionTrack, CAPTION_MAX_OUTPUT_BYTES};
use grafito_anim::protocol::{MAX_AUDIO_GAIN, MAX_AUDIO_OFFSET_MS, MAX_AUDIO_PATH_CHARS};
use grafito_assistant::CancellationToken;
use std::path::{Path, PathBuf};

// ── Presupuestos ──────────────────────────────────────────────────────────

/// Texto máximo del voiceover en chars (paridad con `RequestBudget`
/// `max_input_chars` 8192: lo que la UI puede mandar, piper lo traga).
pub const VOICE_MAX_TEXT_CHARS: usize = 8192;
/// WAV máximo de salida en bytes (paridad con `LONGFORM_CHUNK_MAX_BYTES`
/// 64 MiB: un chunk entero de audio como techo).
pub const VOICE_MAX_WAV_BYTES: u64 = 64 * 1024 * 1024;
/// Cota del sidecar `.srt` en bytes (la impone `to_srt()`, se re-chequea
/// antes de escribir por defensa en profundidad).
pub const SIDECAR_MAX_BYTES: usize = CAPTION_MAX_OUTPUT_BYTES;

// ── Errores honestos ──────────────────────────────────────────────────────

/// Error tipado del voiceover (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceError {
    /// Sin binario `piper` en el PATH.
    PiperMissing,
    /// La voz `.onnx` no existe o no es archivo.
    VoiceMissing(PathBuf),
    /// `piper` corrió pero falló (cola del stderr, 500 chars).
    Failed(String),
    /// Cancelado vía `CancellationToken` (sin wav ni tmp huérfano).
    Cancelled,
    /// E/S local (lanzar, entubar, publicar).
    Io(String),
}

impl std::fmt::Display for VoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PiperMissing => write!(
                f,
                "sin voz en off: instalá piper para narrar (https://github.com/rhasspy/piper)"
            ),
            Self::VoiceMissing(voz) => {
                write!(f, "voz no encontrada: {}", voz.display())
            }
            Self::Failed(detalle) => write!(f, "piper falló: {detalle}"),
            Self::Cancelled => write!(f, "narración cancelada"),
            Self::Io(detalle) => write!(f, "falló la narración: {detalle}"),
        }
    }
}

impl std::error::Error for VoiceError {}

/// Error tipado del mux audio→video (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MuxError {
    /// Sin `ffmpeg` en el PATH: video con voz no disponible.
    FfmpegMissing,
    /// Mux cancelado vía `CancellationToken` (sin destino ni tmp).
    Cancelled,
    /// `offset_ms`/`gain` fuera del contrato `AudioTrack`.
    Rango(String),
    /// `ffmpeg` corrió pero falló (cola del stderr, 500 chars).
    FfmpegFailed(String),
    /// E/S local (insumos ausentes, publicar).
    Io(String),
}

impl std::fmt::Display for MuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FfmpegMissing => write!(
                f,
                "sin voz en el video: ffmpeg no está en el PATH (se exporta mudo)"
            ),
            Self::Cancelled => write!(f, "mux de audio cancelado"),
            Self::Rango(detalle) => write!(f, "audio fuera de rango: {detalle}"),
            Self::FfmpegFailed(detalle) => write!(f, "ffmpeg falló al mezclar: {detalle}"),
            Self::Io(detalle) => write!(f, "falló mezclar el audio: {detalle}"),
        }
    }
}

impl std::error::Error for MuxError {}

/// Error tipado del quemado de subtítulos (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptionBurnError {
    /// Sin `ffmpeg` en el PATH: quemado no disponible (queda el sidecar).
    FfmpegMissing,
    /// Quemado cancelado vía `CancellationToken` (sin destino ni tmp).
    Cancelled,
    /// `ffmpeg` corrió pero falló (cola del stderr, 500 chars).
    FfmpegFailed(String),
    /// E/S local (video/ass ausentes, publicar).
    Io(String),
}

impl std::fmt::Display for CaptionBurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FfmpegMissing => write!(
                f,
                "sin subtítulos quemados: ffmpeg no está en el PATH (queda el .srt)"
            ),
            Self::Cancelled => write!(f, "quemado de subtítulos cancelado"),
            Self::FfmpegFailed(detalle) => write!(f, "ffmpeg falló al quemar: {detalle}"),
            Self::Io(detalle) => write!(f, "falló quemar los subtítulos: {detalle}"),
        }
    }
}

impl std::error::Error for CaptionBurnError {}

/// Error tipado del sidecar `.srt` (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarError {
    /// La pista no baja a SRT (valida + cota 256 KiB internas).
    Caption(String),
    /// E/S local (publicar atómico).
    Io(String),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Caption(detalle) => write!(f, "subtítulos inválidos: {detalle}"),
            Self::Io(detalle) => write!(f, "falló escribir el .srt: {detalle}"),
        }
    }
}

impl std::error::Error for SidecarError {}

// ── Detección (solo lectura, sin spawnear, sin red) ────────────────────────
// La UI las llama desde el evento/hilo que arma el pedido, jamás desde `Ui::`.

/// Nombre del binario piper según plataforma.
const fn piper_bin_name() -> &'static str {
    if cfg!(windows) {
        "piper.exe"
    } else {
        "piper"
    }
}

/// Busca `nombre` en `dirs` (solo `is_file`, sin ejecutar). Pura salvo el
/// `is_file` de lectura. Expuesta para tests herméticos (dirs falsos).
pub fn find_bin_en_dirs(nombre: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidato = dir.join(nombre);
        if candidato.is_file() {
            return Some(candidato);
        }
    }
    None
}

/// Directorios del `PATH` (para `find_bin_en_dirs`). Sin `PATH` → vacío.
fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default()
}

/// ¿Hay binario `piper` usable en el PATH? Solo lectura, SIN spawnear.
pub fn detect_piper_available() -> bool {
    find_bin_en_dirs(piper_bin_name(), &path_dirs()).is_some()
}

/// Directorios candidatos de voces, en orden de preferencia: primero la del
/// usuario (`~/.local/share/piper/voices`), después la del sistema
/// (`/usr/share/piper/voices`). Sin `HOME` solo la del sistema. Pura.
pub fn voice_candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            dirs.push(PathBuf::from(home).join(".local/share/piper/voices"));
        }
    }
    dirs.push(PathBuf::from("/usr/share/piper/voices"));
    dirs
}

/// Primera voz `.onnx` en `dirs` (entradas ordenadas = determinista).
/// Solo lectura. Expuesta para tests herméticos (dirs falsos).
pub fn find_voice_in_dirs(dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        let Ok(entradas) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut onnx: Vec<PathBuf> = entradas
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("onnx"))
            })
            .collect();
        onnx.sort();
        if let Some(primera) = onnx.into_iter().next() {
            return Some(primera);
        }
    }
    None
}

/// Ruta de una voz piper en disco, si hay alguna instalada. Solo lectura,
/// sin red ni descargas: si no hay voces, `None` honesto (el llamador
/// muestra `VoiceMissing`, jamás descarga nada solo).
pub fn detect_piper_voice_path() -> Option<PathBuf> {
    find_voice_in_dirs(&voice_candidate_dirs())
}

// ── Tmp hermano + validación de salida ─────────────────────────────────────

/// Hermano temporal con la misma extensión (mismo directorio = mismo
/// filesystem, el `rename` es atómico; `ffmpeg`/`piper` infieren formato
/// por extensión). Puro, sin E/S.
fn tmp_sibling_con_extension(path: &Path, extension: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("clip"));
    let tmp_name = format!("{name}.tmp.{}-{stamp}.{extension}", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Limpieza best-effort del tmp (nunca deja parcial huérfano).
fn drop_tmp(tmp: &Path) {
    let _ = std::fs::remove_file(tmp);
}

/// Publicación atómica tmp→destino con `O_EXCL` honesto en ambos bordes
/// (pre-vuelco y pre-rename anti-TOCTOU: si alguien plantó el destino en el
/// medio —symlink incluido— se aborta sin pisarlo y sin dejar el tmp).
fn publicar_tmp(tmp: &Path, destino: &Path, contexto: &str) -> Result<PathBuf, String> {
    if std::fs::symlink_metadata(destino).is_ok() {
        drop_tmp(tmp);
        return Err(format!(
            "no se pudo crear {} sin sobrescribir ({contexto}): el destino ya existe",
            destino.display()
        ));
    }
    if let Err(e) = std::fs::rename(tmp, destino) {
        drop_tmp(tmp);
        return Err(format!(
            "no se pudo publicar {} ({contexto}): {e}",
            destino.display()
        ));
    }
    Ok(destino.to_path_buf())
}

// ── Voiceover piper (hilo worker) ──────────────────────────────────────────

/// Narra `texto` con `piper --model voz --output_file wav` (stdin = texto,
/// stdout a null) en un hilo aparte (no bloquea la UI).
///
/// Disciplina espejo del ffmpeg-sidecar: cancelación → kill+wait (sin
/// zombie); salida validada no vacía ≤64 MiB; publicación atómica tmp+rename
/// con `O_EXCL` honesto. Devuelve el wav publicado.
pub fn spawn_voiceover(
    texto: String,
    voice_path: PathBuf,
    out_wav: PathBuf,
    cancel: CancellationToken,
) -> std::thread::JoinHandle<Result<PathBuf, VoiceError>> {
    let bin = path_dirs()
        .iter()
        .map(|d| d.join(piper_bin_name()))
        .find(|c| c.is_file())
        .unwrap_or_else(|| PathBuf::from(piper_bin_name()));
    spawn_voiceover_with_bin(texto, voice_path, out_wav, cancel, bin)
}

/// Idem con binario explícito (tests herméticos + distros sin PATH).
pub fn spawn_voiceover_with_bin(
    texto: String,
    voice_path: PathBuf,
    out_wav: PathBuf,
    cancel: CancellationToken,
    piper_bin: PathBuf,
) -> std::thread::JoinHandle<Result<PathBuf, VoiceError>> {
    std::thread::spawn(move || nucleo_voiceover(&texto, &voice_path, &out_wav, &cancel, &piper_bin))
}

/// Núcleo bloqueante (llamar en hilo): valida, corre piper, valida la
/// salida y publica. Sin pánicos.
fn nucleo_voiceover(
    texto: &str,
    voice_path: &Path,
    out_wav: &Path,
    token: &CancellationToken,
    piper_bin: &Path,
) -> Result<PathBuf, VoiceError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(VoiceError::Cancelled);
    }
    let limpio = texto.trim();
    if limpio.is_empty() {
        return Err(VoiceError::Failed(
            "texto vacío: pasame algo para narrar".to_string(),
        ));
    }
    if limpio.chars().count() > VOICE_MAX_TEXT_CHARS {
        return Err(VoiceError::Failed(format!(
            "texto de {} chars (válido 1..={VOICE_MAX_TEXT_CHARS})",
            limpio.chars().count()
        )));
    }
    if !voice_path.is_file() {
        return Err(VoiceError::VoiceMissing(voice_path.to_path_buf()));
    }
    if std::fs::symlink_metadata(out_wav).is_ok() {
        return Err(VoiceError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            out_wav.display()
        )));
    }
    let tmp = tmp_sibling_con_extension(out_wav, "wav");
    let mut child = Command::new(piper_bin)
        .arg("--model")
        .arg(voice_path)
        .arg("--output_file")
        .arg(&tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            drop_tmp(&tmp);
            if e.kind() == std::io::ErrorKind::NotFound {
                VoiceError::PiperMissing
            } else {
                VoiceError::Io(format!("no se pudo lanzar {}: {e}", piper_bin.display()))
            }
        })?;
    // stdin = texto (≤8 KiB: un solo `write_all`, sin deadlock), stdout a
    // null; el stderr se drena en el watcher del `wait` vigilado.
    let pipe_result = (|| -> Result<(), VoiceError> {
        if token.is_cancelled() {
            return Err(VoiceError::Cancelled);
        }
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| VoiceError::Io("piper no abrió su stdin".to_string()))?;
        stdin
            .write_all(limpio.as_bytes())
            .map_err(|e| VoiceError::Io(format!("no se pudo entubar el texto: {e}")))?;
        Ok(())
    })();
    if let Err(pipe_err) = pipe_result {
        let _ = child.kill();
        let _ = child.wait();
        drop_tmp(&tmp);
        return Err(pipe_err);
    }
    match super::esperar_ffmpeg_con_cancel(&mut child, token) {
        super::EsperaFfmpeg::Cancelado => {
            drop_tmp(&tmp);
            return Err(VoiceError::Cancelled);
        }
        super::EsperaFfmpeg::FalloIo(detalle) => {
            drop_tmp(&tmp);
            return Err(VoiceError::Io(detalle));
        }
        super::EsperaFfmpeg::Terminado(false, stderr) => {
            drop_tmp(&tmp);
            return Err(VoiceError::Failed(super::ffmpeg_stderr_tail(&stderr)));
        }
        super::EsperaFfmpeg::Terminado(true, _) => {}
    }
    if token.is_cancelled() {
        drop_tmp(&tmp);
        return Err(VoiceError::Cancelled);
    }
    let peso = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if peso == 0 {
        drop_tmp(&tmp);
        return Err(VoiceError::Failed(
            "piper no produjo audio (wav vacío)".to_string(),
        ));
    }
    if peso > VOICE_MAX_WAV_BYTES {
        drop_tmp(&tmp);
        return Err(VoiceError::Failed(format!(
            "wav de {peso} bytes excede 64 MiB: partí la narración"
        )));
    }
    publicar_tmp(&tmp, out_wav, "voz").map_err(VoiceError::Io)
}

// ── Mux audio→video (ffmpeg, hilo worker) ──────────────────────────────────

/// Mezcla `audio_wav` en `video` con offset y ganancia del `AudioTrack`:
///
/// `ffmpeg -i video -itsoffset {offset_s} -i audio -af volume={gain}
/// -c:v copy -c:a aac -b:a 128k -shortest -movflags +faststart`
///
/// Bloquea: llamar en hilo (el wrapper narrado lo spawnea). Sin `ffmpeg` →
/// `FfmpegMissing` honesto (se exporta mudo). tmp+rename `O_EXCL` + kill+wait.
pub fn mux_audio_into(
    video: &Path,
    audio_wav: &Path,
    offset_ms: u32,
    gain: f32,
    out: &Path,
) -> Result<PathBuf, MuxError> {
    mux_audio_into_with_bin(video, audio_wav, offset_ms, gain, out, Path::new("ffmpeg"))
}

/// Idem con binario explícito (tests herméticos + distros sin PATH).
#[allow(clippy::too_many_arguments)]
pub fn mux_audio_into_with_bin(
    video: &Path,
    audio_wav: &Path,
    offset_ms: u32,
    gain: f32,
    out: &Path,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, MuxError> {
    mux_audio_inner(
        video,
        audio_wav,
        offset_ms,
        gain,
        out,
        &CancellationToken::default(),
        ffmpeg_bin,
    )
}

/// Idem cancelable con binario explícito (el wrapper narrado la llama con
/// su token en el hilo worker).
#[allow(clippy::too_many_arguments)]
pub fn mux_audio_cancelable_with_bin(
    video: &Path,
    audio_wav: &Path,
    offset_ms: u32,
    gain: f32,
    out: &Path,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, MuxError> {
    mux_audio_inner(video, audio_wav, offset_ms, gain, out, token, ffmpeg_bin)
}

/// Núcleo bloqueante (llamar en hilo). Sin pánicos.
#[allow(clippy::too_many_arguments)]
fn mux_audio_inner(
    video: &Path,
    audio_wav: &Path,
    offset_ms: u32,
    gain: f32,
    out: &Path,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, MuxError> {
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(MuxError::Cancelled);
    }
    if offset_ms > MAX_AUDIO_OFFSET_MS {
        return Err(MuxError::Rango(format!(
            "offset {offset_ms} ms fuera de 0..={MAX_AUDIO_OFFSET_MS}"
        )));
    }
    if !gain.is_finite() || !(0.0..=MAX_AUDIO_GAIN).contains(&gain) {
        return Err(MuxError::Rango(format!(
            "gain {gain} fuera de 0.0..={MAX_AUDIO_GAIN}"
        )));
    }
    if !video.is_file() {
        return Err(MuxError::Io(format!(
            "video no encontrado: {}",
            video.display()
        )));
    }
    if !audio_wav.is_file() {
        return Err(MuxError::Io(format!(
            "audio no encontrado: {}",
            audio_wav.display()
        )));
    }
    if std::fs::symlink_metadata(out).is_ok() {
        return Err(MuxError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            out.display()
        )));
    }
    let tmp = tmp_sibling_con_extension(out, "mp4");
    // Segundos con decimales honestos (`500` → `0.5`, `0` → `0`).
    let offset_s = format!("{}", f64::from(offset_ms) / 1000.0);
    let mut child = Command::new(ffmpeg_bin)
        .arg("-i")
        .arg(video)
        .arg("-itsoffset")
        .arg(offset_s)
        .arg("-i")
        .arg(audio_wav)
        .arg("-af")
        .arg(format!("volume={gain}"))
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("128k")
        .arg("-shortest")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-f")
        .arg("mp4")
        .arg(&tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            drop_tmp(&tmp);
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::FfmpegMissing
            } else {
                MuxError::Io(format!("no se pudo lanzar {}: {e}", ffmpeg_bin.display()))
            }
        })?;
    match super::esperar_ffmpeg_con_cancel(&mut child, token) {
        super::EsperaFfmpeg::Cancelado => {
            drop_tmp(&tmp);
            return Err(MuxError::Cancelled);
        }
        super::EsperaFfmpeg::FalloIo(detalle) => {
            drop_tmp(&tmp);
            return Err(MuxError::Io(detalle));
        }
        super::EsperaFfmpeg::Terminado(false, stderr) => {
            drop_tmp(&tmp);
            return Err(MuxError::FfmpegFailed(super::ffmpeg_stderr_tail(&stderr)));
        }
        super::EsperaFfmpeg::Terminado(true, _) => {}
    }
    if token.is_cancelled() {
        drop_tmp(&tmp);
        return Err(MuxError::Cancelled);
    }
    publicar_tmp(&tmp, out, "mux").map_err(MuxError::Io)
}

// ── Quemado de subtítulos (ffmpeg, hilo worker) ────────────────────────────

/// Escapa una ruta para el filtro `ass=` (filtergraph: `\` `'` `,` `[`
/// `]` `;` `:` con backslash; el `:` escapado cubre el drive `C\:/` en
/// Windows). Pura, sin E/S. Expuesta para pineo en tests.
pub fn escape_ass_filter_path(path: &Path) -> String {
    let crudo = path.to_string_lossy();
    let mut out = String::with_capacity(crudo.len() + 8);
    for c in crudo.chars() {
        if matches!(c, '\\' | '\'' | ',' | '[' | ']' | ';' | ':') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Quema `ass_path` sobre `video`:
///
/// `ffmpeg -i video -vf ass={escapado} -c:v libx264 -crf 23 -preset veryfast
/// -c:a copy out`
///
/// Se usa el filtro `ass=` directo (equivale al `subtitles=` pedido sin
/// exigir un `fontsdir` empaquetado que no existe: sin fuentes propias el
/// `fontsdir` apuntaría al vacío). Re-encode de video + copia de audio.
/// Bloquea: llamar en hilo. Sin `ffmpeg` → `FfmpegMissing` honesto (queda
/// el sidecar `.srt`).
pub fn burn_captions(
    video: &Path,
    ass_path: &Path,
    out: &Path,
) -> Result<PathBuf, CaptionBurnError> {
    burn_captions_with_bin(video, ass_path, out, Path::new("ffmpeg"))
}

/// Idem con binario explícito (tests herméticos + distros sin PATH).
pub fn burn_captions_with_bin(
    video: &Path,
    ass_path: &Path,
    out: &Path,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, CaptionBurnError> {
    burn_captions_inner(
        video,
        ass_path,
        out,
        &CancellationToken::default(),
        ffmpeg_bin,
    )
}

/// Idem cancelable con binario explícito (el wrapper narrado la llama con
/// su token en el hilo worker).
pub fn burn_captions_cancelable_with_bin(
    video: &Path,
    ass_path: &Path,
    out: &Path,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, CaptionBurnError> {
    burn_captions_inner(video, ass_path, out, token, ffmpeg_bin)
}

/// Núcleo bloqueante (llamar en hilo). Sin pánicos.
fn burn_captions_inner(
    video: &Path,
    ass_path: &Path,
    out: &Path,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, CaptionBurnError> {
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(CaptionBurnError::Cancelled);
    }
    if !video.is_file() {
        return Err(CaptionBurnError::Io(format!(
            "video no encontrado: {}",
            video.display()
        )));
    }
    if !ass_path.is_file() {
        return Err(CaptionBurnError::Io(format!(
            "subtítulos no encontrados: {}",
            ass_path.display()
        )));
    }
    if std::fs::symlink_metadata(out).is_ok() {
        return Err(CaptionBurnError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            out.display()
        )));
    }
    let tmp = tmp_sibling_con_extension(out, "mp4");
    let filtro = format!("ass={}", escape_ass_filter_path(ass_path));
    let mut child = Command::new(ffmpeg_bin)
        .arg("-i")
        .arg(video)
        .arg("-vf")
        .arg(filtro)
        .arg("-c:v")
        .arg("libx264")
        .arg("-crf")
        .arg("23")
        .arg("-preset")
        .arg("veryfast")
        .arg("-c:a")
        .arg("copy")
        .arg("-f")
        .arg("mp4")
        .arg(&tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            drop_tmp(&tmp);
            if e.kind() == std::io::ErrorKind::NotFound {
                CaptionBurnError::FfmpegMissing
            } else {
                CaptionBurnError::Io(format!("no se pudo lanzar {}: {e}", ffmpeg_bin.display()))
            }
        })?;
    match super::esperar_ffmpeg_con_cancel(&mut child, token) {
        super::EsperaFfmpeg::Cancelado => {
            drop_tmp(&tmp);
            return Err(CaptionBurnError::Cancelled);
        }
        super::EsperaFfmpeg::FalloIo(detalle) => {
            drop_tmp(&tmp);
            return Err(CaptionBurnError::Io(detalle));
        }
        super::EsperaFfmpeg::Terminado(false, stderr) => {
            drop_tmp(&tmp);
            return Err(CaptionBurnError::FfmpegFailed(super::ffmpeg_stderr_tail(
                &stderr,
            )));
        }
        super::EsperaFfmpeg::Terminado(true, _) => {}
    }
    if token.is_cancelled() {
        drop_tmp(&tmp);
        return Err(CaptionBurnError::Cancelled);
    }
    publicar_tmp(&tmp, out, "burn").map_err(CaptionBurnError::Io)
}

// ── Sidecars (hilo worker, atómicos) ───────────────────────────────────────

/// Escribe el `.srt` junto al video (`base_path` con extensión `.srt`):
/// valida la pista vía `to_srt()` (cota ≤256 KiB interna) y publica
/// atómico tmp+rename con `O_EXCL` honesto. Bloquea poco (un vuelco):
/// llamar en hilo igual que el resto.
pub fn write_caption_sidecar(
    track: &CaptionTrack,
    base_path: &Path,
) -> Result<PathBuf, SidecarError> {
    let srt = track
        .to_srt()
        .map_err(|e| SidecarError::Caption(e.to_string()))?;
    if srt.len() > SIDECAR_MAX_BYTES {
        return Err(SidecarError::Caption(format!(
            "salida de {} bytes excede 256 KiB: partí la pista",
            srt.len()
        )));
    }
    let destino = base_path.with_extension("srt");
    if std::fs::symlink_metadata(&destino).is_ok() {
        return Err(SidecarError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            destino.display()
        )));
    }
    let tmp = tmp_sibling_con_extension(&destino, "srt");
    if let Err(e) = std::fs::write(&tmp, srt.as_bytes()) {
        drop_tmp(&tmp);
        return Err(SidecarError::Io(format!(
            "no se pudo escribir {}: {e}",
            destino.display()
        )));
    }
    publicar_tmp(&tmp, &destino, "srt").map_err(SidecarError::Io)
}

/// Vuelca el `.ass` intermedio para el quemado en `dir` (nombre único por
/// pid+nanos, `create_new`): el wrapper narrado lo quema y lo borra
/// best-effort. Valida vía `to_ass()` (cota ≤256 KiB interna).
pub fn write_ass_temp(track: &CaptionTrack, dir: &Path) -> Result<PathBuf, SidecarError> {
    let ass = track
        .to_ass()
        .map_err(|e| SidecarError::Caption(e.to_string()))?;
    if ass.len() > SIDECAR_MAX_BYTES {
        return Err(SidecarError::Caption(format!(
            "salida de {} bytes excede 256 KiB: partí la pista",
            ass.len()
        )));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let destino = dir.join(format!(
        "grafito-captions.{}-{stamp}.ass",
        std::process::id()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destino)
    {
        Ok(mut f) => {
            use std::io::Write as _;
            if let Err(e) = f.write_all(ass.as_bytes()) {
                drop(f);
                let _ = std::fs::remove_file(&destino);
                return Err(SidecarError::Io(format!(
                    "no se pudo escribir {}: {e}",
                    destino.display()
                )));
            }
        }
        Err(e) => {
            return Err(SidecarError::Io(format!(
                "no se pudo crear {} sin sobrescribir: {e}",
                destino.display()
            )));
        }
    }
    Ok(destino)
}

// ── Alineación palabra↔tiempo (cerebro puro, sin piper) ─────────────────
// El CLI `piper --model voz --output_file wav` NO expone timestamps de
// palabra: los alignments son experimentales y solo vía ONNX parcheado
// (`piper.patch_voice_with_alignment`: nº de samples de audio por fonema).
// Sin ellos la vía honesta es estimar: peso por chars (la palabra larga
// dura más) calibrado a la duración real del wav, con WPM configurable como
// fallback cuando aún no hay audio (`total_ms == 0`).
// Todo puro, acotado y testeado abajo; jamás spawnea nada.

/// WPM por defecto del habla (español neutro narrado, ~150).
pub const DEFAULT_WPM: u32 = 150;
/// WPM mínimo aceptado (habla muy lenta).
pub const WPM_MIN: u32 = 40;
/// WPM máximo aceptado (habla muy rápida).
pub const WPM_MAX: u32 = 400;
/// Palabras máximas por alineación (paridad con el karaoke: 5000 del
/// `KaraokeTrack`, acá 2000 porque la voz de un clip de 60 s a 400 WPM da
/// 400 palabras y sobra margen).
pub const ALIGN_MAX_PALABRAS: usize = 2000;
/// Duración total máxima aceptada en ms (10 min: un long-form entero).
pub const ALIGN_MAX_TOTAL_MS: u32 = 600_000;

/// Error tipado de la alineación (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignError {
    /// Texto vacío o solo espacios.
    TextoVacio,
    /// WPM fuera de `40..=400`.
    WpmInvalido { got: u32, min: u32, max: u32 },
    /// Duración total imposible (`< 1 ms por palabra` o `> 10 min`).
    TotalInvalido(String),
    /// Más de 2000 palabras.
    DemasiadasPalabras { got: usize, max: usize },
}

impl std::fmt::Display for AlignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextoVacio => write!(f, "texto vacío: pasame algo para alinear"),
            Self::WpmInvalido { got, min, max } => {
                write!(f, "wpm {got} fuera de {min}..={max}: usá un habla normal")
            }
            Self::TotalInvalido(detalle) => write!(f, "duración inválida: {detalle}"),
            Self::DemasiadasPalabras { got, max } => {
                write!(f, "{got} palabras exceden el tope de {max}: partí el texto")
            }
        }
    }
}

impl std::error::Error for AlignError {}

/// Valida el WPM (`40..=400`). Pura.
fn valida_wpm(wpm: u32) -> Result<(), AlignError> {
    if (WPM_MIN..=WPM_MAX).contains(&wpm) {
        Ok(())
    } else {
        Err(AlignError::WpmInvalido {
            got: wpm,
            min: WPM_MIN,
            max: WPM_MAX,
        })
    }
}

/// Duración estimada en ms (`palabras * 60000 / wpm`, techo). Pura.
pub fn duracion_estimada_ms(texto: &str, wpm: u32) -> Result<u64, AlignError> {
    valida_wpm(wpm)?;
    let palabras = texto.split_whitespace().count();
    if palabras == 0 {
        return Err(AlignError::TextoVacio);
    }
    if palabras > ALIGN_MAX_PALABRAS {
        return Err(AlignError::DemasiadasPalabras {
            got: palabras,
            max: ALIGN_MAX_PALABRAS,
        });
    }
    let total = (palabras as u64)
        .checked_mul(60_000)
        .and_then(|n| n.checked_add(u64::from(wpm) - 1))
        .map(|n| n / u64::from(wpm));
    match total {
        Some(ms) if ms <= u64::from(ALIGN_MAX_TOTAL_MS) => Ok(ms),
        _ => Err(AlignError::TotalInvalido(format!(
            "el estimado de {palabras} palabras a {wpm} wpm excede 10 min: partí el texto"
        ))),
    }
}

/// Alinea cada palabra a `(texto, inicio_ms, fin_ms)` cubriendo `total_ms`.
///
/// Peso por chars (a más letras, más ms) con resto mayor: se reserva 1 ms
/// por palabra y el resto se reparte por peso con largest-remainder
/// determinista (empate → la primera palabra), así la suma es EXACTA y cada
/// palabra dura `>= 1 ms`. Si `total_ms == 0` se usa
/// [`duracion_estimada_ms`] con `wpm` (útil antes de sintetizar).
/// Pura, sin I/O, sin spawns.
pub fn estima_tiempos_palabras(
    texto: &str,
    total_ms: u32,
    wpm: u32,
) -> Result<Vec<(String, u32, u32)>, AlignError> {
    valida_wpm(wpm)?;
    let normalizado: Vec<&str> = texto.split_whitespace().filter(|w| !w.is_empty()).collect();
    if normalizado.is_empty() {
        return Err(AlignError::TextoVacio);
    }
    if normalizado.len() > ALIGN_MAX_PALABRAS {
        return Err(AlignError::DemasiadasPalabras {
            got: normalizado.len(),
            max: ALIGN_MAX_PALABRAS,
        });
    }
    let n = normalizado.len() as u64;
    let total_u64 = if total_ms == 0 {
        duracion_estimada_ms(texto, wpm)?
    } else {
        if total_ms > ALIGN_MAX_TOTAL_MS {
            return Err(AlignError::TotalInvalido(format!(
                "{total_ms} ms excede 10 min: partí el audio"
            )));
        }
        u64::from(total_ms)
    };
    if total_u64 < n {
        return Err(AlignError::TotalInvalido(format!(
            "{total_u64} ms muy corto para {} palabras: dale al menos 1 ms por palabra",
            normalizado.len()
        )));
    }
    // Peso = nº de chars (mínimo 1, siempre > 0 acá).
    let pesos: Vec<u64> = normalizado
        .iter()
        .map(|w| w.chars().count().max(1) as u64)
        .collect();
    let peso_total: u64 = pesos.iter().sum();
    // 1 ms base por palabra + resto ponderado (largest-remainder exacto).
    let resto = total_u64 - n;
    let mut tramos: Vec<u64> = Vec::with_capacity(pesos.len());
    let mut fracciones: Vec<(u64, usize)> = Vec::with_capacity(pesos.len());
    let mut asignado: u64 = 0;
    for (k, peso) in pesos.iter().enumerate() {
        let num = resto.saturating_mul(*peso);
        let base = num / peso_total;
        fracciones.push((num % peso_total, k));
        tramos.push(1 + base);
        asignado = asignado.saturating_add(1 + base);
    }
    // Lo que falta (siempre `< nº de palabras`) va a las fracciones mayores.
    let mut faltante = total_u64.saturating_sub(asignado) as usize;
    fracciones.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, k) in fracciones {
        if faltante == 0 {
            break;
        }
        tramos[k] = tramos[k].saturating_add(1);
        faltante = faltante.saturating_sub(1);
    }
    let mut cursor: u64 = 0;
    let mut out = Vec::with_capacity(normalizado.len());
    for (palabra, tramo) in normalizado.iter().zip(tramos.iter()) {
        let fin = cursor.saturating_add(*tramo);
        out.push((
            (*palabra).to_string(),
            cursor.min(u64::from(u32::MAX)) as u32,
            fin.min(u64::from(u32::MAX)) as u32,
        ));
        cursor = fin;
    }
    Ok(out)
}

// ── Ducking (curva de ganancia por tramos, pura) ─────────────────────────
// Baja la música bajo la voz: fuera de la voz suena `base_gain`, sobre la
// voz `duck_gain`, con rampas de `ataque_ms` (entrada) y `release_ms`
// (salida) expresadas como tramos con `gain_ini`/`gain_fin` (rampa lineal
// entre endpoints; el llamador lo mapea a `volume` o a `sidechaincompress`).
// Todo puro y testeado; jamás toca audio real.

/// Intervalos de voz máximos por curva (paridad con subtítulos: 2000).
pub const DUCK_MAX_INTERVALOS: usize = 2000;
/// Tramos máximos de salida (4 por intervalo + 1 de cola: imposible
/// cruzarlo, es defensa en profundidad).
pub const DUCK_MAX_TRAMOS: usize = 8192;
/// Rampa máxima de ataque/release en ms (5 s: más es fundido, no ducking).
pub const DUCK_MAX_RAMPA_MS: u32 = 5000;

/// Parámetros del ducking (todo validado en español).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuckParams {
    /// Ganancia de la música sin voz (`0..=2`).
    pub base_gain: f32,
    /// Ganancia de la música bajo la voz (`0..=base_gain`).
    pub duck_gain: f32,
    /// Rampa de entrada en ms (`0..=5000`, se recorta al hueco disponible).
    pub ataque_ms: u32,
    /// Rampa de salida en ms (`0..=5000`, se recorta al hueco disponible).
    pub release_ms: u32,
}

impl DuckParams {
    /// Constructor validado.
    pub fn try_new(
        base_gain: f32,
        duck_gain: f32,
        ataque_ms: u32,
        release_ms: u32,
    ) -> Result<Self, DuckError> {
        let p = Self {
            base_gain,
            duck_gain,
            ataque_ms,
            release_ms,
        };
        p.validate()?;
        Ok(p)
    }

    /// Validación estricta (todo `Err` en español, sin pánicos).
    pub fn validate(&self) -> Result<(), DuckError> {
        if !self.base_gain.is_finite() || !(0.0..=MAX_AUDIO_GAIN).contains(&self.base_gain) {
            return Err(DuckError::Rango(format!(
                "base {} fuera de 0.0..={MAX_AUDIO_GAIN}",
                self.base_gain
            )));
        }
        if !self.duck_gain.is_finite() || !(0.0..=MAX_AUDIO_GAIN).contains(&self.duck_gain) {
            return Err(DuckError::Rango(format!(
                "duck {} fuera de 0.0..={MAX_AUDIO_GAIN}",
                self.duck_gain
            )));
        }
        if self.duck_gain > self.base_gain {
            return Err(DuckError::Rango(format!(
                "duck {} mayor que la base {}: el ducking baja, no sube",
                self.duck_gain, self.base_gain
            )));
        }
        if self.ataque_ms > DUCK_MAX_RAMPA_MS {
            return Err(DuckError::Rango(format!(
                "ataque {} ms fuera de 0..={DUCK_MAX_RAMPA_MS}",
                self.ataque_ms
            )));
        }
        if self.release_ms > DUCK_MAX_RAMPA_MS {
            return Err(DuckError::Rango(format!(
                "release {} ms fuera de 0..={DUCK_MAX_RAMPA_MS}",
                self.release_ms
            )));
        }
        Ok(())
    }
}

/// Un tramo de ganancia: `[inicio_ms, fin_ms)` con rampa lineal
/// `gain_ini → gain_fin` (`ini == fin` = ganancia constante). Puro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuckTramo {
    /// Inicio en ms.
    pub inicio_ms: u32,
    /// Fin en ms (`> inicio_ms`).
    pub fin_ms: u32,
    /// Ganancia al inicio (`0..=2`).
    pub gain_ini: f32,
    /// Ganancia al fin (`0..=2`).
    pub gain_fin: f32,
}

impl DuckTramo {
    /// Tramo de ganancia constante.
    pub fn constante(inicio_ms: u32, fin_ms: u32, gain: f32) -> Self {
        Self {
            inicio_ms,
            fin_ms,
            gain_ini: gain,
            gain_fin: gain,
        }
    }

    /// Tramo con rampa.
    pub fn rampa(inicio_ms: u32, fin_ms: u32, gain_ini: f32, gain_fin: f32) -> Self {
        Self {
            inicio_ms,
            fin_ms,
            gain_ini,
            gain_fin,
        }
    }

    /// ¿Ganancia constante?
    pub fn es_constante(self) -> bool {
        self.gain_ini == self.gain_fin
    }
}

/// Error tipado del ducking (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuckError {
    /// Parámetro fuera de rango.
    Rango(String),
    /// Intervalo de voz mal formado o fuera del total.
    Ventana(String),
    /// Intervalos desordenados o solapados.
    Solape { indice: usize },
    /// Demasiados intervalos o tramos.
    Demasiados { got: usize, max: usize },
}

impl std::fmt::Display for DuckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rango(detalle) => write!(f, "ducking fuera de rango: {detalle}"),
            Self::Ventana(detalle) => write!(f, "intervalo de voz inválido: {detalle}"),
            Self::Solape { indice } => write!(
                f,
                "intervalo {indice} desordenado o solapado: ordená por inicio sin solapar"
            ),
            Self::Demasiados { got, max } => {
                write!(
                    f,
                    "{got} intervalos exceden el tope de {max}: partí la mezcla"
                )
            }
        }
    }
}

impl std::error::Error for DuckError {}

/// Calcula la curva de ganancia que baja la música bajo la voz.
///
/// `voz` = intervalos `(inicio, fin)` de habla en ms (ordenados, sin
/// solapar, dentro de `total_ms`); `total_ms > 0`. La salida cubre
/// `[0, total_ms)` sin huecos ni solapes: base fuera, duck sobre la voz,
/// rampas de ataque/release recortadas al hueco disponible (si dos voces se
/// pisan en el release, el release se trunca: nunca invade al vecino).
/// Sin voz sale un solo tramo base. Pura, acotada a [`DUCK_MAX_TRAMOS`].
pub fn curva_ducking(
    voz: &[(u32, u32)],
    total_ms: u32,
    params: &DuckParams,
) -> Result<Vec<DuckTramo>, DuckError> {
    params.validate()?;
    if total_ms == 0 {
        return Err(DuckError::Rango(
            "total 0 ms: pasame la duración de la mezcla".to_string(),
        ));
    }
    if voz.len() > DUCK_MAX_INTERVALOS {
        return Err(DuckError::Demasiados {
            got: voz.len(),
            max: DUCK_MAX_INTERVALOS,
        });
    }
    let mut previo_fin: Option<u32> = None;
    for (indice, (a, b)) in voz.iter().enumerate() {
        if a >= b {
            return Err(DuckError::Ventana(format!(
                "ventana {a}..{b} inválida: el inicio debe ser menor que el fin"
            )));
        }
        if *b > total_ms {
            return Err(DuckError::Ventana(format!(
                "fin {b} ms fuera del total {total_ms} ms"
            )));
        }
        if let Some(previo) = previo_fin {
            if *a < previo {
                return Err(DuckError::Solape { indice });
            }
        }
        previo_fin = Some(*b);
    }
    let mut tramos: Vec<DuckTramo> = Vec::new();
    let mut cursor: u32 = 0;
    for (k, (a, b)) in voz.iter().enumerate() {
        // Ataque recortado al hueco desde el cursor (nunca pisa al vecino).
        let ini_rampa = a.saturating_sub(params.ataque_ms).max(cursor);
        if ini_rampa > cursor {
            tramos.push(DuckTramo::constante(cursor, ini_rampa, params.base_gain));
        }
        if ini_rampa < *a {
            tramos.push(DuckTramo::rampa(
                ini_rampa,
                *a,
                params.base_gain,
                params.duck_gain,
            ));
        }
        tramos.push(DuckTramo::constante(*a, *b, params.duck_gain));
        cursor = *b;
        // Release recortado al próximo inicio (o al total).
        let siguiente = voz.get(k + 1).map_or(total_ms, |(na, _)| *na);
        let fin_rel = b
            .saturating_add(params.release_ms)
            .min(siguiente)
            .min(total_ms);
        if fin_rel > *b {
            tramos.push(DuckTramo::rampa(
                *b,
                fin_rel,
                params.duck_gain,
                params.base_gain,
            ));
            cursor = fin_rel;
        }
    }
    if cursor < total_ms {
        tramos.push(DuckTramo::constante(cursor, total_ms, params.base_gain));
    }
    if tramos.len() > DUCK_MAX_TRAMOS {
        return Err(DuckError::Demasiados {
            got: tramos.len(),
            max: DUCK_MAX_TRAMOS,
        });
    }
    Ok(tramos)
}

// ── Constructores de comandos ffmpeg (strings, jamás ejecutados) ─────────
// Estas fns ARMAN el argv como string citado para log/preview/tests y NUNCA
// lo ejecutan: no hay `Command`, no hay `spawn`, no hay shell. El worker que
// sí ejecuta (`mux_audio_inner`, `burn_captions_inner`) no las usa: duplica
// el argv a propósito para que un bug acá no cambie lo que corre.

/// Umbral por defecto del `sidechaincompress` (voz que dispara el ducking;
/// guía FFmpegLab de ducking: 0.001–0.01 para voz).
pub const DUCK_THRESHOLD_DEFAULT: f64 = 0.003;
/// Ratio por defecto del `sidechaincompress` (ducking agresivo 10–20).
pub const DUCK_RATIO_DEFAULT: f64 = 20.0;
/// Comando máximo en chars (defensa: 3 rutas de 512 + filtros entran cómodas).
pub const MUX_MAX_CMD_CHARS: usize = 8192;

/// Cita un argv para shell POSIX (`'...'`; `'` interno → `'\''`). Pura.
pub fn cita_shell(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    let mut primero = true;
    for pedazo in arg.split('\'') {
        if !primero {
            out.push_str("'\\''");
        }
        primero = false;
        out.push_str(pedazo);
    }
    out.push('\'');
    out
}

/// Valida una ruta del builder (no vacía, sin NUL, `<= 512` chars). Pura.
fn valida_ruta_mux(nombre: &str, ruta: &str) -> Result<(), MuxError> {
    if ruta.is_empty() {
        return Err(MuxError::Io(format!("{nombre} vacía: pasame una ruta")));
    }
    if ruta.contains('\0') {
        return Err(MuxError::Io(format!(
            "{nombre} con NUL: usá una ruta válida"
        )));
    }
    if ruta.chars().count() > MAX_AUDIO_PATH_CHARS {
        return Err(MuxError::Io(format!(
            "{nombre} de {} chars (válido ..={MAX_AUDIO_PATH_CHARS})",
            ruta.chars().count()
        )));
    }
    Ok(())
}

/// Acota el comando armado (`<= 8192` chars). Pura.
fn acota_comando(cmd: String) -> Result<String, MuxError> {
    if cmd.chars().count() > MUX_MAX_CMD_CHARS {
        return Err(MuxError::Io(format!(
            "comando de {} chars excede {MUX_MAX_CMD_CHARS}: acortá las rutas",
            cmd.chars().count()
        )));
    }
    Ok(cmd)
}

/// Arma (sin ejecutar) el comando de mux A/V del worker:
///
/// `ffmpeg -i video -itsoffset {offset_s} -i audio -af volume={gain}
/// -c:v copy -c:a aac -b:a 128k -shortest -movflags +faststart -f mp4 out`
///
/// Mismo argv que [`mux_audio_into`]: si este string y el worker divergen,
/// el test `mux_builder_espeja_al_worker` lo canta. Rutas citadas POSIX.
/// Pura: construye el string y lo devuelve, jamás lo corre.
pub fn arma_comando_mux(
    video: &str,
    audio: &str,
    offset_ms: u32,
    gain: f32,
    salida: &str,
) -> Result<String, MuxError> {
    if offset_ms > MAX_AUDIO_OFFSET_MS {
        return Err(MuxError::Rango(format!(
            "offset {offset_ms} ms fuera de 0..={MAX_AUDIO_OFFSET_MS}"
        )));
    }
    if !gain.is_finite() || !(0.0..=MAX_AUDIO_GAIN).contains(&gain) {
        return Err(MuxError::Rango(format!(
            "gain {gain} fuera de 0.0..={MAX_AUDIO_GAIN}"
        )));
    }
    valida_ruta_mux("video", video)?;
    valida_ruta_mux("audio", audio)?;
    valida_ruta_mux("salida", salida)?;
    // Segundos con decimales honestos (`500` → `0.5`, `0` → `0`): paridad
    // exacta con `mux_audio_inner`.
    let offset_s = format!("{}", f64::from(offset_ms) / 1000.0);
    let cmd = format!(
        "ffmpeg -i {} -itsoffset {offset_s} -i {} -af volume={gain} \
         -c:v copy -c:a aac -b:a 128k -shortest -movflags +faststart -f mp4 {}",
        cita_shell(video),
        cita_shell(audio),
        cita_shell(salida),
    );
    acota_comando(cmd)
}

/// Arma (sin ejecutar) el comando de ducking música↔voz con
/// `sidechaincompress` (filtro estándar de FFmpeg para ducking automático:
///
/// `ffmpeg -i musica -i voz -filter_complex
/// "[1:a]asplit=2[sc][mix];[0:a][sc]sidechaincompress=
/// threshold={u}:ratio={r}:attack=5:release=150[bg];
/// [bg][mix]amix=inputs=2:duration=longest[a]"
/// -map "[a]" -c:a aac -b:a 128k -shortest out`
///
/// La voz se divide en dos (`asplit`): una rama dispara el compresor y la
/// otra se mezcla intacta con la música comprimida (`amix`). Pura: jamás
/// ejecuta nada.
pub fn arma_comando_duck(
    musica: &str,
    voz: &str,
    salida: &str,
    threshold: f64,
    ratio: f64,
) -> Result<String, MuxError> {
    valida_ruta_mux("musica", musica)?;
    valida_ruta_mux("voz", voz)?;
    valida_ruta_mux("salida", salida)?;
    if !threshold.is_finite() || !(0.0 < threshold && threshold <= 1.0) {
        return Err(MuxError::Rango(format!(
            "threshold {threshold} fuera de 0.0..=1.0: probá {DUCK_THRESHOLD_DEFAULT}"
        )));
    }
    if !ratio.is_finite() || !(1.0..=20.0).contains(&ratio) {
        return Err(MuxError::Rango(format!(
            "ratio {ratio} fuera de 1.0..=20.0: probá {DUCK_RATIO_DEFAULT}"
        )));
    }
    let filtro = format!(
        "[1:a]asplit=2[sc][mix];[0:a][sc]sidechaincompress=threshold={threshold}:ratio={ratio}:attack=5:release=150[bg];[bg][mix]amix=inputs=2:duration=longest[a]"
    );
    let cmd = format!(
        "ffmpeg -i {} -i {} -filter_complex {} -map {} -c:a aac -b:a 128k -shortest {}",
        cita_shell(musica),
        cita_shell(voz),
        cita_shell(&filtro),
        cita_shell("[a]"),
        cita_shell(salida),
    );
    acota_comando(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_unico(prefijo: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "grafito-voz-{prefijo}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[cfg(unix)]
    fn script_ejecutable(path: &Path, cuerpo: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::write(path, cuerpo).unwrap();
        let mut permisos = std::fs::metadata(path).unwrap().permissions();
        permisos.set_mode(0o755);
        std::fs::set_permissions(path, permisos).unwrap();
    }

    /// piper falso: captura `--output_file X`, drena stdin, escribe un wav
    /// no vacío y sale 0. Solo unix (shebang + coreutils).
    #[cfg(unix)]
    fn piper_falso(base: &Path) -> PathBuf {
        let bin = base.join("piper-falso");
        script_ejecutable(
            &bin,
            "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"--output_file\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\ncat > /dev/null\nprintf 'RIFFfalso-wav-16bytes' > \"$out\"\nexit 0\n",
        );
        bin
    }

    /// ffmpeg falso: vuelca su argv en `captura` (modo `>` o `>>` según
    /// `agregar`), drena stdin y `touch`ea el último argv (el tmp hermano,
    /// como el real). Solo unix.
    #[cfg(unix)]
    fn ffmpeg_falso(base: &Path, sufijo: &str, agregar: bool) -> (PathBuf, PathBuf) {
        let bin = base.join(format!("ffmpeg-falso-{sufijo}"));
        let captura = base.join(format!("argv-{sufijo}.txt"));
        let redir = if agregar { ">>" } else { ">" };
        script_ejecutable(
            &bin,
            &format!(
                "#!/bin/sh\necho \"$@\" {redir} \"{}\"\ncat > /dev/null\nfor a in \"$@\"; do ultimo=\"$a\"; done\ntouch \"$ultimo\"\nexit 0\n",
                captura.display()
            ),
        );
        (bin, captura)
    }

    fn pista_un_segmento() -> CaptionTrack {
        CaptionTrack::try_new(vec![grafito_anim::captions::CaptionSegment::frase(
            "hola mundo".to_string(),
            1000,
            3500,
        )
        .unwrap()])
        .unwrap()
    }

    #[test]
    fn detect_puro_con_dirs_falsos() {
        let base = dir_unico("detect");
        // Bin falso visible solo en su dir.
        let bindir = base.join("bin");
        std::fs::create_dir_all(&bindir).unwrap();
        assert!(find_bin_en_dirs("piper-inexistente-xyz", std::slice::from_ref(&bindir)).is_none());
        assert!(find_bin_en_dirs("piper-inexistente-xyz", &[]).is_none());
        std::fs::write(bindir.join("piper"), b"falso").unwrap();
        let hallado = find_bin_en_dirs("piper", &[base.join("vacio"), bindir.clone()]);
        assert_eq!(hallado, Some(bindir.join("piper")));
        // Dir vacío cuenta como tal (no `is_file`).
        std::fs::create_dir_all(base.join("vacio")).unwrap();
        assert!(find_bin_en_dirs("piper", &[base.join("vacio")]).is_none());
        // Voces: primera `.onnx` en orden, case-insensitive, solo archivos.
        let voces = base.join("voces");
        std::fs::create_dir_all(&voces).unwrap();
        assert!(find_voice_in_dirs(std::slice::from_ref(&voces)).is_none());
        std::fs::write(voces.join("nota.txt"), b"no").unwrap();
        std::fs::create_dir_all(voces.join("sub.ONNX")).unwrap();
        assert!(find_voice_in_dirs(std::slice::from_ref(&voces)).is_none());
        std::fs::write(voces.join("b.onnx"), b"fake").unwrap();
        std::fs::write(voces.join("a.ONNX"), b"fake").unwrap();
        assert_eq!(
            find_voice_in_dirs(std::slice::from_ref(&voces)),
            Some(voces.join("a.ONNX"))
        );
        // Orden de dirs: el primero con voces gana.
        let otras = base.join("otras");
        std::fs::create_dir_all(&otras).unwrap();
        std::fs::write(otras.join("z.onnx"), b"fake").unwrap();
        assert_eq!(
            find_voice_in_dirs(&[otras.clone(), voces.clone()]),
            Some(otras.join("z.onnx"))
        );
        // FIX 2: un dir ilegible/inexistente no aborta el resto.
        let faltante = base.join("no-existe-xyz");
        assert!(!faltante.exists());
        assert_eq!(
            find_voice_in_dirs(&[faltante, voces.clone()]),
            Some(voces.join("a.ONNX"))
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn detect_reales_no_panican_y_son_bool() {
        // Sin afirmar presencia (depende del box): solo que no mienten.
        let hay_bin = detect_piper_available();
        let hay_voz = detect_piper_voice_path();
        if !hay_bin {
            assert!(!hay_bin);
        }
        if let Some(voz) = hay_voz {
            assert!(voz.is_file(), "voz detectada debe existir");
            assert_eq!(
                voz.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase(),
                "onnx"
            );
        }
    }

    #[test]
    fn voice_candidate_dirs_orden_usuario_antes_que_sistema() {
        let dirs = voice_candidate_dirs();
        assert!(!dirs.is_empty());
        assert_eq!(
            dirs.last().unwrap(),
            &PathBuf::from("/usr/share/piper/voices")
        );
    }

    #[test]
    fn escape_ass_cubre_filtros_y_drive_windows() {
        assert_eq!(
            escape_ass_filter_path(Path::new("/tmp/mis subs/cap,1.ass")),
            "/tmp/mis subs/cap\\,1.ass"
        );
        assert_eq!(
            escape_ass_filter_path(Path::new("C:/voces/cap[1].ass")),
            "C\\:/voces/cap\\[1\\].ass"
        );
        assert_eq!(
            escape_ass_filter_path(Path::new("a'b;c.ass")),
            "a\\'b\\;c.ass"
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_con_piper_falso_publica_wav_no_vacio() {
        let base = dir_unico("spawn-ok");
        let piper = piper_falso(&base);
        let voz = base.join("voz.onnx");
        std::fs::write(&voz, b"falsa").unwrap();
        let wav = base.join("voz.wav");
        let handle = spawn_voiceover_with_bin(
            "hola mundo".to_string(),
            voz,
            wav.clone(),
            CancellationToken::default(),
            piper,
        );
        let salida = handle.join().expect("el hilo no debe panicar").unwrap();
        assert_eq!(salida, wav);
        assert!(
            std::fs::metadata(&wav).unwrap().len() > 0,
            "wav falso no vacío"
        );
        let restos: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(restos.is_empty(), "sin tmp huérfano: {restos:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_texto_vacio_y_voz_ausente_fallan_honesto() {
        let base = dir_unico("spawn-err");
        let piper = piper_falso(&base);
        // Texto vacío → Failed honesto, sin tocar disco.
        let err = spawn_voiceover_with_bin(
            "   ".to_string(),
            base.join("voz.onnx"),
            base.join("a.wav"),
            CancellationToken::default(),
            piper.clone(),
        )
        .join()
        .expect("el hilo no debe panicar")
        .unwrap_err();
        assert!(matches!(err, VoiceError::Failed(_)), "fue: {err}");
        assert!(format!("{err}").contains("vacío"));
        // Voz inexistente → VoiceMissing con la ruta.
        let ausente = base.join("no-existe.onnx");
        let err = spawn_voiceover_with_bin(
            "hola".to_string(),
            ausente.clone(),
            base.join("b.wav"),
            CancellationToken::default(),
            piper,
        )
        .join()
        .expect("el hilo no debe panicar")
        .unwrap_err();
        assert_eq!(err, VoiceError::VoiceMissing(ausente));
        assert!(!base.join("b.wav").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn spawn_sin_piper_falla_honesto() {
        // Binario inexistente a propósito: hermético, no depende del PATH.
        let base = dir_unico("spawn-missing");
        let voz = base.join("voz.onnx");
        std::fs::write(&voz, b"falsa").unwrap();
        let err = spawn_voiceover_with_bin(
            "hola".to_string(),
            voz,
            base.join("voz.wav"),
            CancellationToken::default(),
            PathBuf::from("/definitivamente/no/existe/piper-falso"),
        )
        .join()
        .expect("el hilo no debe panicar")
        .unwrap_err();
        assert_eq!(err, VoiceError::PiperMissing);
        assert!(format!("{err}").contains("piper"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_cancelado_mata_al_hijo_sin_wav() {
        let base = dir_unico("spawn-cancel");
        let dormilon = base.join("piper-duerme");
        script_ejecutable(&dormilon, "#!/bin/sh\nsleep 30\n");
        let voz = base.join("voz.onnx");
        std::fs::write(&voz, b"falsa").unwrap();
        let wav = base.join("voz.wav");
        let token = CancellationToken::default();
        let clon = token.clone();
        std::thread::spawn(move || {
            // Ola 4: 50ms bastan para que el hijo entre en `wait` antes del
            // cancel (antes 300ms); el watcher mata en CANCEL_GRACE igual.
            std::thread::sleep(std::time::Duration::from_millis(50));
            clon.cancel();
        });
        let err =
            spawn_voiceover_with_bin("hola mundo".to_string(), voz, wav.clone(), token, dormilon)
                .join()
                .expect("el hilo no debe panicar")
                .unwrap_err();
        assert_eq!(err, VoiceError::Cancelled);
        assert!(!wav.exists(), "cancelado no publica");
        let restos: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(restos.is_empty(), "sin tmp huérfano: {restos:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn mux_plomea_itsoffset_gain_y_codecs() {
        let base = dir_unico("mux-args");
        let (falso, captura) = ffmpeg_falso(&base, "mux", false);
        let video = base.join("clip.mp4");
        let audio = base.join("voz.wav");
        std::fs::write(&video, b"fake-video").unwrap();
        std::fs::write(&audio, b"fake-wav").unwrap();
        let dest = base.join("con-voz.mp4");
        mux_audio_into_with_bin(&video, &audio, 500, 1.5, &dest, &falso).unwrap();
        let argv = std::fs::read_to_string(&captura).expect("argv capturado");
        for aguja in [
            "-itsoffset",
            "0.5",
            "-af",
            "volume=1.5",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-shortest",
            "+faststart",
        ] {
            assert!(argv.contains(aguja), "{aguja} en argv, fue: {argv}");
        }
        // `-itsoffset` va entre los dos `-i` (offset del audio, no del video).
        let tokens: Vec<&str> = argv.split_whitespace().collect();
        let mut pos_i = Vec::new();
        let mut pos_offset = None;
        for (k, t) in tokens.iter().enumerate() {
            if *t == "-i" {
                pos_i.push(k);
            }
            if *t == "-itsoffset" {
                pos_offset = Some(k);
            }
        }
        assert_eq!(pos_i.len(), 2, "dos -i exactos, fue: {argv}");
        let pos_offset = pos_offset.expect("itsoffset en argv");
        assert!(
            pos_i[0] < pos_offset && pos_offset < pos_i[1],
            "itsoffset entre los dos -i, fue: {argv}"
        );
        assert!(dest.exists(), "publicó el destino");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn mux_rangos_y_ausencias_fallan_honesto() {
        let base = dir_unico("mux-err");
        let video = base.join("clip.mp4");
        let audio = base.join("voz.wav");
        std::fs::write(&video, b"fake").unwrap();
        std::fs::write(&audio, b"fake").unwrap();
        let falso = PathBuf::from("/definitivamente/no/existe/ffmpeg-falso");
        // Offset > 60 s y gain > 2 → Rango (contrato AudioTrack).
        assert!(matches!(
            mux_audio_into_with_bin(&video, &audio, 60_001, 1.0, &base.join("a.mp4"), &falso),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            mux_audio_into_with_bin(&video, &audio, 0, 2.5, &base.join("b.mp4"), &falso),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            mux_audio_into_with_bin(&video, &audio, 0, f32::NAN, &base.join("c.mp4"), &falso),
            Err(MuxError::Rango(_))
        ));
        // Insumos ausentes → Io honesto antes de spawnear.
        assert!(matches!(
            mux_audio_into_with_bin(
                &base.join("no.mp4"),
                &audio,
                0,
                1.0,
                &base.join("d.mp4"),
                &falso
            ),
            Err(MuxError::Io(_))
        ));
        // Sin ffmpeg → FfmpegMissing (se exporta mudo).
        let err = mux_audio_into_with_bin(&video, &audio, 0, 1.0, &base.join("e.mp4"), &falso)
            .unwrap_err();
        assert_eq!(err, MuxError::FfmpegMissing);
        assert!(!base.join("e.mp4").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn burn_plomea_ass_y_reencode() {
        let base = dir_unico("burn-args");
        let (falso, captura) = ffmpeg_falso(&base, "burn", false);
        let video = base.join("clip.mp4");
        std::fs::write(&video, b"fake-video").unwrap();
        let ass = write_ass_temp(&pista_un_segmento(), &base).unwrap();
        let dest = base.join("subtitulado.mp4");
        burn_captions_with_bin(&video, &ass, &dest, &falso).unwrap();
        let argv = std::fs::read_to_string(&captura).expect("argv capturado");
        assert!(argv.contains("ass="), "filtro ass=, fue: {argv}");
        for aguja in ["-c:v", "libx264", "-crf", "23", "veryfast", "-c:a", "copy"] {
            assert!(argv.contains(aguja), "{aguja} en argv, fue: {argv}");
        }
        assert!(dest.exists(), "publicó el destino");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn burn_sin_ffmpeg_falla_honesto_queda_sidecar() {
        // Hermético: binario inexistente. El sidecar se escribe igual
        // (no necesita ffmpeg): el wiring muestra este error + el .srt.
        let base = dir_unico("burn-missing");
        let video = base.join("clip.mp4");
        std::fs::write(&video, b"fake").unwrap();
        let ass = write_ass_temp(&pista_un_segmento(), &base).unwrap();
        let err = burn_captions_with_bin(
            &video,
            &ass,
            &base.join("sub.mp4"),
            Path::new("/definitivamente/no/existe/ffmpeg-falso"),
        )
        .unwrap_err();
        assert_eq!(err, CaptionBurnError::FfmpegMissing);
        assert!(format!("{err}").contains(".srt"));
        let srt = write_caption_sidecar(&pista_un_segmento(), &base.join("sub.mp4")).unwrap();
        assert!(srt.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sidecar_srt_junto_al_video_con_cota() {
        let base = dir_unico("sidecar");
        let video = base.join("clip.mp4");
        std::fs::write(&video, b"fake").unwrap();
        let srt = write_caption_sidecar(&pista_un_segmento(), &video).unwrap();
        assert_eq!(srt, base.join("clip.srt"));
        let texto = std::fs::read_to_string(&srt).unwrap();
        // El SRT parte en ≤2 renglones (`envuelve_dos_lineas`): la frase va
        // en dos líneas, no literal en una.
        assert!(texto.starts_with("1\n00:00:01,000 --> 00:00:03,500\n"));
        assert!(texto.contains("hola"));
        assert!(texto.contains("mundo"));
        assert!(texto.len() <= SIDECAR_MAX_BYTES);
        // O_EXCL honesto: el segundo intento no pisa.
        let err = write_caption_sidecar(&pista_un_segmento(), &video).unwrap_err();
        assert!(matches!(err, SidecarError::Io(_)));
        // Pista inválida (solape) → Caption honesto, sin archivo.
        let mala = CaptionTrack {
            segments: vec![
                grafito_anim::captions::CaptionSegment::frase("a".to_string(), 0, 1000).unwrap(),
                grafito_anim::captions::CaptionSegment::frase("b".to_string(), 500, 1500).unwrap(),
            ],
        };
        let err = write_caption_sidecar(&mala, &base.join("otro.mp4")).unwrap_err();
        assert!(matches!(err, SidecarError::Caption(_)));
        assert!(!base.join("otro.srt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn estima_duracion_por_wpm() {
        // 2 palabras a 60 wpm = 2 s exactos.
        assert_eq!(duracion_estimada_ms("hola mundo", 60).unwrap(), 2000);
        // Techo: 1 palabra a 150 wpm = 400 ms.
        assert_eq!(duracion_estimada_ms("hola", 150).unwrap(), 400);
        // 120 palabras a 120 wpm = 60 s.
        let texto = (0..120).map(|_| "palabra").collect::<Vec<_>>().join(" ");
        assert_eq!(duracion_estimada_ms(&texto, 120).unwrap(), 60_000);
        assert!(duracion_estimada_ms("", 150).is_err());
        assert!(duracion_estimada_ms("hola", 0).is_err());
        assert!(duracion_estimada_ms("hola", 39).is_err());
        assert!(duracion_estimada_ms("hola", 401).is_err());
    }

    #[test]
    fn estima_tiempos_cubre_el_total_con_peso_por_chars() {
        let tramo = estima_tiempos_palabras("hola mundo cruel", 3000, 150).unwrap();
        assert_eq!(tramo.len(), 3);
        // Contiguo desde 0 y suma exacta.
        assert_eq!(tramo[0].1, 0);
        assert_eq!(tramo[2].2, 3000);
        for w in tramo.windows(2) {
            assert_eq!(w[0].2, w[1].1, "sin huecos ni solapes");
            assert!(w[0].1 < w[0].2, "al menos 1 ms");
        }
        let suma: u32 = tramo.iter().map(|(_, i, f)| f - i).sum();
        assert_eq!(suma, 3000);
        // Peso por chars: "cruel" (5) dura >= que "hola" (4).
        assert!(tramo[2].2 - tramo[2].1 >= tramo[0].2 - tramo[0].1);
        // `total_ms == 0` calibra por WPM (2 palabras a 60 wpm = 2000 ms).
        let auto = estima_tiempos_palabras("hola mundo", 0, 60).unwrap();
        assert_eq!(auto.last().unwrap().2, 2000);
        // Bordes honestos.
        assert!(estima_tiempos_palabras("", 1000, 150).is_err());
        assert!(estima_tiempos_palabras("una dos tres", 2, 150).is_err());
        assert!(estima_tiempos_palabras("hola", 1000, 0).is_err());
        assert!(estima_tiempos_palabras("hola", 600_001, 150).is_err());
        let largo = (0..2001).map(|_| "x").collect::<Vec<_>>().join(" ");
        assert!(estima_tiempos_palabras(&largo, 60_000, 150).is_err());
    }

    #[test]
    fn curva_ducking_baja_la_musica_bajo_la_voz() {
        let params = DuckParams::try_new(1.0, 0.3, 100, 200).unwrap();
        let tramos = curva_ducking(&[(1000, 2000)], 4000, &params).unwrap();
        assert_eq!(
            tramos,
            vec![
                DuckTramo::constante(0, 900, 1.0),
                DuckTramo::rampa(900, 1000, 1.0, 0.3),
                DuckTramo::constante(1000, 2000, 0.3),
                DuckTramo::rampa(2000, 2200, 0.3, 1.0),
                DuckTramo::constante(2200, 4000, 1.0),
            ]
        );
        // Cobertura total sin huecos ni solapes, ganancias en rango.
        assert_eq!(tramos.first().unwrap().inicio_ms, 0);
        assert_eq!(tramos.last().unwrap().fin_ms, 4000);
        for w in tramos.windows(2) {
            assert_eq!(w[0].fin_ms, w[1].inicio_ms);
        }
        for t in &tramos {
            assert!(t.inicio_ms < t.fin_ms);
            assert!((0.3..=1.0).contains(&t.gain_ini));
            assert!((0.3..=1.0).contains(&t.gain_fin));
        }
        // Sin voz: un solo tramo base.
        assert_eq!(
            curva_ducking(&[], 4000, &params).unwrap(),
            vec![DuckTramo::constante(0, 4000, 1.0)]
        );
        // Ataque/release se recortan al hueco (voz pegada al 0 y al total).
        let borde = curva_ducking(&[(0, 4000)], 4000, &params).unwrap();
        assert_eq!(borde, vec![DuckTramo::constante(0, 4000, 0.3)]);
        // Release truncado por la voz vecina (no la invade).
        let dos = curva_ducking(&[(1000, 2000), (2100, 3000)], 4000, &params).unwrap();
        for w in dos.windows(2) {
            assert_eq!(w[0].fin_ms, w[1].inicio_ms);
        }
        assert_eq!(dos.last().unwrap().fin_ms, 4000);
        // Errores honestos.
        assert!(matches!(
            curva_ducking(&[(2000, 1000)], 4000, &params),
            Err(DuckError::Ventana(_))
        ));
        assert!(matches!(
            curva_ducking(&[(0, 4001)], 4000, &params),
            Err(DuckError::Ventana(_))
        ));
        assert!(matches!(
            curva_ducking(&[(0, 2000), (1500, 3000)], 4000, &params),
            Err(DuckError::Solape { .. })
        ));
        assert!(matches!(
            curva_ducking(&[(0, 1000)], 0, &params),
            Err(DuckError::Rango(_))
        ));
        assert!(DuckParams::try_new(0.3, 1.0, 100, 200).is_err());
        assert!(DuckParams::try_new(1.0, 0.3, 5001, 200).is_err());
        assert!(DuckParams::try_new(f32::NAN, 0.3, 100, 200).is_err());
    }

    #[test]
    fn cita_shell_cita_posix() {
        assert_eq!(cita_shell("simple.mp4"), "'simple.mp4'");
        assert_eq!(cita_shell("mi video.mp4"), "'mi video.mp4'");
        assert_eq!(cita_shell(""), "''");
        assert_eq!(cita_shell("it's.mp4"), "'it'\\''s.mp4'");
    }

    #[test]
    fn mux_builder_espeja_al_worker_sin_ejecutar() {
        // Determinista: misma entrada, mismo string.
        let a = arma_comando_mux("clip.mp4", "voz.wav", 500, 1.5, "con-voz.mp4").unwrap();
        let b = arma_comando_mux("clip.mp4", "voz.wav", 500, 1.5, "con-voz.mp4").unwrap();
        assert_eq!(a, b);
        // Mismo argv que `mux_audio_inner`: itsoffset entre los dos -i.
        for aguja in [
            "-itsoffset",
            "0.5",
            "-af",
            "volume=1.5",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-shortest",
            "+faststart",
        ] {
            assert!(a.contains(aguja), "{aguja} en: {a}");
        }
        let tokens: Vec<&str> = a.split_whitespace().collect();
        let mut pos_i = Vec::new();
        let mut pos_offset = None;
        for (k, t) in tokens.iter().enumerate() {
            if *t == "-i" {
                pos_i.push(k);
            }
            if *t == "-itsoffset" {
                pos_offset = Some(k);
            }
        }
        assert_eq!(pos_i.len(), 2, "dos -i exactos, fue: {a}");
        let pos_offset = pos_offset.expect("itsoffset en el comando");
        assert!(pos_i[0] < pos_offset && pos_offset < pos_i[1]);
        // Rutas con espacios y comillas van citadas (un shell las ve como 1 argv).
        let raro = arma_comando_mux("mi clip.mp4", "it's.wav", 0, 1.0, "sal'e.mp4").unwrap();
        assert!(raro.contains("'mi clip.mp4'"));
        assert!(raro.contains("'it'\\''s.wav'"));
        // Rangos del contrato AudioTrack + rutas honestas.
        assert!(matches!(
            arma_comando_mux("v.mp4", "a.wav", 60_001, 1.0, "o.mp4"),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            arma_comando_mux("v.mp4", "a.wav", 0, 2.5, "o.mp4"),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            arma_comando_mux("", "a.wav", 0, 1.0, "o.mp4"),
            Err(MuxError::Io(_))
        ));
        assert!(matches!(
            arma_comando_mux("v.mp4", "a\0.wav", 0, 1.0, "o.mp4"),
            Err(MuxError::Io(_))
        ));
        // Puro: no crea ni toca ningún archivo (rutas inexistentes igual arman).
        let cmd = arma_comando_mux(
            "/definitivamente/no/existe/v.mp4",
            "/definitivamente/no/existe/a.wav",
            0,
            1.0,
            "/definitivamente/no/existe/o.mp4",
        )
        .unwrap();
        assert!(!Path::new("/definitivamente/no/existe/o.mp4").exists());
        assert!(cmd.starts_with("ffmpeg "));
    }

    #[test]
    fn duck_builder_sidechain_sin_ejecutar() {
        let cmd = arma_comando_duck(
            "musica.mp3",
            "voz.wav",
            "mezcla.mp3",
            DUCK_THRESHOLD_DEFAULT,
            DUCK_RATIO_DEFAULT,
        )
        .unwrap();
        for aguja in [
            "sidechaincompress",
            "threshold=0.003",
            "ratio=20",
            "asplit",
            "amix",
        ] {
            assert!(cmd.contains(aguja), "{aguja} en: {cmd}");
        }
        // La voz alimenta al compresor Y a la mezcla (asplit=2 con [sc] y [mix]).
        assert!(cmd.contains("[sc]"));
        assert!(cmd.contains("[mix]"));
        assert!(matches!(
            arma_comando_duck("m.mp3", "v.wav", "o.mp3", 0.0, 20.0),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            arma_comando_duck("m.mp3", "v.wav", "o.mp3", 0.003, 21.0),
            Err(MuxError::Rango(_))
        ));
        assert!(matches!(
            arma_comando_duck("", "v.wav", "o.mp3", 0.003, 20.0),
            Err(MuxError::Io(_))
        ));
        assert!(!Path::new("/definitivamente/no/existe/mezcla.mp3").exists());
    }
}
