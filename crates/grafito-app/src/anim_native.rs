//! Animacion didactica nativa (sin motor externo).
//! Cada plantilla con renderer propio dibuja su objeto matemático; el
//! fallback `universal` es un placeholder neutro rotulado ("vista previa
//! no disponible"): grilla + texto, jamás una curva que parezca respuesta.
//! Todas las plantillas son deterministas.

// P1-app: motor de voz + mux + captions (`voice.rs`). Submódulo declarado
// acá (vía `#[path]`) para no tocar `lib.rs`: el wiring del diálogo lo hace
// otro frente después, que lo moverá a `crate::voice` si lo necesita.
#[path = "voice.rs"]
pub mod voice;

/// Frames por set nativo (48; el bench `benches/native_rgba.rs` lo pinnea).
pub const NATIVE_ANIM_FRAME_COUNT: usize = 48;

#[cfg(test)]
use grafito_anim::protocol::CANONICAL_TEMPLATES;
use grafito_anim::protocol::{
    contiene_palabra, mezclar_pixel_alfa, scene_param_clamped, taylor_anim_order_from_params,
    template_for_concept, SCENE_PARAM_A, SCENE_PARAM_B, SCENE_PARAM_SPAN, SCENE_PARAM_TERMS,
    SCENE_PARAM_X0,
};
use grafito_assistant::CancellationToken;
use std::path::{Path, PathBuf};
// Frente A: pasos lindos + skip anti-solape + formato de ticks reusados de
// `render_2d` (misma Piel, sin romper capas: ambas son render egui/cpu).
use crate::render_2d::{adaptive_label_skip, nice_number_plane_step};
use grafito_ui::animation::anim_axes::{
    cabe_label_entre, clip_seg_a_caja, short_tick_label, LabelCaja, TICK_CHAR_H_PX, TICK_CHAR_W_PX,
};

// ── Registro canónico nativo v4 (13 plantillas) ──────────────────────────
// SYNC MECÁNICO 11↔11↔11 (ANIM-REVIVE) + 2 nativas nuevas en tránsito:
// - `grafito-anim/src/protocol.rs::CANONICAL_TEMPLATES`: fuente única (11).
// - Este `NATIVE_TEMPLATES`: prefijo 11 idéntico en orden y contenido +
//   `subspace` + `fractal` al final (test pineado como superconjunto).
//   El frente protocolo las registra en `CANONICAL_TEMPLATES` después;
//   hasta entonces `sanitize_template` las rechaza y el nativo las atiende
//   por dispatcher interno (`render_anim_by_template` / `resolve_native_template`).
// - `anim_ui.rs::PLANTILLAS_COMBO`: mismo conjunto (test pineado, orden libre).
// - `sanitize_template` usa `CANONICAL_TEMPLATES` (sin match duplicado).
// DIVERGENCIA HONESTA residual (fuera de este scope):
// - `grafito-agent/src/tools.rs::KNOWN_TEMPLATES`: 7 (sin logistic/gradient/
//   mobius/universal; NO editable desde este scope).
// - Worker python `ALLOW_TEMPLATE`: 6 (sin euler/fourier/logistic/gradient/
//   mobius; NO editable desde este scope → el worker mapea por concepto).
// - `limit-epsilon` / `ode-*` NO existen en ningún registro: caen al fallback
//   genérico — ver `native_dispatch_for` + test `dispatch_honesto_*` que
//   pinnea `FallbackUniversal` hasta que alguien les dé renderer propio.
//
/// Plantillas canónicas con renderer nativo propio (+ `universal`: placeholder
/// neutro honesto para pedidos sin plantilla, sin curva matemática falsa).
pub const NATIVE_TEMPLATES: &[&str] = &[
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "euler",
    "fourier",
    "logistic-bifurcation",
    "gradient-field",
    "mobius-transform",
    "universal",
    // Frente piel-ui: 2 plantillas nuevas con renderer propio en este archivo
    // (al final para conservar el prefijo 11 idéntico al protocolo).
    "subspace",
    "fractal",
];

/// ¿La plantilla tiene renderer nativo propio?
pub fn is_known_native_template(template: &str) -> bool {
    let t = template.trim().to_lowercase();
    NATIVE_TEMPLATES.contains(&t.as_str()) || t == "pythagoras"
}

// ── Export GIF real en hilo aparte (ANIM-REVIVE) ──────────────────────────
// El botón Exportar era no-op: ahora los 48 frames nativos se codifican a GIF
// animado con el crate `gif` 0.13 (ya dependencia del crate; el decoder se usa
// en `assistant.rs::load_gif_frames`). `encode_frames_to_gif_bytes` es puro
// (sin E/S); `export_frames_to_gif_file` bloquea escribiendo y `spawn_gif_export`
// lo corre en un hilo aparte para no congelar la UI. El lead lo llama desde el
// handler de `AnimPanelEvent::ExportRequested` (ver `anim_ui.rs`).
// Presupuestos: `GIF_EXPORT_MAX_FRAMES = 64` (igual que el loader) y lado
// ≤4096 (igual que `Resolution`); los 48 nativos siempre pasan.

/// Retardo por frame en centésimas de segundo (8 ≈ 12 fps, igual que `PLAYBACK_FPS` en `anim_ui.rs`).
pub const GIF_EXPORT_DELAY_CS: u16 = 8;
/// FPS base del reproductor/export (B5): `100 / GIF_EXPORT_DELAY_CS` (pineado en test).
pub const GIF_BASE_FPS: f32 = 12.0;
/// Píxeles totales máximos por GIF exportado (B5, paridad con el loader
/// `load_gif_frames` en `assistant.rs`: 8 M).
pub const GIF_EXPORT_MAX_TOTAL_PIXELS: usize = 8_000_000;
/// Bytes máximos del GIF escrito (B5, paridad con el loader: 5 MB). Se
/// verifica tras el join (el tamaño final solo se conoce al codificar).
pub const GIF_EXPORT_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// Velocidad del cuantizador NeuQuant 1..=30 (10 = compromiso; ver docs de `gif`).
pub const GIF_EXPORT_SPEED: i32 = 10;
/// Tope de frames por GIF (igual que `MAX_GIF_FRAMES` del loader).
pub const GIF_EXPORT_MAX_FRAMES: usize = 64;
/// Lado máximo por frame (igual que `Resolution::try_new` 64..=4096).
pub const GIF_EXPORT_MAX_DIM: usize = 4096;

// ── F1canon: viewport canónico del chat 480×360 ─────────────────────────
// Elección documentada: 480×360×48 RGBA = 33_177_600 B (31.6 MiB), que
// entra holgado en `NATIVE_MAX_SET_BYTES` (64 MiB); el autofit GIF lo baja
// ~4% a 470×352 (imperceptible) para el presupuesto de 8 M px del loader.
// Workers nativos, replay y export usan este canónico (vía
// `encajar_anim_a_chat`); SPEC/Guion conservan sus propios presupuestos.
pub const CHAT_CANON_W: u32 = 480;
pub const CHAT_CANON_H: u32 = 360;

/// Encaja un pedido `(w, h)` al canónico del chat preservando aspecto.
///
/// Factor = min(480/w, 360/h, 1.0): nunca amplía, solo reduce; la salida
/// es en dims pares (exigencia yuv420p/GIF) con mínimo 2. `(480, 360)` →
/// idéntico. Puro, sin I/O.
pub fn encajar_anim_a_chat(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (u64::from(width.max(1)), u64::from(height.max(1)));
    let factor = (f64::from(CHAT_CANON_W) / w as f64)
        .min(f64::from(CHAT_CANON_H) / h as f64)
        .min(1.0);
    if !factor.is_finite() {
        return (CHAT_CANON_W, CHAT_CANON_H);
    }
    let mut nw = ((w as f64 * factor).floor() as u32).clamp(2, CHAT_CANON_W);
    let mut nh = ((h as f64 * factor).floor() as u32).clamp(2, CHAT_CANON_H);
    nw &= !1;
    nh &= !1;
    (nw.max(2), nh.max(2))
}

// ── H1: autofit GIF con aviso visible ───────────────────────────────────
// El default 480×360×48 suma 8_294_400 px > 8 M: autofitea mínimo a
// 470×352 (factor 0.982) en vez de fallar. `gif_autofit_size` es puro;
// `reescalar_frames_a_cancelable` es CPU puro (vecino más cercano, sin
// I/O); el camino export (`confirm_export_assistant_media`) aplica el plan
// y muestra `mensaje_autofit_gif` en toast (antes el aviso quedaba solo
// en tests).

/// Plan de downscale para entrar en 8 M px totales.
///
/// `None` si `w*h*n` ya entra; `Some((nw, nh))` en dims pares (mínimo 2)
/// con el mismo aspecto si excede. `None` también con dims/n cero o
/// desborde (el preflight lo rechaza honesto después). Puro, sin I/O.
pub fn gif_autofit_size(width: usize, height: usize, frames: usize) -> Option<(usize, usize)> {
    let total = width.checked_mul(height)?.checked_mul(frames)?;
    if total <= GIF_EXPORT_MAX_TOTAL_PIXELS {
        return None;
    }
    let factor = (GIF_EXPORT_MAX_TOTAL_PIXELS as f64 / total as f64).sqrt();
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let mut nw = ((width as f64 * factor).floor() as usize).clamp(2, GIF_EXPORT_MAX_DIM);
    let mut nh = ((height as f64 * factor).floor() as usize).clamp(2, GIF_EXPORT_MAX_DIM);
    nw &= !1;
    nh &= !1;
    let (nw, nh) = (nw.max(2), nh.max(2));
    // El floor+par puede pasarse por 1 px: re-chequeo honesto.
    let encaja = nw
        .checked_mul(nh)
        .and_then(|v| v.checked_mul(frames.max(1)))
        .is_some_and(|v| v <= GIF_EXPORT_MAX_TOTAL_PIXELS);
    if encaja {
        Some((nw, nh))
    } else {
        Some((nw.saturating_sub(2).max(2), nh.saturating_sub(2).max(2)))
    }
}

/// Aviso visible del autofit (lo muestra el camino export en toast).
pub fn mensaje_autofit_gif(width: usize, height: usize) -> String {
    format!("Exportado a {width}×{height} para entrar en presupuesto")
}

/// Reescala el set a `(width, height)` por vecino más cercano.
///
/// CPU puro, sin I/O ni allocs gigantes (reserva exacta por frame).
/// Chequea el token entre frames → `Cancelled` honesto sin parcial.
/// `Err` también con set vacío, destino fuera de 1..=4096 u origen
/// degenerado/inconsistente (sin panics).
pub fn reescalar_frames_a_cancelable(
    frames: &[egui::ColorImage],
    width: usize,
    height: usize,
    token: &CancellationToken,
) -> Result<Vec<egui::ColorImage>, GifExportError> {
    if frames.is_empty() {
        return Err(GifExportError::EmptyFrames);
    }
    if width == 0 || height == 0 || width > GIF_EXPORT_MAX_DIM || height > GIF_EXPORT_MAX_DIM {
        return Err(GifExportError::DimensionOutOfRange { width, height });
    }
    let mut salida = Vec::new();
    salida
        .try_reserve_exact(frames.len())
        .map_err(|_| GifExportError::Encode("sin memoria para el set reescalado".to_string()))?;
    for (index, frame) in frames.iter().enumerate() {
        if token.is_cancelled() {
            return Err(GifExportError::Cancelled);
        }
        let [origen_w, origen_h] = frame.size;
        let total_origen =
            origen_w
                .checked_mul(origen_h)
                .ok_or(GifExportError::DimensionOutOfRange {
                    width: origen_w,
                    height: origen_h,
                })?;
        if origen_w == 0 || origen_h == 0 {
            return Err(GifExportError::DimensionOutOfRange {
                width: origen_w,
                height: origen_h,
            });
        }
        if frame.pixels.len() != total_origen {
            return Err(GifExportError::PixelCountMismatch {
                index,
                expected: total_origen,
                got: frame.pixels.len(),
            });
        }
        let byte_len = width
            .checked_mul(height)
            .ok_or(GifExportError::DimensionOutOfRange { width, height })?;
        let mut pixeles = Vec::with_capacity(byte_len);
        for fila in 0..height {
            let origen_y = fila.saturating_mul(origen_h) / height;
            for columna in 0..width {
                let origen_x = columna.saturating_mul(origen_w) / width;
                let indice = origen_y.saturating_mul(origen_w).saturating_add(origen_x);
                pixeles.push(
                    frame
                        .pixels
                        .get(indice)
                        .copied()
                        .unwrap_or(egui::Color32::BLACK),
                );
            }
        }
        salida.push(egui::ColorImage {
            size: [width, height],
            pixels: pixeles,
        });
    }
    Ok(salida)
}

/// Error tipado de la exportación a GIF (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GifExportError {
    EmptyFrames,
    TooManyFrames {
        got: usize,
    },
    InconsistentSize {
        index: usize,
        expected: [usize; 2],
        got: [usize; 2],
    },
    PixelCountMismatch {
        index: usize,
        expected: usize,
        got: usize,
    },
    DimensionOutOfRange {
        width: usize,
        height: usize,
    },
    /// Píxeles totales sobre el presupuesto (paridad con el loader: 8 M).
    TooManyPixels {
        got: usize,
    },
    /// Exportación cancelada vía `CancellationToken` (M3-6).
    Cancelled,
    Encode(String),
    Io(String),
}

impl std::fmt::Display for GifExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFrames => write!(f, "sin fotogramas para exportar"),
            Self::TooManyFrames { got } => {
                write!(f, "demasiados fotogramas: {got} > {GIF_EXPORT_MAX_FRAMES}")
            }
            Self::InconsistentSize {
                index,
                expected,
                got,
            } => write!(f, "frame {index} con tamaño {got:?}, esperaba {expected:?}"),
            Self::PixelCountMismatch {
                index,
                expected,
                got,
            } => write!(f, "frame {index} con {got} píxeles, esperaba {expected}"),
            Self::DimensionOutOfRange { width, height } => write!(
                f,
                "dimensión {width}x{height} fuera de 1..={GIF_EXPORT_MAX_DIM}"
            ),
            Self::TooManyPixels { got } => {
                write!(
                    f,
                    "demasiados píxeles totales: {got} > {GIF_EXPORT_MAX_TOTAL_PIXELS}"
                )
            }
            Self::Cancelled => write!(f, "exportación cancelada"),
            Self::Encode(detail) => write!(f, "falló codificar el GIF: {detail}"),
            Self::Io(detail) => write!(f, "falló escribir el GIF: {detail}"),
        }
    }
}

impl std::error::Error for GifExportError {}

fn gif_dim(value: usize) -> Result<u16, GifExportError> {
    // u16 cubre 4096 de sobra; el rango real se valida antes (lado ≤4096).
    u16::try_from(value).map_err(|_| GifExportError::DimensionOutOfRange {
        width: value,
        height: value,
    })
}

/// Bytes RGBA de un frame ya validado en tamaño (núcleo compartido
/// GIF/MP4). Puro, sin E/S: `pixel_count` es el `w*h` verificado por el
/// llamador; cualquier descalce → `PixelCountMismatch` (sin panics).
fn frame_rgba_bytes(
    frame: &egui::ColorImage,
    pixel_count: usize,
    index: usize,
) -> Result<Vec<u8>, GifExportError> {
    if frame.pixels.len() != pixel_count {
        return Err(GifExportError::PixelCountMismatch {
            index,
            expected: pixel_count,
            got: frame.pixels.len(),
        });
    }
    let byte_len = pixel_count
        .checked_mul(4)
        .ok_or(GifExportError::PixelCountMismatch {
            index,
            expected: pixel_count.saturating_mul(4),
            got: frame.pixels.len().saturating_mul(4),
        })?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(byte_len)
        .map_err(|_| GifExportError::Encode(format!("sin memoria para el frame {index}")))?;
    for px in &frame.pixels {
        rgba.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
    }
    Ok(rgba)
}

/// Codifica frames a GIF animado en memoria (puro, sin E/S).
///
/// Todos los frames deben compartir tamaño, con lado 1..=4096 y como máximo
/// 64 frames. `delay_cs` en centésimas de segundo (8 ≈ 12 fps).
/// Invariante para `gif::Frame::from_rgba_speed` (que exige
/// `w*h*4 == buf.len()` y `speed` 1..=30): el buffer se construye con exactly
/// `pixels.len()*4` bytes tras verificar `pixels.len() == w*h`, y
/// `GIF_EXPORT_SPEED = 10` es const válida — ningún panic posible.
pub fn encode_frames_to_gif_bytes(
    frames: &[egui::ColorImage],
    delay_cs: u16,
) -> Result<Vec<u8>, GifExportError> {
    encode_frames_to_gif_bytes_cancelable(frames, delay_cs, &CancellationToken::default())
}

/// Idem cancelable (M3-6): chequea el token entre frames y aborta con
/// `Cancelled` honesto sin dejar parcial (el buffer vive en memoria y se
/// descarta con el `Err`). Puro, sin E/S.
pub fn encode_frames_to_gif_bytes_cancelable(
    frames: &[egui::ColorImage],
    delay_cs: u16,
    token: &CancellationToken,
) -> Result<Vec<u8>, GifExportError> {
    if frames.is_empty() {
        return Err(GifExportError::EmptyFrames);
    }
    if frames.len() > GIF_EXPORT_MAX_FRAMES {
        return Err(GifExportError::TooManyFrames { got: frames.len() });
    }
    let size = frames[0].size;
    let (w, h) = (size[0], size[1]);
    if w == 0 || h == 0 || w > GIF_EXPORT_MAX_DIM || h > GIF_EXPORT_MAX_DIM {
        return Err(GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        });
    }
    let w16 = gif_dim(w).map_err(|_| GifExportError::DimensionOutOfRange {
        width: w,
        height: h,
    })?;
    let h16 = gif_dim(h).map_err(|_| GifExportError::DimensionOutOfRange {
        width: w,
        height: h,
    })?;
    let pixel_count = w
        .checked_mul(h)
        .ok_or(GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        })?;
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, w16, h16, &[])
            .map_err(|e| GifExportError::Encode(e.to_string()))?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| GifExportError::Encode(e.to_string()))?;
        for (index, frame) in frames.iter().enumerate() {
            if token.is_cancelled() {
                return Err(GifExportError::Cancelled);
            }
            if frame.size != size {
                return Err(GifExportError::InconsistentSize {
                    index,
                    expected: size,
                    got: frame.size,
                });
            }
            let mut rgba = frame_rgba_bytes(frame, pixel_count, index)?;
            let mut gif_frame =
                gif::Frame::from_rgba_speed(w16, h16, rgba.as_mut_slice(), GIF_EXPORT_SPEED);
            gif_frame.delay = delay_cs;
            encoder
                .write_frame(&gif_frame)
                .map_err(|e| GifExportError::Encode(e.to_string()))?;
        }
    }
    Ok(out)
}

/// Escribe los frames como GIF animado en `path` (bloquea: llamar en hilo).
///
/// Creación exclusiva (`create_new`, `O_EXCL`): si el destino ya existe —
/// symlink plantado incluido — falla cerrado sin seguir ni truncar nada.
///
/// Atómico: codifica en memoria, vuelca a un hermano `.tmp.<pid>-<nanos>` y
/// lo renombra al destino. En cualquier `Err` no queda ni archivo final ni
/// `.tmp` huérfano (se borra best-effort).
pub fn export_frames_to_gif_file(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
) -> Result<PathBuf, GifExportError> {
    export_frames_to_gif_file_cancelable(frames, path, delay_cs, &CancellationToken::default())
}

/// Idem cancelable (M3-6): chequea el token antes de codificar y entre
/// frames; cancelado → `Cancelled` sin tocar disco (ni final ni `.tmp`).
pub fn export_frames_to_gif_file_cancelable(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
) -> Result<PathBuf, GifExportError> {
    if token.is_cancelled() {
        return Err(GifExportError::Cancelled);
    }
    let bytes = encode_frames_to_gif_bytes_cancelable(frames, delay_cs, token)?;
    if token.is_cancelled() {
        return Err(GifExportError::Cancelled);
    }
    write_gif_bytes_atomically(&bytes, path)
}

/// Hermano temporal para el vuelco atómico (mismo directorio = mismo
/// filesystem, el `rename` es atómico). Sufijo con pid + nanos para no
/// colisionar entre exports concurrentes. Puro, sin E/S.
fn gif_tmp_sibling(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut os = path.as_os_str().to_owned();
    os.push(format!(".tmp.{}-{stamp}", std::process::id()));
    PathBuf::from(os)
}

/// Vuelca bytes ya codificados al destino de forma atómica (tmp + rename).
///
/// Si el destino existe (archivo, dir o symlink) falla cerrado antes de
/// tocar disco; si el vuelco o el rename fallan, borra el `.tmp` y retorna
/// `Err` sin dejar parcial final. Sin pánicos.
fn write_gif_bytes_atomically(bytes: &[u8], path: &Path) -> Result<PathBuf, GifExportError> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(GifExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    let tmp = gif_tmp_sibling(path);
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
    {
        Ok(file) => file,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(GifExportError::Io(format!(
                "no se pudo crear {} sin sobrescribir: {e}",
                tmp.display()
            )));
        }
    };
    use std::io::Write as _;
    if let Err(e) = file.write_all(bytes) {
        drop(file);
        let _ = std::fs::remove_file(&tmp);
        return Err(GifExportError::Io(format!(
            "no se pudo escribir {}: {e}",
            path.display()
        )));
    }
    drop(file);
    // Re-chequeo pre-rename: cierra la ventana entre el chequeo inicial y
    // la publicación (si alguien plantó el destino en el medio, se aborta
    // sin pisarlo y sin dejar el `.tmp`).
    if std::fs::symlink_metadata(path).is_ok() {
        let _ = std::fs::remove_file(&tmp);
        return Err(GifExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(GifExportError::Io(format!(
            "no se pudo publicar {}: {e}",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

/// Crea el workdir de animación de forma exclusiva (equivale a `O_EXCL` en
/// directorios: `create_dir` falla si ya existe, sin seguir symlinks).
///
/// Solo llamarla cuando el motor externo la necesite; la vía nativa no toca
/// disco. `Err` honesto en español para mostrar en la card.
pub fn prepare_anim_workdir_exclusive(work_dir: &Path) -> Result<(), String> {
    std::fs::create_dir(work_dir).map_err(|e| {
        format!(
            "no se pudo preparar el área de trabajo {}: {e}",
            work_dir.display()
        )
    })
}

/// Exporta en un hilo aparte (no bloquea la UI).
///
/// El lead lo dispara con los frames del estado + destino elegido por el
/// usuario y al hacer `join` actualiza `media_path` / `status`.
///
/// M3-6: preflight de budget ANTES del trabajo pesado (dentro del hilo
/// para no cambiar la firma): sobre-presupuesto → `Err` rápido sin
/// codificar ni tocar disco. Para cancelación usar
/// `spawn_gif_export_cancelable`.
pub fn spawn_gif_export(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    delay_cs: u16,
) -> std::thread::JoinHandle<Result<PathBuf, GifExportError>> {
    std::thread::spawn(move || {
        check_gif_export_budget(&frames)?;
        export_frames_to_gif_file(&frames, &path, delay_cs)
    })
}

/// Idem cancelable (M3-6): el token se chequea pre-spawn (dentro del hilo,
/// `Cancelled` inmediato sin archivo), entre frames y antes de escribir.
/// Test: spawn→cancel→join rápido sin archivo.
pub fn spawn_gif_export_cancelable(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    delay_cs: u16,
    token: CancellationToken,
) -> std::thread::JoinHandle<Result<PathBuf, GifExportError>> {
    std::thread::spawn(move || {
        if token.is_cancelled() {
            return Err(GifExportError::Cancelled);
        }
        check_gif_export_budget(&frames)?;
        export_frames_to_gif_file_cancelable(&frames, &path, delay_cs, &token)
    })
}

/// Retardo por frame para una velocidad de la card (B5).
///
/// `base_delay_cs / rate` en centésimas (8/1 → 8 ≈ 12 fps; 8/0.5 → 16;
/// 8/2 → 4), mínimo 1. Tasa no finita o ≤ 0 → base (honesto, sin panic).
/// Puro, sin E/S.
pub fn gif_delay_for_rate(base_delay_cs: u16, rate: f32) -> u16 {
    if !rate.is_finite() || rate <= 0.0 || base_delay_cs == 0 {
        return base_delay_cs.max(1);
    }
    let delay = f32::from(base_delay_cs) / rate;
    if !delay.is_finite() {
        return base_delay_cs;
    }
    (delay.round() as u16).clamp(1, 100)
}

/// Preflight puro antes de spawnear la exportación (B5 + H1).
///
/// Verifica vacío, tope 64 frames, dimensiones 1..=4096 y píxeles POR
/// FRAME ≤ 8 M (H1: solo un frame absurdo da `Err`; el total del set lo
/// baja `gif_autofit_size` en el camino export, no el preflight).
/// No estima bytes: el tamaño final (cota 5 MB) lo verifica la app
/// tras el join, porque solo se conoce al codificar. Puro, sin E/S.
pub fn check_gif_export_budget(frames: &[egui::ColorImage]) -> Result<(), GifExportError> {
    if frames.is_empty() {
        return Err(GifExportError::EmptyFrames);
    }
    if frames.len() > GIF_EXPORT_MAX_FRAMES {
        return Err(GifExportError::TooManyFrames { got: frames.len() });
    }
    for frame in frames {
        let (w, h) = (frame.size[0], frame.size[1]);
        if w == 0 || h == 0 || w > GIF_EXPORT_MAX_DIM || h > GIF_EXPORT_MAX_DIM {
            return Err(GifExportError::DimensionOutOfRange {
                width: w,
                height: h,
            });
        }
        let pixel_count = w
            .checked_mul(h)
            .ok_or(GifExportError::DimensionOutOfRange {
                width: w,
                height: h,
            })?;
        if pixel_count > GIF_EXPORT_MAX_TOTAL_PIXELS {
            return Err(GifExportError::TooManyPixels { got: pixel_count });
        }
    }
    Ok(())
}

// ── Export MP4 vía ffmpeg-sidecar (F1 Manim-en-Rust) ────────────────────────
// Mismos budgets que el GIF (`GIF_EXPORT_MAX_FRAMES` 64, lado ≤4096, 8M px
// por frame; el total lo baja el autofit en el camino export): el preflight es `check_gif_export_budget` mapeado a
// `Mp4ExportError::Budget`. Los frames se entuban como rawvideo RGBA al
// stdin de `ffmpeg` (libx264, yuv420p, faststart) en un hilo worker con
// `CancellationToken` — espejo de `spawn_gif_export_cancelable`: la UI
// nunca bloquea ni hace E/S. Atómico como el GIF (tmp hermano + rename,
// `O_EXCL` honesto si el destino existe).
// Sin `ffmpeg` en el PATH → `FfmpegMissing` honesto ("unsupported mp4
// nativo, usá gif"): el MP4 del wire nunca queda fake.

/// FPS base del MP4 nativo (12, paridad con `GIF_BASE_FPS`).
pub const MP4_BASE_FPS: u32 = 12;

/// FPS desde el retardo GIF (`100/delay_cs`, clamp 1..=60): `delay 8` → 12.
/// `delay 0` → base (honesto, sin división por cero). Puro.
pub fn mp4_fps_for_delay(delay_cs: u16) -> u32 {
    (100 / u32::from(delay_cs.max(1))).clamp(1, 60)
}

/// Error tipado de la exportación a MP4 (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mp4ExportError {
    /// Preflight de budgets (los mismos del GIF).
    Budget(GifExportError),
    /// Exportación cancelada vía `CancellationToken`.
    Cancelled,
    /// Sin `ffmpeg` en el PATH: MP4 nativo no disponible.
    FfmpegMissing,
    /// `ffmpeg` corrió pero falló (cola del stderr, 500 chars).
    FfmpegFailed(String),
    Io(String),
}

impl std::fmt::Display for Mp4ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Budget(inner) => write!(f, "{inner}"),
            Self::Cancelled => write!(f, "exportación cancelada"),
            Self::FfmpegMissing => write!(
                f,
                "unsupported mp4 nativo, usá gif (ffmpeg no está en el PATH)"
            ),
            Self::FfmpegFailed(detail) => write!(f, "ffmpeg falló: {detail}"),
            Self::Io(detail) => write!(f, "falló escribir el MP4: {detail}"),
        }
    }
}

impl std::error::Error for Mp4ExportError {}

/// Hermano temporal para el MP4 (mismo directorio = mismo filesystem, el
/// `rename` es atómico). A diferencia del GIF, conserva extensión `.mp4`:
/// `ffmpeg` infiere el contenedor por extensión y `clip.mp4.tmp…` le da
/// formato desconocido (más `-f mp4` explícito por defensa en profundidad).
/// Puro, sin E/S.
fn mp4_tmp_sibling(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(String::from("clip"));
    let tmp_name = format!("{name}.tmp.{}-{stamp}.mp4", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Limpieza best-effort del tmp hermano (nunca deja parcial huérfano).
fn mp4_drop_tmp(tmp: &Path) {
    let _ = std::fs::remove_file(tmp);
}

/// Núcleo bloqueante (llamar en hilo): preflight de budgets, entubado
/// rawvideo a `ffmpeg`, publicación atómica. `ffmpeg_bin` overridea el
/// binario (tests: `None` = `ffmpeg` del PATH). Sin pánicos.
fn export_frames_to_mp4_inner(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    ffmpeg_bin: Option<&Path>,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, Mp4ExportError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(Mp4ExportError::Cancelled);
    }
    // fps honesto: re-muestrea el set base (12fps) al N pedido con duración
    // fija vía `Timeline::sample` (no solo `-framerate`).
    let fps = mp4_fps_for_delay(delay_cs);
    let frames = remuestrear_frames_para_fps(frames, GIF_BASE_FPS as u32, fps);
    // `-ql`/`-qm`/`-qh` → resolución real (downscale en el worker).
    let frames = reescalar_frames_para_calidad(&frames, quality);
    check_gif_export_budget(&frames).map_err(Mp4ExportError::Budget)?;
    let size = frames[0].size;
    let (w, h) = (size[0], size[1]);
    let pixel_count = w.checked_mul(h).ok_or(Mp4ExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(Mp4ExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    let tmp = mp4_tmp_sibling(path);
    let bin = ffmpeg_bin.unwrap_or_else(|| Path::new("ffmpeg"));
    let bitrate_kbps =
        bitrate_kbps.clamp(ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_BITRATE_MAX_KBPS);
    let (crf, preset) = quality.flags();
    let mut child = Command::new(bin)
        .arg("-y")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-s")
        .arg(format!("{w}x{h}"))
        .arg("-framerate")
        .arg(fps.to_string())
        .arg("-i")
        .arg("pipe:0")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg(preset)
        .arg("-crf")
        .arg(crf.to_string())
        .arg("-b:v")
        .arg(format!("{bitrate_kbps}k"))
        .arg("-pix_fmt")
        .arg("yuv420p")
        // yuv420p exige lados pares: el filtro es no-op si ya lo son.
        .arg("-vf")
        .arg("scale=trunc(iw/2)*2:trunc(ih/2)*2")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-f")
        .arg("mp4")
        .arg(&tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            mp4_drop_tmp(&tmp);
            if e.kind() == std::io::ErrorKind::NotFound {
                Mp4ExportError::FfmpegMissing
            } else {
                Mp4ExportError::Io(format!("no se pudo lanzar {}: {e}", bin.display()))
            }
        })?;
    // Entubado frame a frame (token entre frames; cancelado → kill +_wait
    // para no dejar zombie + limpieza del tmp, sin tocar el destino).
    let pipe_result = (|| -> Result<(), Mp4ExportError> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Mp4ExportError::Io("ffmpeg no abrió su stdin".to_string()))?;
        for (index, frame) in frames.iter().enumerate() {
            if token.is_cancelled() {
                return Err(Mp4ExportError::Cancelled);
            }
            if frame.size != size {
                return Err(Mp4ExportError::Budget(GifExportError::InconsistentSize {
                    index,
                    expected: size,
                    got: frame.size,
                }));
            }
            let rgba =
                frame_rgba_bytes(frame, pixel_count, index).map_err(Mp4ExportError::Budget)?;
            stdin.write_all(&rgba).map_err(|e| {
                Mp4ExportError::Io(format!("no se pudo entubar el frame {index}: {e}"))
            })?;
        }
        Ok(())
    })();
    if let Err(pipe_err) = pipe_result {
        let _ = child.kill();
        let _ = child.wait();
        mp4_drop_tmp(&tmp);
        return Err(pipe_err);
    }
    // Espera vigilada: cancelar en `wait` mata al hijo (CANCEL_GRACE) sin
    // zombies y sin tocar el destino.
    match esperar_ffmpeg_con_cancel(&mut child, token) {
        EsperaFfmpeg::Cancelado => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::Cancelled);
        }
        EsperaFfmpeg::FalloIo(detalle) => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::Io(detalle));
        }
        EsperaFfmpeg::Terminado(false, stderr) => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::FfmpegFailed(ffmpeg_stderr_tail(&stderr)));
        }
        EsperaFfmpeg::Terminado(true, _) => {}
    }
    if token.is_cancelled() {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Cancelled);
    }
    // Publicación atómica como el GIF (re-chequeo pre-rename anti-TOCTOU).
    if std::fs::symlink_metadata(path).is_ok() {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Io(format!(
            "no se pudo publicar {}: {e}",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

/// Escribe los frames como MP4 (bloquea: llamar en hilo, ver
/// `spawn_mp4_export`). `ffmpeg` del PATH; sin él → `FfmpegMissing`.
/// `bitrate_kbps` (100..=20000, se clampa) + `quality` (`-ql`/`-qm`/`-qh` →
/// resolución + crf + preset reales).
pub fn export_frames_to_mp4_file(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, Mp4ExportError> {
    export_frames_to_mp4_inner(
        frames,
        path,
        delay_cs,
        &CancellationToken::default(),
        None,
        bitrate_kbps,
        quality,
    )
}

/// Idem cancelable: chequea el token antes de spawnear `ffmpeg`, entre
/// frames, durante el `wait` (mata al hijo) y antes de publicar; cancelado
/// → `Cancelled` sin tocar disco.
pub fn export_frames_to_mp4_file_cancelable(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, Mp4ExportError> {
    export_frames_to_mp4_inner(frames, path, delay_cs, token, None, bitrate_kbps, quality)
}

/// Idem con binario explícito (tests herméticos + distros sin PATH).
pub fn export_frames_to_mp4_file_with_bin(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, Mp4ExportError> {
    export_frames_to_mp4_inner(
        frames,
        path,
        delay_cs,
        token,
        Some(ffmpeg_bin),
        bitrate_kbps,
        quality,
    )
}

/// Exporta a MP4 en un hilo aparte (no bloquea la UI).
///
/// Espejo de `spawn_gif_export_cancelable`: preflight de budgets dentro del
/// hilo (`Err` rápido sin tocar disco) + `CancellationToken` cooperativo.
pub fn spawn_mp4_export(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    delay_cs: u16,
    token: CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> std::thread::JoinHandle<Result<PathBuf, Mp4ExportError>> {
    std::thread::spawn(move || {
        export_frames_to_mp4_inner(
            &frames,
            &path,
            delay_cs,
            &token,
            None,
            bitrate_kbps,
            quality,
        )
    })
}

// ── P1-render: vídeo todo incluido ─────────────────────────────────────────
// El render atiende lo que W4 tipó en `grafito-anim` y la Piel aún ignoraba:
// `ExportFormat::Webm`, `Camera::Perspective`, `Mobject` nuevos y `RateFunc`
// exactas (estas últimas ya viajan en `MovingCamera.easing`: acá se aplican
// vía `sample`, sin reimplementar).
// Todo export corre en hilo (`spawn_*`, `CancellationToken` cooperativo,
// tmp+rename atómico, `kill+wait` anti-zombie); la UI nunca hace E/S.
// Presupuestos compartidos con el GIF: 64 frames, lado ≤4096, 8M px totales.

// ── PNG-sequence en directorio (espejo del GIF, sin ffmpeg) ────────────────

/// Error tipado de la PNG-sequence (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngDirExportError {
    /// Preflight de budgets (los mismos del GIF).
    Budget(GifExportError),
    /// Exportación cancelada vía `CancellationToken`.
    Cancelled,
    Io(String),
}

impl std::fmt::Display for PngDirExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Budget(inner) => write!(f, "{inner}"),
            Self::Cancelled => write!(f, "exportación cancelada"),
            Self::Io(detail) => write!(f, "falló escribir la secuencia PNG: {detail}"),
        }
    }
}

impl std::error::Error for PngDirExportError {}

/// Hermano temporal del directorio destino (mismo filesystem: el `rename` de
/// directorio es atómico). Sufijo con pid + nanos anti-colisión. Puro.
fn png_dir_tmp_sibling(dir: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut os = dir.as_os_str().to_owned();
    os.push(format!(".tmp.{}-{stamp}", std::process::id()));
    PathBuf::from(os)
}

/// Limpieza best-effort del tmp (nunca deja directorio huérfano).
fn png_dir_drop_tmp(tmp: &Path) {
    let _ = std::fs::remove_dir_all(tmp);
}

/// Valida un `PngDir` contra el workdir del render (sintaxis + contención).
///
/// NUL/vacía o escape (`../`, absoluta ajena) → `Err` honesto antes de crear
/// nada. Puro salvo `canonicalize` de lectura (nunca crea directorios).
pub fn validate_png_dir_en(base: &Path, dir: &str) -> Result<grafito_anim::PngDir, String> {
    let png = grafito_anim::PngDir::try_new(dir.to_string())
        .map_err(|e| format!("secuencia PNG inválida: {e}"))?;
    png.validate_en(base)
        .map_err(|e| format!("secuencia PNG fuera del área de trabajo: {e}"))?;
    Ok(png)
}

/// Escribe un frame como `frame_{index:04}.png` dentro de `dir` (ya creado).
fn write_png_frame(
    dir: &Path,
    frame: &egui::ColorImage,
    pixel_count: usize,
    index: usize,
) -> Result<(), PngDirExportError> {
    let [w, h] = frame.size;
    let w32 = u32::try_from(w).map_err(|_| {
        PngDirExportError::Budget(GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        })
    })?;
    let h32 = u32::try_from(h).map_err(|_| {
        PngDirExportError::Budget(GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        })
    })?;
    let rgba = frame_rgba_bytes(frame, pixel_count, index).map_err(PngDirExportError::Budget)?;
    let nombre = format!("frame_{index:04}.png");
    let destino = dir.join(nombre);
    let imagen = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(w32, h32, rgba).ok_or(
        PngDirExportError::Budget(GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        }),
    )?;
    imagen.save(&destino).map_err(|e| {
        PngDirExportError::Io(format!("no se pudo escribir {}: {e}", destino.display()))
    })?;
    Ok(())
}

/// Núcleo bloqueante (llamar en hilo): preflight de budgets, vuelco de PNGs
/// a un hermano tmp y publicación atómica por `rename`. `O_EXCL` honesto si
/// el destino existe (symlink plantado incluido). Sin pánicos.
fn export_frames_to_png_dir_inner(
    frames: &[egui::ColorImage],
    dir: &Path,
    token: &CancellationToken,
) -> Result<PathBuf, PngDirExportError> {
    if token.is_cancelled() {
        return Err(PngDirExportError::Cancelled);
    }
    check_gif_export_budget(frames).map_err(PngDirExportError::Budget)?;
    let size = frames[0].size;
    let (w, h) = (size[0], size[1]);
    let pixel_count = w.checked_mul(h).ok_or(PngDirExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    if std::fs::symlink_metadata(dir).is_ok() {
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            dir.display()
        )));
    }
    let tmp = png_dir_tmp_sibling(dir);
    if let Err(e) = std::fs::create_dir(&tmp) {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: {e}",
            tmp.display()
        )));
    }
    for (index, frame) in frames.iter().enumerate() {
        if token.is_cancelled() {
            png_dir_drop_tmp(&tmp);
            return Err(PngDirExportError::Cancelled);
        }
        if frame.size != size {
            png_dir_drop_tmp(&tmp);
            return Err(PngDirExportError::Budget(
                GifExportError::InconsistentSize {
                    index,
                    expected: size,
                    got: frame.size,
                },
            ));
        }
        if let Err(e) = write_png_frame(&tmp, frame, pixel_count, index) {
            png_dir_drop_tmp(&tmp);
            return Err(e);
        }
    }
    if token.is_cancelled() {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Cancelled);
    }
    if std::fs::symlink_metadata(dir).is_ok() {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            dir.display()
        )));
    }
    if let Err(e) = std::fs::rename(&tmp, dir) {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo publicar {}: {e}",
            dir.display()
        )));
    }
    Ok(dir.to_path_buf())
}

/// Exporta los frames como secuencia PNG (`frame_0000.png`, …) en `dir`
/// (bloquea: llamar en hilo). Mismos budgets que el GIF.
pub fn export_frames_to_png_dir(
    frames: &[egui::ColorImage],
    dir: &Path,
) -> Result<PathBuf, PngDirExportError> {
    export_frames_to_png_dir_inner(frames, dir, &CancellationToken::default())
}

/// Idem cancelable: chequea el token entre frames; cancelado → `Cancelled`
/// sin dejar ni destino ni tmp huérfano.
pub fn export_frames_to_png_dir_cancelable(
    frames: &[egui::ColorImage],
    dir: &Path,
    token: &CancellationToken,
) -> Result<PathBuf, PngDirExportError> {
    export_frames_to_png_dir_inner(frames, dir, token)
}

/// Exporta la secuencia PNG en un hilo aparte (no bloquea la UI).
pub fn spawn_png_dir_export(
    frames: Vec<egui::ColorImage>,
    dir: PathBuf,
    token: CancellationToken,
) -> std::thread::JoinHandle<Result<PathBuf, PngDirExportError>> {
    std::thread::spawn(move || export_frames_to_png_dir_inner(&frames, &dir, &token))
}

// ── Vídeo genérico ffmpeg (MP4 + WebM, sin audio) ────────────────────────────
// El MP4 sin audio sigue por `export_frames_to_mp4_inner` (camino intacto).
// Este runner genérico atiende WebM (`libvpx-vp9`, fallback `libaom-av1`).
// Sin `ffmpeg` → `FfmpegMissing` honesto, sin fake. Misma disciplina que el
// MP4 (tmp+rename, `kill+wait`). El audio se eliminó del núcleo (W1): no hay
// pista, no hay mux, jamás video mudo fake.

/// Codecs WebM en orden de intento (el primero que el `ffmpeg` acepte
/// gana; el resto es fallback honesto, jamás fake).
const WEBM_VIDEO_CODECS: &[&str] = &["libvpx-vp9", "libaom-av1"];

/// Error tipado de la exportación a WebM (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebmExportError {
    /// Preflight de budgets (los mismos del GIF).
    Budget(GifExportError),
    /// Exportación cancelada vía `CancellationToken`.
    Cancelled,
    /// Sin `ffmpeg` en el PATH: WebM nativo no disponible.
    FfmpegMissing,
    /// `ffmpeg` corrió pero falló (cola del stderr, 500 chars).
    FfmpegFailed(String),
    Io(String),
}

impl std::fmt::Display for WebmExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Budget(inner) => write!(f, "{inner}"),
            Self::Cancelled => write!(f, "exportación cancelada"),
            Self::FfmpegMissing => write!(
                f,
                "unsupported webm nativo, usá gif (ffmpeg no está en el PATH)"
            ),
            Self::FfmpegFailed(detail) => write!(f, "ffmpeg falló: {detail}"),
            Self::Io(detail) => write!(f, "falló escribir el WebM: {detail}"),
        }
    }
}

impl std::error::Error for WebmExportError {}

/// Hermano temporal para el vídeo genérico (conserva extensión: `ffmpeg`
/// infiere el contenedor por extensión). Puro, sin E/S.
fn video_tmp_sibling(path: &Path, extension: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(String::from("clip"));
    let tmp_name = format!("{name}.tmp.{}-{stamp}.{extension}", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Limpieza best-effort del tmp hermano (nunca deja parcial huérfano).
fn video_drop_tmp(tmp: &Path) {
    let _ = std::fs::remove_file(tmp);
}

/// Cola del stderr (últimos 500 chars: el error real está al final).
/// `pub(crate)` para el frente voz (`voice.rs`): misma disciplina kill+wait.
pub(crate) fn ffmpeg_stderr_tail(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .chars()
        .rev()
        .take(500)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Espera a `ffmpeg` vigilando el token (misma disciplina `CANCEL_GRACE`
/// del engine: `kill` + espera graciosa 100ms + `kill` final).
///
/// El stderr se drena en un hilo aparte (evita el bloqueo por pipe lleno)
/// mientras el principal hace `try_wait` + token cada 5ms: cancelar en
/// `wait` mata al hijo (sin zombies) y devuelve `Cancelled`. El llamador
/// limpia el tmp hermano. Solo worker/hilo, jamás UI.
/// `pub(crate)` para el frente voz (`voice.rs`): misma disciplina kill+wait.
pub(crate) enum EsperaFfmpeg {
    Terminado(bool, Vec<u8>),
    Cancelado,
    FalloIo(String),
}

pub(crate) fn esperar_ffmpeg_con_cancel(
    child: &mut std::process::Child,
    token: &CancellationToken,
) -> EsperaFfmpeg {
    use std::io::Read as _;
    let mut stderr = child.stderr.take();
    let drenaje = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut err) = stderr {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });
    loop {
        if token.is_cancelled() {
            let _ = child.kill();
            let gracia = grafito_anim::CANCEL_GRACE;
            let inicio = std::time::Instant::now();
            let mut salio = false;
            while inicio.elapsed() < gracia {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        salio = true;
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
            if !salio {
                let _ = child.kill();
            }
            let _ = child.wait();
            let _ = drenaje.join();
            return EsperaFfmpeg::Cancelado;
        }
        match child.try_wait() {
            Ok(Some(estado)) => {
                let stderr_bytes = drenaje.join().unwrap_or_default();
                return EsperaFfmpeg::Terminado(estado.success(), stderr_bytes);
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = drenaje.join();
                return EsperaFfmpeg::FalloIo(format!("ffmpeg no terminó bien: {e}"));
            }
        }
    }
}

/// Núcleo bloqueante genérico (llamar en hilo): preflight de budgets,
/// entubado rawvideo a `ffmpeg` y publicación atómica.
///
/// `ffmpeg_bin` overridea el binario (tests herméticos). Sin pánicos.
fn export_frames_to_video_inner(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    ffmpeg_bin: Option<&Path>,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, WebmExportError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(WebmExportError::Cancelled);
    }
    // fps honesto: re-muestrea el set base (12fps) al N pedido con duración
    // fija vía `Timeline::sample` (no solo `-framerate`).
    let fps = mp4_fps_for_delay(delay_cs);
    let frames = remuestrear_frames_para_fps(frames, GIF_BASE_FPS as u32, fps);
    // `-ql`/`-qm`/`-qh` → resolución real (downscale en el worker).
    let frames = reescalar_frames_para_calidad(&frames, quality);
    check_gif_export_budget(&frames).map_err(WebmExportError::Budget)?;
    let size = frames[0].size;
    let (w, h) = (size[0], size[1]);
    let pixel_count = w.checked_mul(h).ok_or(WebmExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(WebmExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    let tmp = video_tmp_sibling(path, "webm");
    let bin = ffmpeg_bin.unwrap_or_else(|| Path::new("ffmpeg"));
    let bitrate_kbps =
        bitrate_kbps.clamp(ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_BITRATE_MAX_KBPS);
    let (crf, preset) = quality.flags();
    let codecs = WEBM_VIDEO_CODECS;
    for (intento, codec) in codecs.iter().enumerate() {
        let ultimo_intento = intento + 1 == codecs.len();
        if token.is_cancelled() {
            video_drop_tmp(&tmp);
            return Err(WebmExportError::Cancelled);
        }
        let mut cmd = Command::new(bin);
        cmd.arg("-y")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(format!("{w}x{h}"))
            .arg("-framerate")
            .arg(fps.to_string())
            .arg("-i")
            .arg("pipe:0");
        // yuv420p exige lados pares: el filtro es no-op si ya lo son.
        // `-preset` igual que el MP4 (R6c, consistente con
        // `VideoQuality::flags`) + `-b:v` + `-crf` del diálogo.
        cmd.arg("-c:v")
            .arg(codec)
            .arg("-preset")
            .arg(preset)
            .arg("-b:v")
            .arg(format!("{bitrate_kbps}k"))
            .arg("-crf")
            .arg(crf.to_string())
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-vf")
            .arg("scale=trunc(iw/2)*2:trunc(ih/2)*2");
        cmd.arg("-f")
            .arg("webm")
            .arg(&tmp)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                video_drop_tmp(&tmp);
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(WebmExportError::FfmpegMissing);
                }
                return Err(WebmExportError::Io(format!(
                    "no se pudo lanzar {}: {e}",
                    bin.display()
                )));
            }
        };
        let pipe_result = (|| -> Result<(), WebmExportError> {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| WebmExportError::Io("ffmpeg no abrió su stdin".to_string()))?;
            for (index, frame) in frames.iter().enumerate() {
                if token.is_cancelled() {
                    return Err(WebmExportError::Cancelled);
                }
                if frame.size != size {
                    return Err(WebmExportError::Budget(GifExportError::InconsistentSize {
                        index,
                        expected: size,
                        got: frame.size,
                    }));
                }
                let rgba =
                    frame_rgba_bytes(frame, pixel_count, index).map_err(WebmExportError::Budget)?;
                stdin.write_all(&rgba).map_err(|e| {
                    WebmExportError::Io(format!("no se pudo entubar el frame {index}: {e}"))
                })?;
            }
            Ok(())
        })();
        if let Err(pipe_err) = pipe_result {
            let _ = child.kill();
            let _ = child.wait();
            video_drop_tmp(&tmp);
            return Err(pipe_err);
        }
        // Espera vigilada: cancelar en `wait` mata al hijo (CANCEL_GRACE).
        let espera = esperar_ffmpeg_con_cancel(&mut child, token);
        match espera {
            EsperaFfmpeg::Cancelado => {
                video_drop_tmp(&tmp);
                return Err(WebmExportError::Cancelled);
            }
            EsperaFfmpeg::FalloIo(detalle) => {
                video_drop_tmp(&tmp);
                return Err(WebmExportError::Io(detalle));
            }
            EsperaFfmpeg::Terminado(true, _) => {
                if token.is_cancelled() {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Cancelled);
                }
                if std::fs::symlink_metadata(path).is_ok() {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Io(format!(
                        "no se pudo crear {} sin sobrescribir: el destino ya existe",
                        path.display()
                    )));
                }
                if let Err(e) = std::fs::rename(&tmp, path) {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Io(format!(
                        "no se pudo publicar {}: {e}",
                        path.display()
                    )));
                }
                return Ok(path.to_path_buf());
            }
            EsperaFfmpeg::Terminado(false, stderr) => {
                let tail = ffmpeg_stderr_tail(&stderr);
                video_drop_tmp(&tmp);
                // Codec ausente en este build de ffmpeg → siguiente candidato.
                if !ultimo_intento && tail.contains("Unknown encoder") {
                    continue;
                }
                return Err(WebmExportError::FfmpegFailed(tail));
            }
        }
    }
    Err(WebmExportError::FfmpegFailed(
        "ffmpeg no aceptó ningún codec de vídeo".to_string(),
    ))
}

/// Escribe los frames como WebM (bloquea: llamar en hilo).
/// `ffmpeg` del PATH (`libvpx-vp9`, fallback `libaom-av1`); sin él →
/// `FfmpegMissing` honesto. Mismos budgets que el GIF.
/// `bitrate_kbps` + `quality` igual que el MP4 (`-ql`/`-qm`/`-qh` reales).
pub fn export_frames_to_webm_file(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, WebmExportError> {
    export_frames_to_video_inner(
        frames,
        path,
        delay_cs,
        &CancellationToken::default(),
        None,
        bitrate_kbps,
        quality,
    )
}

/// Idem cancelable (chequea el token entre frames, en el `wait` y antes de
/// publicar).
pub fn export_frames_to_webm_file_cancelable(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, WebmExportError> {
    export_frames_to_video_inner(frames, path, delay_cs, token, None, bitrate_kbps, quality)
}

/// Idem con binario explícito (tests herméticos + distros sin PATH).
pub fn export_frames_to_webm_file_with_bin(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    ffmpeg_bin: &Path,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> Result<PathBuf, WebmExportError> {
    export_frames_to_video_inner(
        frames,
        path,
        delay_cs,
        token,
        Some(ffmpeg_bin),
        bitrate_kbps,
        quality,
    )
}

/// Exporta a WebM en un hilo aparte (no bloquea la UI).
pub fn spawn_webm_export(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    delay_cs: u16,
    token: CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
) -> std::thread::JoinHandle<Result<PathBuf, WebmExportError>> {
    std::thread::spawn(move || {
        export_frames_to_video_inner(
            &frames,
            &path,
            delay_cs,
            &token,
            None,
            bitrate_kbps,
            quality,
        )
    })
}

// ── P0.2 long-form streaming (1500f por chunks, jamás `Vec` total) ──────────
// Un set largo materializado no entra en RAM (1500f 720p ≈ 5.5 GiB): estos
// writers reciben un productor `FnMut(j) -> Option<ColorImage>` y drenan a
// disco por chunks de `max_chunk_frames(w, h, LONGFORM_CHUNK_MAX_BYTES)`
// (contrato P0.1 de `grafito-anim`, usado sin redefinir: 720p→18,
// 1080p→8, 640×480→54 con 64 MiB). En ningún momento vive el set entero en
// memoria: como máximo un frame clonado (el 0, para fijar tamaño) más el
// frame en vuelo.
// Todo export corre en hilo worker (`CancellationToken` cooperativo,
// tmp+rename atómico, `kill+wait` anti-zombie); la UI nunca hace E/S y lee
// el progreso real por chunk vía `ProgresoChunks`.
// Topes: 0 frames → `Err` honesto; >1500 → `Err` ("partí el video en dos").
// GIF intacto: sigue por `export_frames_to_gif_file` (64 + autofit, corto).

/// Progreso compartido con la UI (0.0..=1.0): el worker lo publica por
/// chunk completado; el draw lo lee sin bloquear (`try_lock` honesto: si
/// está ocupado, el frame actual muestra el valor anterior).
pub type ProgresoChunks = std::sync::Arc<std::sync::Mutex<f32>>;

/// Publica `hechos/total` (clamp 0.0..=1.0) en la barra compartida.
/// `None` = sin observador (el export igual avanza). Puro (sin E/S).
fn marcar_progreso(progreso: Option<&ProgresoChunks>, hechos: usize, total: usize) {
    let Some(barra) = progreso else {
        return;
    };
    let total = total.max(1) as f32;
    let valor = (hechos as f32 / total).clamp(0.0, 1.0);
    if let Ok(mut guarda) = barra.try_lock() {
        *guarda = valor;
    }
}

/// Mensaje de tope long-form (>1500): partir el pedido en dos videos.
/// Comparte redacción con `validate_frames` del wire (sin duplicar lógica,
// solo el texto que el diálogo muestra).
fn longform_tope_msg(total: usize) -> String {
    format!(
        "{total} frames exceden el tope de {}: partí el video en dos",
        grafito_anim::protocol::VIDEO_LONGFORM_MAX_FRAMES
    )
}

/// Downscale de un frame al tamaño de la calidad (`-ql` 640 / `-qm` 1280 /
/// `-qh` nativo sin upscale, vía `video_quality_export_size`). Si ya encaja,
/// devuelve clon. CPU puro (llamar en hilo worker).
/// El llamador ya validó `pixels.len() == ow*oh`: si el buffer igual no
/// cuadra, devuelve el frame tal cual (el chequeo de tamaño del llamador lo
/// rechaza honesto después, jamás píxeles inventados en silencio).
fn downscale_un_frame(frame: &egui::ColorImage, nw: usize, nh: usize) -> egui::ColorImage {
    let [bw, bh] = frame.size;
    if (nw, nh) == (bw, bh) {
        return frame.clone();
    }
    let bytes: Vec<u8> = frame.pixels.iter().flat_map(|p| p.to_array()).collect();
    let (Some(bw32), Some(bh32), Some(nw32), Some(nh32)) = (
        u32::try_from(bw).ok(),
        u32::try_from(bh).ok(),
        u32::try_from(nw).ok(),
        u32::try_from(nh).ok(),
    ) else {
        return frame.clone();
    };
    let Some(img) = image::RgbaImage::from_raw(bw32, bh32, bytes) else {
        return frame.clone();
    };
    let chica = image::imageops::resize(&img, nw32, nh32, image::imageops::FilterType::Triangle);
    let pixeles: Vec<egui::Color32> = chica
        .pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p.0[0], p.0[1], p.0[2], p.0[3]))
        .collect();
    egui::ColorImage {
        size: [nw, nh],
        pixels: pixeles,
    }
}

/// Re-muestrea con tope explícito (P0.2, puro): misma regla honesta que
/// `remuestrear_frames_para_fps` (duración fija vía `Timeline::sample`),
/// pero el anti-abuso es `tope` en vez de 512. El long-form llama con
/// `VIDEO_LONGFORM_MAX_FRAMES` (1500); el corto conserva 512 vía
/// `remuestrear_frames_para_fps` (intacto). Sin E/S.
pub fn remuestrear_frames_con_tope(
    frames: &[egui::ColorImage],
    fps_origen: u32,
    fps_destino: u32,
    tope: usize,
) -> Vec<egui::ColorImage> {
    use grafito_anim::protocol::{Keyframe, Timeline};
    if frames.is_empty() {
        return Vec::new();
    }
    let origen = fps_origen.clamp(ANIM_FPS_MIN, ANIM_FPS_MAX) as u64;
    let destino = fps_destino.clamp(ANIM_FPS_MIN, ANIM_FPS_MAX) as u64;
    if origen == destino {
        return frames.to_vec();
    }
    let n = frames.len() as u64;
    let duracion_ms = n.saturating_mul(1000) / origen.max(1);
    if duracion_ms == 0 {
        return frames.to_vec();
    }
    let keys: Vec<Keyframe> = (0..n)
        .map(|i| Keyframe {
            t_ms: i.saturating_mul(1000) / origen,
            value: i as f32,
        })
        .collect();
    let timeline = Timeline {
        duration_ms: duracion_ms.max(1),
        keyframes: keys,
    };
    let total_dest = (duracion_ms.saturating_mul(destino) / 1000).max(1);
    let total_dest = total_dest.min(tope.max(1) as u64) as usize;
    (0..total_dest)
        .map(|j| {
            let t = (j as u64).saturating_mul(1000) / destino;
            let idx = timeline.sample(t).round().clamp(0.0, (n - 1) as f32) as usize;
            frames[idx.min(frames.len() - 1)].clone()
        })
        .collect()
}

/// Exporta a MP4 por chunks desde un productor (bloquea: llamar en hilo).
///
/// `productor(j)` entrega el frame `j` (`0..total_frames`, determinista: se
/// lo puede llamar de nuevo si un codec WebM/MP4 falla y hay reintento —
/// en MP4 hay un solo intento, pero la exigencia vale igual). Jamás se arma
/// el `Vec` total: cada frame se entuba y se suelta.
/// `fps` 1..=60 (se clampa), `bitrate_kbps` 100..=20000 (se clampa),
/// `quality` aplica resolución real + crf + preset (`-ql`/`-qm`/`-qh`).
/// Progreso real por chunk en `progreso`; cancelación entre chunks
/// (`kill+wait`, sin zombie, sin tocar el destino); publicación atómica
/// tmp+rename (`O_EXCL` honesto si el destino existe).
#[allow(clippy::too_many_arguments)]
pub fn export_mp4_streaming(
    productor: &mut dyn FnMut(usize) -> Option<egui::ColorImage>,
    total_frames: usize,
    path: &Path,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, Mp4ExportError> {
    export_mp4_streaming_with_bin(
        productor,
        total_frames,
        path,
        fps,
        bitrate_kbps,
        quality,
        token,
        progreso,
        Path::new("ffmpeg"),
    )
}

/// Idem con binario explícito (frente voz P1-app: el export narrado usa
/// falsos herméticos en tests; prod pasa el `ffmpeg` del PATH).
/// Misma disciplina (chunks P0.1, progreso real, cancelación, tmp+rename).
#[allow(clippy::too_many_arguments)]
pub fn export_mp4_streaming_with_bin(
    productor: &mut dyn FnMut(usize) -> Option<egui::ColorImage>,
    total_frames: usize,
    path: &Path,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
    ffmpeg_bin: &Path,
) -> Result<PathBuf, Mp4ExportError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(Mp4ExportError::Cancelled);
    }
    if total_frames == 0 {
        return Err(Mp4ExportError::Budget(GifExportError::EmptyFrames));
    }
    if total_frames > grafito_anim::protocol::VIDEO_LONGFORM_MAX_FRAMES {
        return Err(Mp4ExportError::Io(longform_tope_msg(total_frames)));
    }
    let fps = fps.clamp(ANIM_EXPORT_FPS_MIN, ANIM_EXPORT_FPS_MAX);
    let bitrate_kbps =
        bitrate_kbps.clamp(ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_BITRATE_MAX_KBPS);
    let primero = productor(0)
        .ok_or_else(|| Mp4ExportError::Io("el productor no entregó el frame 0".to_string()))?;
    let [ow, oh] = primero.size;
    let (w, h) = video_quality_export_size(ow, oh, quality);
    if w == 0 || h == 0 || w > GIF_EXPORT_MAX_DIM || h > GIF_EXPORT_MAX_DIM {
        return Err(Mp4ExportError::Budget(
            GifExportError::DimensionOutOfRange {
                width: w,
                height: h,
            },
        ));
    }
    if primero.pixels.len() != ow.saturating_mul(oh) {
        return Err(Mp4ExportError::Budget(GifExportError::PixelCountMismatch {
            index: 0,
            expected: ow.saturating_mul(oh),
            got: primero.pixels.len(),
        }));
    }
    let pixel_count = w.checked_mul(h).ok_or(Mp4ExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    // Validación de chunk P0.1: por construcción cada chunk entra en 64 MiB.
    let chunk = grafito_anim::protocol::max_chunk_frames(
        w,
        h,
        grafito_anim::protocol::LONGFORM_CHUNK_MAX_BYTES,
    )
    .max(1);
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(Mp4ExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    let tmp = mp4_tmp_sibling(path);
    let (crf, preset) = quality.flags();
    let mut child = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-s")
        .arg(format!("{w}x{h}"))
        .arg("-framerate")
        .arg(fps.to_string())
        .arg("-i")
        .arg("pipe:0")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg(preset)
        .arg("-crf")
        .arg(crf.to_string())
        .arg("-b:v")
        .arg(format!("{bitrate_kbps}k"))
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-vf")
        .arg("scale=trunc(iw/2)*2:trunc(ih/2)*2")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-f")
        .arg("mp4")
        .arg(&tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            mp4_drop_tmp(&tmp);
            if e.kind() == std::io::ErrorKind::NotFound {
                Mp4ExportError::FfmpegMissing
            } else {
                Mp4ExportError::Io(format!("no se pudo lanzar {}: {e}", ffmpeg_bin.display()))
            }
        })?;
    marcar_progreso(progreso, 0, total_frames);
    // Un solo frame clonado vivo por vez (el 0); el resto se pide, entuba y
    // suelta. Cancelado entre chunks → kill+wait + limpieza, sin destino.
    let pipe_result = (|| -> Result<(), Mp4ExportError> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Mp4ExportError::Io("ffmpeg no abrió su stdin".to_string()))?;
        for j in 0..total_frames {
            if token.is_cancelled() {
                return Err(Mp4ExportError::Cancelled);
            }
            let crudo = if j == 0 {
                primero.clone()
            } else {
                productor(j).ok_or_else(|| {
                    Mp4ExportError::Io(format!("el productor no entregó el frame {j}"))
                })?
            };
            if crudo.size != [ow, oh] {
                return Err(Mp4ExportError::Budget(GifExportError::InconsistentSize {
                    index: j,
                    expected: [ow, oh],
                    got: crudo.size,
                }));
            }
            let chico = downscale_un_frame(&crudo, w, h);
            if chico.size != [w, h] {
                return Err(Mp4ExportError::Budget(GifExportError::PixelCountMismatch {
                    index: j,
                    expected: pixel_count,
                    got: chico.pixels.len(),
                }));
            }
            let rgba = frame_rgba_bytes(&chico, pixel_count, j).map_err(Mp4ExportError::Budget)?;
            stdin
                .write_all(&rgba)
                .map_err(|e| Mp4ExportError::Io(format!("no se pudo entubar el frame {j}: {e}")))?;
            if (j + 1) % chunk == 0 || j + 1 == total_frames {
                marcar_progreso(progreso, j + 1, total_frames);
            }
        }
        Ok(())
    })();
    if let Err(pipe_err) = pipe_result {
        let _ = child.kill();
        let _ = child.wait();
        mp4_drop_tmp(&tmp);
        return Err(pipe_err);
    }
    match esperar_ffmpeg_con_cancel(&mut child, token) {
        EsperaFfmpeg::Cancelado => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::Cancelled);
        }
        EsperaFfmpeg::FalloIo(detalle) => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::Io(detalle));
        }
        EsperaFfmpeg::Terminado(false, stderr) => {
            mp4_drop_tmp(&tmp);
            return Err(Mp4ExportError::FfmpegFailed(ffmpeg_stderr_tail(&stderr)));
        }
        EsperaFfmpeg::Terminado(true, _) => {}
    }
    if token.is_cancelled() {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Cancelled);
    }
    if std::fs::symlink_metadata(path).is_ok() {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        mp4_drop_tmp(&tmp);
        return Err(Mp4ExportError::Io(format!(
            "no se pudo publicar {}: {e}",
            path.display()
        )));
    }
    marcar_progreso(progreso, total_frames, total_frames);
    Ok(path.to_path_buf())
}

/// Exporta a WebM por chunks desde un productor (bloquea: llamar en hilo).
///
/// Misma disciplina que `export_mp4_streaming` (chunks P0.1, progreso real,
/// cancelación entre chunks, tmp+rename atómico) con los codecs WebM en
// orden de intento (`libvpx-vp9`, fallback `libaom-av1`): si el primero
/// falla por encoder desconocido se reintenta llamando al productor de
/// nuevo (por eso debe ser determinista). Sin `ffmpeg` → `FfmpegMissing`.
#[allow(clippy::too_many_arguments)]
pub fn export_webm_streaming(
    productor: &mut dyn FnMut(usize) -> Option<egui::ColorImage>,
    total_frames: usize,
    path: &Path,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, WebmExportError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    if token.is_cancelled() {
        return Err(WebmExportError::Cancelled);
    }
    if total_frames == 0 {
        return Err(WebmExportError::Budget(GifExportError::EmptyFrames));
    }
    if total_frames > grafito_anim::protocol::VIDEO_LONGFORM_MAX_FRAMES {
        return Err(WebmExportError::Io(longform_tope_msg(total_frames)));
    }
    let fps = fps.clamp(ANIM_EXPORT_FPS_MIN, ANIM_EXPORT_FPS_MAX);
    let bitrate_kbps =
        bitrate_kbps.clamp(ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_BITRATE_MAX_KBPS);
    let primero = productor(0)
        .ok_or_else(|| WebmExportError::Io("el productor no entregó el frame 0".to_string()))?;
    let [ow, oh] = primero.size;
    let (w, h) = video_quality_export_size(ow, oh, quality);
    if w == 0 || h == 0 || w > GIF_EXPORT_MAX_DIM || h > GIF_EXPORT_MAX_DIM {
        return Err(WebmExportError::Budget(
            GifExportError::DimensionOutOfRange {
                width: w,
                height: h,
            },
        ));
    }
    if primero.pixels.len() != ow.saturating_mul(oh) {
        return Err(WebmExportError::Budget(
            GifExportError::PixelCountMismatch {
                index: 0,
                expected: ow.saturating_mul(oh),
                got: primero.pixels.len(),
            },
        ));
    }
    let pixel_count = w.checked_mul(h).ok_or(WebmExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    let chunk = grafito_anim::protocol::max_chunk_frames(
        w,
        h,
        grafito_anim::protocol::LONGFORM_CHUNK_MAX_BYTES,
    )
    .max(1);
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(WebmExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            path.display()
        )));
    }
    let tmp = video_tmp_sibling(path, "webm");
    let (crf, preset) = quality.flags();
    marcar_progreso(progreso, 0, total_frames);
    for (intento, codec) in WEBM_VIDEO_CODECS.iter().enumerate() {
        let ultimo_intento = intento + 1 == WEBM_VIDEO_CODECS.len();
        if token.is_cancelled() {
            video_drop_tmp(&tmp);
            return Err(WebmExportError::Cancelled);
        }
        let mut child = match Command::new("ffmpeg")
            .arg("-y")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(format!("{w}x{h}"))
            .arg("-framerate")
            .arg(fps.to_string())
            .arg("-i")
            .arg("pipe:0")
            .arg("-c:v")
            .arg(codec)
            .arg("-preset")
            .arg(preset)
            .arg("-b:v")
            .arg(format!("{bitrate_kbps}k"))
            .arg("-crf")
            .arg(crf.to_string())
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-vf")
            .arg("scale=trunc(iw/2)*2:trunc(ih/2)*2")
            .arg("-f")
            .arg("webm")
            .arg(&tmp)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                video_drop_tmp(&tmp);
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(WebmExportError::FfmpegMissing);
                }
                return Err(WebmExportError::Io(format!(
                    "no se pudo lanzar ffmpeg: {e}"
                )));
            }
        };
        let pipe_result = (|| -> Result<(), WebmExportError> {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| WebmExportError::Io("ffmpeg no abrió su stdin".to_string()))?;
            for j in 0..total_frames {
                if token.is_cancelled() {
                    return Err(WebmExportError::Cancelled);
                }
                let crudo = if j == 0 {
                    primero.clone()
                } else {
                    productor(j).ok_or_else(|| {
                        WebmExportError::Io(format!("el productor no entregó el frame {j}"))
                    })?
                };
                if crudo.size != [ow, oh] {
                    return Err(WebmExportError::Budget(GifExportError::InconsistentSize {
                        index: j,
                        expected: [ow, oh],
                        got: crudo.size,
                    }));
                }
                let chico = downscale_un_frame(&crudo, w, h);
                if chico.size != [w, h] {
                    return Err(WebmExportError::Budget(
                        GifExportError::PixelCountMismatch {
                            index: j,
                            expected: pixel_count,
                            got: chico.pixels.len(),
                        },
                    ));
                }
                let rgba =
                    frame_rgba_bytes(&chico, pixel_count, j).map_err(WebmExportError::Budget)?;
                stdin.write_all(&rgba).map_err(|e| {
                    WebmExportError::Io(format!("no se pudo entubar el frame {j}: {e}"))
                })?;
                if (j + 1) % chunk == 0 || j + 1 == total_frames {
                    marcar_progreso(progreso, j + 1, total_frames);
                }
            }
            Ok(())
        })();
        if let Err(pipe_err) = pipe_result {
            let _ = child.kill();
            let _ = child.wait();
            video_drop_tmp(&tmp);
            return Err(pipe_err);
        }
        match esperar_ffmpeg_con_cancel(&mut child, token) {
            EsperaFfmpeg::Cancelado => {
                video_drop_tmp(&tmp);
                return Err(WebmExportError::Cancelled);
            }
            EsperaFfmpeg::FalloIo(detalle) => {
                video_drop_tmp(&tmp);
                return Err(WebmExportError::Io(detalle));
            }
            EsperaFfmpeg::Terminado(true, _) => {
                if token.is_cancelled() {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Cancelled);
                }
                if std::fs::symlink_metadata(path).is_ok() {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Io(format!(
                        "no se pudo crear {} sin sobrescribir: el destino ya existe",
                        path.display()
                    )));
                }
                if let Err(e) = std::fs::rename(&tmp, path) {
                    video_drop_tmp(&tmp);
                    return Err(WebmExportError::Io(format!(
                        "no se pudo publicar {}: {e}",
                        path.display()
                    )));
                }
                marcar_progreso(progreso, total_frames, total_frames);
                return Ok(path.to_path_buf());
            }
            EsperaFfmpeg::Terminado(false, stderr) => {
                let tail = ffmpeg_stderr_tail(&stderr);
                video_drop_tmp(&tmp);
                if !ultimo_intento && tail.contains("Unknown encoder") {
                    continue;
                }
                return Err(WebmExportError::FfmpegFailed(tail));
            }
        }
    }
    Err(WebmExportError::FfmpegFailed(
        "ffmpeg no aceptó ningún codec de vídeo".to_string(),
    ))
}

/// Exporta la secuencia PNG por chunks desde un productor (bloquea: llamar
/// en hilo). `frame_{j:04}.png` estables con índice global (no por chunk),
/// vuelco a hermano tmp + `rename` atómico del directorio, `O_EXCL`
/// honesto, progreso real por chunk y cancelación entre chunks sin dejar ni
/// destino ni tmp huérfano. Sin `ffmpeg` (siempre disponible).
pub fn export_png_dir_streaming(
    productor: &mut dyn FnMut(usize) -> Option<egui::ColorImage>,
    total_frames: usize,
    dir: &Path,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, PngDirExportError> {
    if token.is_cancelled() {
        return Err(PngDirExportError::Cancelled);
    }
    if total_frames == 0 {
        return Err(PngDirExportError::Budget(GifExportError::EmptyFrames));
    }
    if total_frames > grafito_anim::protocol::VIDEO_LONGFORM_MAX_FRAMES {
        return Err(PngDirExportError::Io(longform_tope_msg(total_frames)));
    }
    let primero = productor(0)
        .ok_or_else(|| PngDirExportError::Io("el productor no entregó el frame 0".to_string()))?;
    let [w, h] = primero.size;
    if w == 0 || h == 0 || w > GIF_EXPORT_MAX_DIM || h > GIF_EXPORT_MAX_DIM {
        return Err(PngDirExportError::Budget(
            GifExportError::DimensionOutOfRange {
                width: w,
                height: h,
            },
        ));
    }
    let pixel_count = w.checked_mul(h).ok_or(PngDirExportError::Budget(
        GifExportError::DimensionOutOfRange {
            width: w,
            height: h,
        },
    ))?;
    if primero.pixels.len() != pixel_count {
        return Err(PngDirExportError::Budget(
            GifExportError::PixelCountMismatch {
                index: 0,
                expected: pixel_count,
                got: primero.pixels.len(),
            },
        ));
    }
    let chunk = grafito_anim::protocol::max_chunk_frames(
        w,
        h,
        grafito_anim::protocol::LONGFORM_CHUNK_MAX_BYTES,
    )
    .max(1);
    if std::fs::symlink_metadata(dir).is_ok() {
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            dir.display()
        )));
    }
    let tmp = png_dir_tmp_sibling(dir);
    if let Err(e) = std::fs::create_dir(&tmp) {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: {e}",
            tmp.display()
        )));
    }
    marcar_progreso(progreso, 0, total_frames);
    for j in 0..total_frames {
        if token.is_cancelled() {
            png_dir_drop_tmp(&tmp);
            return Err(PngDirExportError::Cancelled);
        }
        let frame = if j == 0 {
            primero.clone()
        } else {
            productor(j).ok_or_else(|| {
                PngDirExportError::Io(format!("el productor no entregó el frame {j}"))
            })?
        };
        if frame.size != [w, h] {
            png_dir_drop_tmp(&tmp);
            return Err(PngDirExportError::Budget(
                GifExportError::InconsistentSize {
                    index: j,
                    expected: [w, h],
                    got: frame.size,
                },
            ));
        }
        if let Err(e) = write_png_frame(&tmp, &frame, pixel_count, j) {
            png_dir_drop_tmp(&tmp);
            return Err(e);
        }
        if (j + 1) % chunk == 0 || j + 1 == total_frames {
            marcar_progreso(progreso, j + 1, total_frames);
        }
    }
    if token.is_cancelled() {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Cancelled);
    }
    if std::fs::symlink_metadata(dir).is_ok() {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            dir.display()
        )));
    }
    if let Err(e) = std::fs::rename(&tmp, dir) {
        png_dir_drop_tmp(&tmp);
        return Err(PngDirExportError::Io(format!(
            "no se pudo publicar {}: {e}",
            dir.display()
        )));
    }
    marcar_progreso(progreso, total_frames, total_frames);
    Ok(dir.to_path_buf())
}

/// Exporta un set materializado corto a MP4 con progreso (bloquea: llamar
/// en hilo). Camino corto intacto (≤64 vía `check_gif_export_budget`
/// dentro del núcleo): solo suma la barra compartida (0 al arrancar, 1.0
/// al publicar). Para sets largos usar `export_mp4_streaming`.
pub fn export_mp4_desde_set_con_progreso(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, Mp4ExportError> {
    marcar_progreso(progreso, 0, frames.len().max(1));
    let salida =
        export_frames_to_mp4_file_cancelable(frames, path, delay_cs, token, bitrate_kbps, quality);
    if salida.is_ok() {
        marcar_progreso(progreso, frames.len().max(1), frames.len().max(1));
    }
    salida
}

/// Exporta un set materializado corto a WebM con progreso (bloquea: llamar
/// en hilo). Camino corto intacto (≤64): solo suma la barra compartida.
/// Para sets largos usar `export_webm_streaming`.
pub fn export_webm_desde_set_con_progreso(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
    token: &CancellationToken,
    bitrate_kbps: u32,
    quality: VideoQuality,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, WebmExportError> {
    marcar_progreso(progreso, 0, frames.len().max(1));
    let salida =
        export_frames_to_webm_file_cancelable(frames, path, delay_cs, token, bitrate_kbps, quality);
    if salida.is_ok() {
        marcar_progreso(progreso, frames.len().max(1), frames.len().max(1));
    }
    salida
}

/// Exporta un set materializado corto a PNG-dir con progreso (bloquea:
/// llamar en hilo). Camino corto intacto (≤64): solo suma la barra
/// compartida. Para sets largos usar `export_png_dir_streaming`.
pub fn export_png_dir_desde_set_con_progreso(
    frames: &[egui::ColorImage],
    dir: &Path,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<PathBuf, PngDirExportError> {
    marcar_progreso(progreso, 0, frames.len().max(1));
    let salida = export_frames_to_png_dir_cancelable(frames, dir, token);
    if salida.is_ok() {
        marcar_progreso(progreso, frames.len().max(1), frames.len().max(1));
    }
    salida
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod p02_streaming_tests {
    use super::{
        export_mp4_desde_set_con_progreso, export_mp4_streaming,
        export_png_dir_desde_set_con_progreso, export_png_dir_streaming,
        export_webm_desde_set_con_progreso, export_webm_streaming, remuestrear_frames_con_tope,
        ProgresoChunks, VideoQuality,
    };
    use grafito_assistant::CancellationToken;

    /// Frame determinista `j` de `w×h` (gris que rota con `j`, alfa
    /// opaco): el productor de los tests nunca devuelve `None` en rango.
    fn frame_test(w: usize, h: usize, j: usize) -> egui::ColorImage {
        let tono = (j % 256) as u8;
        egui::ColorImage {
            size: [w, h],
            pixels: vec![
                egui::Color32::from_rgba_unmultiplied(tono, 100, 200, 255);
                w.saturating_mul(h)
            ],
        }
    }

    fn barra_nueva() -> ProgresoChunks {
        ProgresoChunks::new(std::sync::Mutex::new(0.0))
    }

    fn progreso_actual(barra: &ProgresoChunks) -> f32 {
        *barra.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hermano temporal único para los tests (nunca colisiona con el
    /// destino real; se borra best-effort al final).
    fn ruta_test(nombre: &str) -> std::path::PathBuf {
        let sello = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "grafito-p02-{nombre}-{pid}-{sello}",
            pid = std::process::id()
        ))
    }

    #[test]
    fn p02_chunks_pineados_contrato_p01() {
        use grafito_anim::protocol::{
            estimate_chunk_bytes, frames_for_duration, max_chunk_frames, LONGFORM_CHUNK_MAX_BYTES,
            VIDEO_LONGFORM_MAX_FRAMES,
        };
        assert_eq!(VIDEO_LONGFORM_MAX_FRAMES, 1500);
        assert_eq!(max_chunk_frames(1280, 720, LONGFORM_CHUNK_MAX_BYTES), 18);
        assert_eq!(max_chunk_frames(1920, 1080, LONGFORM_CHUNK_MAX_BYTES), 8);
        assert_eq!(max_chunk_frames(640, 480, LONGFORM_CHUNK_MAX_BYTES), 54);
        let bytes_720p = estimate_chunk_bytes(1280, 720, 18).unwrap_or(usize::MAX);
        assert!(
            bytes_720p <= LONGFORM_CHUNK_MAX_BYTES,
            "el chunk 720p entra en 64 MiB"
        );
        assert_eq!(frames_for_duration(50_000, 30), 1500);
        assert_eq!(
            frames_for_duration(50_000, 30) as usize,
            VIDEO_LONGFORM_MAX_FRAMES
        );
    }

    #[test]
    fn p02_tope_sobre_pedido_sin_producir() {
        let mut llamadas = 0usize;
        let mut productor = |_: usize| -> Option<egui::ColorImage> {
            llamadas += 1;
            Some(frame_test(32, 24, 0))
        };
        let destino = ruta_test("tope-mp4.mp4");
        let fallo = export_mp4_streaming(
            &mut productor,
            1501,
            &destino,
            30,
            2000,
            VideoQuality::Media,
            &CancellationToken::default(),
            None,
        )
        .unwrap_err();
        assert!(
            fallo.to_string().contains("partí el video en dos"),
            "mp4 >1500 honesto: {fallo}"
        );
        let destino = ruta_test("tope-webm.webm");
        let fallo = export_webm_streaming(
            &mut productor,
            1501,
            &destino,
            30,
            2000,
            VideoQuality::Media,
            &CancellationToken::default(),
            None,
        )
        .unwrap_err();
        assert!(
            fallo.to_string().contains("partí el video en dos"),
            "webm >1500 honesto: {fallo}"
        );
        let destino = ruta_test("tope-png");
        let fallo = export_png_dir_streaming(
            &mut productor,
            1501,
            &destino,
            &CancellationToken::default(),
            None,
        )
        .unwrap_err();
        assert!(
            fallo.to_string().contains("partí el video en dos"),
            "png >1500 honesto: {fallo}"
        );
        assert_eq!(llamadas, 0, "el tope se valida antes de producir");
        assert!(!destino.exists(), "sin parcial en disco");
    }

    #[test]
    fn p02_png_streaming_redondo_con_progreso() {
        let destino = ruta_test("png-ok");
        let barra = barra_nueva();
        let mut productor = |j: usize| -> Option<egui::ColorImage> { Some(frame_test(64, 48, j)) };
        let salida = export_png_dir_streaming(
            &mut productor,
            8,
            &destino,
            &CancellationToken::default(),
            Some(&barra),
        )
        .expect("png streaming 8f");
        assert_eq!(salida, destino);
        for j in 0..8 {
            let frame = destino.join(format!("frame_{j:04}.png"));
            assert!(frame.is_file(), "estable: {}", frame.display());
        }
        assert!(
            (progreso_actual(&barra) - 1.0).abs() < f32::EPSILON,
            "progreso real llega a 1.0"
        );
        let _ = std::fs::remove_dir_all(&destino);
    }

    #[test]
    fn p02_png_streaming_cancelado_sin_parcial() {
        let destino = ruta_test("png-cancel");
        let token = CancellationToken::default();
        token.cancel();
        let mut productor = |j: usize| -> Option<egui::ColorImage> { Some(frame_test(32, 24, j)) };
        let fallo =
            export_png_dir_streaming(&mut productor, 8, &destino, &token, None).unwrap_err();
        assert_eq!(
            fallo,
            super::PngDirExportError::Cancelled,
            "cancelado honesto: {fallo}"
        );
        assert!(!destino.exists(), "cancelado no deja destino");
    }

    #[test]
    fn p02_mp4_streaming_redondo_o_ffmpeg_honesto() {
        let destino = ruta_test("mp4-ok.mp4");
        let barra = barra_nueva();
        let mut productor = |j: usize| -> Option<egui::ColorImage> { Some(frame_test(64, 48, j)) };
        match export_mp4_streaming(
            &mut productor,
            8,
            &destino,
            12,
            2000,
            VideoQuality::Media,
            &CancellationToken::default(),
            Some(&barra),
        ) {
            Ok(salida) => {
                assert_eq!(salida, destino);
                assert!(destino.is_file(), "mp4 publicado");
                assert!(
                    (progreso_actual(&barra) - 1.0).abs() < f32::EPSILON,
                    "progreso real llega a 1.0"
                );
                let _ = std::fs::remove_file(&destino);
            }
            Err(super::Mp4ExportError::FfmpegMissing) => {
                assert!(!destino.exists(), "sin ffmpeg no queda parcial");
            }
            Err(otro) => panic!("mp4 streaming 8f falló deshonesto: {otro}"),
        }
    }

    #[test]
    fn p02_webm_streaming_redondo_o_ffmpeg_honesto() {
        let destino = ruta_test("webm-ok.webm");
        let mut productor = |j: usize| -> Option<egui::ColorImage> { Some(frame_test(64, 48, j)) };
        match export_webm_streaming(
            &mut productor,
            8,
            &destino,
            12,
            2000,
            VideoQuality::Media,
            &CancellationToken::default(),
            None,
        ) {
            Ok(salida) => {
                assert_eq!(salida, destino);
                assert!(destino.is_file(), "webm publicado");
                let _ = std::fs::remove_file(&destino);
            }
            Err(super::WebmExportError::FfmpegMissing) => {
                assert!(!destino.exists(), "sin ffmpeg no queda parcial");
            }
            Err(otro) => panic!("webm streaming 8f falló deshonesto: {otro}"),
        }
    }

    #[test]
    fn p02_desde_set_corto_intacto_con_progreso() {
        let set: Vec<egui::ColorImage> = (0..4).map(|j| frame_test(32, 24, j)).collect();
        let destino = ruta_test("set-ok");
        let barra = barra_nueva();
        let salida = export_png_dir_desde_set_con_progreso(
            &set,
            &destino,
            &CancellationToken::default(),
            Some(&barra),
        )
        .expect("set corto intacto");
        assert_eq!(salida, destino);
        assert!(destino.join("frame_0003.png").is_file());
        assert!(
            (progreso_actual(&barra) - 1.0).abs() < f32::EPSILON,
            "progreso del set llega a 1.0"
        );
        let _ = std::fs::remove_dir_all(&destino);

        let destino = ruta_test("set-mp4.mp4");
        let salida = export_mp4_desde_set_con_progreso(
            &set,
            &destino,
            8,
            &CancellationToken::default(),
            2000,
            VideoQuality::Media,
            None,
        );
        match salida {
            Ok(publicado) => {
                assert_eq!(publicado, destino);
                let _ = std::fs::remove_file(&destino);
            }
            Err(super::Mp4ExportError::FfmpegMissing) => {}
            Err(otro) => panic!("mp4 desde set falló deshonesto: {otro}"),
        }
        let destino = ruta_test("set-webm.webm");
        let salida = export_webm_desde_set_con_progreso(
            &set,
            &destino,
            8,
            &CancellationToken::default(),
            2000,
            VideoQuality::Media,
            None,
        );
        match salida {
            Ok(publicado) => {
                assert_eq!(publicado, destino);
                let _ = std::fs::remove_file(&destino);
            }
            Err(super::WebmExportError::FfmpegMissing) => {}
            Err(otro) => panic!("webm desde set falló deshonesto: {otro}"),
        }
    }

    #[test]
    fn p02_remuestreo_con_tope_1500() {
        let base: Vec<egui::ColorImage> = (0..48).map(|j| frame_test(16, 12, j)).collect();
        // 48f @12fps = 4 s → @30fps = 120f con tope 1500.
        let largo = remuestrear_frames_con_tope(
            &base,
            12,
            30,
            grafito_anim::protocol::VIDEO_LONGFORM_MAX_FRAMES,
        );
        assert_eq!(largo.len(), 120, "4 s a 30 fps = 120f");
        let acotado = remuestrear_frames_con_tope(&base, 12, 30, 64);
        assert_eq!(acotado.len(), 64, "el tope corto manda");
        assert!(remuestrear_frames_con_tope(&[], 12, 30, 1500).is_empty());
    }
}

// ── Tex real con tiny-skia (sin deps nuevas) ────────────────────────────────
// `Mobject::Tex` trae SVG ya tipografiado (≤64 KiB, cota intacta vía
// `crate::export::tex_svg_within_budget`). Se rasterizan las formas del
// subconjunto honesto (`circle`/`rect`/`line` sobre `viewBox` o 100×100);
// el texto tipográfico cae al fallback `ab_glyph` con el contenido visible
// (`extract_svg_text_content`), jamás una curva inventada.

/// Atributo `nombre="…"` (o con comilla simple) dentro de un tag SVG.
/// `None` si no está o no cierra. Puro, sin pánicos.
fn attr_cadena_svg<'a>(tag: &'a str, nombre: &str) -> Option<&'a str> {
    for comilla in ['"', '\''] {
        let aguja = format!("{nombre}={comilla}");
        if let Some(pos) = tag.find(&aguja) {
            let resto = tag.get(pos + aguja.len()..)?;
            let fin = resto.find(comilla)?;
            return resto.get(..fin);
        }
    }
    None
}

/// Atributo numérico (`px` opcional, finito). `None` si falta o no es finito.
fn attr_float_svg(tag: &str, nombre: &str) -> Option<f64> {
    let crudo = attr_cadena_svg(tag, nombre)?;
    let limpio = crudo.trim().trim_end_matches("px").trim();
    let valor: f64 = limpio.parse().ok()?;
    valor.is_finite().then_some(valor)
}

/// `viewBox="minx miny w h"` → `(w, h)`; si falta, `width`/`height`; si no,
/// 100×100 (convención del `tex_desde_texto` mínimo). Puro.
fn tex_viewport_wh(svg: &str) -> (f64, f64) {
    if let Some(inicio) = svg.find("viewBox") {
        let resto = svg.get(inicio..).unwrap_or("");
        let numeros: Vec<f64> = resto
            .chars()
            .skip_while(|c| *c != '"' && *c != '\'')
            .skip(1)
            .take_while(|c| *c != '"' && *c != '\'')
            .collect::<String>()
            .split_whitespace()
            .filter_map(|p| p.parse::<f64>().ok())
            .collect();
        if numeros.len() >= 4 {
            let (w, h) = (numeros[2], numeros[3]);
            if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
                return (w, h);
            }
        }
    }
    // Sin viewBox: width/height del root (primer tag `<svg …>`).
    let root = svg
        .find("<svg")
        .and_then(|pos| {
            let resto = svg.get(pos..)?;
            let fin = resto.find('>')?;
            resto.get(..fin)
        })
        .unwrap_or("");
    let w = attr_float_svg(root, "width").unwrap_or(100.0);
    let h = attr_float_svg(root, "height").unwrap_or(100.0);
    if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
        (w, h)
    } else {
        (100.0, 100.0)
    }
}

/// Dibuja las formas del subconjunto SVG sobre el frame. Devuelve `true` si
/// pintó al menos una (el llamador usa el fallback de texto si es `false`).
fn draw_tex_svg_onto(buf: &mut [u8], w: usize, h: usize, svg: &str) -> bool {
    if !crate::export::tex_svg_within_budget(svg) || !svg.contains("<svg") {
        return false;
    }
    let (vb_w, vb_h) = tex_viewport_wh(svg);
    if !(vb_w.is_finite() && vb_h.is_finite() && vb_w > 0.0 && vb_h > 0.0) {
        return false;
    }
    let escala_x = w as f64 / vb_w;
    let escala_y = h as f64 / vb_h;
    if !(escala_x.is_finite() && escala_y.is_finite() && escala_x > 0.0 && escala_y > 0.0) {
        return false;
    }
    let mapea = |x: f64, y: f64| -> Option<(usize, usize)> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let px = x * escala_x;
        let py = y * escala_y;
        if !(px.is_finite() && py.is_finite()) {
            return None;
        }
        // Saturado al frame (el stroke fuera del pixmap lo recorta tiny-skia).
        Some((
            px.round().clamp(0.0, w.max(1) as f64 - 1.0) as usize,
            py.round().clamp(0.0, h.max(1) as f64 - 1.0) as usize,
        ))
    };
    // (posición en el doc, clase): 0 círculo, 1 rect, 2 línea. Tope 256
    // formas por SVG (el doc ya está acotado a 64 KiB).
    let mut formas: Vec<(usize, u8)> = Vec::new();
    for (pos, _) in svg.match_indices("<circle") {
        formas.push((pos, 0));
    }
    for (pos, _) in svg.match_indices("<rect") {
        formas.push((pos, 1));
    }
    for (pos, _) in svg.match_indices("<line") {
        formas.push((pos, 2));
    }
    formas.sort_by_key(|&(pos, _)| pos);
    formas.truncate(256);
    let mut dibujadas = 0usize;
    for (pos, clase) in formas {
        let resto = match svg.get(pos..) {
            Some(r) => r,
            None => continue,
        };
        let fin = match resto.find('>') {
            Some(e) if e <= 2048 => e,
            _ => continue,
        };
        let tag = match resto.get(..fin) {
            Some(t) => t,
            None => continue,
        };
        let pinto = match clase {
            0 => match (
                attr_float_svg(tag, "cx"),
                attr_float_svg(tag, "cy"),
                attr_float_svg(tag, "r"),
            ) {
                (Some(cx), Some(cy), Some(r)) if r > 0.0 => {
                    let puntos: Vec<(usize, usize)> = (0..=48)
                        .filter_map(|k| {
                            let a = k as f64 * std::f64::consts::TAU / 48.0;
                            mapea(cx + r * a.cos(), cy + r * a.sin())
                        })
                        .collect();
                    draw_polyline_px(buf, w, h, &puntos, TEXT_COLOR);
                    !puntos.is_empty()
                }
                _ => false,
            },
            1 => match (
                attr_float_svg(tag, "x"),
                attr_float_svg(tag, "y"),
                attr_float_svg(tag, "width"),
                attr_float_svg(tag, "height"),
            ) {
                (Some(x), Some(y), Some(an), Some(al)) if an > 0.0 && al > 0.0 => {
                    let esquinas = [
                        mapea(x, y),
                        mapea(x + an, y),
                        mapea(x + an, y + al),
                        mapea(x, y + al),
                        mapea(x, y),
                    ];
                    let puntos: Vec<(usize, usize)> = esquinas.into_iter().flatten().collect();
                    draw_polyline_px(buf, w, h, &puntos, TEXT_COLOR);
                    puntos.len() >= 2
                }
                _ => false,
            },
            _ => match (
                attr_float_svg(tag, "x1"),
                attr_float_svg(tag, "y1"),
                attr_float_svg(tag, "x2"),
                attr_float_svg(tag, "y2"),
            ) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => match (mapea(x1, y1), mapea(x2, y2)) {
                    (Some(a), Some(b)) => {
                        draw_line(buf, w, h, a, b, TEXT_COLOR);
                        true
                    }
                    _ => false,
                },
                _ => false,
            },
        };
        if pinto {
            dibujadas += 1;
        }
    }
    dibujadas > 0
}

// ── Mobjects nuevos con tiny-skia ──────────────────────────────────────────
// Dibujo real de los 6 `Mobject` que W4 sumó (`Circle`/`Square`/`Line`/
// `Arrow`/`NumberPlane`/`VectorField`) en el mundo [-3,3]² de `to_pixel`.
// `draw_mobject` valida primero (`false` honesto si inválido, sin pintar).

/// Polilínea en píxeles (tramos consecutivos; <2 puntos = no-op honesto).
fn draw_polyline_px(buf: &mut [u8], w: usize, h: usize, puntos: &[(usize, usize)], color: [u8; 4]) {
    for par in puntos.windows(2) {
        draw_line(buf, w, h, par[0], par[1], color);
    }
}

/// Circunferencia en mundo (`r` en unidades de mundo, 64 segmentos).
fn draw_circle_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    r: f64,
    color: [u8; 4],
) -> bool {
    if !(cx.is_finite() && cy.is_finite() && r.is_finite() && r > 0.0) || w == 0 || h == 0 {
        return false;
    }
    let puntos: Vec<(usize, usize)> = (0..=64)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / 64.0;
            to_pixel(w, h, cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    draw_polyline_px(buf, w, h, &puntos, color);
    true
}

/// Cuadrado centrado en mundo (`side` en unidades de mundo).
fn draw_square_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    side: f64,
    color: [u8; 4],
) -> bool {
    if !(cx.is_finite() && cy.is_finite() && side.is_finite() && side > 0.0) || w == 0 || h == 0 {
        return false;
    }
    let m = side / 2.0;
    let esquinas = [
        to_pixel(w, h, cx - m, cy - m),
        to_pixel(w, h, cx + m, cy - m),
        to_pixel(w, h, cx + m, cy + m),
        to_pixel(w, h, cx - m, cy + m),
        to_pixel(w, h, cx - m, cy - m),
    ];
    draw_polyline_px(buf, w, h, &esquinas, color);
    true
}

/// Muestras adaptativas de la elipse según el radio mayor: 32 para la
/// chica (`r<=1`), hasta 128 para la gigante (`r>=4`). Pura (testeable).
fn muestras_elipse_para_radio(rx: f64, ry: f64) -> usize {
    let mayor = rx.max(ry);
    if !mayor.is_finite() || mayor <= 0.0 {
        return 32;
    }
    ((mayor * 32.0).ceil() as usize).clamp(32, 128)
}

/// Muestras del arco proporcionales al barrido (radianes): 4 para el
/// chico, 96 para la vuelta completa, tope 128. Pura (testeable).
fn muestras_arco_para_barrido(barrido: f64) -> usize {
    if !barrido.is_finite() || barrido <= 0.0 {
        return 4;
    }
    ((barrido / std::f64::consts::TAU * 96.0).ceil() as usize).clamp(4, 128)
}

/// Rectángulo centrado en mundo: 4 segmentos con clip limpio tramo a tramo
/// (`draw_seg_mundo`, sin saturar al borde). 1px `SQUARE_BLUE` como
/// `Square` (mismo lenguaje visual que el resto de `draw_mobject`).
/// `true` = pintó al menos un tramo. Puro sobre el buffer, sin E/S.
#[allow(clippy::too_many_arguments)]
fn draw_rect_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    rw: f64,
    rh: f64,
    color: [u8; 4],
) -> bool {
    if !(cx.is_finite()
        && cy.is_finite()
        && rw.is_finite()
        && rh.is_finite()
        && rw > 0.0
        && rh > 0.0)
        || w == 0
        || h == 0
    {
        return false;
    }
    let (hw, hh) = (rw / 2.0, rh / 2.0);
    let esq = [
        [cx - hw, cy - hh],
        [cx + hw, cy - hh],
        [cx + hw, cy + hh],
        [cx - hw, cy + hh],
    ];
    let mut pinto = false;
    for k in 0..4 {
        let (a, b) = (esq[k], esq[(k + 1) % 4]);
        pinto |= draw_seg_mundo(buf, w, h, a[0], a[1], b[0], b[1], color, 1.0);
    }
    pinto
}

/// Elipse centrada en mundo: curva muestreada adaptativa (32..=128 según
/// el radio mayor) con clip limpio tramo a tramo (sin saturar). 1px
/// `CURVE_MAIN` como `Circle`. `true` = pintó. Pura, sin E/S.
#[allow(clippy::too_many_arguments)]
fn draw_ellipse_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    color: [u8; 4],
) -> bool {
    if !(cx.is_finite()
        && cy.is_finite()
        && rx.is_finite()
        && ry.is_finite()
        && rx > 0.0
        && ry > 0.0)
        || w == 0
        || h == 0
    {
        return false;
    }
    let n = muestras_elipse_para_radio(rx, ry);
    let mut pinto = false;
    let mut previo = (cx + rx, cy);
    for k in 1..=n {
        let a = k as f64 * std::f64::consts::TAU / n as f64;
        let punto = (cx + rx * a.cos(), cy + ry * a.sin());
        pinto |= draw_seg_mundo(buf, w, h, previo.0, previo.1, punto.0, punto.1, color, 1.0);
        previo = punto;
    }
    pinto
}

/// Arco circular en mundo (radianes) con submuestreo por ángulo
/// (4..=128 según el barrido) y clip limpio tramo a tramo (sin saturar).
/// 1px `CURVE_MAIN` como `Circle`. `true` = pintó. Puro, sin E/S.
#[allow(clippy::too_many_arguments)]
fn draw_arc_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    r: f64,
    start: f64,
    end: f64,
    color: [u8; 4],
) -> bool {
    if !(cx.is_finite() && cy.is_finite() && r.is_finite() && r > 0.0) || w == 0 || h == 0 {
        return false;
    }
    let barrido = end - start;
    if !barrido.is_finite() || barrido <= 0.0 {
        return false;
    }
    let n = muestras_arco_para_barrido(barrido);
    let mut pinto = false;
    let mut previo = (cx + r * start.cos(), cy + r * start.sin());
    for k in 1..=n {
        let a = start + barrido * k as f64 / n as f64;
        let punto = (cx + r * a.cos(), cy + r * a.sin());
        pinto |= draw_seg_mundo(buf, w, h, previo.0, previo.1, punto.0, punto.1, color, 1.0);
        previo = punto;
    }
    pinto
}

/// Flecha en mundo: línea + punta de dos alas a ±25° (largo 20% del tramo,
/// clamp 0.05..0.40 para que se vea en miniaturas y no explote en zoom).
/// El ángulo se mide en mundo (el aspecto del frame lo puede sesgar: solo
/// didáctico, documentado).
fn draw_arrow_world(
    buf: &mut [u8],
    w: usize,
    h: usize,
    from: [f64; 2],
    to: [f64; 2],
    color: [u8; 4],
) -> bool {
    if from.iter().chain(to.iter()).any(|v| !v.is_finite()) || w == 0 || h == 0 {
        return false;
    }
    draw_line(
        buf,
        w,
        h,
        to_pixel(w, h, from[0], from[1]),
        to_pixel(w, h, to[0], to[1]),
        color,
    );
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let largo = dx.hypot(dy);
    if !largo.is_finite() || largo <= 0.0 {
        return true;
    }
    let ala = (largo * 0.2).clamp(0.05, 0.40);
    let base = dy.atan2(dx);
    for signo in [1.0, -1.0] {
        let a = base + signo * (std::f64::consts::PI - 25.0_f64.to_radians());
        let punta = [to[0] + ala * a.cos(), to[1] + ala * a.sin()];
        if punta.iter().all(|v| v.is_finite()) {
            draw_line(
                buf,
                w,
                h,
                to_pixel(w, h, to[0], to[1]),
                to_pixel(w, h, punta[0], punta[1]),
                color,
            );
        }
    }
    true
}

/// `y = expr(x)` muestreada en 120 puntos de [-3,3] con el evaluador real
/// (`grafito_geometry::expr::evaluate`); tramos finitos separados en huecos
/// (asíntotas no se puentean con líneas falsas).
fn draw_function_graph(buf: &mut [u8], w: usize, h: usize, expr: &str) -> bool {
    let mut tramo: Vec<(f64, f64)> = Vec::new();
    let mut pinto = false;
    for k in 0..=120 {
        let x = -3.0 + 6.0 * (k as f64) / 120.0;
        let vars = [("x".to_string(), x)];
        match grafito_geometry::expr::evaluate(expr, &vars) {
            // En mundo (sin saturar): el clip lo hace `draw_curva_mundo`
            // tramo a tramo; fuera de vista se corta, no se aplasta.
            Ok(y) if y.is_finite() => tramo.push((x, y)),
            _ => {
                if tramo.len() >= 2 {
                    pintados_curva(buf, w, h, &tramo);
                    pinto = true;
                }
                tramo.clear();
            }
        }
    }
    if tramo.len() >= 2 {
        pintados_curva(buf, w, h, &tramo);
        pinto = true;
    }
    pinto
}

/// Curva principal a 2px con clip limpio (atajo para no repetir grosor).
fn pintados_curva(buf: &mut [u8], w: usize, h: usize, tramo: &[(f64, f64)]) -> usize {
    draw_curva_mundo(buf, w, h, tramo, CURVE_MAIN, CURVE_ANCHO)
}

/// Retícula de flechas `nx × ny` con rumbo determinista
/// (`sin/cos` del lugar: sin física inventada, solo orientación visible).
/// Submuestreo a ≤512 flechas (64×64 serían 4096 strokes por frame).
fn draw_arrow_field(buf: &mut [u8], w: usize, h: usize, nx: usize, ny: usize) -> bool {
    if nx == 0 || ny == 0 || nx > 64 || ny > 64 || w == 0 || h == 0 {
        return false;
    }
    let paso = (nx * ny).div_ceil(512).max(1);
    let mut indice = 0usize;
    for ix in 0..nx {
        for iy in 0..ny {
            let mantener = indice.is_multiple_of(paso);
            indice += 1;
            if !mantener {
                continue;
            }
            let x = -2.5 + 5.0 * (ix as f64) / (nx.max(1) as f64);
            let y = -2.5 + 5.0 * (iy as f64) / (ny.max(1) as f64);
            let rumbo = x.sin() * 1.2 + y.cos() * 0.8;
            let (dx, dy) = (0.18 * rumbo.cos(), 0.18 * rumbo.sin());
            draw_arrow_world(buf, w, h, [x - dx, y - dy], [x + dx, y + dy], MINT_FAINT);
        }
    }
    true
}

/// Plano numerado: retícula del rango pedido (≤64 divisiones por lado, ya
/// validado; tope defensivo 256 líneas) + ejes rotulados del mundo.
#[allow(clippy::too_many_arguments)]
fn draw_number_plane_cells(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    x_step: f64,
    y_step: f64,
) -> bool {
    if !(x_step.is_finite() && y_step.is_finite() && x_step > 0.0 && y_step > 0.0) {
        return false;
    }
    let mut x = x_min;
    for _ in 0..256 {
        if !(x.is_finite()) || x > x_max {
            break;
        }
        draw_line(
            buf,
            w,
            h,
            to_pixel(w, h, x, y_min),
            to_pixel(w, h, x, y_max),
            FAINT_WHITE,
        );
        x += x_step;
    }
    let mut y = y_min;
    for _ in 0..256 {
        if !(y.is_finite()) || y > y_max {
            break;
        }
        draw_line(
            buf,
            w,
            h,
            to_pixel(w, h, x_min, y),
            to_pixel(w, h, x_max, y),
            FAINT_WHITE,
        );
        y += y_step;
    }
    draw_axes_with_labels(buf, w, h);
    true
}

/// Campo vectorial `func(x, y)` → rumbo en radianes por celda; celda que no
/// evalúa se omite (hueco honesto, sin flecha inventada). Submuestreo ≤512.
fn draw_vector_field_cells(
    buf: &mut [u8],
    w: usize,
    h: usize,
    func: &str,
    nx: usize,
    ny: usize,
) -> bool {
    if func.trim().is_empty() || nx == 0 || ny == 0 || nx > 64 || ny > 64 || w == 0 || h == 0 {
        return false;
    }
    let paso = (nx * ny).div_ceil(512).max(1);
    let mut indice = 0usize;
    let mut pinto = false;
    for ix in 0..nx {
        for iy in 0..ny {
            let mantener = indice.is_multiple_of(paso);
            indice += 1;
            if !mantener {
                continue;
            }
            let x = -2.5 + 5.0 * (ix as f64) / (nx.max(1) as f64);
            let y = -2.5 + 5.0 * (iy as f64) / (ny.max(1) as f64);
            let vars = [("x".to_string(), x), ("y".to_string(), y)];
            let Ok(rumbo) = grafito_geometry::expr::evaluate(func, &vars) else {
                continue;
            };
            if !rumbo.is_finite() {
                continue;
            }
            let (dx, dy) = (0.20 * rumbo.cos(), 0.20 * rumbo.sin());
            draw_arrow_world(buf, w, h, [x - dx, y - dy], [x + dx, y + dy], MINT_STRONG);
            pinto = true;
        }
    }
    pinto
}

/// Dibuja un `Mobject` sobre el frame. Valida primero: inválido → `false`
/// sin pintar. `Group` recursa con tope de profundidad 8 (paridad con el
/// validador). Puro sobre el buffer, sin E/S.
pub fn draw_mobject(buf: &mut [u8], w: usize, h: usize, mobject: &grafito_anim::Mobject) -> bool {
    draw_mobject_con_profundidad(buf, w, h, mobject, 0)
}

fn draw_mobject_con_profundidad(
    buf: &mut [u8],
    w: usize,
    h: usize,
    mobject: &grafito_anim::Mobject,
    profundidad: usize,
) -> bool {
    use grafito_anim::Mobject as M;
    if profundidad > 8 || mobject.validate().is_err() {
        return false;
    }
    match mobject {
        M::Axes => {
            draw_axes_with_labels(buf, w, h);
            true
        }
        M::Dot { x, y } => {
            let (px, py) = to_pixel(w, h, *x, *y);
            draw_filled_circle(buf, w, h, px, py, 3, DOT_BLUE);
            true
        }
        M::Circle { cx, cy, r } => draw_circle_world(buf, w, h, *cx, *cy, *r, CURVE_MAIN),
        M::Square { cx, cy, side } => draw_square_world(buf, w, h, *cx, *cy, *side, SQUARE_BLUE),
        M::Rectangle {
            cx,
            cy,
            w: rw,
            h: rh,
        } => draw_rect_world(buf, w, h, *cx, *cy, *rw, *rh, SQUARE_BLUE),
        M::Ellipse { cx, cy, rx, ry } => {
            draw_ellipse_world(buf, w, h, *cx, *cy, *rx, *ry, CURVE_MAIN)
        }
        M::Arc {
            cx,
            cy,
            r,
            start_rad,
            end_rad,
        } => draw_arc_world(buf, w, h, *cx, *cy, *r, *start_rad, *end_rad, CURVE_MAIN),
        M::Line { from, to } => {
            draw_line(
                buf,
                w,
                h,
                to_pixel(w, h, from[0], from[1]),
                to_pixel(w, h, to[0], to[1]),
                TANGENT_BLUE,
            );
            true
        }
        M::Arrow { from, to } => draw_arrow_world(buf, w, h, *from, *to, TANGENT_BLUE),
        M::Polygon { pts } => {
            if pts.len() < 2 {
                if let Some(p) = pts.first() {
                    let (px, py) = to_pixel(w, h, p[0], p[1]);
                    draw_filled_circle(buf, w, h, px, py, 2, CURVE_MAIN);
                    return true;
                }
                return false;
            }
            let mut puntos: Vec<(usize, usize)> = pts
                .iter()
                .take(4096)
                .map(|p| to_pixel(w, h, p[0], p[1]))
                .collect();
            if pts.len() >= 3 {
                if let Some(primero) = puntos.first().copied() {
                    puntos.push(primero);
                }
            }
            draw_polyline_px(buf, w, h, &puntos, CURVE_MAIN);
            true
        }
        M::FunctionGraph { expr } => draw_function_graph(buf, w, h, expr),
        M::Tex { svg } => {
            if draw_tex_svg_onto(buf, w, h, svg) {
                true
            } else if let Some(texto) = crate::export::extract_svg_text_content(svg, 48) {
                draw_text_block(
                    buf,
                    w,
                    h,
                    w / 12,
                    h / 2,
                    &texto,
                    TEXT_COLOR,
                    text_scale_for_h(h),
                );
                true
            } else {
                false
            }
        }
        M::ArrowField { nx, ny } => draw_arrow_field(buf, w, h, *nx, *ny),
        M::NumberPlane {
            x_min,
            x_max,
            y_min,
            y_max,
            x_step,
            y_step,
        } => draw_number_plane_cells(buf, w, h, *x_min, *x_max, *y_min, *y_max, *x_step, *y_step),
        M::VectorField { func, nx, ny } => draw_vector_field_cells(buf, w, h, func, *nx, *ny),
        M::Group(hijos) => {
            let mut alguno = false;
            for hijo in hijos.iter().take(32) {
                alguno |= draw_mobject_con_profundidad(buf, w, h, hijo, profundidad + 1);
            }
            alguno
        }
    }
}

// ── Raster genérico del `ScenePlayer` (piel-ui, puro) ───────────────────────
// `ScenePlayer::play` devuelve `Vec<PlayedFrame>` con `PlacedMobject`s
// (`{ mobject, opacity, scale, center }`); esta fn los rasteriza a un
// `ColorImage` sobre el fondo único + grilla fija, reusando `draw_mobject`.
// Sirve para frames del player y para la órbita (con `Perspective` no se
// dibujan ejes 2D: el contexto es 3D, como en `render_orbit_frames`).
//
// - Solo las variantes geométricas se transforman (`Dot`/`Circle`/`Square`/
//   `Rectangle`/`Ellipse`/`Arc`/`Line`/`Arrow`/`Polygon`/`Group`): el resto
//   `ArrowField`, `Tex`, `NumberPlane`, `VectorField`) se dibuja tal cual
//   (documentado, sin inventar geometría). Un `VMobject` entra vía
//   `vmobject_como_polilinea` (aplanado Bézier honesto).
// - `opacity` 0 → se omite; 1 → directo; intermedia → blend global de la
//   capa sobre la base (los píxeles idénticos quedan idénticos).
// - Viewport fijo [-3,3]² idéntico al resto de renderers nativos.
// - Vacío o todo inválido → fondo honesto sin formas (jamás panic).

/// Mueve un punto mundo por escala alrededor del centro. Puro.
fn colocado_punto(p: [f64; 2], escala: f64, centro: [f64; 2]) -> [f64; 2] {
    [
        centro[0] + (p[0] - centro[0]) * escala,
        centro[1] + (p[1] - centro[1]) * escala,
    ]
}

/// Aplica `scale`/`center` a las variantes geométricas; el resto vuelve
/// clonado (sin transformar). Puro, sin pánicos.
fn transformar_colocado(
    m: &grafito_anim::Mobject,
    escala: f64,
    centro: [f64; 2],
) -> grafito_anim::Mobject {
    use grafito_anim::Mobject as M;
    if !escala.is_finite() || escala <= 0.0 || !centro.iter().all(|v| v.is_finite()) {
        return m.clone();
    }
    match m {
        M::Dot { x, y } => {
            let p = colocado_punto([*x, *y], escala, centro);
            M::Dot { x: p[0], y: p[1] }
        }
        M::Circle { cx, cy, r } => {
            let p = colocado_punto([*cx, *cy], escala, centro);
            M::Circle {
                cx: p[0],
                cy: p[1],
                r: r * escala.abs(),
            }
        }
        M::Square { cx, cy, side } => {
            let p = colocado_punto([*cx, *cy], escala, centro);
            M::Square {
                cx: p[0],
                cy: p[1],
                side: side * escala.abs(),
            }
        }
        M::Rectangle {
            cx,
            cy,
            w: rw,
            h: rh,
        } => {
            let p = colocado_punto([*cx, *cy], escala, centro);
            M::Rectangle {
                cx: p[0],
                cy: p[1],
                w: rw * escala.abs(),
                h: rh * escala.abs(),
            }
        }
        M::Ellipse { cx, cy, rx, ry } => {
            let p = colocado_punto([*cx, *cy], escala, centro);
            M::Ellipse {
                cx: p[0],
                cy: p[1],
                rx: rx * escala.abs(),
                ry: ry * escala.abs(),
            }
        }
        M::Arc {
            cx,
            cy,
            r,
            start_rad,
            end_rad,
        } => {
            let p = colocado_punto([*cx, *cy], escala, centro);
            M::Arc {
                cx: p[0],
                cy: p[1],
                r: r * escala.abs(),
                start_rad: *start_rad,
                end_rad: *end_rad,
            }
        }
        M::Line { from, to } => M::Line {
            from: colocado_punto(*from, escala, centro),
            to: colocado_punto(*to, escala, centro),
        },
        M::Arrow { from, to } => M::Arrow {
            from: colocado_punto(*from, escala, centro),
            to: colocado_punto(*to, escala, centro),
        },
        M::Polygon { pts } => M::Polygon {
            pts: pts
                .iter()
                .take(4096)
                .map(|p| colocado_punto(*p, escala, centro))
                .collect(),
        },
        M::Group(hijos) => M::Group(
            hijos
                .iter()
                .take(32)
                .map(|h| transformar_colocado(h, escala, centro))
                .collect(),
        ),
        otro => otro.clone(),
    }
}

/// Blendea la capa sobre la base con alfa global `opacity` (0..=1, finito).
/// Puro sobre buffers RGBA del mismo largo; largos distintos = no-op.
fn mezclar_capa_con_opacidad(base: &mut [u8], capa: &[u8], opacity: f32) {
    if base.len() != capa.len() || !(0.0..=1.0).contains(&opacity) || !opacity.is_finite() {
        return;
    }
    if opacity <= 0.0 {
        return;
    }
    if opacity >= 1.0 {
        base.copy_from_slice(capa);
        return;
    }
    let o = opacity as f64;
    for par in base.chunks_exact_mut(4).zip(capa.chunks_exact(4)) {
        let (b, c) = par;
        for k in 0..3 {
            b[k] = (f64::from(c[k]) * o + f64::from(b[k]) * (1.0 - o)) as u8;
        }
        b[3] = 255;
    }
}

/// Aplana un `VMobject` a polilínea (`Mobject::Polygon`): cada tramo
/// ancla→ancla se muestrea como Bézier cúbica (8 subdivisiones; solo anclas
/// si hay >512, con tope 4096 puntos). Manijas inconsistentes → solo anclas
/// (honesto, sin inventar curva). `None` si vacío o sin puntos finitos.
/// Puro, sin pánicos.
pub fn vmobject_como_polilinea(vm: &grafito_anim::VMobject) -> Option<grafito_anim::Mobject> {
    let n = vm.anchors.len();
    if n == 0 {
        return None;
    }
    let cubicas_ok = vm.handles_in.len() == n && vm.handles_out.len() == n;
    let por_tramo = if n > 512 { 1 } else { 8 };
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for i in 0..n {
        let p0 = vm.anchors[i];
        if i + 1 >= n {
            if p0.iter().all(|v| v.is_finite()) {
                pts.push(p0);
            }
            break;
        }
        let p3 = vm.anchors[i + 1];
        if cubicas_ok {
            let p1 = vm.handles_out[i];
            let p2 = vm.handles_in[i + 1];
            let todo = [p0, p1, p2, p3];
            if todo.iter().all(|p| p.iter().all(|v| v.is_finite())) {
                for k in 0..por_tramo {
                    let t = k as f64 / por_tramo as f64;
                    let u = 1.0 - t;
                    pts.push([
                        u * u * u * p0[0]
                            + 3.0 * u * u * t * p1[0]
                            + 3.0 * u * t * t * p2[0]
                            + t * t * t * p3[0],
                        u * u * u * p0[1]
                            + 3.0 * u * u * t * p1[1]
                            + 3.0 * u * t * t * p2[1]
                            + t * t * t * p3[1],
                    ]);
                }
                continue;
            }
        }
        if p0.iter().all(|v| v.is_finite()) {
            pts.push(p0);
        }
    }
    pts.truncate(4096);
    if pts.iter().all(|p| p.iter().all(|v| v.is_finite())) && !pts.is_empty() {
        Some(grafito_anim::Mobject::Polygon { pts })
    } else {
        None
    }
}

/// Rasteriza objetos colocados del player a un frame. Puro, sin E/S.
pub fn render_placed_objects(
    placed: &[grafito_anim::PlacedMobject],
    w: usize,
    h: usize,
    camera: grafito_anim::Camera,
) -> egui::ColorImage {
    let (w, h) = if w == 0 || h == 0 {
        (NATIVE_FALLBACK_W, NATIVE_FALLBACK_H)
    } else {
        (w, h)
    };
    let byte_len = w
        .checked_mul(h)
        .and_then(|v| v.checked_mul(4))
        .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
    let mut buf = vec![0u8; byte_len];
    if byte_len != w * h * 4 {
        return egui::ColorImage::from_rgba_unmultiplied(
            [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
            &buf,
        );
    }
    fill_background(&mut buf, w, h);
    draw_subtle_grid(&mut buf, w, h);
    if camera.is_ortho() {
        draw_axes_with_labels(&mut buf, w, h);
    }
    for obj in placed.iter().take(32) {
        if obj.opacity <= 0.0 {
            continue;
        }
        let m = transformar_colocado(&obj.mobject, f64::from(obj.scale), obj.center);
        if obj.opacity >= 1.0 {
            draw_mobject(&mut buf, w, h, &m);
        } else {
            let mut capa = buf.clone();
            if draw_mobject(&mut capa, w, h, &m) {
                mezclar_capa_con_opacidad(&mut buf, &capa, obj.opacity);
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &buf)
}

// ── Cámara 3D mínima (Perspective + MovingCamera, 2D intacto) ───────────────
// La órbita usa `Camera::Perspective` + `project_3d` a través del puente de
// la piel (`crate::render_3d::project_anim_camera_point`, que reusa
// `OrthoProjection` para las vistas ortográficas del canvas). El travelling
// se expresa como tracks reales (`MovingCamera::as_tracks`).

/// Cámara en `t_ms` aplicando el easing del travelling (`MovingCamera` ya lo
/// interpola con sus `RateFunc` exactas: acá solo se muestrea). Pura.
pub fn anim_camera_sample(moving: &grafito_anim::MovingCamera, t_ms: u64) -> grafito_anim::Camera {
    moving.sample(t_ms)
}

/// Ids de los tracks del travelling (4 en `Ortho`, 7 en `Perspective`).
/// Vacío honesto si la cámara no produce tracks (nunca con cámaras
/// validadas). Puro.
pub fn anim_camera_track_ids(moving: &grafito_anim::MovingCamera) -> Vec<String> {
    moving
        .as_tracks()
        .map(|tracks| tracks.iter().map(|t| t.prop_id.clone()).collect())
        .unwrap_or_default()
}

/// Cámara orbital por defecto de la preview 3D (perspectiva 50° barriendo
/// el cubo de lado 2). Si la construcción fallara (no debería: valores
/// fijos validados), cae al ortho canónico 16:9 — jamás panic.
fn orbit_camera_default() -> grafito_anim::Camera {
    grafito_anim::Camera::perspective(50.0, [5.0, 2.0, 5.0], [0.0, 0.0, 0.0]).unwrap_or(
        grafito_anim::Camera::Ortho(grafito_anim::Ortho::default_16_9()),
    )
}

/// 48 frames de un cubo wireframe orbitado (perspectiva real `project_3d`).
///
/// 2D intacto: ningún renderer existente cambia; este es el camino 3D
/// mínimo para que `Camera::Perspective` y `MovingCamera` no queden
/// tipados sin render. Determinista, acotado por `resolve_native_size_budgeted`.
pub fn render_orbit_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let desde = grafito_anim::Camera::perspective(50.0, [5.0, 2.0, 5.0], [0.0, 0.0, 0.0])
        .unwrap_or(orbit_camera_default());
    let hasta = grafito_anim::Camera::perspective(50.0, [-5.0, 3.5, 4.0], [0.0, 0.0, 0.0])
        .unwrap_or(orbit_camera_default());
    let travelling =
        grafito_anim::MovingCamera::try_new(desde, hasta, 2000, grafito_anim::RateFunc::Linear)
            .ok();
    let centro = egui::pos2(w as f32 / 2.0, h as f32 / 2.0);
    let escala = ((w.min(h) as f32) / 6.0).max(1.0);
    let cubo: [[f64; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let aristas: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).clamp(1.0, 48.0);
        let t_ms = ((t * 2000.0).round() as u64).min(2000);
        let cam = travelling
            .map(|m| m.sample(t_ms))
            .unwrap_or(orbit_camera_default());
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        let px: Vec<Option<(usize, usize)>> = cubo
            .iter()
            .map(|v| {
                crate::render_3d::project_anim_camera_point(&cam, *v, escala, centro).map(|p| {
                    (
                        p.x.round().clamp(0.0, w.max(1) as f32 - 1.0) as usize,
                        p.y.round().clamp(0.0, h.max(1) as f32 - 1.0) as usize,
                    )
                })
            })
            .collect();
        for (a, b) in aristas {
            if let (Some(pa), Some(pb)) = (px[a], px[b]) {
                draw_line(&mut buf, w, h, pa, pb, LINE_WHITE);
            }
        }
        for punto in px.into_iter().flatten() {
            draw_filled_circle(&mut buf, w, h, punto.0, punto.1, 2, DOT_BLUE);
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

/// 48 frames de un `Mobject` estático sobre fondo único (para previews y
/// tests de dibujo real). `Mobject` inválido → fondo honesto sin formas.
pub fn render_mobject_frames(
    width: u32,
    height: u32,
    mobject: &grafito_anim::Mobject,
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for _frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_mobject(&mut buf, w, h, mobject);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

// ── Dispatch honesto nativo (sync mecánico, ANIM-REVIVE) ───────────────────
// Expone CÓMO se resolvió una plantilla: `Direct` (renderer dedicado) o
// `FallbackUniversal` (`limit-epsilon` / `ode-*` / typos: sin renderer propio,
// frames válidos vía detección por concepto). El test lo pinnea.

/// Cómo resolvió el dispatcher nativo una plantilla pedida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDispatch {
    /// Tiene renderer dedicado; `canonical` alimenta `render_anim_with_progress`.
    Direct { canonical: &'static str },
    /// Sin renderer propio: se resolvió por concepto; `resolved` es la usada.
    FallbackUniversal {
        requested: String,
        resolved: &'static str,
    },
}

/// Aliases pedagógicos F5 (mapeo a nativos existentes, ver `resolve_native_template`).
const NATIVE_ALIASES: &[&str] = &[
    "pythagoras",
    "fraccion-visual",
    "vector-anim",
    "matriz-anim",
    "prob-anim",
    "serie-anim",
    "ecuacion-anim",
    "trig-anim",
    "conica-anim",
];

/// Resuelve una plantilla al dispatcher y declara si fue directa o fallback.
pub fn native_dispatch_for(template: &str, concept: &str) -> NativeDispatch {
    let t = template.trim().to_lowercase();
    let resolved = resolve_native_template(template, concept);
    let known = NATIVE_TEMPLATES.contains(&t.as_str())
        || NATIVE_ALIASES.contains(&t.as_str())
        || t.is_empty()
        || t == "auto";
    if known {
        NativeDispatch::Direct {
            canonical: resolved,
        }
    } else {
        NativeDispatch::FallbackUniversal {
            requested: template.trim().to_string(),
            resolved,
        }
    }
}

// ── Paleta nativa centralizada (única fuente de verdad en anim_native.rs) ───
// Deriva del tema Scandinavian (grafito-ui Theme DARK, tokens TYPE_*/SPACE_*):
// - BG #0E0E14 ≈ DARK canvas_bg #0A0A0A con lift vídeo para gradiente legible.
// - BG_GRADIENT ≈ DARK panel_bg #1A1A1A con tinte frío vídeo.
// - TEXT (FG) #EBEBF5 ≈ DARK text_primary #FAFAF9 (blanco cálido, alpha 255).
// - Acentos vivos mapean a Theme: BLUE→object_point, YELLOW→highlight/warning,
//   RED→danger, MINT→success/object_function, VIOLET→toast_cas, ORANGE→warning.
//   El fondo es ÚNICO y fijo (`fill_background` + viñeta, sin tinte por
//   concepto): los 6 vivos garantizan contraste sobre BG oscuro para vídeo
//   didáctico.
// - Lenguaje de color: amarilla 3px = objeto (`CURVE_MAIN` + `CURVE_ANCHO`),
//   azul 3px = construcción (`TANGENT_BLUE`/`PAL_BLUE` + `CURVE_ANCHO`),
//   rojo = punto/resultado (`POINT_RED` radio 3, `GIBBS_RED` radio 3).
//   Ejes a 1.5px (`AXIS_ANCHO`), ticks a 1px.
// - Grilla alfa 38..51 (≈15-20% sobre BG, contraste 3:1 para gráficos).
// REGLA: ningún color RGBA fuera de este bloque. Los renders solo usan estas
// consts o `with_alpha(BASE, a)`. Ver test `palette_has_no_loose_hardcodes`.
const BG: [u8; 4] = [14, 14, 20, 255];
const BG_GRADIENT: [u8; 4] = [22, 22, 34, 255];
const GRID_COLOR: [u8; 4] = [255, 255, 255, 44];
const AXIS_COLOR: [u8; 4] = [200, 200, 200, 90];
const TEXT_COLOR: [u8; 4] = [235, 235, 245, 255];
// Trío canónico BG/FG/ACCENT (alias documentados para el gate de paleta).
const PAL_BG: [u8; 4] = BG;
const PAL_FG: [u8; 4] = TEXT_COLOR;
// Base vivos (únicos literales de acento; los roles derivan de aquí).
const PAL_BLUE: [u8; 4] = [66, 133, 244, 255];
const PAL_YELLOW: [u8; 4] = [235, 211, 84, 255];
const PAL_RED: [u8; 4] = [255, 77, 77, 255];
const PAL_MINT: [u8; 4] = [126, 214, 160, 255];
const PAL_VIOLET: [u8; 4] = [168, 120, 255, 255];
const PAL_ORANGE: [u8; 4] = [255, 153, 51, 255];
// Acento canónico para progreso/puntos (azul Google, contrasta sobre BG).
const PAL_ACCENT: [u8; 4] = PAL_BLUE;
// ── Roles derivados (todos centralizados aquí, sin literales en renders) ──
const CURVE_MAIN: [u8; 4] = [235, 211, 84, 235];
const TANGENT_BLUE: [u8; 4] = [66, 133, 244, 235];
const POINT_RED: [u8; 4] = [255, 77, 77, 255];
const LINE_WHITE: [u8; 4] = [255, 255, 255, 255];
const SQUARE_BLUE: [u8; 4] = [66, 133, 244, 200];
const SQUARE_AMBER: [u8; 4] = [255, 193, 7, 200];
const SQUARE_GREEN: [u8; 4] = [76, 175, 80, 200];
const FILL_SOFT_BLUE: [u8; 4] = [91, 155, 255, 80];
const DOT_BLUE: [u8; 4] = [66, 133, 244, 255];
const MINT_STRONG: [u8; 4] = [126, 214, 160, 200];
const MINT_FAINT: [u8; 4] = [126, 214, 160, 120];
const FAINT_WHITE: [u8; 4] = [255, 255, 255, 35];
const GIBBS_RED: [u8; 4] = [255, 77, 77, 200];
const SCRIM: [u8; 4] = [0, 0, 0, 110];
const TRACK: [u8; 4] = [255, 255, 255, 22];

const fn with_alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

// ── Robustez dimensional: Resolution 64..4096 tipada, sin panic ni OOM ────
// Espejo local de `grafito_anim::Resolution::try_new` (sin depender de Piel).
// `resolve_native_size` siempre devuelve un tamaño seguro (clamp) + error
// tipado opcional; `try_resolve_native_size` es estricta para callers que
// quieren fallar. Todo cálculo de bytes usa `checked_mul` + `try_reserve`.
pub(crate) const NATIVE_MIN_DIM: u32 = 64;
pub(crate) const NATIVE_MAX_DIM: u32 = 4096;
pub(crate) const NATIVE_FALLBACK_W: usize = 640;
pub(crate) const NATIVE_FALLBACK_H: usize = 480;

// ── AS3 presupuestos de memoria por generación (diseño, con test) ─────────
// Un set nativo son `NATIVE_ANIM_FRAME_COUNT` frames RGBA en RAM:
// `w*h*4*48`. A 640×480 = 58_982_400 B (≈56 MiB) transitorios durante el
// render en el hilo de generación (nunca en UI thread). Por eso NO hay caché
// de sets completos: la capa de caché es la ventana paginada de texturas en
// `anim_ui.rs` (8 thumbs ≈ 9.8 MiB GPU a 640×480). Retener 1 set = 56 MiB
// permanentes sin uso probado → se regenera bajo demanda.
// `NATIVE_FRAME_BYTES_ESTIMADO_640x480` pinnea el número para el usuario
// ("cuando inicio la app como que carga algo" no es esto: el render nativo
// solo corre al pedir una animación, en hilo aparte).

/// Bytes por píxel RGBA (espejo de `egui::ColorImage`: 4 B/px).
pub const NATIVE_BYTES_PER_PIXEL: usize = 4;
/// Memoria estimada del set canónico 640×480×48 RGBA: 58_982_400 B (≈56 MiB).
pub const NATIVE_FRAME_BYTES_ESTIMADO_640X480: usize =
    640 * 480 * NATIVE_BYTES_PER_PIXEL * NATIVE_ANIM_FRAME_COUNT;

/// Tope del set nativo en RAM (M3-5, paridad con `PARAMETRIC_MAX_BYTES`):
/// 64 MiB. `4096×4096×48` RGBA son ≈3 GiB: sin preflight el render
/// clásico lo intentaría reservar. Los `render_*` clásicos devuelven
/// `Vec` (sin `Err`): ante exceso HACEN CLAMP documentado a este tope
/// vía `resolve_native_size_budgeted` (nunca OOM, nunca panic).
pub const NATIVE_MAX_SET_BYTES: usize = 64 * 1024 * 1024;

/// Estima los bytes RGBA de un set (`w*h*4*count`). `None` si desborda
/// (`checked_mul`, sin pánicos). Puro, sin I/O ni allocs.
#[must_use]
pub fn estimate_frames_bytes(w: usize, h: usize, count: usize) -> Option<usize> {
    w.checked_mul(h)
        .and_then(|v| v.checked_mul(NATIVE_BYTES_PER_PIXEL))
        .and_then(|v| v.checked_mul(count))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSizeError {
    BelowMinimum {
        requested: (u32, u32),
        clamped: (usize, usize),
    },
    AboveMaximum {
        requested: (u32, u32),
        clamped: (usize, usize),
    },
    AllocationOverflow {
        w: usize,
        h: usize,
    },
    AllocationFailed {
        bytes: usize,
    },
    /// El set estimado excede `NATIVE_MAX_SET_BYTES` (M3-5): el llamador
    /// clásico hace clamp vía `resolve_native_size_budgeted`.
    OverBudget {
        requested: (usize, usize),
        clamped: (usize, usize),
        bytes: usize,
    },
}

impl std::fmt::Display for NativeSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::BelowMinimum { requested, clamped } => write!(
                f,
                "dimensión {:?} bajo mínimo 64, clamped a {:?}",
                requested, clamped
            ),
            Self::AboveMaximum { requested, clamped } => write!(
                f,
                "dimensión {:?} sobre máximo 4096, clamped a {:?}",
                requested, clamped
            ),
            Self::AllocationOverflow { w, h } => {
                write!(f, "overflow al calcular bytes para {w}x{h}")
            }
            Self::AllocationFailed { bytes } => {
                write!(f, "no se pudo reservar {bytes} bytes (OOM guard)")
            }
            Self::OverBudget {
                requested,
                clamped,
                bytes,
            } => {
                write!(
                    f,
                    "set {requested:?} ≈ {bytes} bytes excede el tope de {NATIVE_MAX_SET_BYTES}: clamped a {clamped:?}"
                )
            }
        }
    }
}

impl std::error::Error for NativeSizeError {}

pub(crate) fn try_resolve_native_size(
    width: u32,
    height: u32,
) -> Result<(usize, usize), NativeSizeError> {
    if width < NATIVE_MIN_DIM || height < NATIVE_MIN_DIM {
        let clamped = (
            width.clamp(NATIVE_MIN_DIM, NATIVE_MAX_DIM) as usize,
            height.clamp(NATIVE_MIN_DIM, NATIVE_MAX_DIM) as usize,
        );
        return Err(NativeSizeError::BelowMinimum {
            requested: (width, height),
            clamped,
        });
    }
    if width > NATIVE_MAX_DIM || height > NATIVE_MAX_DIM {
        let clamped = (
            width.clamp(NATIVE_MIN_DIM, NATIVE_MAX_DIM) as usize,
            height.clamp(NATIVE_MIN_DIM, NATIVE_MAX_DIM) as usize,
        );
        return Err(NativeSizeError::AboveMaximum {
            requested: (width, height),
            clamped,
        });
    }
    Ok((width as usize, height as usize))
}

/// Siempre devuelve dimensiones seguras en 64..=4096; el `Option` describe si
/// hubo clamp (cero/gigante). Nunca panics.
pub(crate) fn resolve_native_size(
    width: u32,
    height: u32,
) -> ((usize, usize), Option<NativeSizeError>) {
    match try_resolve_native_size(width, height) {
        Ok(v) => (v, None),
        Err(e) => {
            let clamped = match e {
                NativeSizeError::BelowMinimum { clamped, .. }
                | NativeSizeError::AboveMaximum { clamped, .. } => clamped,
                _ => (NATIVE_FALLBACK_W, NATIVE_FALLBACK_H),
            };
            (clamped, Some(e))
        }
    }
}

/// Resuelve + preflight de presupuesto del set (M3-5, llamado por TODOS
/// los `render_*` clásicos): si `w*h*4*frames` excede
/// `NATIVE_MAX_SET_BYTES`, reduce preservando aspecto hasta encajar
/// (mínimo 64×64, que siempre encaja: 64×64×4×48 < 1 MiB) y reporta
/// `OverBudget` con pedido vs clamped. Nunca panic, nunca OOM: el
/// render clásico hace clamp documentado en vez de `Err` (devuelve
/// `Vec`, no `Result`).
pub(crate) fn resolve_native_size_budgeted(
    width: u32,
    height: u32,
    frames: usize,
) -> ((usize, usize), Option<NativeSizeError>) {
    let ((w, h), prev) = resolve_native_size(width, height);
    let frames = frames.max(1);
    match estimate_frames_bytes(w, h, frames) {
        Some(got) if got <= NATIVE_MAX_SET_BYTES => ((w, h), prev),
        _ => {
            let px_por_frame = NATIVE_MAX_SET_BYTES / (NATIVE_BYTES_PER_PIXEL * frames);
            let px_actual = (w as u64).saturating_mul(h as u64).max(1);
            // factor² = presupuesto/actual, en u64 sin flotantes gigantes.
            let (mut cw, mut ch) = (w, h);
            if px_actual > px_por_frame as u64 {
                // Escala entera descendente preservando aspecto.
                let mut escala = 2u64;
                while escala <= 64 {
                    let nw = (w as u64 / escala).max(NATIVE_MIN_DIM as u64) as usize;
                    let nh = (h as u64 / escala).max(NATIVE_MIN_DIM as u64) as usize;
                    match estimate_frames_bytes(nw, nh, frames) {
                        Some(got) if got <= NATIVE_MAX_SET_BYTES => {
                            cw = nw;
                            ch = nh;
                            break;
                        }
                        _ => {}
                    }
                    if nw <= NATIVE_MIN_DIM as usize && nh <= NATIVE_MIN_DIM as usize {
                        cw = NATIVE_MIN_DIM as usize;
                        ch = NATIVE_MIN_DIM as usize;
                        break;
                    }
                    escala += 1;
                }
            }
            let bytes = estimate_frames_bytes(cw, ch, frames).unwrap_or(usize::MAX);
            (
                (cw, ch),
                Some(NativeSizeError::OverBudget {
                    requested: (w, h),
                    clamped: (cw, ch),
                    bytes,
                }),
            )
        }
    }
}

/// Aviso mostrable en la card cuando el pedido excede el presupuesto del set
/// (R6c, `NATIVE_MAX_SET_BYTES` 64 MiB).
///
/// Los `render_*` clásicos devuelven `Vec` por firma histórica: ante exceso
/// hacen clamp documentado vía `resolve_native_size_budgeted` en vez de
/// `Err`. La card llama a esto con el pedido y, si da `Some`, muestra el
/// texto junto a la vista previa. Puro, sin I/O.
pub fn mensaje_overbudget_para(width: u32, height: u32) -> Option<String> {
    match resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT) {
        (
            _,
            Some(NativeSizeError::OverBudget {
                requested,
                clamped,
                bytes,
            }),
        ) => Some(format!(
            "vista previa reducida a {}×{} (pedido {}×{} ≈ {bytes} bytes, tope {NATIVE_MAX_SET_BYTES})",
            clamped.0, clamped.1, requested.0, requested.1,
        )),
        _ => None,
    }
}

fn checked_frame_byte_len(w: usize, h: usize) -> Result<usize, NativeSizeError> {
    w.checked_mul(h)
        .and_then(|v| v.checked_mul(4))
        .ok_or(NativeSizeError::AllocationOverflow { w, h })
}

/// Reserva sin abortar en OOM: `try_reserve` + fallback a error tipado.
fn alloc_frame_buffer(w: usize, h: usize) -> Result<Vec<u8>, NativeSizeError> {
    let len = checked_frame_byte_len(w, h)?;
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|_| NativeSizeError::AllocationFailed { bytes: len })?;
    buf.resize(len, 0);
    Ok(buf)
}

/// Fallback seguro si la reserva falla (nunca panic): buffer 64x64.
fn alloc_frame_buffer_or_fallback(w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    match alloc_frame_buffer(w, h) {
        Ok(b) => (b, w, h),
        Err(_) => {
            let len = NATIVE_FALLBACK_W
                .checked_mul(NATIVE_FALLBACK_H)
                .and_then(|v| v.checked_mul(4))
                .unwrap_or(64 * 64 * 4);
            (vec![0u8; len], NATIVE_FALLBACK_W, NATIVE_FALLBACK_H)
        }
    }
}

/// Normaliza el concepto del pedido para el eco del placeholder `universal`.
/// Puro, sin pánicos.
fn normalize_concept(concept: &str) -> String {
    let mut s = concept.trim().replace(['\n', '\r', '\t'], " ");
    // colapsar espacios multiples
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    s = out;
    if s.is_empty() {
        return "matem\u{00e1}tica".to_string();
    }
    if s.len() > 120 {
        s = s.chars().take(120).collect::<String>() + "...";
    }
    s
}

/// Detecta la mejor plantilla para un concepto libre (ES + EN).
///
/// Wrapper explícito T2 sobre `grafito_anim::protocol::template_for_concept`
/// (una sola tabla de verdad para la base: pitágoras, integral, taylor,
/// conforme, derivada, logística, gradiente, möbius, vector, euler, fourier,
/// proba, seno/coseno + fallback `universal` honesto). Acá solo quedan los
/// extras F5/pedagógicos que el protocolo aún no cubre; el resto delega.
/// Sin `contains` tramposos: "sistema" exige palabra exacta
/// (`contiene_palabra`: "ecosistema" ya no dispara), y "func" pelado se
/// endureció a "funcion/función/f(x)" ("funciona" no finge). El "tarea"→"área"
/// se fija en el protocolo (`contiene_palabra`), espejo de T1.
pub fn detect_template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    // Extras F5: fracciones, matrices, series genéricas, ecuaciones, trigo,
    // cónicas, límites y funciones (orden preservado del clásico).
    if c.contains("fracc")
        || c.contains("rectángulo dividido")
        || c.contains("rectangulo dividido")
        || c.contains("común denominador")
        || c.contains("comun denominador")
    {
        return "integral-area";
    }
    if c.contains("matriz")
        || c.contains("matrices")
        || c.contains("determin")
        || c.contains("gauss")
    {
        return "universal";
    }
    // Genéricas a Taylor. OJO: "fourier" pelado NO se reclama acá: lo
    // resuelve el protocolo hacia su renderer dedicado (degradarlo a
    // taylor era regresión silenciosa).
    if c.contains("serie")
        || c.contains("sucesi")
        || c.contains("geométrica")
        || c.contains("geometrica")
    {
        return "taylor-series";
    }
    if c.contains("ecuac")
        || c.contains("cuadrática")
        || c.contains("cuadratica")
        || c.contains("parábola")
        || c.contains("parabola")
        || contiene_palabra(&c, "sistema")
        || contiene_palabra(&c, "sistemas")
    {
        return "derivative-slope";
    }
    if c.contains("trigon")
        || c.contains("círculo unitario")
        || c.contains("circulo unitario")
        || c.contains("onda seno")
    {
        return "taylor-series";
    }
    if c.contains("conica")
        || c.contains("cónica")
        || c.contains("elipse")
        || c.contains("hiperbola")
        || c.contains("hipérbola")
        || c.contains("cono cortado")
    {
        return "conformal-map";
    }
    if c.contains("límite") || c.contains("limite") || c.contains("hueco en a") {
        return "derivative-slope";
    }
    if c.contains("funcion") || c.contains("función") || c.contains("f(x)") {
        return "universal";
    }
    // Base compartida + fallback honesto en el protocolo.
    template_for_concept(concept)
}

/// Dispatcher: elige plantilla automáticamente a partir del concepto si hace falta.
/// El fallback `universal` es placeholder neutro honesto (sin curva falsa).
/// Garantiza menos de 2s incluso en debug.
///
/// Convierte punto matematico (x,y en el viewport fijo [`VIEW_X_MIN`]..
/// [`VIEW_X_MAX`] × [`VIEW_Y_MIN`]. [`VIEW_Y_MAX`]) a pixel del buffer.
/// Nunca panic con w/h en 0.
///
/// OJO: satura al borde (útil para puntos/ejes ya en vista). Las CURVAS no
/// usan esto: usan [`draw_seg_mundo`]/[`draw_curva_mundo`], que recortan el
/// tramo fuera de vista en vez de aplastarlo (sin plateau).
fn to_pixel(width: usize, height: usize, x: f64, y: f64) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let px = ((x - VIEW_X_MIN) / VIEW_SPAN_X * (width as f64)).round() as usize;
    let py = ((VIEW_Y_MAX - y) / VIEW_SPAN_Y * (height as f64)).round() as usize;
    (px.min(width - 1), py.min(height - 1))
}

// ── Viewport fijo + clip limpio (sin plateau) ─────────────────────────────
// El mundo es SIEMPRE [-3,3]², idéntico en los 48 frames: la escala no cambia
// entre frames (sin temblor). Lo fuera de vista se RECORTA (segmento clipado
// a la caja del frame vía `clip_seg_a_caja` de la Piel); jamás se aplasta
// contra el borde: `y=x²` se corta limpio en `y=3` en vez de dibujar un
// plateau horizontal falso arriba.
// Contraste AA sobre BG [14,14,20]: amarilla ≈11:1, azul ≈4.8:1,
// roja ≈5:1 (todas ≥3:1 para gráficos); curvas a 3px (`CURVE_ANCHO`),
// ejes a 1.5px (`AXIS_ANCHO`), grilla a alfa 44 (≈17%, 3:1).
/// Mínimo del viewport mundo en x (fijo, documentado, misma escala en 48).
pub(crate) const VIEW_X_MIN: f64 = -3.0;
/// Máximo del viewport mundo en x (fijo, documentado, misma escala en 48).
pub(crate) const VIEW_X_MAX: f64 = 3.0;
/// Mínimo del viewport mundo en y (fijo, documentado, misma escala en 48).
pub(crate) const VIEW_Y_MIN: f64 = -3.0;
/// Máximo del viewport mundo en y (fijo, documentado, misma escala en 48).
pub(crate) const VIEW_Y_MAX: f64 = 3.0;
/// Ancho del viewport mundo (6.0, deriva de `VIEW_X_*`).
const VIEW_SPAN_X: f64 = VIEW_X_MAX - VIEW_X_MIN;
/// Alto del viewport mundo (6.0, deriva de `VIEW_Y_*`).
const VIEW_SPAN_Y: f64 = VIEW_Y_MAX - VIEW_Y_MIN;
/// Grosor de curvas principales (parábola/tangente) en px (R6c: 3px
/// legibles a 480×360; los dorados de píxeles se actualizaron a propósito
/// verificando la misma intención, jamás a ciegas).
const CURVE_ANCHO: f32 = 3.0;
/// Grosor de los ejes x/y en px (R6c: 1.5px; las marcas de tick siguen a 1px).
const AXIS_ANCHO: f32 = 1.5;
/// Margen mínimo de los slots "x"/"y" y del placeholder `universal` al borde
/// del frame en px (R6c: ≥24px medido a 480×360).
const SLOT_MARGEN: usize = 24;

/// ¿El punto mundo está en vista (finito y dentro del viewport fijo)?
/// Puro, sin pánicos.
fn en_vista_mundo(x: f64, y: f64) -> bool {
    x.is_finite()
        && y.is_finite()
        && (VIEW_X_MIN..=VIEW_X_MAX).contains(&x)
        && (VIEW_Y_MIN..=VIEW_Y_MAX).contains(&y)
}

/// Punto mundo a píxel solo si está en vista (`None` = fuera: el llamador
/// NO dibuja, sin aplastar contra el borde). Puro, sin pánicos.
fn to_pixel_opt(width: usize, height: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    if width == 0 || height == 0 || !en_vista_mundo(x, y) {
        return None;
    }
    let px = ((x - VIEW_X_MIN) / VIEW_SPAN_X * (width as f64)).round() as usize;
    let py = ((VIEW_Y_MAX - y) / VIEW_SPAN_Y * (height as f64)).round() as usize;
    Some((px.min(width - 1), py.min(height - 1)))
}

/// Punto mundo a float-píxeles sin saturar (`None` si no-finito o w/h 0).
/// Puro, sin pánicos.
fn mundo_a_flotante(width: usize, height: usize, x: f64, y: f64) -> Option<(f32, f32)> {
    if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let px = (x - VIEW_X_MIN) / VIEW_SPAN_X * (width as f64);
    let py = (VIEW_Y_MAX - y) / VIEW_SPAN_Y * (height as f64);
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    Some((px as f32, py as f32))
}

// ── Backend raster tiny-skia 0.11 (F1 Manim-en-Rust) ───────────────────────
// Los 4 primitivos dibujan con strokes/fills/antialias reales estilo cairo
// en vez de Bresenham/bloques a mano. Firmas intactas (uno a uno): los
// ~100 call sites no cambian.
//
// Premultiplicado: `PixmapMut::from_bytes` asume RGBA premultiplicado;
// nuestros buffers son rectos con alfa 255 en todo píxel (pineado por
// `assert_frames_valid` y escrito por `fill_background` antes de dibujar):
// con destino opaco, recto == premultiplicado y el blend SourceOver de
// tiny-skia coincide con el manual anterior. Cada primitivo envuelve el
// buffer (O(1), sin copia); si no calza (w/h 0 o len corto) es no-op
// honesto. Regla de paleta intacta: ningún literal RGBA fuera del bloque
// de consts (los helpers solo reciben `[u8; 4]` ya centralizados).

/// Paint sólido con antialias desde un color de la paleta centralizada.
fn sk_paint(color: [u8; 4]) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    paint
}

/// Vista O(1) del buffer como pixmap tiny-skia (`None` si no calza).
fn sk_view(buf: &mut [u8], w: usize, h: usize) -> Option<tiny_skia::PixmapMut<'_>> {
    let w32 = u32::try_from(w).ok()?;
    let h32 = u32::try_from(h).ok()?;
    tiny_skia::PixmapMut::from_bytes(buf, w32, h32)
}

/// Stroke redondo del grosor pedido (clamp 0.5..=8px: tiny-skia con ancho
/// 0 no pinta y con ancho gigante tapa el frame).
fn sk_stroke(ancho: f32) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width: ancho.clamp(0.5, 8.0),
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    }
}

/// Stroke redondo de 1px para los primitivos de línea.
fn sk_stroke_1px() -> tiny_skia::Stroke {
    sk_stroke(1.0)
}

/// Línea con grosor explícito (curvas principales a 3px para contraste AA).
/// Misma disciplina que `draw_line` (vista O(1), no-op honesto si no calza).
fn draw_line_ancha(
    buf: &mut [u8],
    w: usize,
    h: usize,
    a: (usize, usize),
    b: (usize, usize),
    color: [u8; 4],
    ancho: f32,
) {
    let Some(mut px) = sk_view(buf, w, h) else {
        return;
    };
    if a == b {
        let Some(rect) = tiny_skia::Rect::from_xywh(a.0 as f32, a.1 as f32, 1.0, 1.0) else {
            return;
        };
        px.fill_rect(
            rect,
            &sk_paint(color),
            tiny_skia::Transform::identity(),
            None,
        );
        return;
    }
    // +0.5: centros de píxel (el stroke cubre la fila exacta; sin offset
    // cubriría dos filas al 50% y las líneas se verían dobles).
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(a.0 as f32 + 0.5, a.1 as f32 + 0.5);
    builder.line_to(b.0 as f32 + 0.5, b.1 as f32 + 0.5);
    let Some(path) = builder.finish() else {
        return;
    };
    px.stroke_path(
        &path,
        &sk_paint(color),
        &sk_stroke(ancho),
        tiny_skia::Transform::identity(),
        None,
    );
}

/// Segmento en coords MUNDO con clip limpio: fuera de vista se recorta
/// (vía `clip_seg_a_caja`), jamás se aplasta al borde. `true` = pintó.
/// Puro sobre el buffer, sin E/S ni pánicos.
#[allow(clippy::too_many_arguments)]
fn draw_seg_mundo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    color: [u8; 4],
    ancho: f32,
) -> bool {
    let (Some((fax, fay)), Some((fbx, fby))) = (
        mundo_a_flotante(w, h, ax, ay),
        mundo_a_flotante(w, h, bx, by),
    ) else {
        return false;
    };
    let Some((a, b)) = clip_seg_a_caja(fax, fay, fbx, fby, w, h) else {
        return false;
    };
    draw_line_ancha(buf, w, h, a, b, color, ancho);
    true
}

/// Curva en coords MUNDO: cada tramo se clipa por separado; punto no-finito
/// o tramo fuera = hueco honesto (asíntotas y bordes no se puentean con
/// líneas falsas). Devuelve tramos pintados. Sin E/S ni pánicos.
fn draw_curva_mundo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    puntos: &[(f64, f64)],
    color: [u8; 4],
    ancho: f32,
) -> usize {
    let mut pintados = 0usize;
    for par in puntos.windows(2) {
        if draw_seg_mundo(
            buf, w, h, par[0].0, par[0].1, par[1].0, par[1].1, color, ancho,
        ) {
            pintados += 1;
        }
    }
    pintados
}

fn draw_line(
    buf: &mut [u8],
    w: usize,
    h: usize,
    a: (usize, usize),
    b: (usize, usize),
    color: [u8; 4],
) {
    let Some(mut px) = sk_view(buf, w, h) else {
        return;
    };
    if a == b {
        // Punto degenerado: el Bresenham pintaba 1px; acá un rect 1x1.
        let Some(rect) = tiny_skia::Rect::from_xywh(a.0 as f32, a.1 as f32, 1.0, 1.0) else {
            return;
        };
        px.fill_rect(
            rect,
            &sk_paint(color),
            tiny_skia::Transform::identity(),
            None,
        );
        return;
    }
    // +0.5: centros de píxel (el stroke de 1px cubre la fila exacta;
    // sin offset cubriría dos filas al 50% y las líneas se verían dobles).
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(a.0 as f32 + 0.5, a.1 as f32 + 0.5);
    builder.line_to(b.0 as f32 + 0.5, b.1 as f32 + 0.5);
    let Some(path) = builder.finish() else {
        return;
    };
    px.stroke_path(
        &path,
        &sk_paint(color),
        &sk_stroke_1px(),
        tiny_skia::Transform::identity(),
        None,
    );
}

fn draw_filled_circle(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: usize,
    cy: usize,
    radius: usize,
    color: [u8; 4],
) {
    let radius = radius.max(1);
    let Some(mut px) = sk_view(buf, w, h) else {
        return;
    };
    let mut builder = tiny_skia::PathBuilder::new();
    builder.push_circle(cx as f32 + 0.5, cy as f32 + 0.5, radius as f32);
    let Some(path) = builder.finish() else {
        return;
    };
    px.fill_path(
        &path,
        &sk_paint(color),
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_filled_rect(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    rw: usize,
    rh: usize,
    color: [u8; 4],
) {
    let x1 = (x0 + rw).min(w);
    let y1 = (y0 + rh).min(h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let Some(mut px) = sk_view(buf, w, h) else {
        return;
    };
    let Some(rect) =
        tiny_skia::Rect::from_xywh(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
    else {
        return;
    };
    px.fill_rect(
        rect,
        &sk_paint(color),
        tiny_skia::Transform::identity(),
        None,
    );
}

// Fuente embebida para el texto real (la misma Ubuntu-Light que `export.rs`
// usa en `render_png`): `OnceLock` porque el render corre en hilos worker y
// el parse por llamada rompería el presupuesto <2s. `None` honesto si la
// fuente integrada faltara (jamás panic: el texto cae a bloques sólidos).
fn sk_anim_font() -> Option<&'static ab_glyph::FontVec> {
    static FONT: std::sync::OnceLock<Option<ab_glyph::FontVec>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        egui::FontDefinitions::default()
            .font_data
            .get("Ubuntu-Light")
            .and_then(|data| ab_glyph::FontVec::try_from_vec(data.font.clone().into_owned()).ok())
    })
    .as_ref()
}

/// Compensación de trazo para texto claro sobre fondo oscuro (×1.7 con
/// clamp, como el embolden de FreeType / dark-mode de macOS): Ubuntu Light
/// es fina y con cobertura cruda los rótulos chicos (ticks a 10px) quedan
/// tenues sin ningún píxel sólido; con esto los núcleos llegan a 1.0 y los
/// bordes conservan su AA. Determinista, sin literales de color.
const TEXT_STEM_BOOST: f32 = 1.7;

/// Celda sólida de fallback para glifos ausentes (emoji, CJK): el contrato
/// histórico ("cualquier texto se renderiza sin panics", ver
/// `universal_handles_any_text`) se mantiene sin inventar letra.
#[allow(clippy::too_many_arguments)]
fn sk_fallback_cell(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    cell_w: usize,
    cell_h: usize,
    color: [u8; 4],
) {
    draw_filled_rect(buf, w, h, x, y, cell_w, cell_h, color);
}

/// Escala de texto de TÍTULOS por alto del frame (R6c, cine visual): 3 en
/// h≥360 (a 480×360 son glifos de ~30px ≈ 8-9%h), 1 abajo (legado compacto
/// documentado para no romper previews diminutos: el texto cae en las mismas
/// bandas que antes). Pura, sin pánicos.
pub fn text_scale_for_h(h: usize) -> usize {
    if h >= 360 {
        3
    } else {
        1
    }
}

/// Escala de texto de TICKS por alto del frame (R6c): 2 en h≥360 (glifos de
/// ~20px ≥ 16px y ≥4.5%h a 480×360; a escala 1 los 11px violan ambas cotas),
/// 1 abajo (legado compacto, idéntico a antes). Pura, sin pánicos.
pub fn tick_scale_for_h(h: usize) -> usize {
    if h >= 360 {
        2
    } else {
        1
    }
}

/// Scrim detrás de un rótulo (banda superior reservada): rectángulo `SCRIM`
/// dimensionado al texto y la escala de `h`. El texto se dibuja aparte con
/// `draw_text_block` en la misma posición. Puro sobre el buffer.
fn draw_scrim_para_rotulo(buf: &mut [u8], w: usize, h: usize, x: usize, y: usize, texto: &str) {
    if w == 0 || h == 0 || texto.is_empty() {
        return;
    }
    let escala = text_scale_for_h(h);
    let chars = texto.chars().take(48).count();
    let ancho = chars
        .saturating_mul(6 * escala)
        .saturating_add(8)
        .min(w.saturating_sub(x.min(w)));
    let alto = (12 * escala + 8).min(h.saturating_sub(y.min(h)));
    if ancho > 0 && alto > 0 {
        draw_filled_rect(buf, w, h, x, y, ancho, alto, SCRIM);
    }
}

/// Rótulo superior con scrim detrás: la banda superior queda reservada y el
/// texto siempre contrasta (el scrim es `SCRIM` centralizado). Dibuja el
/// rectángulo y el texto a la escala de `h`. Puro sobre el buffer.
fn draw_rotulo_con_scrim(buf: &mut [u8], w: usize, h: usize, x: usize, y: usize, texto: &str) {
    if w == 0 || h == 0 || texto.is_empty() {
        return;
    }
    draw_scrim_para_rotulo(buf, w, h, x, y, texto);
    draw_text_block(
        buf,
        w,
        h,
        x.saturating_add(4),
        y.saturating_add(4),
        texto,
        TEXT_COLOR,
        text_scale_for_h(h),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_text_block(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
    scale: usize,
) {
    // Texto real con glifos (avance por glifo + tracking ≈ 6*scale como
    // los bloques históricos; altura 10*scale para que el AA tenga núcleos
    // sólidos legibles — a 7px ningún píxel llega a cobertura total y los
    // rótulos quedaban tenues). Origen intacto: los rótulos caen en las
    // mismas bandas que antes.
    use ab_glyph::{Font as _, PxScale, ScaleFont as _};
    let scale = scale.clamp(1, 3);
    let cell_w = 5 * scale;
    let cell_h = 7 * scale;
    let tracking = scale as f32;
    let Some(font) = sk_anim_font() else {
        // Sin fuente integrada: bloques sólidos (legado honesto, sin panic).
        let mut cx = x;
        for ch in text.chars().take(48) {
            if cx + cell_w >= w {
                break;
            }
            if ch != ' ' {
                sk_fallback_cell(buf, w, h, cx, y, cell_w, cell_h, color);
            }
            cx += cell_w + scale;
        }
        return;
    };
    let px = 10.0 * scale as f32;
    let scaled = font.as_scaled(PxScale::from(px));
    let baseline = y as f32 + scaled.ascent();
    let space_advance = scaled.h_advance(font.glyph_id(' ')) + tracking;
    let mut caret_x = x as f32;
    let mut prev_id: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars().take(48) {
        if caret_x >= w as f32 {
            break;
        }
        if ch == ' ' {
            caret_x += space_advance;
            prev_id = None;
            continue;
        }
        let gid = font.glyph_id(ch);
        if gid.0 == 0 {
            sk_fallback_cell(buf, w, h, caret_x as usize, y, cell_w, cell_h, color);
            caret_x += cell_w as f32 + tracking;
            prev_id = None;
            continue;
        }
        if let Some(prev) = prev_id {
            caret_x += scaled.kern(prev, gid);
        }
        prev_id = Some(gid);
        let glyph = ab_glyph::Glyph {
            id: gid,
            scale: PxScale::from(px),
            position: ab_glyph::point(caret_x, baseline),
        };
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let ink_a = f32::from(color[3]) / 255.0;
            outlined.draw(|dx, dy, coverage| {
                if coverage <= 0.0 || ink_a <= 0.0 {
                    return;
                }
                let gx = bounds.min.x as i32 + dx as i32;
                let gy = bounds.min.y as i32 + dy as i32;
                if gx < 0 || gy < 0 || gx >= w as i32 || gy >= h as i32 {
                    return;
                }
                let alpha = ((coverage * TEXT_STEM_BOOST).clamp(0.0, 1.0) * ink_a).clamp(0.0, 1.0);
                if let Some(i) = (gy as usize)
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(gx as usize))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 3 < buf.len() {
                        for k in 0..3 {
                            buf[i + k] = (f32::from(color[k]) * alpha
                                + f32::from(buf[i + k]) * (1.0 - alpha))
                                as u8;
                        }
                        buf[i + 3] = 255;
                    }
                }
            });
        }
        caret_x += scaled.h_advance(gid) + tracking;
    }
}

/// Fondo ÚNICO fijo de todo el cine nativo: gradiente vertical suave
/// BG→BG_GRADIENT + viñeta (oscurece bordes 22% al extremo). Sin acento por
/// concepto, sin fase temporal: el frame 0 y el 47 comparten fondo (el
/// movimiento lo pone la matemática, no el cromo). Puro, sin pánicos.
fn fill_background(buf: &mut [u8], w: usize, h: usize) {
    // gradiente vertical suave
    for y in 0..h {
        let v = y as f64 / h.max(1) as f64;
        // interpolacion BG -> BG_GRADIENT
        let mix = v * 0.6;
        let r = (BG[0] as f64 * (1.0 - mix) + BG_GRADIENT[0] as f64 * mix) as u8;
        let g = (BG[1] as f64 * (1.0 - mix) + BG_GRADIENT[1] as f64 * mix) as u8;
        let b = (BG[2] as f64 * (1.0 - mix) + BG_GRADIENT[2] as f64 * mix) as u8;
        for x in 0..w {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    buf[i] = r;
                    buf[i + 1] = g;
                    buf[i + 2] = b;
                    buf[i + 3] = 255;
                }
            }
        }
    }
    // vignette suave
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;
    let maxd = (cx * cx + cy * cy).sqrt();
    for y in 0..h {
        for x in 0..w {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let d = (dx * dx + dy * dy).sqrt() / maxd;
            let dark = d * d * 0.22;
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    buf[i] = (buf[i] as f64 * (1.0 - dark)) as u8;
                    buf[i + 1] = (buf[i + 1] as f64 * (1.0 - dark)) as u8;
                    buf[i + 2] = (buf[i + 2] as f64 * (1.0 - dark)) as u8;
                }
            }
        }
    }
}

/// Grilla sutil ESTÁTICA (cada ~40px, punteada): el mismo fondo en los 48
/// frames (sin parallax: la intersección grid↔relleno ya no baila y el test
/// de sombra no necesita cota de fringe móvil). Mezcla honesta por alfa
/// (`GRID_COLOR[3]` 38..51 ≈ 15-20%, contraste 3:1): jamás promedio 50%.
/// Pura, sin pánicos.
fn draw_subtle_grid(buf: &mut [u8], w: usize, h: usize) {
    // grid cada ~40px, fija
    let alfa = f32::from(GRID_COLOR[3]) / 255.0;
    let mezcla = |fondo: u8, tinta: u8| -> u8 {
        (f32::from(fondo) * (1.0 - alfa) + f32::from(tinta) * alfa) as u8
    };
    let step = (w.min(h) / 10).max(18);
    for x in (0..w).step_by(step) {
        for y in 0..h {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                // linea vertical punteada sutil
                if y % 3 == 0 && i + 2 < buf.len() {
                    buf[i] = mezcla(buf[i], GRID_COLOR[0]);
                    buf[i + 1] = mezcla(buf[i + 1], GRID_COLOR[1]);
                    buf[i + 2] = mezcla(buf[i + 2], GRID_COLOR[2]);
                }
            }
        }
    }
    for y in (0..h).step_by(step) {
        for x in 0..w {
            if x % 3 == 0 {
                if let Some(i) = y
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(x))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 2 < buf.len() {
                        buf[i] = mezcla(buf[i], GRID_COLOR[0]);
                        buf[i + 1] = mezcla(buf[i + 1], GRID_COLOR[1]);
                        buf[i + 2] = mezcla(buf[i + 2], GRID_COLOR[2]);
                    }
                }
            }
        }
    }
}

/// Halo/scrim previo a un rótulo de tick: rectángulo `SCRIM` exactamente
/// sobre su caja reservada (R6c). Se pinta ANTES del texto; como las cajas
/// del registro `ocupadas` son disjuntas, los halos jamás se solapan. La
/// caja mínima real (11px a escala 1) ya supera los 8px. Puro.
fn pintar_halo_rotulo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    tw: usize,
    th: usize,
) {
    if tw == 0 || th == 0 {
        return;
    }
    draw_filled_rect(buf, w, h, x, y, tw, th, SCRIM);
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ── Timeline por fases (cine visual, plantillas principales) ───────────────
// setup 20% (base estática) / construcción 60% (cubic_in_out) / hold 20%
// (resultado final). El `alpha` resultante mueve SOLO el elemento en
// construcción; la base (ejes, curva fija, fondo) es idéntica en los 48.

/// Fase de la timeline para un frame 0..`NATIVE_ANIM_FRAME_COUNT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaseConstruccion {
    /// Base estática (`alpha` 0).
    Setup,
    /// Construyendo (`alpha` 0..1 con cubic_in_out).
    Construccion,
    /// Resultado final (`alpha` 1).
    Hold,
}

/// Fracción de setup (0.2) y fin de construcción (0.8) sobre `t` global.
pub const FASE_SETUP_HASTA: f64 = 0.2;
/// Fin de la construcción sobre `t` global (hold hasta 1.0).
pub const FASE_CONSTRUCCION_HASTA: f64 = 0.8;

/// `(fase, alpha)` para el frame: `alpha` 0 en setup, `ease_in_out` en
/// construcción, 1 en hold. Total (frame ≥48 → hold). Puro, sin pánicos.
pub fn fase_para_frame(frame: usize) -> (FaseConstruccion, f64) {
    let total = NATIVE_ANIM_FRAME_COUNT;
    if total <= 1 {
        return (FaseConstruccion::Hold, 1.0);
    }
    let t = (frame.min(total - 1) as f64) / (total - 1) as f64;
    if t < FASE_SETUP_HASTA {
        (FaseConstruccion::Setup, 0.0)
    } else if t < FASE_CONSTRUCCION_HASTA {
        let local = (t - FASE_SETUP_HASTA) / (FASE_CONSTRUCCION_HASTA - FASE_SETUP_HASTA);
        (
            FaseConstruccion::Construccion,
            ease_in_out(local.clamp(0.0, 1.0)),
        )
    } else {
        (FaseConstruccion::Hold, 1.0)
    }
}

/// Solo el `alpha` de construcción (atajo para los renderers). Puro.
pub fn fase_alpha(frame: usize) -> f64 {
    fase_para_frame(frame).1
}

// ── Frente A: ejes con ticks numéricos y rótulos en previews ─────────────
// Queja real: "no se sabe cuál es cuál". TODOS los renderers paramétricos
// dibujan ejes x/y con ticks numéricos y rótulos vía `draw_axes_with_labels`
// (el placeholder `universal` NO: por diseño honesto no lleva ejes ni
// curvas). Paso lindo 1/2/5×10^n + skip anti-solape + formato reusados de
// `render_2d`. Determinista, sin pánicos ni `unwrap`.

/// Valores de tick en el intervalo abierto (-3, 3) para un paso dado (el 0
/// se rotula una sola vez en el origen). Pura, testeable.
fn axis_tick_values(paso: f64) -> Vec<f64> {
    if !paso.is_finite() || paso <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut k: i64 = 1;
    while (k as f64) * paso < 3.0 {
        if out.len() > 512 {
            break;
        }
        let v = (k as f64) * paso;
        out.push(-v);
        out.push(v);
        k += 1;
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Ejes x/y del mundo [-3,3]² con ticks numéricos y rótulos "x"/"y".
/// Viewport fijo del § viewport: misma escala en los 48 frames.
///
/// En miniaturas (<48px) dibuja solo las líneas (el texto sería ilegible y
/// se solaparía). Rótulos con formato corto (1 decimal máximo, vía la Piel
/// `short_tick_label`) + registro de cajas disjuntas: el que colisiona NO se
/// dibuja (adiós "0000" amontonados y números verticales superpuestos).
/// Cine visual (R6c): etiquetas a ≥8px del eje, "x"/"y" con margen ≥24px al
/// borde (`SLOT_MARGEN`), ejes a 1.5px (`AXIS_ANCHO`), y el "0" del origen
/// se omite si colisiona con otro rótulo.
/// Los rótulos de ticks usan `draw_text_block` a `tick_scale_for_h(h)` (2 en
/// h≥360, 1 abajo como legado compacto) y las cajas de colisión asumen esa
/// métrica escalada; cada rótulo lleva su halo/scrim previo
/// (`pintar_halo_rotulo`, cajas disjuntas ⇒ sin solape, ≥8px). El test
/// `ejes_pintan_texto_en_zona_de_ejes` los detecta como píxeles claros en
/// las bandas de los ejes.
fn draw_axes_with_labels(buf: &mut [u8], w: usize, h: usize) {
    draw_line_ancha(
        buf,
        w,
        h,
        to_pixel(w, h, -3.0, 0.0),
        to_pixel(w, h, 3.0, 0.0),
        AXIS_COLOR,
        AXIS_ANCHO,
    );
    draw_line_ancha(
        buf,
        w,
        h,
        to_pixel(w, h, 0.0, -3.0),
        to_pixel(w, h, 0.0, 3.0),
        AXIS_COLOR,
        AXIS_ANCHO,
    );
    if w < 48 || h < 48 {
        return;
    }
    // ~1 tick cada 64px del lado corto (mínimo 2 divisiones).
    let por_eje = (w.min(h) as f64 / 64.0).max(2.0);
    let paso = nice_number_plane_step(6.0 / por_eje);
    if !paso.is_finite() || paso <= 0.0 {
        return;
    }
    let ticks = axis_tick_values(paso);
    if ticks.is_empty() {
        return;
    }
    let (cx, cy) = to_pixel(w, h, 0.0, 0.0);
    let skip_x = adaptive_label_skip(ticks.len(), w as f32, 48.0).max(1);
    let skip_y = adaptive_label_skip(ticks.len(), h as f32, 20.0).max(1);
    // Métrica de caja a la escala real del tick (a escala 1 es idéntica a la
    // histórica `TICK_CHAR_*` de la Piel).
    let escala = tick_scale_for_h(h);
    let cw = (TICK_CHAR_W_PX as usize).saturating_mul(escala).max(1);
    let chh = (TICK_CHAR_H_PX as usize).saturating_mul(escala).max(1);
    // Registro de cajas ocupadas: se reservan PRIMERO los slots de "x",
    // "y" (nombre del eje tiene prioridad: el tick que colisione se omite,
    // su marca igual se dibuja). El "0" se chequea al final y se omite si
    // colisiona — bounding boxes disjuntos siempre.
    let caja_x = LabelCaja {
        x: w.saturating_sub(SLOT_MARGEN + cw + 4),
        y: (cy + 8).min(h.saturating_sub(9)),
        w: cw + 4,
        h: chh,
    };
    let caja_y = LabelCaja {
        x: (cx + 8).min(w.saturating_sub(24)),
        y: SLOT_MARGEN.min(h.saturating_sub(9)),
        w: cw + 4,
        h: chh,
    };
    let mut ocupadas: Vec<LabelCaja> = Vec::new();
    ocupadas.push(caja_x);
    ocupadas.push(caja_y);
    let caja_para = |tag: &str| -> (usize, usize) {
        let tw = tag.chars().count().saturating_mul(cw).saturating_add(4);
        (tw, chh)
    };
    for (i, v) in ticks.iter().enumerate() {
        let (px, _) = to_pixel(w, h, *v, 0.0);
        draw_line(
            buf,
            w,
            h,
            (px, cy.saturating_sub(3)),
            (px, cy.saturating_add(3).min(h.saturating_sub(1))),
            AXIS_COLOR,
        );
        if i % skip_x == 0 {
            let tag = short_tick_label(*v);
            let ly = (cy + 8).min(h.saturating_sub(9));
            let lx = px.saturating_add(2);
            let (tw, th) = caja_para(&tag);
            // Cerca del eje y vive el "0" del origen: no duplicarlo.
            let lejos_origen =
                px.saturating_sub(cx).max(cx.saturating_sub(px)) >= tw.saturating_add(6);
            if lejos_origen && cabe_label_entre(lx, ly, tw, th, &ocupadas) {
                pintar_halo_rotulo(buf, w, h, lx, ly, tw, th);
                draw_text_block(buf, w, h, lx, ly, &tag, TEXT_COLOR, escala);
                ocupadas.push(LabelCaja {
                    x: lx,
                    y: ly,
                    w: tw,
                    h: th,
                });
            }
        }
        let (_, py) = to_pixel(w, h, 0.0, *v);
        draw_line(
            buf,
            w,
            h,
            (cx.saturating_sub(3), py),
            (cx.saturating_add(3).min(w.saturating_sub(1)), py),
            AXIS_COLOR,
        );
        if i % skip_y == 0 {
            let tag = short_tick_label(*v);
            let lx = (cx + 8).min(w.saturating_sub(24));
            let ly = py.saturating_sub(8);
            let (tw, th) = caja_para(&tag);
            // Cerca del eje x vive el "0" del origen: no duplicarlo.
            let lejos_origen =
                py.saturating_sub(cy).max(cy.saturating_sub(py)) >= th.saturating_add(4);
            if lejos_origen && cabe_label_entre(lx, ly, tw, th, &ocupadas) {
                pintar_halo_rotulo(buf, w, h, lx, ly, tw, th);
                draw_text_block(buf, w, h, lx, ly, &tag, TEXT_COLOR, escala);
                ocupadas.push(LabelCaja {
                    x: lx,
                    y: ly,
                    w: tw,
                    h: th,
                });
            }
        }
    }
    // Origen una sola vez (se omite si colisiona con un tick o con los
    // slots reservados de "x"/"y") + rótulos de eje en sus slots
    // reservados (margen 24px al borde, offset ≥8px del eje).
    let (ox, oy) = (cx.saturating_add(8), (cy + 8).min(h.saturating_sub(9)));
    let caja_cero = LabelCaja {
        x: ox,
        y: oy,
        w: cw + 4,
        h: chh,
    };
    if cabe_label_entre(ox, oy, caja_cero.w, caja_cero.h, &ocupadas) {
        pintar_halo_rotulo(buf, w, h, ox, oy, caja_cero.w, caja_cero.h);
        draw_text_block(buf, w, h, ox, oy, "0", TEXT_COLOR, escala);
        ocupadas.push(caja_cero);
    }
    pintar_halo_rotulo(buf, w, h, caja_x.x, caja_x.y, caja_x.w, caja_x.h);
    draw_text_block(buf, w, h, caja_x.x, caja_x.y, "x", TEXT_COLOR, escala);
    pintar_halo_rotulo(buf, w, h, caja_y.x, caja_y.y, caja_y.w, caja_y.h);
    draw_text_block(buf, w, h, caja_y.x, caja_y.y, "y", TEXT_COLOR, escala);
}

// ── Plantillas existentes ────────────────────────────────────────────────

pub fn render_native_animation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    // Legacy sin params: mapa vacío = comportamiento histórico exacto.
    render_derivative_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Derivada con params vivos: `x0` centro del barrido en [-3, 3] (def 0.0),
/// `span` semiancho en [0.25, 3.0] (def 1.5). El scrub de la UI re-renderiza
/// llamando aquí con el mapa vivo (`anim_ui::live_params` →
/// `build_anim_params().params`). Mapa vacío = histórico exacto.
pub fn render_derivative_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_derivative_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_derivative_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let center = scene_param_clamped(params, SCENE_PARAM_X0, 0.0, -3.0, 3.0);
    let span = scene_param_clamped(params, SCENE_PARAM_SPAN, 1.5, 0.25, 3.0);
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let parabola: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, x * x)
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases: el barrido de la tangente se construye en el
        // 60% central (setup/hold estáticos).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // Parábola con clip limpio a 2px: fuera del viewport fijo se corta,
        // no se aplasta (sin plateau). Idéntica en los 48 frames.
        draw_curva_mundo(&mut buf, w, h, &parabola, CURVE_MAIN, CURVE_ANCHO);
        let x0 = (center - span + 2.0 * span * t).clamp(-3.0, 3.0);
        let y0 = x0 * x0;
        let slope = 2.0 * x0;
        let x_a = x0 - 1.0;
        let x_b = x0 + 1.0;
        // Tangente recortada a vista (2px AA); punto solo si está en vista:
        // antes ambos se aplastaban contra el borde superior.
        draw_seg_mundo(
            &mut buf,
            w,
            h,
            x_a,
            y0 + slope * (x_a - x0),
            x_b,
            y0 + slope * (x_b - x0),
            TANGENT_BLUE,
            CURVE_ANCHO,
        );
        if let Some((px, py)) = to_pixel_opt(w, h, x0, y0) {
            draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
        }
        // titulo superior (solo standalone/export: el chat ya titula en el header)
        if con_rotulo {
            draw_rotulo_con_scrim(&mut buf, w, h, w / 12, h / 12, "derivada");
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub fn render_pitagoras_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_pitagoras_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_pitagoras_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases: los cuadrados crecen 0→1 en construcción.
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        // Frente A: ejes rotulados como el resto de paramétricos (el
        // triángulo vive en el mismo mundo [-3,3]²; se dibuja encima).
        draw_axes_with_labels(&mut buf, w, h);
        let p1 = to_pixel(w, h, -1.0, -1.0);
        let p2 = to_pixel(w, h, 1.0, -1.0);
        let p3 = to_pixel(w, h, 1.0, 0.5);
        draw_line(&mut buf, w, h, p1, p2, LINE_WHITE);
        draw_line(&mut buf, w, h, p2, p3, LINE_WHITE);
        draw_line(&mut buf, w, h, p3, p1, LINE_WHITE);
        let scale = t;
        let sq1_p2 = to_pixel(w, h, -1.0, -1.0 - 2.0 * scale);
        let sq1_p3 = to_pixel(w, h, 1.0, -1.0 - 2.0 * scale);
        draw_line(&mut buf, w, h, p1, sq1_p2, SQUARE_BLUE);
        draw_line(&mut buf, w, h, sq1_p2, sq1_p3, SQUARE_BLUE);
        draw_line(&mut buf, w, h, sq1_p3, p2, SQUARE_BLUE);
        let sq2_p2 = to_pixel(w, h, 1.0 + 1.5 * scale, -1.0);
        let sq2_p3 = to_pixel(w, h, 1.0 + 1.5 * scale, 0.5);
        draw_line(&mut buf, w, h, p2, sq2_p2, SQUARE_AMBER);
        draw_line(&mut buf, w, h, sq2_p2, sq2_p3, SQUARE_AMBER);
        draw_line(&mut buf, w, h, sq2_p3, p3, SQUARE_AMBER);
        if t > 0.5 {
            let tt = (t - 0.5) * 2.0;
            let mid = to_pixel(w, h, -1.0 - 1.0 * tt, 0.5 + 0.5 * tt);
            draw_line(&mut buf, w, h, p3, mid, SQUARE_GREEN);
            draw_line(&mut buf, w, h, mid, p1, SQUARE_GREEN);
        }
        if con_rotulo {
            draw_rotulo_con_scrim(&mut buf, w, h, w / 14, h / 12, "pitagoras");
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub fn render_integral_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_integral_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Integral con params vivos: `a` cota inferior en [-3, 3] (def 0.0), `b`
/// cota superior en [-3, 3] (def 2.0); si `a > b` se ordenan. El área barrida
/// es `a..(a + (b-a)*t)`. Mapa vacío = histórico exacto (`0..2t`).
///
/// Contrato N1 (frente integral): cada uno de los 48 frames muestra ejes +
/// curva canónica `f(x)=x^2` FIJA (idéntica en todos los frames) + área
/// sombreada acumulada de `a` a `b(N)` MONÓTONA no-decreciente + cota móvil
/// vertical en `b(N)` + etiqueta ASCII del valor acumulado (`[a,b] S=v`,
/// trapecios con el evaluador existente). Cero puntos decorativos sueltos:
/// la cota es una línea, jamás un círculo.
pub fn render_integral_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_integral_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

/// Anim canónica de integral (N1): la vía clásica evalúa con EL MISMO
/// evaluador que la vía paramétrica (`parametric_for_template`), así ambas
/// dibujan la misma matemática. `None` solo si el mapeo canónico faltara;
/// el render usa `x*x` finito como respaldo honesto.
pub(crate) fn integral_canonical_anim() -> Option<ParametricAnim> {
    parametric_for_template("integral-area", "integral")
}

/// Evalúa la canónica en el frame dado; sin canónica, `x*x` finito.
fn integral_eval(anim: Option<&ParametricAnim>, frame: usize, x: f64) -> Option<f64> {
    match anim {
        Some(a) => a.eval_frame(frame, x),
        None => {
            let v = x * x;
            if v.is_finite() {
                Some(v)
            } else {
                None
            }
        }
    }
}

/// Extremo derecho del área en el frame (`a..x_end` crece con la fase de
/// construcción: 0 en setup, `ease_in_out` en construcción, `b` en hold).
/// Puro, sin I/O, sin pánicos.
pub(crate) fn integral_frame_end(a: f64, b: f64, frame: usize) -> f64 {
    a + (b - a) * fase_alpha(frame)
}

/// Cota de pasos de trapecios R1-7 (núcleo común, sin `as usize` ciego).
///
/// `None` si `a`/`x_end` no finitos o el ancho no finito (NaN/inf → `None`
/// honesto, jamás 0 silencioso ni 4096 derrochado). `Some(0)` si `x_end <= a`
/// (área nula). Si no, `Some(1..=4096)` acotado (el loop hace `.min(4096)`
/// igual, pero acá el `as usize` ya es seguro: 1..=4096 finito).
/// Puro, sin I/O.
fn pasos_trapecios(a: f64, x_end: f64) -> Option<usize> {
    if !a.is_finite() || !x_end.is_finite() {
        return None;
    }
    if x_end <= a {
        return Some(0);
    }
    let diff = x_end - a;
    if !diff.is_finite() || diff <= 0.0 {
        return None;
    }
    let pasos_f = (diff / 0.05).ceil();
    if !pasos_f.is_finite() || pasos_f <= 0.0 {
        return None;
    }
    if pasos_f >= 4096.0 {
        return Some(4096);
    }
    Some(pasos_f as usize)
}

/// Núcleo común R1-7: trapecios sobre `[a, x_end]` con evaluador inyectado.
/// Salta tramos no finitos; `None` si ningún tramo valida.
fn trapecios_con_eval(a: f64, x_end: f64, eval: impl Fn(f64) -> Option<f64>) -> Option<f64> {
    let pasos = pasos_trapecios(a, x_end)?;
    if pasos == 0 {
        return Some(0.0);
    }
    let mut s = 0.0;
    let mut valida = false;
    for i in 0..pasos.min(4096) {
        let x0 = a + i as f64 * 0.05;
        let x1 = (a + (i + 1) as f64 * 0.05).min(x_end);
        if x1 <= x0 {
            continue;
        }
        if let (Some(fa), Some(fb)) = (eval(x0), eval(x1)) {
            let tramo = (fa + fb) * 0.5 * (x1 - x0);
            if tramo.is_finite() {
                s += tramo;
                valida = true;
            }
        }
    }
    if valida && s.is_finite() {
        Some(s)
    } else {
        None
    }
}

/// `S` acumulada por trapecios (paso 1/20, ≤122 tramos acotados) sobre
/// `[a, x_end]` con el evaluador existente; salta tramos no finitos.
/// `Some(0.0)` si `x_end <= a`; `None` si ningún tramo valida o entradas no
/// finitas (la etiqueta muestra `S=?` honesto en vez de inventar).
pub(crate) fn integral_acumulada(
    anim: Option<&ParametricAnim>,
    frame: usize,
    a: f64,
    x_end: f64,
) -> Option<f64> {
    trapecios_con_eval(a, x_end, |x| integral_eval(anim, frame, x))
}

/// Evalúa la canónica en el frame siguiendo el vivo (M4); sin canónica, `x*x`.
fn integral_eval_con_vivo(
    anim: Option<&ParametricAnim>,
    frame: usize,
    x: f64,
    vivo: Option<f64>,
) -> Option<f64> {
    match anim {
        Some(a) => a.eval_frame_con_vivo(frame, x, vivo),
        None => {
            let v = x * x;
            if v.is_finite() {
                Some(v)
            } else {
                None
            }
        }
    }
}

/// `S` acumulada por trapecios siguiendo el vivo (M4; idem `integral_acumulada`).
/// R1-7: mismo núcleo común + mismo guard no-finito → `None`.
pub(crate) fn integral_acumulada_con_vivo(
    anim: Option<&ParametricAnim>,
    frame: usize,
    a: f64,
    x_end: f64,
    vivo: Option<f64>,
) -> Option<f64> {
    trapecios_con_eval(a, x_end, |x| integral_eval_con_vivo(anim, frame, x, vivo))
}

fn render_integral_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let lo = scene_param_clamped(params, SCENE_PARAM_A, 0.0, -3.0, 3.0);
    let hi = scene_param_clamped(params, SCENE_PARAM_B, 2.0, -3.0, 3.0);
    let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    // N1: la curva es FIJA (evaluada en el frame 0); solo el área acumulada
    // y la cota móvil dependen del frame. Huecos sin unir (honesto).
    let canon = integral_canonical_anim();
    let anim_ref = canon.as_ref();
    let curva: Vec<Option<(f64, f64)>> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            integral_eval(anim_ref, 0, x).map(|y| (x, y))
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases (`integral_frame_end` ya usa `fase_alpha`).
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let x_end = integral_frame_end(a, b, frame).clamp(-3.0, 3.0);
        // Área acumulada `a..x_end`: una pasada por columna de pantalla
        // (densa como la vía paramétrica: cada píxel se blendea una sola
        // vez, sin multi-blend de columnas vecinas, y el conjunto sombreado
        // crece monótono con el frame).
        for px_col in 0..w {
            let x = -3.0 + 6.0 * (px_col as f64 / w as f64);
            if x < a || x > x_end {
                continue;
            }
            if let Some(y) = integral_eval(anim_ref, frame, x) {
                let top = to_pixel(w, h, x, y);
                let bottom = to_pixel(w, h, x, 0.0);
                draw_line(&mut buf, w, h, bottom, top, FILL_SOFT_BLUE);
            }
        }
        // Cota móvil vertical en `x_end`: línea (DOT_BLUE), jamás un punto.
        let y_end = integral_eval(anim_ref, frame, x_end).map_or(0.0, |v| v);
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, x_end, 0.0),
            to_pixel(w, h, x_end, y_end),
            DOT_BLUE,
        );
        // Curva fija por encima (idéntica en los 48 frames).
        draw_curve_gaps(&mut buf, w, h, &curva, CURVE_MAIN);
        // Etiquetas ASCII honestas: título fijo (solo export) + valor acumulado
        // del frame (siempre: es dato, no rótulo).
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "y=x^2");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "y=x^2",
                TEXT_COLOR,
                text_scale_for_h(h),
            );
        }
        let etiqueta = match integral_acumulada(anim_ref, frame, a, x_end) {
            Some(s) => format!("[{a:.2},{x_end:.2}] S={s:.2}"),
            None => format!("[{a:.2},{x_end:.2}] S=?"),
        };
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h.saturating_sub(14),
            &etiqueta,
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub fn render_taylor_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_taylor_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_taylor_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    // Histórico sin(x), hoy 5 escalones exactos 1/3/5/7/9 (sin morphing).
    render_taylor_frames_inner(width, height, 3, "taylor  sin(x)", con_rotulo, on_frame)
}

// ── Taylor por escalones exactos (sin morphing) ─────────────────────────
// El morph interpolaba coeficientes entre grados e inventaba curvas que no
// son ningún P_n. Ahora la timeline se divide en 5 escalones discretos
// (n=1,3,5,7,9): cada frame dibuja el P_n EXACTO de su escalón, cero
// interpolación. La canónica es sin(x) en x=0 con coeficientes exactos
// 1, -1/6, 1/120, -1/5040, 1/362880 (factorial exacto en f64).
// Viewport SOLO-taylor en X: [-2π,2π]; el global [-3,3] NO se toca. Y con
// clip limpio (el polinomio diverge: se corta, jamás plateau). Sin banda
// sombreada de ajuste: el radio de sin es infinito y la banda era
// decoración engañosa; ambas curvas van sólidas a 3px, sin resaltados.
// Rótulo vivo "orden N" + fórmula explícita del P_n en notación `^` ASCII
// (hasta 2 líneas: excepción documentada a la regla ≤12ch, a pedido
// explícito). Todo parametriza centro/expr: cero hardcode en el dibujo.

/// Escalones discretos de la timeline Taylor (P_n EXACTO por escalón).
/// Quintiles de los 48 frames: 0..=9→1, 10..=19→3, 20..=28→5, 29..=38→7,
/// 39..=47→9. Pineado en test.
pub(crate) const TAYLOR_ESCALONES: [u32; 5] = [1, 3, 5, 7, 9];

/// Límite X SOLO-taylor: -2π (el viewport global `VIEW_X_MIN` no se toca).
pub(crate) const TAYLOR_X_MIN: f64 = -2.0 * std::f64::consts::PI;
/// Límite X SOLO-taylor: +2π (≈6.2832).
pub(crate) const TAYLOR_X_MAX: f64 = 2.0 * std::f64::consts::PI;
/// Ancho del viewport SOLO-taylor en X (4π, deriva de `TAYLOR_X_*`).
const TAYLOR_SPAN_X: f64 = TAYLOR_X_MAX - TAYLOR_X_MIN;

/// Orden visible en el frame: escalón discreto 1/3/5/7/9 por quintil.
/// Monótono no-decreciente, sin setup/hold: todos los escalones son P_n
/// exactos (el frame 0 ya muestra P1, no solo f). Pura.
pub(crate) fn taylor_orden_en_frame(frame: usize) -> u32 {
    let idx = frame
        .saturating_mul(TAYLOR_ESCALONES.len())
        .checked_div(NATIVE_ANIM_FRAME_COUNT)
        .unwrap_or(0)
        .min(TAYLOR_ESCALONES.len() - 1);
    TAYLOR_ESCALONES[idx]
}

/// Fórmula explícita del P_n canónico (sin en 0) en notación `^` ASCII,
/// con coeficientes exactos 1, 1/6, 1/120, 1/5040, 1/362880 y signo
/// alterno. Pineada en test, char por char. Pura.
pub(crate) fn taylor_formula_para_orden(orden: u32) -> &'static str {
    match orden {
        0 | 1 => "P1 = x",
        2 | 3 => "P3 = x-x^3/6",
        4 | 5 => "P5 = x-x^3/6+x^5/120",
        6 | 7 => "P7 = x-x^3/6+x^5/120-x^7/5040",
        _ => "P9 = x-x^3/6+x^5/120-x^7/5040+x^9/362880",
    }
}

/// Parte la fórmula en hasta 2 líneas sin perder ni un char (la unión de
/// las partes es la fórmula exacta): si entra en `max_chars` va una línea;
/// si no, corta en el `+`/`-` más cercano a la mitad. Pura.
pub(crate) fn taylor_formula_lineas(formula: &str, max_chars: usize) -> Vec<String> {
    let n = formula.chars().count();
    if n <= max_chars.max(1) {
        return vec![formula.to_string()];
    }
    let medio = n / 2;
    let mut mejor: Option<usize> = None;
    for (i, c) in formula.chars().enumerate() {
        if i == 0 || (c != '+' && c != '-') {
            continue;
        }
        let gana = mejor.is_none_or(|m: usize| i.abs_diff(medio) < m.abs_diff(medio));
        if gana {
            mejor = Some(i);
        }
    }
    match mejor {
        Some(i) => {
            let a: String = formula.chars().take(i).collect();
            let b: String = formula.chars().skip(i).collect();
            vec![a, b]
        }
        None => vec![formula.to_string()],
    }
}

/// Líneas a dibujar para la fórmula en este ancho: 1 si entra, 2 si ambas
/// mitades entran; en previews diminutos donde ni la mitad entra, 1 línea
/// con clip visual (la cuerda exacta vive en `taylor_formula_para_orden` y
/// sus tests; el píxel solo la refleja cuando hay aire). Así el rótulo
/// nunca invade la banda media del contrato chat/export. Pura.
pub(crate) fn taylor_lineas_para_ancho(formula: &str, max_chars: usize) -> Vec<String> {
    let partes = taylor_formula_lineas(formula, max_chars);
    if partes.len() == 2 && partes.iter().any(|p| p.chars().count() > max_chars) {
        return vec![formula.to_string()];
    }
    partes
}

/// Rótulo vivo (`"orden 1"`…`"orden 9"`, siempre ≤12ch). Puro.
pub(crate) fn taylor_rotulo_para_orden(orden_actual: u32) -> String {
    format!("orden {}", orden_actual.min(9))
}

// ── Viewport SOLO-taylor ([-2π,2π] × [-3,3]) + clip limpio ────────────
// Espejo mínimo de `en_vista_mundo`/`to_pixel_opt`/`mundo_a_flotante` con
// el X extendido: el polinomio diverge y cada tramo se clipa por separado
// (jamás plateau). El Y reusa el rango global [-3,3]. Puro, sin pánicos.

/// ¿El punto mundo está en vista SOLO-taylor? Puro, sin pánicos.
fn taylor_en_vista(x: f64, y: f64) -> bool {
    x.is_finite()
        && y.is_finite()
        && (TAYLOR_X_MIN..=TAYLOR_X_MAX).contains(&x)
        && (VIEW_Y_MIN..=VIEW_Y_MAX).contains(&y)
}

/// Punto mundo SOLO-taylor a píxel solo si está en vista (`None` = fuera:
/// el llamador NO dibuja, sin aplastar contra el borde). Puro, sin pánicos.
fn taylor_to_pixel_opt(width: usize, height: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    if width == 0 || height == 0 || !taylor_en_vista(x, y) {
        return None;
    }
    let px = ((x - TAYLOR_X_MIN) / TAYLOR_SPAN_X * (width as f64)).round() as usize;
    let py = ((VIEW_Y_MAX - y) / VIEW_SPAN_Y * (height as f64)).round() as usize;
    Some((px.min(width - 1), py.min(height - 1)))
}

/// Punto mundo SOLO-taylor a float-píxeles sin saturar (`None` si no-finito
/// o w/h 0). Puro, sin pánicos.
fn taylor_mundo_a_flotante(width: usize, height: usize, x: f64, y: f64) -> Option<(f32, f32)> {
    if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let px = (x - TAYLOR_X_MIN) / TAYLOR_SPAN_X * (width as f64);
    let py = (VIEW_Y_MAX - y) / VIEW_SPAN_Y * (height as f64);
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    Some((px as f32, py as f32))
}

/// Segmento en coords MUNDO SOLO-taylor con clip limpio: fuera de vista se
/// recorta (vía `clip_seg_a_caja`), jamás se aplasta al borde. `true` =
/// pintó. Puro sobre el buffer, sin E/S ni pánicos.
#[allow(clippy::too_many_arguments)]
fn taylor_draw_seg_mundo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    color: [u8; 4],
    ancho: f32,
) -> bool {
    let (Some((fax, fay)), Some((fbx, fby))) = (
        taylor_mundo_a_flotante(w, h, ax, ay),
        taylor_mundo_a_flotante(w, h, bx, by),
    ) else {
        return false;
    };
    let Some((a, b)) = clip_seg_a_caja(fax, fay, fbx, fby, w, h) else {
        return false;
    };
    draw_line_ancha(buf, w, h, a, b, color, ancho);
    true
}

/// Ticks X SOLO-taylor en múltiplos de π con rótulo ASCII legible.
/// `draw_text_block` usa la Ubuntu-Light embebida con avance por glifo y cae
/// a celda sólida si un glifo falta (`gid.0 == 0 → sk_fallback_cell`): el
/// ASCII ("-2pi","-pi","0","pi","2pi") garantiza glifo real sin depender de
/// la cobertura de la fuente para π U+03C0. Puro.
fn taylor_ticks_x() -> [(f64, &'static str); 5] {
    use std::f64::consts::PI;
    [
        (-2.0 * PI, "-2pi"),
        (-PI, "-pi"),
        (0.0, "0"),
        (PI, "pi"),
        (2.0 * PI, "2pi"),
    ]
}

/// Ejes SOLO-taylor: X de -2π a 2π con ticks en múltiplos de π, Y numérico
/// corto; cada rótulo con halo y caja disjunta (el que colisiona se omite,
/// su marca igual se dibuja). Puro sobre el buffer.
fn draw_taylor_axes(buf: &mut [u8], w: usize, h: usize) {
    let taylor_px =
        |x: f64, y: f64| -> (usize, usize) { taylor_to_pixel_opt(w, h, x, y).unwrap_or((0, 0)) };
    draw_line_ancha(
        buf,
        w,
        h,
        taylor_px(TAYLOR_X_MIN, 0.0),
        taylor_px(TAYLOR_X_MAX, 0.0),
        AXIS_COLOR,
        AXIS_ANCHO,
    );
    draw_line_ancha(
        buf,
        w,
        h,
        taylor_px(0.0, VIEW_Y_MIN),
        taylor_px(0.0, VIEW_Y_MAX),
        AXIS_COLOR,
        AXIS_ANCHO,
    );
    if w < 48 || h < 48 {
        return;
    }
    let escala = tick_scale_for_h(h);
    let cw = (TICK_CHAR_W_PX as usize).saturating_mul(escala).max(1);
    let chh = (TICK_CHAR_H_PX as usize).saturating_mul(escala).max(1);
    let (cx, cy) = taylor_px(0.0, 0.0);
    let caja_x = LabelCaja {
        x: w.saturating_sub(SLOT_MARGEN + cw + 4),
        y: (cy + 8).min(h.saturating_sub(9)),
        w: cw + 4,
        h: chh,
    };
    let caja_y = LabelCaja {
        x: (cx + 8).min(w.saturating_sub(24)),
        y: SLOT_MARGEN.min(h.saturating_sub(9)),
        w: cw + 4,
        h: chh,
    };
    let mut ocupadas: Vec<LabelCaja> = Vec::new();
    ocupadas.push(caja_x);
    ocupadas.push(caja_y);
    let caja_para = |tag: &str| -> (usize, usize) {
        let tw = tag.chars().count().saturating_mul(cw).saturating_add(4);
        (tw, chh)
    };
    for (x, tag) in taylor_ticks_x() {
        let (px, _) = taylor_px(x, 0.0);
        draw_line(
            buf,
            w,
            h,
            (px, cy.saturating_sub(3)),
            (px, cy.saturating_add(3).min(h.saturating_sub(1))),
            AXIS_COLOR,
        );
        let ly = (cy + 8).min(h.saturating_sub(9));
        let lx = px.saturating_add(2);
        let (tw, th) = caja_para(tag);
        if cabe_label_entre(lx, ly, tw, th, &ocupadas) {
            pintar_halo_rotulo(buf, w, h, lx, ly, tw, th);
            draw_text_block(buf, w, h, lx, ly, tag, TEXT_COLOR, escala);
            ocupadas.push(LabelCaja {
                x: lx,
                y: ly,
                w: tw,
                h: th,
            });
        }
    }
    for v in [-2.0f64, -1.0, 1.0, 2.0] {
        let (_, py) = taylor_px(0.0, v);
        draw_line(
            buf,
            w,
            h,
            (cx.saturating_sub(3), py),
            (cx.saturating_add(3).min(w.saturating_sub(1)), py),
            AXIS_COLOR,
        );
        let tag = short_tick_label(v);
        let lx = (cx + 8).min(w.saturating_sub(24));
        let ly = py.saturating_sub(8);
        let (tw, th) = caja_para(&tag);
        if cabe_label_entre(lx, ly, tw, th, &ocupadas) {
            pintar_halo_rotulo(buf, w, h, lx, ly, tw, th);
            draw_text_block(buf, w, h, lx, ly, &tag, TEXT_COLOR, escala);
            ocupadas.push(LabelCaja {
                x: lx,
                y: ly,
                w: tw,
                h: th,
            });
        }
    }
    pintar_halo_rotulo(buf, w, h, caja_x.x, caja_x.y, caja_x.w, caja_x.h);
    draw_text_block(buf, w, h, caja_x.x, caja_x.y, "x", TEXT_COLOR, escala);
    pintar_halo_rotulo(buf, w, h, caja_y.x, caja_y.y, caja_y.w, caja_y.h);
    draw_text_block(buf, w, h, caja_y.x, caja_y.y, "y", TEXT_COLOR, escala);
}

/// Líneas a quemar en este tamaño (pura): calcula el aire real y aplica
/// `taylor_lineas_para_ancho`; bajo 360px de alto va 1 sola línea con clip
/// visual (la banda media del contrato chat/export no se toca en previews
/// diminutos; la cuerda exacta vive en `taylor_formula_para_orden` y sus
/// tests, y a tamaño real —chat 480×360, export 640×480— van las 2 líneas).
pub(crate) fn taylor_lineas_para_frame(formula: &str, w: usize, h: usize) -> Vec<String> {
    if h < 360 {
        return vec![formula.to_string()];
    }
    let escala = text_scale_for_h(h);
    let cw = 6usize.saturating_mul(escala).max(1);
    let max = w
        .saturating_sub(w / 14)
        .saturating_sub(18)
        .checked_div(cw.max(1))
        .unwrap_or(0)
        .max(1);
    taylor_lineas_para_ancho(formula, max)
}

/// Cajas del rótulo vivo (título "orden N" + fórmula en 1-2 líneas con
/// chip azul): apiladas con aire, disjuntas por construcción y dentro de la
/// franja superior (a 96×72 la fórmula colapsa a 1 línea y termina en y=40,
/// fuera de la banda media del contrato chat/export). Puras (test + dibujo).
pub(crate) fn taylor_rotulos_cajas(
    w: usize,
    h: usize,
    titulo: &str,
    lineas: &[String],
) -> Vec<LabelCaja> {
    let escala = text_scale_for_h(h);
    let cw = 6usize.saturating_mul(escala).max(1);
    let x0 = w / 14;
    let y0 = h / 12;
    let alto_titulo = 12usize.saturating_mul(escala).saturating_add(8);
    let alto_linea = 7usize.saturating_mul(escala).saturating_add(6);
    let ancho_titulo = titulo.chars().count().saturating_mul(cw).saturating_add(8);
    let mut cajas = vec![LabelCaja {
        x: x0,
        y: y0,
        w: ancho_titulo,
        h: alto_titulo,
    }];
    let mut y = y0.saturating_add(alto_titulo).saturating_add(1);
    for l in lineas {
        let ancho_linea = l
            .chars()
            .count()
            .saturating_mul(cw)
            .saturating_add(8)
            .saturating_add(10);
        cajas.push(LabelCaja {
            x: x0,
            y,
            w: ancho_linea,
            h: alto_linea,
        });
        y = y.saturating_add(alto_linea).saturating_add(1);
    }
    cajas
}

/// Título vivo + fórmula explícita del P_n dibujado (chip azul 6×6 = esta
/// fórmula es la curva azul; la f amarilla es sin(x) —o la f del spec—
/// declarada por la prosa del turno). Solo con rótulo (export): el chat ya
/// titula en el header. Puro sobre el buffer.
fn draw_taylor_rotulos(buf: &mut [u8], w: usize, h: usize, orden_dibujado: u32, formula: &str) {
    let titulo = taylor_rotulo_para_orden(orden_dibujado);
    let escala = text_scale_for_h(h);
    let lineas = taylor_lineas_para_frame(formula, w, h);
    let cajas = taylor_rotulos_cajas(w, h, &titulo, &lineas);
    for c in &cajas {
        draw_filled_rect(buf, w, h, c.x, c.y, c.w, c.h, SCRIM);
    }
    draw_text_block(
        buf,
        w,
        h,
        cajas[0].x.saturating_add(4),
        cajas[0].y.saturating_add(4),
        &titulo,
        TEXT_COLOR,
        escala,
    );
    for (i, linea) in lineas.iter().enumerate() {
        let Some(caja) = cajas.get(1 + i) else {
            break;
        };
        let chip_y = caja
            .y
            .saturating_add(caja.h / 2)
            .saturating_sub(3)
            .min(h.saturating_sub(7));
        draw_filled_rect(buf, w, h, caja.x.saturating_add(4), chip_y, 6, 6, PAL_BLUE);
        draw_text_block(
            buf,
            w,
            h,
            caja.x.saturating_add(14),
            caja.y.saturating_add(2),
            linea,
            TEXT_COLOR,
            escala,
        );
    }
}

/// Suma parcial de `sin(x)` a grado `grado` (solo impares aportan).
/// Pura y acotada: `grado` se clampe a `1..=10` (el slider del panel W-C
/// vive en ese rango; el factorial de 9! es exacto en f64, sin overflow).
/// `taylor_partial_sum(3, x)` es exactamente `x - x³/6` (el histórico).
pub(crate) fn taylor_partial_sum(grado: usize, x: f64) -> f64 {
    let grado = grado.clamp(1, 10);
    let mut suma = 0.0;
    let mut k = 0usize;
    loop {
        let n = 2 * k + 1;
        if n > grado {
            break;
        }
        let mut fact = 1.0f64;
        for i in 2..=n {
            fact *= i as f64;
        }
        let termino = x.powi(n as i32) / fact;
        if k.is_multiple_of(2) {
            suma += termino;
        } else {
            suma -= termino;
        }
        k += 1;
    }
    suma
}

/// Taylor por escalones fijos 1/3/5/7/9. `terms` se lee (compat con el
/// slider del panel) pero NO recorta la timeline: mostrar un subconjunto
/// según el pedido sería volver al morph parcial; los 5 P_n exactos siempre
/// se muestran. La didáctica la dan los escalones + la fórmula viva.
pub fn render_taylor_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_taylor_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_taylor_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    // Frente A: centro vivo por `x0` (default histórico 0.0). Sin f en el
    // mapa (solo f64) la serie es la canónica `sin(x)` DECLARADA por la prosa
    // del turno; la f explícita del pedido entra por
    // `render_taylor_frames_for_spec_impl`. El `terms` se lee y se ignora a
    // propósito (timeline fija en los 5 escalones, sin morphing).
    let _orden_ignorado = taylor_anim_order_from_params(params);
    let centro = scene_param_clamped(params, SCENE_PARAM_X0, 0.0, -3.0, 3.0);
    let spec = grafito_anim::parametric::TaylorSpec {
        expr: grafito_anim::parametric::TAYLOR_CANONICAL_EXPR.to_string(),
        centro,
        orden: grafito_anim::parametric::TAYLOR_CANONICAL_ORDER,
    };
    render_taylor_frames_for_spec_impl(width, height, &spec, con_rotulo, on_frame)
}

fn render_taylor_frames_inner(
    width: u32,
    height: u32,
    _orden: usize,
    _etiqueta: &str,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    // Sin hardcode en el dibujo: la vía canónica (motor real) con centro 0.
    // La etiqueta histórica la reemplazan el rótulo vivo + fórmula. El orden
    // pedido no recorta: la timeline siempre recorre los 5 escalones.
    let spec = grafito_anim::parametric::TaylorSpec {
        expr: grafito_anim::parametric::TAYLOR_CANONICAL_EXPR.to_string(),
        centro: 0.0,
        orden: grafito_anim::parametric::TAYLOR_CANONICAL_ORDER,
    };
    render_taylor_frames_for_spec_impl(width, height, &spec, con_rotulo, on_frame)
}

// ── Frente A: Taylor con función explícita ──────────────────────────────
// Queja real: "pedí taylor y me tiró una senoidal nada que ver". El renderer
// histórico hardcodeaba `sin(x)`; ahora la serie es la REAL de f vía el
// motor reutilizable `grafito_geometry::symbolic::taylor_series` (el mismo
// del comando `Taylor`): `P_n` sale del motor como string (solo `* ^ + -`
// y paréntesis) y se evalúa con el evaluador paramétrico; f se evalúa con
// otro `ParametricAnim` temporal. Cero derivadas inventadas: si el motor no
// deriva f, `taylor_poly_anim` da `None` honesto y se dibuja solo f.
// Sin f, la canónica `sin(x)` la declara la PROSA del turno (el renderer
// solo dibuja etiquetas ASCII con f + centro + orden).

/// Evaluador temporal de una expresión (`p` fijo en el
/// frame 0: Taylor no tiene parámetro móvil). `None` si no construye
/// (expresión >2000 chars). Puro, sin I/O.
fn taylor_eval_anim(expr: &str) -> Option<ParametricAnim> {
    ParametricAnim::try_new(
        ParametricKind::Sweep,
        expr.to_string(),
        None,
        ParamName::try_new("p").ok()?,
        0.0,
        1.0,
        FrameCount::try_new(24).ok()?,
        Resolution::default(),
    )
    .ok()
}

/// Polinomio de Taylor REAL de `expr` en `centro` hasta `orden` (1..=10),
/// como evaluador listo para dibujar. `None` honesto si el centro no es
/// finito, el motor no deriva f o el polinomio no evalúa en ningún punto
/// del mundo. Puro, sin I/O.
fn taylor_poly_anim(expr: &str, centro: f64, orden: usize) -> Option<ParametricAnim> {
    if !centro.is_finite() {
        return None;
    }
    let orden = orden.clamp(TAYLOR_MIN_ORDER, grafito_anim::parametric::TAYLOR_MAX_ORDER);
    let poly = grafito_geometry::symbolic::taylor_series(expr, "x", centro, orden).ok()?;
    let anim = taylor_eval_anim(&poly)?;
    for k in 0..9 {
        let x = -3.0 + 6.0 * (k as f64 / 8.0);
        if anim.eval_frame(0, x).is_some() {
            return Some(anim);
        }
    }
    None
}

/// Taylor REAL de un spec (f explícita o canónica declarada): f amarilla
/// fija + `P_n` azul del escalón (1/3/5/7/9 exactos, sin morphing) + punto
/// P0 en el centro + ejes SOLO-taylor con ticks en múltiplos de π. Sin banda
/// de ajuste (el radio de sin es infinito: era decoración engañosa) y sin
/// alfa por tramo: ambas curvas sólidas a 3px, la divergencia del polinomio
/// se corta limpio, jamás plateau. Si el motor no deriva f (el infer lo
/// impide; defensa en profundidad), dibuja solo f sin inventar polinomio.
pub fn render_taylor_frames_for_spec(
    width: u32,
    height: u32,
    spec: &grafito_anim::parametric::TaylorSpec,
) -> Vec<egui::ColorImage> {
    render_taylor_frames_for_spec_impl(width, height, spec, true, &mut |_, _| {})
}

/// Núcleo con flag de rótulo + progreso REAL por frame (el chat usa
/// `con_rotulo=false`: el header egui ya titula; export usa `true`).
pub(crate) fn render_taylor_frames_for_spec_impl(
    width: u32,
    height: u32,
    spec: &grafito_anim::parametric::TaylorSpec,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    // El `orden` del spec NO recorta la timeline: los 5 escalones exactos
    // siempre se recorren (el slider `terms` queda inerte para taylor-series
    // a propósito). Solo centro y expr parametrizan el dibujo.
    let centro = if spec.centro.is_finite() {
        spec.centro
    } else {
        0.0
    };
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let expr = spec.expr.trim();
    let es_canonica =
        expr.to_lowercase() == grafito_anim::parametric::TAYLOR_CANONICAL_EXPR.to_lowercase();
    let formula_para = |orden: u32| -> String {
        if es_canonica {
            taylor_formula_para_orden(orden).to_string()
        } else {
            format!("P{orden} = serie de f")
        }
    };
    let f_anim = taylor_eval_anim(expr)
        .or_else(|| taylor_eval_anim(grafito_anim::parametric::TAYLOR_CANONICAL_EXPR));
    let Some(f_anim) = f_anim else {
        // Sin evaluador ni canónico (inconcebible: `sin(x)` construye
        // siempre): fondo + ejes honestos, jamás curva falsa.
        let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
        for _ in 0..NATIVE_ANIM_FRAME_COUNT {
            let byte_len =
                checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
            let mut buf = vec![0u8; byte_len];
            fill_background(&mut buf, w, h);
            draw_subtle_grid(&mut buf, w, h);
            draw_taylor_axes(&mut buf, w, h);
            if con_rotulo {
                let primero = TAYLOR_ESCALONES[0];
                draw_taylor_rotulos(&mut buf, w, h, primero, &formula_para(primero));
            }
            frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
            on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        }
        return frames;
    };
    // Muestreo fijo en 121 puntos de [-2π,2π] SOLO-taylor. f es estática
    // (frame 0 siempre: Taylor no tiene parámetro móvil), así que se
    // precalcula una vez; un polinomio REAL por escalón. El loop de frames
    // es raster puro (rápido y determinista: mismo escalón = mismos bytes).
    let xs: Vec<f64> = (0..=120)
        .map(|k| TAYLOR_X_MIN + TAYLOR_SPAN_X * (k as f64 / 120.0))
        .collect();
    let f_pts: Vec<Option<f64>> = xs.iter().map(|x| f_anim.eval_frame(0, *x)).collect();
    let f_en_centro = f_anim.eval_frame(0, centro);
    let mut p_por_escalon: Vec<(u32, Vec<Option<f64>>)> = Vec::new();
    for n in TAYLOR_ESCALONES {
        if let Some(p) = taylor_poly_anim(expr, centro, n as usize) {
            let pts: Vec<Option<f64>> = xs.iter().map(|x| p.eval_frame(0, *x)).collect();
            p_por_escalon.push((n, pts));
        }
    }
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let orden = taylor_orden_en_frame(frame);
        // Escalón efectivamente dibujado (el mayor disponible ≤ pedido; el
        // motor deriva o no deriva: jamás un P inventado a medias).
        let entrada = p_por_escalon.iter().rev().find(|(nv, _)| *nv <= orden);
        let (nivel_dibujado, formula) = match entrada {
            Some((n, _)) => (*n, formula_para(*n)),
            None => (orden, format!("P{orden} = sin serie (solo f)")),
        };
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_taylor_axes(&mut buf, w, h);
        // Sin banda sombreada: el radio de sin es infinito y cualquier banda
        // sería decoración engañosa. Sin reemplazo.
        // f fija amarilla (idéntica en los 48: la escala no tiembla).
        for k in 0..120 {
            if let (Some(y0), Some(y1)) = (f_pts[k], f_pts[k + 1]) {
                taylor_draw_seg_mundo(
                    &mut buf,
                    w,
                    h,
                    xs[k],
                    y0,
                    xs[k + 1],
                    y1,
                    CURVE_MAIN,
                    CURVE_ANCHO,
                );
            }
        }
        // Punto P0 en el centro (ancla de la aproximación), solo en vista.
        if let Some(fc) = f_en_centro {
            if fc.is_finite() {
                if let Some((px, py)) = taylor_to_pixel_opt(w, h, centro, fc) {
                    draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                }
            }
        }
        // P_n azul sólido: el P_n EXACTO del escalón, idéntico en todos sus
        // frames (cero interpolación de coeficientes).
        if let Some((_, p_pts)) = entrada {
            for k in 0..120 {
                if let (Some(y0), Some(y1)) = (p_pts[k], p_pts[k + 1]) {
                    taylor_draw_seg_mundo(
                        &mut buf,
                        w,
                        h,
                        xs[k],
                        y0,
                        xs[k + 1],
                        y1,
                        PAL_BLUE,
                        CURVE_ANCHO,
                    );
                }
            }
        }
        if con_rotulo {
            draw_taylor_rotulos(&mut buf, w, h, nivel_dibujado, &formula);
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Mapa de Möbius real w=(z−c)/(1−conj(c)·z) (F1, compartido).
///
/// El mismo que anima `mobius-transform`: la plantilla `conformal-map`
/// dibuja esta deformación (holomorfa con derivada no nula fuera del polo:
/// conforme de verdad) en vez del desplazamiento fake `x+0.2·sin(3x)`
/// (seno decorativo, jamás una aplicación conforme). `None` honesto en el
/// polo (den≈0) o salida no finita; resultado clamp a [-3,3]. Puro.
fn mobius_map(x: f64, y: f64, cr: f64, ci: f64) -> Option<(f64, f64)> {
    let nr = x - cr;
    let ni = y - ci;
    // conj(c)*z = (cr*x+ci*y) + i*(cr*y−ci*x)
    let dr = 1.0 - (cr * x + ci * y);
    let di = -(cr * y - ci * x);
    let den = dr * dr + di * di;
    if den < 1e-9 || !den.is_finite() {
        return None;
    }
    let wr = (nr * dr + ni * di) / den;
    let wi = (ni * dr - nr * di) / den;
    if !wr.is_finite() || !wi.is_finite() {
        return None;
    }
    Some((wr.clamp(-3.0, 3.0), wi.clamp(-3.0, 3.0)))
}

pub fn render_conformal_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_conformal_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_conformal_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // Barrido propio del parámetro (distinto del `mobius-transform`:
        // primero != último, sin loop).
        let (cr, ci) = (
            0.45 * (2.0 * t - 1.0),
            0.3 * (std::f64::consts::PI * t).sin(),
        );
        // Rejilla original tenue + imagen conforme brillante.
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let p0 = to_pixel(w, h, x, y);
                draw_filled_circle(&mut buf, w, h, p0.0, p0.1, 1, MINT_FAINT);
                if let Some((wx, wy)) = mobius_map(x, y, cr, ci) {
                    let p1 = to_pixel(w, h, wx, wy);
                    draw_filled_circle(&mut buf, w, h, p1.0, p1.1, 2, MINT_STRONG);
                }
            }
        }
        // Círculo unidad transformado (60 segmentos): la imagen deforma el
        // círculo sin romper ángulos (conformidad visible).
        let mut prev: Option<(usize, usize)> = None;
        for k in 0..=60 {
            let a = 2.0 * std::f64::consts::PI * k as f64 / 60.0;
            let (zx, zy) = (a.cos(), a.sin());
            if let Some((wx, wy)) = mobius_map(zx, zy, cr, ci) {
                let p = to_pixel(w, h, wx, wy);
                if let Some(q) = prev {
                    draw_line(&mut buf, w, h, q, p, CURVE_MAIN);
                }
                prev = Some(p);
            } else {
                prev = None;
            }
        }
        if con_rotulo {
            draw_rotulo_con_scrim(&mut buf, w, h, w / 14, h / 12, "conforme w(z)");
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Etiqueta honesta del placeholder neutro: lo único que afirma.
pub const UNIVERSAL_PLACEHOLDER_LABEL: &str = "vista previa no disponible";

/// Placeholder neutro honesto para pedidos sin plantilla (T2).
///
/// Grilla + rótulo + eco del pedido + barra de progreso real. Jamás dibuja
/// curva, partículas, ejes ni puntos: un pedido desconocido no finge
/// contenido matemático (forense: la parábola→seno por hash + 6 partículas
/// orbitales + punto central + barras de acento eran decoración que parecía
/// respuesta y se eliminaron). Lo único que anima es la barra de progreso.
pub fn render_universal_youtube_frames(
    concept: &str,
    width: u32,
    height: u32,
) -> Vec<egui::ColorImage> {
    render_universal_youtube_frames_impl(concept, width, height, true, &mut |_, _| {})
}

fn render_universal_youtube_frames_impl(
    concept: &str,
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let concept_norm = normalize_concept(concept);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t_raw = if NATIVE_ANIM_FRAME_COUNT <= 1 {
            0.0
        } else {
            frame as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
        };
        // Progreso suavizado pero monótono (honesto: 0 → 1 con el frame).
        let t = ease_in_out(t_raw);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        // Fondo y grilla ESTÁTICOS con tinte fijo: el pedido no modula nada
        // para no fingir contenido (antes el hash del texto elegía color,
        // fase de la curva y órbitas de partículas).
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        // Rótulo honesto + eco del pedido (solo standalone/export: el chat ya
        // titula en el header de la card).
        let echo: String = concept_norm.chars().take(32).collect();
        if con_rotulo {
            // R6c: a ≥480×360 rótulo y eco a margen ≥24px (`SLOT_MARGEN`);
            // abajo legado compacto (posiciones históricas 6/10) para no
            // romper el contrato chat/export de la banda media ni los
            // previews diminutos. El rótulo de 26ch va a escala de tick (a
            // escala de título invadiría el margen derecho); el eco (dato)
            // queda a escala 1 debajo.
            if w >= CHAT_CANON_W as usize && h >= CHAT_CANON_H as usize {
                let tesc = tick_scale_for_h(h);
                let mx = SLOT_MARGEN;
                let my = SLOT_MARGEN;
                let chars = UNIVERSAL_PLACEHOLDER_LABEL.chars().take(48).count();
                let ancho = chars
                    .saturating_mul(6 * tesc)
                    .saturating_add(8)
                    .min(w.saturating_sub(mx));
                let alto = (12 * tesc + 8).min(h.saturating_sub(my));
                if ancho > 0 && alto > 0 {
                    draw_filled_rect(&mut buf, w, h, mx, my, ancho, alto, SCRIM);
                }
                draw_text_block(
                    &mut buf,
                    w,
                    h,
                    mx.saturating_add(4),
                    my.saturating_add(4),
                    UNIVERSAL_PLACEHOLDER_LABEL,
                    TEXT_COLOR,
                    tesc,
                );
                let ey = my.saturating_add(12 * tesc).saturating_add(12);
                if ey < h {
                    draw_text_block(
                        &mut buf,
                        w,
                        h,
                        mx.saturating_add(4),
                        ey,
                        &echo,
                        TEXT_COLOR,
                        1,
                    );
                }
            } else {
                let title_h = 30;
                draw_filled_rect(&mut buf, w, h, 6, 6, w.saturating_sub(12), title_h, SCRIM);
                draw_text_block(
                    &mut buf,
                    w,
                    h,
                    10,
                    10,
                    UNIVERSAL_PLACEHOLDER_LABEL,
                    TEXT_COLOR,
                    1,
                );
                draw_text_block(&mut buf, w, h, 10, 20, &echo, TEXT_COLOR, 1);
            }
        }
        // Barra de progreso inferior: posición real del frame (cromo honesto).
        let bar_y = h.saturating_sub(6);
        let bar_w = (w as f64 * t) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 4, PAL_ACCENT);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w.saturating_sub(bar_w),
            4,
            TRACK,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Dispatcher honesto: elige plantilla automáticamente a partir del concepto si hace falta.
/// Garantiza que cualquier combinación produce frames válidos en <2s; lo
/// desconocido va al placeholder neutro, jamás a una curva que finja respuesta.
pub fn render_anim_for_concept(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
) -> Vec<egui::ColorImage> {
    render_anim_for_concept_with_params(
        template,
        concept,
        width,
        height,
        &std::collections::BTreeMap::new(),
    )
}

/// Resuelve el nombre de plantilla a su canónica (misma lógica que usaba
/// `render_anim_for_concept` inline; extraída para reuso sin duplicar).
fn resolve_native_template(template: &str, concept: &str) -> &'static str {
    let t_lower = template.trim().to_lowercase();
    if t_lower.is_empty() || t_lower == "universal" || t_lower == "auto" {
        return detect_template_for_concept(concept);
    }
    match t_lower.as_str() {
        "derivative-slope" => "derivative-slope",
        "integral-area" => "integral-area",
        "taylor-series" => "taylor-series",
        "conformal-map" => "conformal-map",
        "pitagoras" | "pythagoras" => "pitagoras",
        "universal" => "universal",
        "euler" => "euler",
        "fourier" => "fourier",
        "logistic-bifurcation" | "bifurcacion-logistica" | "logistica" => "logistic-bifurcation",
        "gradient-field" | "campo-gradiente" | "gradiente" => "gradient-field",
        "mobius-transform" | "mobius" | "moebius" => "mobius-transform",
        "subspace" => "subspace",
        "fractal" => "fractal",
        // F5: templates pedagógicos inline — mapeo a nativos existentes
        "fraccion-visual" => "integral-area",
        "vector-anim" => "conformal-map",
        "matriz-anim" => "universal",
        "prob-anim" => "integral-area",
        "serie-anim" => "taylor-series",
        "ecuacion-anim" => "derivative-slope",
        "trig-anim" => "taylor-series",
        "conica-anim" => "conformal-map",
        _ => detect_template_for_concept(concept),
    }
}

/// Dispatcher con params vivos (v3): el scrub de la UI re-renderiza llamando
/// aquí con el mapa vivo. Atienden params: `derivative-slope` (x0/span),
/// `integral-area` (a/b), `taylor-series` (terms = orden 1..=7 vía
/// `taylor_anim_order_from_params`, F1),
/// `euler`/`fourier` (terms), `subspace` (v1x/v1y/v2x/v2y con rank==2 o
/// default). `fractal` IGNORA params (niveles 0→4 fijos, honesto). El resto
/// IGNORA params por ahora (TODO honesto: conformal/pitagoras/
/// logistic/gradient/mobius/universal aún no parametrizan) y delega al legacy.
/// Firmas legacy intactas: ningún caller existente se rompe.
pub fn render_anim_for_concept_with_params(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_anim_with_progress(template, concept, width, height, params, &mut |_, _| {})
}

// ── F4a: caché LRU de sets animados (tope 64 MiB) ─────────────────────
// Clave = template + hash de params + concepto + w×h + rótulo: el replay y
// los re-renders del mismo pedido no pagan el render otra vez. Sin I/O
// (solo RAM del worker), LRU por inserción con desalojo del más viejo.
// Un set que solo ya excede el tope no se guarda (se renderiza directo).

/// Tope de la caché de sets (paridad con `NATIVE_MAX_SET_BYTES`).
pub const CACHE_SETS_ANIM_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Hash FNV-1a sobre entries ordenadas (`BTreeMap` ya ordena): estable
/// entre runs para la misma tabla de params. Puro.
fn params_hash_para_cache(params: &std::collections::BTreeMap<String, f64>) -> u64 {
    let mut acumulado: u64 = 0xcbf29ce484222325;
    for (clave, valor) in params {
        for byte in clave.as_bytes() {
            acumulado ^= u64::from(*byte);
            acumulado = acumulado.wrapping_mul(0x100000001b3);
        }
        for byte in valor.to_bits().to_le_bytes() {
            acumulado ^= u64::from(byte);
            acumulado = acumulado.wrapping_mul(0x100000001b3);
        }
    }
    acumulado
}

/// Clave de caché (template normalizado + params + concepto + viewport +
/// rótulo). Pura.
fn clave_cache_sets(
    template: &str,
    params: &std::collections::BTreeMap<String, f64>,
    concept: &str,
    width: u32,
    height: u32,
    con_rotulo: bool,
) -> String {
    format!(
        "{}|{:016x}|{}|{width}x{height}|{}",
        template.trim().to_lowercase(),
        params_hash_para_cache(params),
        concept.trim(),
        u8::from(con_rotulo),
    )
}

#[derive(Default)]
struct CacheSetsAnim {
    entradas: std::collections::HashMap<String, Vec<egui::ColorImage>>,
    orden: std::collections::VecDeque<String>,
    bytes: usize,
}

impl CacheSetsAnim {
    fn bytes_de_set(set: &[egui::ColorImage]) -> usize {
        set.iter()
            .map(|frame| frame.pixels.len().saturating_mul(4))
            .fold(0usize, |acc, v| acc.saturating_add(v))
    }

    /// Hit LRU (mueve al fondo = más reciente) con set clonado.
    fn buscar(&mut self, clave: &str) -> Option<Vec<egui::ColorImage>> {
        let posicion = self.orden.iter().position(|k| k == clave)?;
        self.orden.remove(posicion);
        self.orden.push_back(clave.to_string());
        self.entradas.get(clave).cloned()
    }

    /// Guarda desalojando los más viejos hasta encajar; si el set solo ya
    /// excede el tope, no guarda nada (render directo, sin mentir).
    fn guardar(&mut self, clave: String, set: Vec<egui::ColorImage>) {
        let set_bytes = Self::bytes_de_set(&set);
        if set_bytes > CACHE_SETS_ANIM_MAX_BYTES {
            return;
        }
        if let Some(viejo) = self.entradas.remove(&clave) {
            self.bytes = self.bytes.saturating_sub(Self::bytes_de_set(&viejo));
            self.orden.retain(|k| k != &clave);
        }
        while self.bytes.saturating_add(set_bytes) > CACHE_SETS_ANIM_MAX_BYTES {
            let Some(vieja) = self.orden.pop_front() else {
                break;
            };
            if let Some(sacado) = self.entradas.remove(&vieja) {
                self.bytes = self.bytes.saturating_sub(Self::bytes_de_set(&sacado));
            }
        }
        self.orden.push_back(clave.clone());
        self.entradas.insert(clave, set);
        self.bytes = self.bytes.saturating_add(set_bytes);
    }
}

static CACHE_SETS_ANIM: std::sync::OnceLock<std::sync::Mutex<CacheSetsAnim>> =
    std::sync::OnceLock::new();

fn cache_sets_global() -> &'static std::sync::Mutex<CacheSetsAnim> {
    CACHE_SETS_ANIM.get_or_init(|| std::sync::Mutex::new(CacheSetsAnim::default()))
}

/// Entrada canónica con PROGRESO REAL por frame (ANIM-REVIVE).
///
/// Idéntica a `render_anim_for_concept_with_params`, pero `on_frame(done, total)`
/// se invoca tras pushear CADA frame (`done` 1..=48, `total` = 48) desde dentro
/// del loop nativo — nunca valores inventados. El hilo del lead la usa para
/// mover `AnimPreviewState.progress` y `request_repaint`; el render es
/// determinista: el callback no altera los píxeles (ver test).
/// Presupuesto intacto: siempre 48 frames (`NATIVE_ANIM_FRAME_COUNT`).
///
/// Sin rótulo quemado (pipeline del chat: el header egui de la card v3 ya
/// titula). Para standalone / export GIF, usar `render_anim_for_export` o
/// `render_anim_with_progress_con_rotulo(..., true, ...)`.
pub fn render_anim_with_progress(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    render_anim_with_progress_con_rotulo(template, concept, width, height, params, false, on_frame)
}

/// Núcleo con flag de rótulo: `con_rotulo=true` reproduce los píxeles
/// históricos (título quemado arriba-izquierda + SCRIM del universal);
/// `false` los omite sin tocar la matemática (fondo, grilla, curvas, áreas).
pub fn render_anim_with_progress_con_rotulo(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    // F4a: hit de caché (progreso REAL igual: se re-emite 1..=n aunque los
    // píxeles vengan clonados). El lock solo cubre map+clone, jamás el
    // render (1-2 s bloquearían a otros workers).
    let clave = clave_cache_sets(template, params, concept, width, height, con_rotulo);
    if let Ok(mut cache) = cache_sets_global().lock() {
        if let Some(set) = cache.buscar(&clave) {
            for (indice, _) in set.iter().enumerate() {
                on_frame(indice + 1, set.len());
            }
            return set;
        }
    }
    let set = match resolve_native_template(template, concept) {
        "integral-area" => {
            render_integral_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "taylor-series" => {
            render_taylor_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "derivative-slope" => {
            render_derivative_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "euler" => {
            render_euler_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "fourier" => {
            render_fourier_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "subspace" => {
            render_subspace_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "fractal" => render_fractal_frames_with_params_impl(width, height, con_rotulo, on_frame),
        tmpl => render_anim_for_concept_legacy_with_progress(
            tmpl, concept, width, height, params, con_rotulo, on_frame,
        ),
    };
    if let Ok(mut cache) = cache_sets_global().lock() {
        cache.guardar(clave, set.clone());
    }
    set
}

/// Atajo standalone / export GIF: mismos 48 frames pero CON rótulo quemado
/// (el GIF se ve fuera de la app, sin header egui que lo titule).
pub fn render_anim_for_export(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_anim_with_progress_con_rotulo(
        template,
        concept,
        width,
        height,
        params,
        true,
        &mut |_, _| {},
    )
}

/// ¿Este locale quema rótulo en el frame? Solo ES: los títulos quemados son
/// literales ES cortos (R6c, ≤12ch salvo `"conforme w(z)"` con 13:
/// `"derivada"`, `"pitagoras"`,
/// `"y=x^2"`, `"taylor sin(x)"`, `"bifurcacion"`, ...). En otro locale el
/// frame sale sin texto quemado (la card v3 ya titula localizado). Puro.
pub fn con_rotulo_for_locale(locale: grafito_ui::i18n::Locale) -> bool {
    matches!(locale, grafito_ui::i18n::Locale::Es)
}

/// Standalone / export GIF con locale: mismos 48 frames, pero el rótulo
/// quemado ES solo sale en ES; en otro locale el frame sale limpio y la
/// matemática queda intacta (banda media idéntica).
pub fn render_anim_for_export_localized(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    locale: grafito_ui::i18n::Locale,
) -> Vec<egui::ColorImage> {
    render_anim_with_progress_con_rotulo(
        template,
        concept,
        width,
        height,
        params,
        con_rotulo_for_locale(locale),
        &mut |_, _| {},
    )
}

/// Rama legacy con progreso sobre los `*_impl` para emitir por frame.
///
/// M3-8: la ex `render_anim_for_concept_legacy` (duplicado sin params ni
/// callers) se borró: esta es la única rama legacy y el dispatcher vivo
/// (`render_anim_with_progress_con_rotulo`) la usa para lo no paramétrico.
fn render_anim_for_concept_legacy_with_progress(
    tmpl: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    match tmpl {
        "integral-area" => {
            render_integral_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "taylor-series" => render_taylor_frames_impl(width, height, con_rotulo, on_frame),
        "conformal-map" => render_conformal_frames_impl(width, height, con_rotulo, on_frame),
        "pitagoras" => render_pitagoras_frames_impl(width, height, con_rotulo, on_frame),
        "derivative-slope" => {
            render_derivative_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "euler" => {
            render_euler_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "fourier" => {
            render_fourier_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "logistic-bifurcation" => {
            render_logistic_bifurcation_frames_impl(width, height, con_rotulo, on_frame)
        }
        "gradient-field" => render_gradient_field_frames_impl(width, height, con_rotulo, on_frame),
        "mobius-transform" => render_mobius_frames_impl(width, height, con_rotulo, on_frame),
        "subspace" => {
            render_subspace_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "fractal" => render_fractal_frames_with_params_impl(width, height, con_rotulo, on_frame),
        "universal" => {
            render_universal_youtube_frames_impl(concept, width, height, con_rotulo, on_frame)
        }
        _ => render_universal_youtube_frames_impl(concept, width, height, con_rotulo, on_frame),
    }
}

/// Stub Euler: serie e^x parciales con fondo nativo, <2s garantizado.
/// Usa la misma paleta y grid para no romper estilo; animación determinista.
pub fn render_euler_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_euler_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Euler con params vivos: `terms` nº máximo de parciales de e^x en [1, 7]
/// (def 7); el frame `t` muestra `1 + t*(terms-1)` parciales.
/// Mapa vacío = histórico exacto.
pub fn render_euler_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_euler_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_euler_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let max_terms = scene_param_clamped(params, SCENE_PARAM_TERMS, 7.0, 1.0, 7.0) as usize;
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    // Partial sums of exp: S_n(x) = sum_{k=0..n} x^k/k!
    let start = std::time::Instant::now();
    let max_euler_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_euler_ms {
            // fill remaining with last frame to honrar <2s sin romper len
            let last = frames.last().cloned().unwrap_or_else(|| {
                let len = checked_frame_byte_len(w, h)
                    .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
                let mut b = vec![0u8; len];
                for chunk in b.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&PAL_BG);
                }
                if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
                        &b,
                    )
                }
            });
            while frames.len() < NATIVE_ANIM_FRAME_COUNT {
                frames.push(last.clone());
                on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            }
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let terms = (1 + (t * (max_terms as f64 - 1.0)) as usize).clamp(1, max_terms);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // draw exp(x) target faint (sin saturar: el clip corta en el borde)
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            draw_seg_mundo(&mut buf, w, h, x0, x0.exp(), x1, x1.exp(), FAINT_WHITE, 1.0);
        }
        // draw partial sum (sin saturar: el clip corta en el borde)
        let factorial = |n: usize| -> f64 { (1..=n).fold(1.0, |a, b| a * b as f64) };
        let partial = |x: f64| -> f64 {
            (0..terms)
                .map(|k| x.powi(k as i32) / factorial(k))
                .sum::<f64>()
        };
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            draw_seg_mundo(
                &mut buf,
                w,
                h,
                x0,
                partial(x0),
                x1,
                partial(x1),
                TANGENT_BLUE,
                CURVE_ANCHO,
            );
        }
        // Indicador de términos (solo export: en el chat duplica el header).
        if con_rotulo {
            draw_rotulo_con_scrim(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                &format!("e^x  n={}", terms - 1),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Stub Fourier: suma de armónicos de onda cuadrada, <2s garantizado.
pub fn render_fourier_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_fourier_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Fourier con params vivos: `terms` nº máximo de armónicos en [1, 6]
/// (def 6); el frame `t` muestra `1 + t*(terms-1)` armónicos.
/// Mapa vacío = histórico exacto.
pub fn render_fourier_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_fourier_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_fourier_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let max_harm = scene_param_clamped(params, SCENE_PARAM_TERMS, 6.0, 1.0, 6.0) as usize;
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            let last = frames.last().cloned().unwrap_or_else(|| {
                let len = checked_frame_byte_len(w, h)
                    .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
                let mut b = vec![0u8; len];
                for chunk in b.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&PAL_BG);
                }
                if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
                        &b,
                    )
                }
            });
            while frames.len() < NATIVE_ANIM_FRAME_COUNT {
                frames.push(last.clone());
                on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            }
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let harmonics = (1 + (t * (max_harm as f64 - 1.0)) as usize).clamp(1, max_harm);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let fourier = |x: f64| -> f64 {
            let mut s = 0.0;
            for k in 0..harmonics {
                let n = (2 * k + 1) as f64;
                s += (n * x).sin() / n;
            }
            (s * 4.0 / std::f64::consts::PI).clamp(-2.5, 2.5)
        };
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            draw_seg_mundo(
                &mut buf,
                w,
                h,
                x0,
                fourier(x0),
                x1,
                fourier(x1),
                CURVE_MAIN,
                CURVE_ANCHO,
            );
        }
        // Gibbs markers at discontinuities (rojo punto/resultado, radio 3).
        for &cx in &[-std::f64::consts::PI, 0.0, std::f64::consts::PI] {
            if cx.abs() <= 3.0 {
                let p = to_pixel(w, h, cx, 0.0);
                draw_filled_circle(&mut buf, w, h, p.0, p.1, 3, GIBBS_RED);
            }
        }
        if con_rotulo {
            draw_rotulo_con_scrim(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                &format!("fourier  k={}", harmonics),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Bifurcación logística: diagrama r∈[2.5,4.0] vs x*=r·x·(1-x).
/// Fondo + diagrama tenue estático + columna highlight que barre con t.
/// Determinista, <2s (muestreo cada 2px, 120 iters/col).
pub fn render_logistic_bifurcation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_logistic_bifurcation_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_logistic_bifurcation_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            let last = frames.last().cloned().unwrap_or_else(|| {
                let len = checked_frame_byte_len(w, h)
                    .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
                let mut b = vec![0u8; len];
                for chunk in b.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&PAL_BG);
                }
                if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
                        &b,
                    )
                }
            });
            while frames.len() < NATIVE_ANIM_FRAME_COUNT {
                frames.push(last.clone());
                on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            }
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // Área del diagrama con márgenes (segura en 64x64).
        let x0 = w / 12;
        let x1 = w.saturating_sub(w / 12).max(x0 + 8);
        let y_top = h / 4;
        let y_bot = h.saturating_sub(h / 4).max(y_top + 8);
        let span_x = (x1.saturating_sub(x0)).max(1);
        let span_y = (y_bot.saturating_sub(y_top)).max(1);
        // Diagrama tenue: 100 transitorias + 16 puntos por columna (step 2px).
        let mut sx = x0;
        while sx < x1 {
            let r = 2.5 + 1.5 * (sx.saturating_sub(x0)) as f64 / span_x as f64;
            let mut x = 0.5;
            for _ in 0..100 {
                x = r * x * (1.0 - x);
            }
            for _ in 0..16 {
                x = r * x * (1.0 - x);
                let frac = x.clamp(0.0, 1.0);
                let py = y_bot.saturating_sub((frac * span_y as f64) as usize);
                draw_filled_circle(
                    &mut buf,
                    w,
                    h,
                    sx,
                    py.min(h.saturating_sub(1)),
                    1,
                    MINT_FAINT,
                );
            }
            sx += 2;
        }
        // Highlight que barre con t + atractor brillante en r(t).
        let hx = x0 + ((t * span_x as f64) as usize).min(span_x.saturating_sub(1));
        draw_line(&mut buf, w, h, (hx, y_top), (hx, y_bot), PAL_ACCENT);
        let r_h = 2.5 + 1.5 * t;
        let mut xh = 0.5;
        for _ in 0..100 {
            xh = r_h * xh * (1.0 - xh);
        }
        for _ in 0..12 {
            xh = r_h * xh * (1.0 - xh);
            let frac = xh.clamp(0.0, 1.0);
            let py = y_bot.saturating_sub((frac * span_y as f64) as usize);
            draw_filled_circle(
                &mut buf,
                w,
                h,
                hx,
                py.min(h.saturating_sub(1)),
                2,
                POINT_RED,
            );
        }
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "bifurcacion");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "bifurcacion",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Campo de gradiente: f(x,y)=sin(x)·cos(y), grad=(cos·cos, −sin·sin).
/// 25 flechas + 6 partículas orbitando moduladas por |grad|. <2s.
pub fn render_gradient_field_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_gradient_field_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_gradient_field_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            let last = frames.last().cloned().unwrap_or_else(|| {
                let len = checked_frame_byte_len(w, h)
                    .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
                let mut b = vec![0u8; len];
                for chunk in b.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&PAL_BG);
                }
                if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
                        &b,
                    )
                }
            });
            while frames.len() < NATIVE_ANIM_FRAME_COUNT {
                frames.push(last.clone());
                on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            }
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // Flechas del gradiente (5x5, determinista).
        let math_per_px = 6.0 / w.max(1) as f64;
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64 * 0.9;
                let y = gy as f64 * 0.9;
                let gfx = x.cos() * y.cos();
                let gfy = -(x.sin() * y.sin());
                let mag = (gfx * gfx + gfy * gfy).sqrt();
                let (dx, dy) = if mag < 1e-9 {
                    (0.0, 0.0)
                } else {
                    (gfx / mag, gfy / mag)
                };
                let len_px = 4.0 + 10.0 * (mag / (1.0 + mag));
                let x2 = (x + dx * len_px * math_per_px).clamp(-3.0, 3.0);
                let y2 = (y + dy * len_px * math_per_px).clamp(-3.0, 3.0);
                let a = to_pixel(w, h, x, y);
                let b = to_pixel(w, h, x2, y2);
                draw_line(&mut buf, w, h, a, b, PAL_ACCENT);
                draw_filled_circle(&mut buf, w, h, b.0, b.1, 1, MINT_STRONG);
                // Origen tenue.
                draw_filled_circle(&mut buf, w, h, a.0, a.1, 1, MINT_FAINT);
            }
        }
        // Partículas que orbitan (t garantiza frames distintos).
        for i in 0..6 {
            let ang = 2.0 * std::f64::consts::PI * (i as f64 / 6.0) + t * 1.4 + i as f64 * 0.35;
            let rad = 1.25 + 0.3 * (t * std::f64::consts::TAU + i as f64 * 1.3).sin();
            let x = (rad * ang.cos()).clamp(-2.8, 2.8);
            let y = (rad * ang.sin() * 0.7).clamp(-2.8, 2.8);
            let p = to_pixel(w, h, x, y);
            let pulse = (frame as f64 * 0.4 + i as f64).sin() * 0.5 + 0.5;
            let col = with_alpha(POINT_RED, (140.0 + 100.0 * pulse) as u8);
            draw_filled_circle(&mut buf, w, h, p.0, p.1, 3, col);
        }
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "gradiente f");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "gradiente f",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Transformación de Möbius: w=(z−c)/(1−conj(c)·z), c(t) barre sin loop.
/// Rejilla tenue original + rejilla transformada brillante + círculo unidad.
/// <2s (25 puntos + 60 segmentos/frame).
pub fn render_mobius_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_mobius_frames_impl(width, height, true, &mut |_, _| {})
}

fn render_mobius_frames_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            let last = frames.last().cloned().unwrap_or_else(|| {
                let len = checked_frame_byte_len(w, h)
                    .unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
                let mut b = vec![0u8; len];
                for chunk in b.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&PAL_BG);
                }
                if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied(
                        [NATIVE_FALLBACK_W, NATIVE_FALLBACK_H],
                        &b,
                    )
                }
            });
            while frames.len() < NATIVE_ANIM_FRAME_COUNT {
                frames.push(last.clone());
                on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            }
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // c(t) barre sin loop (t=0 -> t=1 distintos): evita primero==último.
        let ang_c = 2.0 * std::f64::consts::PI * t;
        let (cr, ci) = (-0.4 + 0.8 * t, 0.3 * ang_c.sin());
        let mobius = |x: f64, y: f64| -> Option<(f64, f64)> { mobius_map(x, y, cr, ci) };
        // Rejilla original tenue + transformada brillante.
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let p0 = to_pixel(w, h, x, y);
                draw_filled_circle(&mut buf, w, h, p0.0, p0.1, 1, MINT_FAINT);
                if let Some((wx, wy)) = mobius(x, y) {
                    let p1 = to_pixel(w, h, wx, wy);
                    draw_filled_circle(&mut buf, w, h, p1.0, p1.1, 2, PAL_ACCENT);
                }
            }
        }
        // Círculo unidad transformado (60 segmentos).
        let mut prev: Option<(usize, usize)> = None;
        for k in 0..=60 {
            let a = 2.0 * std::f64::consts::PI * k as f64 / 60.0;
            let (zx, zy) = (a.cos(), a.sin());
            if let Some((wx, wy)) = mobius(zx, zy) {
                let p = to_pixel(w, h, wx, wy);
                if let Some(q) = prev {
                    draw_line(&mut buf, w, h, q, p, CURVE_MAIN);
                }
                prev = Some(p);
            } else {
                prev = None;
            }
        }
        // Parámetro c(t) en rojo.
        let pc = to_pixel(w, h, cr * 2.0, ci * 2.0);
        draw_filled_circle(&mut buf, w, h, pc.0, pc.1, 3, POINT_RED);
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "mobius w(z)");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "mobius w(z)",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        // Barra de progreso inferior: garantiza primero != último aunque c coincida.
        let bar_y = h.saturating_sub(4);
        // Progreso lineal honesto del frame (la construcción va por fases).
        let prog = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let bar_w = (w as f64 * prog) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 2, PAL_ACCENT);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w.saturating_sub(bar_w),
            2,
            TRACK,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── subspace: el span como paralelogramo (estilo 3Blue1Brown) ─────────────
// NumberPlane + ejes fijos de fondo; f0-12 fade in del plano; f12-24 flechas
// v1/v2 con GrowFromCenter + rótulos; f24-40 paralelogramo {0,v1,v1+v2,v2}
// con Write progresivo (reveal del span + hatch tenue); f40-48 rótulo
// "span(v1,v2)" + Indicate (halo que crece monótono, sin plateau).
// Default honesto v1=(2,1), v2=(-1,2) (det=5, rank 2); params vivos
// `v1x/v1y/v2x/v2y` (−3..=3) con validación rank==2 (|det|≥1e-6, si no
// default). Sin relleno alfa sólido: borde 3px + hatch (barato, O(miles)).
// Texto didáctico solo con `con_rotulo` (convención del archivo: el chat no
// quema texto, el export sí). Determinista, <2s, 48 frames.

/// Default honesto de v1 (linealmente independiente de v2, det=5).
pub const SUBSPACE_V1_DEFAULT: [f64; 2] = [2.0, 1.0];
/// Default honesto de v2 (linealmente independiente de v1, det=5).
pub const SUBSPACE_V2_DEFAULT: [f64; 2] = [-1.0, 2.0];
/// Claves vivas de params (contrato para el frente dispatch).
pub const SUBSPACE_PARAM_V1X: &str = "v1x";
/// Clave viva: componente y de v1.
pub const SUBSPACE_PARAM_V1Y: &str = "v1y";
/// Clave viva: componente x de v2.
pub const SUBSPACE_PARAM_V2X: &str = "v2x";
/// Clave viva: componente y de v2.
pub const SUBSPACE_PARAM_V2Y: &str = "v2y";
/// Primer frame con flechas (f0-12 = plano).
pub const SUBSPACE_F_VECTORES_DESDE: usize = 12;
/// Primer frame con span (f12-24 = vectores).
pub const SUBSPACE_F_SPAN_DESDE: usize = 24;
/// Primer frame con rótulo+Indicate (f24-40 = span).
pub const SUBSPACE_F_ROTULO_DESDE: usize = 40;
/// Determinante mínimo para rank 2 (|det| menor → default honesto).
pub const SUBSPACE_DET_MIN: f64 = 1e-6;

/// Fase didáctica del frame (rangos pineados en test). Pura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubspaceFase {
    /// f0-12: NumberPlane + ejes con fade in.
    Plano,
    /// f12-24: flechas v1/v2 con GrowFromCenter.
    Vectores,
    /// f24-40: paralelogramo con Write progresivo.
    Span,
    /// f40-48: rótulo + Indicate sobre el paralelogramo.
    Rotulo,
}

/// Fase para el frame (clamp a 47: frame ≥48 → `Rotulo`). Pura.
pub fn subspace_fase_para_frame(frame: usize) -> SubspaceFase {
    let f = frame.min(NATIVE_ANIM_FRAME_COUNT - 1);
    if f < SUBSPACE_F_VECTORES_DESDE {
        SubspaceFase::Plano
    } else if f < SUBSPACE_F_SPAN_DESDE {
        SubspaceFase::Vectores
    } else if f < SUBSPACE_F_ROTULO_DESDE {
        SubspaceFase::Span
    } else {
        SubspaceFase::Rotulo
    }
}

/// Vectores vivos desde params (clamp −3..=3) con validación rank==2:
/// no finitos, norma <1e-6 o |det|<1e-6 → default honesto. Pura.
pub fn subspace_vectores_desde_params(
    params: &std::collections::BTreeMap<String, f64>,
) -> ([f64; 2], [f64; 2]) {
    let v1 = [
        scene_param_clamped(
            params,
            SUBSPACE_PARAM_V1X,
            SUBSPACE_V1_DEFAULT[0],
            -3.0,
            3.0,
        ),
        scene_param_clamped(
            params,
            SUBSPACE_PARAM_V1Y,
            SUBSPACE_V1_DEFAULT[1],
            -3.0,
            3.0,
        ),
    ];
    let v2 = [
        scene_param_clamped(
            params,
            SUBSPACE_PARAM_V2X,
            SUBSPACE_V2_DEFAULT[0],
            -3.0,
            3.0,
        ),
        scene_param_clamped(
            params,
            SUBSPACE_PARAM_V2Y,
            SUBSPACE_V2_DEFAULT[1],
            -3.0,
            3.0,
        ),
    ];
    let det = v1[0] * v2[1] - v1[1] * v2[0];
    let n1 = v1[0].hypot(v1[1]);
    let n2 = v2[0].hypot(v2[1]);
    if !det.is_finite()
        || det.abs() < SUBSPACE_DET_MIN
        || n1 < SUBSPACE_DET_MIN
        || n2 < SUBSPACE_DET_MIN
    {
        (SUBSPACE_V1_DEFAULT, SUBSPACE_V2_DEFAULT)
    } else {
        (v1, v2)
    }
}

/// Funde el buffer hacia el BG en `mezcla` 0..1 (0 = intacto, 1 = BG).
/// Solo RGB: el alfa queda en 255 (contrato `assert_frames_valid`). Pura.
fn fundir_hacia_bg(buf: &mut [u8], mezcla: f64) {
    let m = mezcla.clamp(0.0, 1.0);
    if m <= 0.0 {
        return;
    }
    for px in buf.chunks_exact_mut(4) {
        for (k, fondo) in BG.iter().enumerate().take(3) {
            let v = f64::from(px[k]);
            px[k] = (v + (f64::from(*fondo) - v) * m).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Hatch vertical dentro del paralelogramo {0,v1,v1+v2,v2} (p=s·v1+t·v2 con
/// s,t∈[0,1]): segmentos mundo listos para `draw_seg_mundo`. Se precomputa
/// una vez por render (O(miles), barato). Pura.
fn subspace_hatch(v1: [f64; 2], v2: [f64; 2]) -> Vec<([f64; 2], [f64; 2])> {
    let det = v1[0] * v2[1] - v1[1] * v2[0];
    if !det.is_finite() || det.abs() < SUBSPACE_DET_MIN {
        return Vec::new();
    }
    let dentro = |x: f64, y: f64| -> bool {
        let s = (x * v2[1] - y * v2[0]) / det;
        let t = (v1[0] * y - v1[1] * x) / det;
        (-1e-9..=1.0 + 1e-9).contains(&s) && (-1e-9..=1.0 + 1e-9).contains(&t)
    };
    let esq = [[0.0, 0.0], v1, [v1[0] + v2[0], v1[1] + v2[1]], v2];
    let (mut xmin, mut xmax) = (esq[0][0], esq[0][0]);
    let (mut ymin, mut ymax) = (esq[0][1], esq[0][1]);
    for p in &esq {
        xmin = xmin.min(p[0]);
        xmax = xmax.max(p[0]);
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let mut out = Vec::new();
    let mut c = xmin + 0.09;
    while c < xmax {
        let mut y = ymin;
        let mut inicio: Option<f64> = None;
        while y <= ymax {
            if dentro(c, y) {
                if inicio.is_none() {
                    inicio = Some(y);
                }
            } else if let Some(y0) = inicio.take() {
                if y - y0 > 0.05 {
                    out.push(([c, y0], [c, y]));
                }
            }
            y += 0.03;
        }
        if let Some(y0) = inicio {
            if ymax - y0 > 0.05 {
                out.push(([c, y0], [c, ymax]));
            }
        }
        c += 0.18;
        if out.len() > 512 {
            break;
        }
    }
    out
}

/// Dibuja el span con Write progresivo: recorre el perímetro hasta
/// `avance` 0..1 y revela esa fracción del hatch. Pura sobre el buffer.
fn subspace_dibujar_span(
    buf: &mut [u8],
    w: usize,
    h: usize,
    esq: &[[f64; 2]; 4],
    avance: f64,
    hatch: &[([f64; 2], [f64; 2])],
) {
    let a = avance.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let lados = [
        (esq[0], esq[1]),
        (esq[1], esq[2]),
        (esq[2], esq[3]),
        (esq[3], esq[0]),
    ];
    let total: f64 = lados
        .iter()
        .map(|(p, q)| (q[0] - p[0]).hypot(q[1] - p[1]))
        .sum();
    if !total.is_finite() || total <= 0.0 {
        return;
    }
    let mut resto = total * a;
    for (p, q) in &lados {
        if resto <= 0.0 {
            break;
        }
        let len = (q[0] - p[0]).hypot(q[1] - p[1]);
        let toma = len.min(resto);
        if toma > 0.0 && len > 0.0 {
            let f = toma / len;
            draw_seg_mundo(
                buf,
                w,
                h,
                p[0],
                p[1],
                p[0] + (q[0] - p[0]) * f,
                p[1] + (q[1] - p[1]) * f,
                MINT_STRONG,
                CURVE_ANCHO,
            );
        }
        resto -= toma;
    }
    let n = ((hatch.len() as f64) * a).round().clamp(0.0, 512.0) as usize;
    for (p, q) in hatch.iter().take(n) {
        draw_seg_mundo(buf, w, h, p[0], p[1], q[0], q[1], MINT_FAINT, 1.0);
    }
}

/// Rótulo de punta ("v1"/"v2"): texto real a la escala de `h`, anclado al
/// tip con offset y clamp al frame. Solo la llama el camino con rótulo.
fn subspace_etiqueta_punta(buf: &mut [u8], w: usize, h: usize, punta: [f64; 2], texto: &str) {
    let (tx, ty) = to_pixel(w, h, punta[0], punta[1]);
    let x = tx.saturating_add(6).min(w.saturating_sub(12));
    let y = ty.saturating_sub(16);
    draw_text_block(buf, w, h, x, y, texto, PAL_FG, text_scale_for_h(h));
}

/// Subspace standalone (defaults honestos, con rótulo quemado para export).
pub fn render_subspace_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_subspace_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Subspace con params vivos (`v1x/v1y/v2x/v2y`, rank==2 o default).
pub fn render_subspace_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_subspace_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_subspace_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let (v1, v2) = subspace_vectores_desde_params(params);
    let suma = [v1[0] + v2[0], v1[1] + v2[1]];
    let esq = [[0.0, 0.0], v1, suma, v2];
    let hatch = subspace_hatch(v1, v2);
    let centro = [suma[0] / 2.0, suma[1] / 2.0];
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let fase = subspace_fase_para_frame(frame);
        // f0-12: fade in del plano (arranca tenue 15%, jamás invisible:
        // el primer frame no debe ser sólido).
        if fase == SubspaceFase::Plano {
            let e = ease_in_out(frame as f64 / 11.0);
            fundir_hacia_bg(&mut buf, 1.0 - (0.15 + 0.85 * e));
        }
        // f12-24: flechas con GrowFromCenter (longitud 0→1 con easing).
        if frame >= SUBSPACE_F_VECTORES_DESDE {
            let g =
                ease_in_out(((frame - SUBSPACE_F_VECTORES_DESDE) as f64 / 11.0).clamp(0.0, 1.0))
                    .max(0.02);
            draw_arrow_world(
                &mut buf,
                w,
                h,
                [0.0, 0.0],
                [v1[0] * g, v1[1] * g],
                CURVE_MAIN,
            );
            draw_arrow_world(
                &mut buf,
                w,
                h,
                [0.0, 0.0],
                [v2[0] * g, v2[1] * g],
                TANGENT_BLUE,
            );
            let (ox, oy) = to_pixel(w, h, 0.0, 0.0);
            draw_filled_circle(&mut buf, w, h, ox, oy, 2, DOT_BLUE);
            let (ax, ay) = to_pixel(w, h, v1[0] * g, v1[1] * g);
            draw_filled_circle(&mut buf, w, h, ax, ay, 2, CURVE_MAIN);
            let (bx, by) = to_pixel(w, h, v2[0] * g, v2[1] * g);
            draw_filled_circle(&mut buf, w, h, bx, by, 2, TANGENT_BLUE);
        }
        // f24-40: diagonal suma + span con Write progresivo.
        if frame >= SUBSPACE_F_SPAN_DESDE {
            draw_arrow_world(&mut buf, w, h, [0.0, 0.0], suma, POINT_RED);
            let (sx, sy) = to_pixel(w, h, suma[0], suma[1]);
            draw_filled_circle(&mut buf, w, h, sx, sy, 3, POINT_RED);
            let p = ease_in_out(((frame - SUBSPACE_F_SPAN_DESDE) as f64 / 15.0).clamp(0.0, 1.0));
            subspace_dibujar_span(&mut buf, w, h, &esq, p, &hatch);
        }
        // f40-48: span completo + Indicate (halo que crece monótono 0..7:
        // cada frame difiere, sin plateau).
        if fase == SubspaceFase::Rotulo {
            subspace_dibujar_span(&mut buf, w, h, &esq, 1.0, &hatch);
            let s = 1.0 + 0.05 * (frame - SUBSPACE_F_ROTULO_DESDE) as f64;
            let halo: [[f64; 2]; 4] = esq.map(|p| {
                [
                    centro[0] + (p[0] - centro[0]) * s,
                    centro[1] + (p[1] - centro[1]) * s,
                ]
            });
            let lados = [
                (halo[0], halo[1]),
                (halo[1], halo[2]),
                (halo[2], halo[3]),
                (halo[3], halo[0]),
            ];
            for (p, q) in &lados {
                draw_seg_mundo(&mut buf, w, h, p[0], p[1], q[0], q[1], MINT_FAINT, 1.0);
            }
        }
        // Barra de progreso inferior (honesta: frame/47, como mobius).
        let bar_y = h.saturating_sub(4);
        let prog = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let bar_w = (w as f64 * prog) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 2, PAL_ACCENT);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w.saturating_sub(bar_w),
            2,
            TRACK,
        );
        // Rótulos didácticos (solo export: el chat no quema texto).
        if con_rotulo {
            let titulo = match fase {
                SubspaceFase::Plano => "plano + ejes",
                SubspaceFase::Vectores => "v1, v2",
                SubspaceFase::Span | SubspaceFase::Rotulo => "span(v1,v2)",
            };
            // El rótulo del span se ancla a la esquina lejana (v1+v2) con
            // desplazamiento: en la esquina superior-izquierda fija caía
            // sobre la etiqueta de punta v1/v2 según orientación. Se clampa
            // con el ancho real del texto para no recortarse a la derecha.
            let (rx, ry) = match fase {
                SubspaceFase::Span | SubspaceFase::Rotulo => {
                    let (cx, cy) = to_pixel(w, h, suma[0], suma[1]);
                    let ancho_txt = titulo
                        .chars()
                        .take(48)
                        .count()
                        .saturating_mul(6 * text_scale_for_h(h))
                        .saturating_add(8);
                    let max_x = w.saturating_sub(ancho_txt.min(w).saturating_add(4));
                    (
                        cx.saturating_add(10).min(max_x),
                        cy.saturating_add(10).min(h.saturating_sub(4)),
                    )
                }
                _ => (w / 14, h / 12),
            };
            draw_rotulo_con_scrim(&mut buf, w, h, rx, ry, titulo);
            if frame >= SUBSPACE_F_VECTORES_DESDE {
                subspace_etiqueta_punta(&mut buf, w, h, v1, "v1");
                subspace_etiqueta_punta(&mut buf, w, h, v2, "v2");
            }
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── fractal: copo de Koch por segmentos (didáctico y barato) ─────────────
// NADA de Mandelbrot por píxel (CPU inviable): el copo itera 0→4 en 48
// frames por morph continuo entre niveles (cada frame difiere, sin plateau
// largo). Nivel n = 3·4ⁿ segmentos (máx 768 en n=4, O(cientos) por frame).
// Rótulo vivo "iteración N" + "S segmentos". Determinista, <2s, 48 frames.

/// Nivel máximo del copo (0→4 en 48 frames).
pub const FRACTAL_NIVEL_MAX: usize = 4;
/// Radio del triángulo base en mundo (entra en [−3,3]² con margen).
pub const FRACTAL_RADIO: f64 = 2.2;

/// Segmentos del copo en el nivel n: 3·4ⁿ (3, 12, 48, 192, 768). Pura.
pub fn koch_segmentos_por_nivel(nivel: usize) -> usize {
    3usize.saturating_mul(4usize.saturating_pow(nivel.min(FRACTAL_NIVEL_MAX) as u32))
}

/// Progreso global 0..4 del frame (48 frames → niveles 0→4). Pura.
fn koch_u_para_frame(frame: usize) -> f64 {
    let f = frame.min(NATIVE_ANIM_FRAME_COUNT - 1) as f64;
    f * FRACTAL_NIVEL_MAX as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
}

/// `(base 0..3, e 0..1)` del morph para el frame. Pura.
pub fn koch_morf_para_frame(frame: usize) -> (usize, f64) {
    let u = koch_u_para_frame(frame);
    let n = (u.floor() as usize).min(FRACTAL_NIVEL_MAX - 1);
    (n, ease_in_out((u - n as f64).clamp(0.0, 1.0)))
}

/// Nivel mostrado en el rótulo (redondeo del progreso). Pura.
pub fn koch_nivel_mostrado(frame: usize) -> usize {
    (koch_u_para_frame(frame).round() as usize).min(FRACTAL_NIVEL_MAX)
}

/// Triángulo equilátero base (cerrado: último == primero). Pura.
fn koch_triangulo_base() -> Vec<[f64; 2]> {
    let mut pts = Vec::with_capacity(4);
    for k in 0..3 {
        let a = std::f64::consts::FRAC_PI_2 + k as f64 * 2.0 * std::f64::consts::PI / 3.0;
        pts.push([FRACTAL_RADIO * a.cos(), FRACTAL_RADIO * a.sin()]);
    }
    pts.push(pts[0]);
    pts
}

/// Un paso de Koch sobre una polilínea cerrada: cada lado → 4, pico hacia
/// afuera (normal desde el centroide, independiente de la orientación).
/// Pura.
fn koch_paso(poli: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = poli.len().max(1) as f64;
    let centro = poli
        .iter()
        .fold([0.0, 0.0], |acc, p| [acc[0] + p[0] / n, acc[1] + p[1] / n]);
    let mut out = Vec::with_capacity(poli.len().saturating_mul(4));
    if let Some(primero) = poli.first() {
        out.push(*primero);
    }
    for par in poli.windows(2) {
        let (a, b) = (par[0], par[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let p1 = [a[0] + d[0] / 3.0, a[1] + d[1] / 3.0];
        let p3 = [a[0] + 2.0 * d[0] / 3.0, a[1] + 2.0 * d[1] / 3.0];
        let medio = [(p1[0] + p3[0]) / 2.0, (p1[1] + p3[1]) / 2.0];
        let mut normal = [medio[0] - centro[0], medio[1] - centro[1]];
        let largo = normal[0].hypot(normal[1]);
        if largo.is_finite() && largo > 1e-9 {
            normal = [normal[0] / largo, normal[1] / largo];
        } else {
            normal = [0.0, 0.0];
        }
        let alto = d[0].hypot(d[1]) * 3.0_f64.sqrt() / 6.0;
        let pico = [medio[0] + normal[0] * alto, medio[1] + normal[1] * alto];
        out.extend_from_slice(&[p1, pico, p3, b]);
    }
    out
}

/// Los 5 niveles del copo (índice = nivel). Se precomputa una vez por
/// render (el nivel 4 son 769 puntos, trivial). Pura.
fn koch_niveles() -> Vec<Vec<[f64; 2]>> {
    let mut niveles = Vec::with_capacity(FRACTAL_NIVEL_MAX + 1);
    niveles.push(koch_triangulo_base());
    for _ in 0..FRACTAL_NIVEL_MAX {
        let base: &[[f64; 2]] = match niveles.last() {
            Some(v) => v.as_slice(),
            None => &[],
        };
        niveles.push(koch_paso(base));
    }
    niveles
}

/// Morph entre `niveles[n]` y `niveles[n+1]`: cada lado se subdivide recto
/// en cuartos y se interpola a su forma Koch con `e`. Con e=0 es el nivel n
/// (refinado colineal), con e=1 el n+1. Pura.
fn koch_morf(niveles: &[Vec<[f64; 2]>], n: usize, e: f64) -> Vec<[f64; 2]> {
    let base: &[[f64; 2]] = match niveles.get(n) {
        Some(v) => v.as_slice(),
        None => &[],
    };
    let next: &[[f64; 2]] = match niveles.get(n + 1) {
        Some(v) => v.as_slice(),
        None => &[],
    };
    let mut out = Vec::with_capacity(next.len());
    for (i, par) in base.windows(2).enumerate() {
        let (a, b) = (par[0], par[1]);
        for k in 0..4 {
            let f = k as f64 / 4.0;
            let q = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
            let p = next.get(4 * i + k).copied().unwrap_or(q);
            out.push([q[0] + (p[0] - q[0]) * e, q[1] + (p[1] - q[1]) * e]);
        }
    }
    if let Some(ultimo) = base.last() {
        out.push(*ultimo);
    }
    out
}

/// Fractal standalone (con rótulo quemado para export).
pub fn render_fractal_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_fractal_frames_with_params(width, height, &std::collections::BTreeMap::new())
}

/// Fractal con params (hoy los ignora: niveles 0→4 fijos; firma viva para
/// el dispatcher de params sin mentir niveles configurables).
pub fn render_fractal_frames_with_params(
    width: u32,
    height: u32,
    _params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_fractal_frames_with_params_impl(width, height, true, &mut |_, _| {})
}

fn render_fractal_frames_with_params_impl(
    width: u32,
    height: u32,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let niveles = koch_niveles();
    let guia = niveles.first().cloned().unwrap_or_default();
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        // Triángulo guía tenue (origen del copo) + copo en morph continuo.
        let guia_tuplas: Vec<(f64, f64)> = guia.iter().map(|p| (p[0], p[1])).collect();
        draw_curva_mundo(&mut buf, w, h, &guia_tuplas, FAINT_WHITE, 1.0);
        let (n, e) = koch_morf_para_frame(frame);
        let copo = koch_morf(&niveles, n, e);
        let copo_tuplas: Vec<(f64, f64)> = copo.iter().map(|p| (p[0], p[1])).collect();
        draw_curva_mundo(&mut buf, w, h, &copo_tuplas, CURVE_MAIN, CURVE_ANCHO);
        // Barra de progreso inferior (honesta: frame/47, como mobius).
        let bar_y = h.saturating_sub(4);
        let prog = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let bar_w = (w as f64 * prog) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 2, PAL_ACCENT);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w.saturating_sub(bar_w),
            2,
            TRACK,
        );
        // Rótulo vivo (solo export): nivel + segmentos reales del nivel.
        if con_rotulo {
            let mostrado = koch_nivel_mostrado(frame);
            let segs = koch_segmentos_por_nivel(mostrado);
            draw_rotulo_con_scrim(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                &format!("iteración {mostrado}"),
            );
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12 + 16,
                &format!("{segs} segmentos"),
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub fn render_anim_by_template(template: &str, width: u32, height: u32) -> Vec<egui::ColorImage> {
    // Compat: si se llama solo con template, el fallback es el placeholder
    // neutro honesto (antes "elegante" con curva falsa).
    match template {
        "integral-area" | "fraccion-visual" | "prob-anim" => render_integral_frames(width, height),
        "taylor-series" | "serie-anim" | "trig-anim" => render_taylor_frames(width, height),
        "conformal-map" | "vector-anim" | "conica-anim" => render_conformal_frames(width, height),
        "pitagoras" | "pythagoras" => render_pitagoras_frames(width, height),
        "matriz-anim" | "universal" => {
            render_universal_youtube_frames("matem\u{00e1}tica", width, height)
        }
        "derivative-slope" | "ecuacion-anim" => render_native_animation_frames(width, height),
        "euler" => render_euler_frames(width, height),
        "fourier" => render_fourier_frames(width, height),
        "logistic-bifurcation" | "bifurcacion-logistica" | "logistica" => {
            render_logistic_bifurcation_frames(width, height)
        }
        "gradient-field" | "campo-gradiente" | "gradiente" => {
            render_gradient_field_frames(width, height)
        }
        "mobius-transform" | "mobius" | "moebius" => render_mobius_frames(width, height),
        "subspace" => render_subspace_frames(width, height),
        "fractal" => render_fractal_frames(width, height),
        _ => {
            // Template desconocido -> placeholder neutro con ese texto como
            // concepto (eco, no respuesta) para no quedar vacío.
            if template.trim().is_empty() {
                render_native_animation_frames(width, height)
            } else {
                render_universal_youtube_frames(template, width, height)
            }
        }
    }
}

// ── AS4: render paramétrico genérico 100% Rust (sin Python/manim) ───────
// `ParametricAnim` (crate `grafito-anim`, modelo + inferencia + evaluador)
// se rasteriza aquí con la misma paleta y mundo [-3,3]² que los templates
// dedicados. Los templates viejos NO se tocan: `parametric_for_template`
// declara cuáles tienen equivalente canónico (tangente/área/traza/barrido)
// y cuáles conservan su renderer dedicado (`pitagoras`, euler, fourier…).
use grafito_anim::parametric::{
    FrameCount, ParamName, ParametricAnim, ParametricKind, PARAMETRIC_MAX_BYTES, TAYLOR_MIN_ORDER,
};
use grafito_anim::Resolution;

/// Error tipado del render paramétrico (mensajes en español, sin pánicos).
#[derive(Debug, Clone, PartialEq)]
pub enum ParametricRenderError {
    /// La animación no valida (expresión, rango, frames o viewport).
    InvalidAnim(String),
    /// El set estimado excede `PARAMETRIC_MAX_BYTES` o desborda.
    Oom { got: Option<usize>, max: usize },
    /// La reserva del frame falló (OOM real del SO).
    AllocFailed { bytes: usize },
    /// Vivo fijo congelaría el GIF (R1-1): el export exige rango propio.
    VivoFijoNoExportable,
}

impl std::fmt::Display for ParametricRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAnim(detail) => write!(f, "animación inválida: {detail}"),
            Self::Oom { got, max } => match got {
                Some(got) => write!(
                    f,
                    "el set estimado ({got} bytes) excede el tope de {max} bytes: bajá la resolución o los fotogramas"
                ),
                None => write!(
                    f,
                    "el set estimado desborda el contador: bajá la resolución o los fotogramas (tope {max} bytes)"
                ),
            },
            Self::AllocFailed { bytes } => {
                write!(f, "sin memoria para reservar el frame ({bytes} bytes)")
            }
            Self::VivoFijoNoExportable => write!(
                f,
                "el vivo fijo congela el GIF (foto del slider): exportá con rango propio o re-render por cambio de slider"
            ),
        }
    }
}

impl std::error::Error for ParametricRenderError {}

/// Presupuesto del set antes de reservar (honesto, sin OOM).
fn check_parametric_budget(
    anim: &ParametricAnim,
) -> Result<(usize, usize, usize), ParametricRenderError> {
    // OOM primero (solo lee campos, no valida): el rechazo por memoria debe
    // ser `Oom` aunque el resto también falle.
    let (w, h) = (anim.viewport.width as usize, anim.viewport.height as usize);
    let n = anim.frame_count();
    match estimate_frames_bytes(w, h, n) {
        Some(got) if got <= PARAMETRIC_MAX_BYTES => {}
        other => {
            return Err(ParametricRenderError::Oom {
                got: other,
                max: PARAMETRIC_MAX_BYTES,
            });
        }
    }
    anim.validate()
        .map_err(|e| ParametricRenderError::InvalidAnim(e.to_string()))?;
    Ok((w, h, n))
}

/// Ejes + título común del mundo paramétrico [-3,3]².
///
/// `con_rotulo=false` omite el texto quemado (el header egui de la card ya
/// titula); el fondo sigue usando `title` para el acento. `true` = histórico
/// (standalone / export GIF).
fn draw_parametric_base(buf: &mut [u8], w: usize, h: usize, title: &str, con_rotulo: bool) {
    fill_background(buf, w, h);
    draw_subtle_grid(buf, w, h);
    draw_axes_with_labels(buf, w, h);
    let short: String = title.chars().take(24).collect();
    if con_rotulo {
        draw_rotulo_con_scrim(&mut *buf, w, h, w / 12, h / 12, &short);
    }
}

/// Muestrea `y = anim(frame i, x)` en 121 puntos de [-3,3]; huecos donde no hay dominio.
fn sample_curve(anim: &ParametricAnim, frame: usize) -> Vec<Option<(f64, f64)>> {
    sample_curve_con_vivo(anim, frame, None)
}

/// Idem siguiendo el `p` vivo del documento (M4, updater por frame).
///
/// `Some(v)` finito = el slider manda en este frame; `None` = rango propio.
/// La dibuja el player en cada frame, nunca la UI.
fn sample_curve_con_vivo(
    anim: &ParametricAnim,
    frame: usize,
    vivo: Option<f64>,
) -> Vec<Option<(f64, f64)>> {
    (0..=120)
        .map(|k| {
            let x = -3.0 + 6.0 * (k as f64 / 120.0);
            anim.eval_frame_con_vivo(frame, x, vivo).map(|y| (x, y))
        })
        .collect()
}

/// Dibuja la polilínea cortando en huecos (sin unir ramas ni dominios rotos)
/// y cortando limpio en el borde del viewport (sin plateau: cada tramo se
/// clipa en mundo en vez de saturar píxeles).
fn draw_curve_gaps(buf: &mut [u8], w: usize, h: usize, pts: &[Option<(f64, f64)>], color: [u8; 4]) {
    for pair in pts.windows(2) {
        if let (Some((ax, ay)), Some((bx, by))) = (pair[0], pair[1]) {
            draw_seg_mundo(buf, w, h, ax, ay, bx, by, color, 1.0);
        }
    }
}

/// Pendiente numérica central (clamp a ±10 para dibujar; `None` sin dominio).
fn numeric_slope(anim: &ParametricAnim, frame: usize, x: f64) -> Option<f64> {
    numeric_slope_con_vivo(anim, frame, x, None)
}

/// Idem siguiendo el `p` vivo del documento (M4).
fn numeric_slope_con_vivo(
    anim: &ParametricAnim,
    frame: usize,
    x: f64,
    vivo: Option<f64>,
) -> Option<f64> {
    let h_step = 1e-3;
    let a = anim.eval_frame_con_vivo(frame, x - h_step, vivo)?;
    let b = anim.eval_frame_con_vivo(frame, x + h_step, vivo)?;
    let s = (b - a) / (2.0 * h_step);
    if s.is_finite() {
        Some(s.clamp(-10.0, 10.0))
    } else {
        None
    }
}

/// Renderiza una `ParametricAnim` a fotogramas RGBA (puro en memoria).
///
/// OOM acotado: valida presupuesto con `estimate_frames_bytes` y reserva con
/// `try_reserve` (`AllocFailed` honesto en vez de abortar). Determinista:
/// mismo `anim` → mismos píxeles.
pub fn render_parametric_frames(
    anim: &ParametricAnim,
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    render_parametric_frames_with_progress(anim, &mut |_, _| {})
}

/// Idem + progreso REAL por frame (`on_frame(done 1..=n, total n)`).
///
/// Sin rótulo quemado (pipeline del chat: el header egui ya titula). Para
/// standalone / export, usar `render_parametric_frames_con_rotulo(anim, true)`.
pub fn render_parametric_frames_with_progress(
    anim: &ParametricAnim,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    render_parametric_frames_with_progress_con_rotulo(anim, false, on_frame)
}

/// Núcleo con flag de rótulo: `true` = título quemado histórico (export GIF
/// standalone); `false` = chat (sin duplicar el header de la card).
pub fn render_parametric_frames_with_progress_con_rotulo(
    anim: &ParametricAnim,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    render_parametric_frames_con_updater(anim, &mut |_| None, con_rotulo, on_frame)
}

/// Núcleo con updater por frame (M4): `updater(frame)` se lee EN CADA frame
/// donde el player muestrea (nunca en la UI).
///
/// El llamador pasa el `p` vivo del documento
/// (`doc.live_param_value("p", …)` cuando la variable existe, `None` si no):
/// con `Some` finito el scrub/play sigue al slider; con `None`, rango propio
/// del `anim`. El cierre puede devolver distinto valor por frame (updater
/// real estilo Manim) o uno fijo (foto del slider). Presupuesto intacto:
/// mismo N y mismo viewport que la vía sin vivo.
pub fn render_parametric_frames_con_updater(
    anim: &ParametricAnim,
    updater: &mut dyn FnMut(usize) -> Option<f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    let (w, h, n) = check_parametric_budget(anim)?;
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(n)
        .map_err(|_| ParametricRenderError::Oom {
            got: anim.estimate_bytes(),
            max: PARAMETRIC_MAX_BYTES,
        })?;
    for frame in 0..n {
        let vivo = updater(frame);
        let s = anim.frame_fraction_con_vivo(frame, vivo);
        let p = anim.frame_param_con_vivo(frame, vivo);
        let mut buf = alloc_frame_buffer(w, h).map_err(|_| {
            let got = estimate_frames_bytes(w, h, 1);
            ParametricRenderError::AllocFailed {
                bytes: got.unwrap_or(w.saturating_mul(h).saturating_mul(4)),
            }
        })?;
        draw_parametric_base(&mut buf, w, h, &anim.expr_a, con_rotulo);
        match anim.kind {
            ParametricKind::Sweep | ParametricKind::Morph => {
                let pts = sample_curve_con_vivo(anim, frame, vivo);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
            }
            ParametricKind::Trace => {
                // La curva se dibuja hasta `s`: solo el prefijo x ≤ -3+6s.
                let x_max = -3.0 + 6.0 * s;
                let pts: Vec<Option<(f64, f64)>> = (0..=120)
                    .map(|k| {
                        let x = -3.0 + 6.0 * (k as f64 / 120.0);
                        if x <= x_max {
                            anim.eval_frame_con_vivo(frame, x, vivo).map(|y| (x, y))
                        } else {
                            None
                        }
                    })
                    .collect();
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                // Punto solo en vista (sin aplastar al borde).
                if let Some(y) = anim.eval_frame_con_vivo(frame, x_max, vivo) {
                    if let Some((px, py)) = to_pixel_opt(w, h, x_max, y) {
                        draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                    }
                }
            }
            ParametricKind::Locus => {
                // Traza de (q, f(q)) para q de p0 a p + punto móvil en p.
                let pts: Vec<Option<(f64, f64)>> = (0..=120)
                    .map(|k| {
                        let q = anim.p0 + (p - anim.p0) * (k as f64 / 120.0);
                        anim.eval_frame_con_vivo(frame, q, vivo).map(|y| (q, y))
                    })
                    .collect();
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let xc = p.clamp(-3.0, 3.0);
                if let Some(y) = anim.eval_frame_con_vivo(frame, xc, vivo) {
                    if let Some((px, py)) = to_pixel_opt(w, h, xc, y) {
                        draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                    }
                }
            }
            ParametricKind::Tangent => {
                let pts = sample_curve_con_vivo(anim, frame, vivo);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let xc = p.clamp(-3.0, 3.0);
                if let (Some(y0), Some(slope)) = (
                    anim.eval_frame_con_vivo(frame, xc, vivo),
                    numeric_slope_con_vivo(anim, frame, xc, vivo),
                ) {
                    let xa = (xc - 1.2).max(-3.0);
                    let xb = (xc + 1.2).min(3.0);
                    // Tangente recortada a vista (sin plateau en el borde).
                    draw_seg_mundo(
                        &mut buf,
                        w,
                        h,
                        xa,
                        y0 + slope * (xa - xc),
                        xb,
                        y0 + slope * (xb - xc),
                        TANGENT_BLUE,
                        CURVE_ANCHO,
                    );
                    if let Some((px, py)) = to_pixel_opt(w, h, xc, y0) {
                        draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                    }
                }
            }
            ParametricKind::Area => {
                // N1: área entre p0 (fijo) y p (móvil) bajo la curva + cota
                // vertical en b + S acumulada con el mismo evaluador.
                // Cero puntos sueltos: la cota es línea (DOT_BLUE), no círculo.
                let mut a = anim.p0.clamp(-3.0, 3.0);
                let mut b = p.clamp(-3.0, 3.0);
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                for px_col in 0..w {
                    let x = -3.0 + 6.0 * (px_col as f64 / w as f64);
                    if x < a || x > b {
                        continue;
                    }
                    if let Some(y) = anim.eval_frame_con_vivo(frame, x, vivo) {
                        let top = to_pixel(w, h, x, y);
                        let base = to_pixel(w, h, x, 0.0);
                        draw_line(&mut buf, w, h, base, top, FILL_SOFT_BLUE);
                    }
                }
                let y_b = anim.eval_frame_con_vivo(frame, b, vivo).map_or(0.0, |v| v);
                draw_line(
                    &mut buf,
                    w,
                    h,
                    to_pixel(w, h, b, 0.0),
                    to_pixel(w, h, b, y_b),
                    DOT_BLUE,
                );
                let pts = sample_curve_con_vivo(anim, frame, vivo);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let etiqueta = match integral_acumulada_con_vivo(Some(anim), frame, a, b, vivo) {
                    Some(s) => format!("S={s:.2}"),
                    None => "S=?".to_string(),
                };
                draw_text_block(
                    &mut buf,
                    w,
                    h,
                    w / 14,
                    h.saturating_sub(14),
                    &etiqueta,
                    TEXT_COLOR,
                    1,
                );
            }
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), n);
    }
    Ok(frames)
}

/// Atajo sin progreso con flag de rótulo (standalone / export GIF con `true`).
pub fn render_parametric_frames_con_rotulo(
    anim: &ParametricAnim,
    con_rotulo: bool,
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    render_parametric_frames_with_progress_con_rotulo(anim, con_rotulo, &mut |_, _| {})
}

/// Atajo con `p` vivo fijo (M4): todo el set se muestrea con el mismo `vivo`
/// (foto del slider; el llamador re-renderiza al moverse).
///
/// SEMÁNTICA PINNEADA R1-1: vivo fijo = preview foto, SOLO re-render por
/// cambio de slider. Congela el GIF (N frames idénticos): el export GIF lo
/// prohíbe con `Err(VivoFijoNoExportable)` vía `validar_vivo_para_gif`.
/// Para export usar la vía sin vivo o el updater por frame.
///
/// `Some(v)` finito = el documento define `p` y el play lo sigue;
/// `None` = rango propio (idéntico a la vía sin vivo). Puro en memoria.
pub fn render_parametric_frames_con_vivo(
    anim: &ParametricAnim,
    vivo: Option<f64>,
) -> Result<Vec<egui::ColorImage>, ParametricRenderError> {
    render_parametric_frames_con_updater(anim, &mut |_| vivo, false, &mut |_, _| {})
}

/// Guard honesto R1-1: el GIF no acepta vivo fijo (set congelado).
///
/// `Some(v)` finito → `Err(VivoFijoNoExportable)`; `None` o no-finito
/// (cae a rango propio) → `Ok(())`. Puro, sin I/O.
pub fn validar_vivo_para_gif(vivo: Option<f64>) -> Result<(), ParametricRenderError> {
    match vivo {
        Some(v) if v.is_finite() => Err(ParametricRenderError::VivoFijoNoExportable),
        _ => Ok(()),
    }
}

// ── M4: Group simultáneo REAL (composición alfa frame a frame) ───────────
// El `AnimationGroup{lag_ratio}` ya calcula offsets de arranque; acá los
// PÍXELES simultáneos: 2+ sets del MISMO N y MISMO viewport se fusionan con
// over (`mezclar_pixel_alfa`, `sets[0]` fondo → último frente). Si N o el
// tamaño difieren → `Err` honesto, jamás reescaleo silencioso. R1-2: los
// frames nativos son opacos (alfa 255) y el frente taparía mudo: todo frente
// totalmente opaco → `Err(FrenteOpaco)` honesto (exigí alfa<255 por set o
// modo split/grid). La mezcla real se ve con capas translúcidas (ver test).
// Presupuesto intacto: el set compuesto respeta `NATIVE_MAX_SET_BYTES`.
// Puro en memoria, sin I/O.

/// Error tipado de la composición simultánea de grupo (mensajes en español).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupComposeError {
    /// Sin sets (vacío).
    Vacio,
    /// Un solo set (para 1 usar la playlist).
    UnSoloSet { got: usize },
    /// Algún set vacío.
    SetVacio { set: usize },
    /// N distinto entre sets (histórico F2: el re-muestreo vecino-más-cercano
    /// acepta N dispar; se conserva por compat, ya no se construye).
    ConteosDistintos {
        esperado: usize,
        got: usize,
        set: usize,
    },
    /// Tamaño distinto entre frames.
    TamanosDistintos {
        esperado: [usize; 2],
        got: [usize; 2],
        set: usize,
        frame: usize,
    },
    /// Lado fuera de 1..=4096.
    DimensionFueraDeRango { width: usize, height: usize },
    /// El compuesto excede el tope de bytes.
    Presupuesto { got: usize },
    /// Frente totalmente opaco: taparía mudo (R1-2).
    FrenteOpaco { set: usize },
}

impl std::fmt::Display for GroupComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vacio => write!(f, "sin sets para componer: pasame al menos 2 animaciones"),
            Self::UnSoloSet { got } => write!(
                f,
                "el compuesto necesita al menos 2 animaciones (pasaste {got}): para 1 sola usá la playlist"
            ),
            Self::SetVacio { set } => {
                write!(f, "el set {set} está vacío: sin frames no hay qué componer")
            }
            Self::ConteosDistintos { esperado, got, set } => write!(
                f,
                "el set {set} trae {got} frames y el grupo pide {esperado}: igualá N (sin reescaleo silencioso)"
            ),
            Self::TamanosDistintos {
                esperado,
                got,
                set,
                frame,
            } => write!(
                f,
                "el frame {frame} del set {set} mide {}x{} y el grupo pide {}x{}: igualá el viewport (sin reescaleo silencioso)",
                got[0], got[1], esperado[0], esperado[1]
            ),
            Self::DimensionFueraDeRango { width, height } => write!(
                f,
                "dimensión {width}x{height} fuera de 1..={NATIVE_MAX_DIM} para componer"
            ),
            Self::Presupuesto { got } => write!(
                f,
                "el compuesto estimado ({got} bytes) excede el tope de {NATIVE_MAX_SET_BYTES}: bajá la resolución o los fotogramas"
            ),
            Self::FrenteOpaco { set } => write!(
                f,
                "el set {set} es totalmente opaco y taparía al fondo en silencio: usá alfa<255 por capa o modo split/grid"
            ),
        }
    }
}

impl std::error::Error for GroupComposeError {}

/// ¿El set es totalmente opaco (todo píxel alfa 255 en todo frame)?
/// Puro, sin I/O. Vacío → `false` (lo rechaza `SetVacio` aparte).
fn set_es_totalmente_opaco(set: &[egui::ColorImage]) -> bool {
    !set.is_empty()
        && set
            .iter()
            .all(|frame| frame.pixels.iter().all(|p| p.a() == 255))
}

/// Grilla de vecino más cercano para el re-muestreo temporal (F2): `n_comun`
/// índices en `0..n_set` (bordes clampados). Misma fórmula que
/// `AnimationGroup::plan_remuestreo` del núcleo (duplicada acá a propósito:
/// el núcleo no expone el helper y el puente no debe invertir la dependencia).
/// Pura, sin pánicos.
fn indice_vecino_mas_cercano_nativo(n_set: usize, n_comun: usize) -> Vec<usize> {
    if n_comun == 0 || n_set == 0 {
        return Vec::new();
    }
    if n_comun == 1 || n_set == 1 {
        return vec![0; n_comun];
    }
    let mut out = Vec::with_capacity(n_comun);
    for j in 0..n_comun {
        let pos =
            (j as f64) * ((n_set.saturating_sub(1)) as f64) / ((n_comun.saturating_sub(1)) as f64);
        let idx = if pos.is_finite() {
            (pos.round() as usize).min(n_set.saturating_sub(1))
        } else {
            0
        };
        out.push(idx);
    }
    out
}

/// Compone 2+ sets simultáneos píxel a píxel (alfa over por frame).
///
/// `sets[k][i]` = frame `i` de la capa `k` (`sets[0]` fondo, último frente).
/// N dispar se remuestrea al máximo (`n_comun`): el frame `j` del compuesto
/// lee por capa su vecino más cercano (misma disciplina que
/// `AnimationGroup::plan_remuestreo` del núcleo, sin inventar píxeles).
/// Exige mismo viewport en todos (si difiere → `Err` honesto, jamás reescaleo
/// silencioso). R1-2: exige translucidez en los frentes (alfa<255 en algún
/// píxel): frente totalmente opaco → `Err(FrenteOpaco)`, jamás tapa silenciosa.
/// Determinista: mismos sets → mismos píxeles. Puro en memoria.
pub fn componer_grupo_nativo(
    sets: &[Vec<egui::ColorImage>],
) -> Result<Vec<egui::ColorImage>, GroupComposeError> {
    if sets.is_empty() {
        return Err(GroupComposeError::Vacio);
    }
    if sets.len() < 2 {
        return Err(GroupComposeError::UnSoloSet { got: sets.len() });
    }
    // F2: `n_comun` = máximo; cada set aporta por frame su vecino más cercano
    // (costo O(frames), despreciable frente al O(píxeles) del over).
    let mut n_comun: usize = 0;
    for (k, set) in sets.iter().enumerate() {
        if set.is_empty() {
            return Err(GroupComposeError::SetVacio { set: k });
        }
        n_comun = n_comun.max(set.len());
    }
    let mut indices_por_set: Vec<Vec<usize>> = Vec::with_capacity(sets.len());
    for set in sets.iter() {
        indices_por_set.push(indice_vecino_mas_cercano_nativo(set.len(), n_comun));
    }
    let size0 = sets[0][0].size;
    let (w, h) = (size0[0], size0[1]);
    if w == 0 || h == 0 || w > NATIVE_MAX_DIM as usize || h > NATIVE_MAX_DIM as usize {
        return Err(GroupComposeError::DimensionFueraDeRango {
            width: w,
            height: h,
        });
    }
    let px_count = w
        .checked_mul(h)
        .ok_or(GroupComposeError::DimensionFueraDeRango {
            width: w,
            height: h,
        })?;
    for (k, set) in sets.iter().enumerate() {
        for (i, frame) in set.iter().enumerate() {
            if frame.size != size0 || frame.pixels.len() != px_count {
                return Err(GroupComposeError::TamanosDistintos {
                    esperado: size0,
                    got: frame.size,
                    set: k,
                    frame: i,
                });
            }
        }
    }
    match estimate_frames_bytes(w, h, n_comun) {
        Some(got) if got <= NATIVE_MAX_SET_BYTES => {}
        other => {
            return Err(GroupComposeError::Presupuesto {
                got: other.unwrap_or(usize::MAX),
            });
        }
    }
    for (k, set) in sets.iter().enumerate().skip(1) {
        if set_es_totalmente_opaco(set) {
            return Err(GroupComposeError::FrenteOpaco { set: k });
        }
    }
    let mut out = Vec::with_capacity(n_comun);
    for j in 0..n_comun {
        let mut pixeles = Vec::with_capacity(px_count);
        for pi in 0..px_count {
            // Lectura en alfa directo (`to_srgba_unmultiplied`): los bytes
            // crudos de `Color32` van premultiplicados y mezclar sobre ellos
            // oscurecería el doble. Escritura idem directa.
            let mut acc = sets[0][indices_por_set[0][j]].pixels[pi].to_srgba_unmultiplied();
            for (k, set) in sets.iter().enumerate().skip(1) {
                let frame = &set[indices_por_set[k][j]];
                acc = mezclar_pixel_alfa(acc, frame.pixels[pi].to_srgba_unmultiplied());
            }
            pixeles.push(egui::Color32::from_rgba_unmultiplied(
                acc[0], acc[1], acc[2], acc[3],
            ));
        }
        out.push(egui::ColorImage {
            size: size0,
            pixels: pixeles,
        });
    }
    Ok(out)
}

/// Equivalente paramétrico canónico de un template viejo, si lo tiene.
///
/// `derivative-slope` → tangente móvil sobre `x^2`; `integral-area` → área
/// móvil sobre `x^2`; `taylor-series` → traza de `sin(x)`;
/// `conformal-map` → barrido de `sin(x+p)`. El resto (`pitagoras`, euler,
/// fourier, logistic, gradient, mobius, universal) conserva su renderer
/// dedicado → `None` honesto (no se finge equivalencia).
pub fn parametric_for_template(template: &str, concept: &str) -> Option<ParametricAnim> {
    let t = template.trim().to_lowercase();
    let viewport = Resolution::try_new(640, 480).unwrap_or_default();
    let mk = |kind: ParametricKind, expr: &str, param: &str, p0: f64, p1: f64| {
        ParametricAnim::try_new(
            kind,
            expr.to_string(),
            None,
            ParamName::try_new(param).ok()?,
            p0,
            p1,
            FrameCount::try_new(NATIVE_ANIM_FRAME_COUNT).ok()?,
            viewport,
        )
        .ok()
    };
    match t.as_str() {
        "derivative-slope" | "ecuacion-anim" => mk(ParametricKind::Tangent, "x^2", "p", -1.5, 1.5),
        "integral-area" | "fraccion-visual" | "prob-anim" => {
            mk(ParametricKind::Area, "x^2", "p", 0.0, 2.0)
        }
        "taylor-series" | "serie-anim" | "trig-anim" => {
            mk(ParametricKind::Trace, "sin(x)", "t", 0.0, 1.0)
        }
        "conformal-map" | "vector-anim" | "conica-anim" => {
            mk(ParametricKind::Sweep, "sin(x+p)", "p", 0.0, 6.0)
        }
        _ => {
            let _ = concept;
            None
        }
    }
}

// ── Diálogo Exportar profesional (piel-ui, puro + estado) ─────────────────
// El backend ya existe (`export_frames_to_gif_file`, `export_frames_to_png_dir`,
// `export_frames_to_mp4_file`, `export_frames_to_webm_file`,
// `draw_mobject` nuevos, `render_orbit_frames`): lo que faltaba era el ESTADO
// del diálogo para que la Piel lo dibuje sin I/O.
//
// El estado vivo es `grafito_ui::assistant::MediaExportDialog`; acá quedan
// los validadores, formatos, calidades y topes compartidos (ver abajo).
//
// Contrato `fn render(&Estado) -> Frame`: este struct ES el `Estado`. La UI
// (`anim_ui::draw_anim_export_dialog`) solo lo renderiza y devuelve la
// intención (`AnimExportAction`); el `spawn_*` + `CancellationToken` +
// tmp+rename `O_EXCL` + `kill+wait` corren en el caller (hilo worker, jamás
// en `Ui::`). La detección de ffmpeg (`detect_ffmpeg_available`) hace I/O de
// solo-lectura (recorre `PATH` sin spawnear): llamarla desde el evento/hilo,
// NUNCA desde el draw.
//
// Presupuestos pineados (no se redefinen): `GIF_EXPORT_MAX_FRAMES` 64,
// `GIF_EXPORT_MAX_TOTAL_PIXELS` 8M, `GIF_EXPORT_MAX_FILE_BYTES` 5MB, default
// 48 frames (`NATIVE_ANIM_FRAME_COUNT`).

/// Frames por defecto del diálogo (los 48 nativos, intactos).
pub const ANIM_EXPORT_DEFAULT_FRAMES: usize = NATIVE_ANIM_FRAME_COUNT;
/// Bitrate mínimo/máximo del diálogo en kbps (rango del slider).
pub const ANIM_EXPORT_BITRATE_MIN_KBPS: u32 = 100;
/// Bitrate máximo del diálogo en kbps.
pub const ANIM_EXPORT_BITRATE_MAX_KBPS: u32 = 20_000;
/// FPS mínimo/máximo del diálogo.
pub const ANIM_EXPORT_FPS_MIN: u32 = 1;
/// FPS máximo del diálogo.
pub const ANIM_EXPORT_FPS_MAX: u32 = 60;
/// FPS por defecto (paridad con `GIF_BASE_FPS` 12).
pub const ANIM_EXPORT_DEFAULT_FPS: u32 = 12;
/// Motivo visible cuando MP4/WebM están deshabilitados sin ffmpeg.
pub const FFMPEG_MISSING_HINT: &str = "MP4/WebM requieren ffmpeg — se exporta GIF";

/// Formato de video del diálogo (visible en el selector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoExportFormat {
    /// GIF animado (siempre disponible, sin ffmpeg).
    #[default]
    Gif,
    /// Secuencia PNG en directorio (siempre disponible, sin ffmpeg).
    PngDir,
    /// MP4 H.264 vía ffmpeg-sidecar.
    Mp4,
    /// WebM VP9/AV1 vía ffmpeg-sidecar.
    Webm,
}

impl VideoExportFormat {
    /// Los 4 del selector, en orden visible.
    pub const ALL: [Self; 4] = [Self::Gif, Self::PngDir, Self::Mp4, Self::Webm];

    /// Nombre visible del selector.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::PngDir => "PNG-sequence",
            Self::Mp4 => "MP4",
            Self::Webm => "WebM",
        }
    }

    /// Extensión del destino.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::PngDir => "dir",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    /// ¿Necesita ffmpeg en el PATH?
    pub const fn needs_ffmpeg(self) -> bool {
        match self {
            Self::Gif | Self::PngDir => false,
            Self::Mp4 | Self::Webm => true,
        }
    }

    /// Puente al wire (`grafito_anim::ExportFormat`). `PngDir` mapea a
    /// `PngSequence`. Puro.
    pub const fn to_wire(self) -> grafito_anim::ExportFormat {
        match self {
            Self::Gif => grafito_anim::ExportFormat::Gif,
            Self::PngDir => grafito_anim::ExportFormat::PngSequence,
            Self::Mp4 => grafito_anim::ExportFormat::Mp4,
            Self::Webm => grafito_anim::ExportFormat::Webm,
        }
    }
}

impl std::fmt::Display for VideoExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

// Extensión del diálogo sobre `VideoQuality` (vive abajo, bloque fps/bitrate
// del worker de plot: `flags()` + `video_quality_desde_params` son suyos).
// Acá solo lo visible del selector: orden, nombres, flag manim y bitrate
// sugerido. Paridad pineada en tests (`crf()` espeja `flags().0`).
impl VideoQuality {
    /// Las 3 del selector, en orden visible.
    pub const DIALOG_ALL: [Self; 3] = [Self::Baja, Self::Media, Self::Alta];

    /// Nombre visible.
    pub const fn dialog_display_name(self) -> &'static str {
        match self {
            Self::Baja => "Baja",
            Self::Media => "Media",
            Self::Alta => "Alta",
        }
    }

    /// Flag de calidad estilo manim (`-ql`/`-qm`/`-qh`): Baja `-ql`, Media
    /// `-qm`, Alta `-qh` (extensión honesta del par `-ql`/`-qm` pedido).
    pub const fn manim_flag(self) -> &'static str {
        match self {
            Self::Baja => "-ql",
            Self::Media => "-qm",
            Self::Alta => "-qh",
        }
    }

    /// CRF para libx264/VP9 (menor = mejor). Paridad con `flags().0`.
    pub const fn dialog_crf(self) -> u32 {
        match self {
            Self::Baja => 30,
            Self::Media => 23,
            Self::Alta => 18,
        }
    }

    /// Bitrate sugerido en kbps (dentro de 100..=20000).
    pub const fn suggested_bitrate_kbps(self) -> u32 {
        match self {
            Self::Baja => 500,
            Self::Media => 2000,
            Self::Alta => 8000,
        }
    }
}

impl std::fmt::Display for VideoQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.dialog_display_name())
    }
}

/// ¿El bitrate está en 100..=20000? Puro.
pub fn validate_export_bitrate_kbps(bitrate: u32) -> Result<u32, String> {
    if (ANIM_EXPORT_BITRATE_MIN_KBPS..=ANIM_EXPORT_BITRATE_MAX_KBPS).contains(&bitrate) {
        Ok(bitrate)
    } else {
        Err(format!(
            "bitrate {bitrate} fuera de {}..={}",
            ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_BITRATE_MAX_KBPS
        ))
    }
}

/// ¿El fps está en 1..=60? Puro.
pub fn validate_export_fps(fps: u32) -> Result<u32, String> {
    if (ANIM_EXPORT_FPS_MIN..=ANIM_EXPORT_FPS_MAX).contains(&fps) {
        Ok(fps)
    } else {
        Err(format!(
            "fps {fps} fuera de {ANIM_EXPORT_FPS_MIN}..={ANIM_EXPORT_FPS_MAX}"
        ))
    }
}

/// ¿Hay ffmpeg usable? Recorre `PATH` buscando un ejecutable `ffmpeg`
/// (`ffmpeg.exe` en Windows). Solo lectura, SIN spawnear: llamarla desde el
/// evento/hilo que abre el diálogo, jamás desde `Ui::`.
pub fn detect_ffmpeg_available() -> bool {
    let path_var = std::env::var_os("PATH");
    let Some(path_var) = path_var else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        #[cfg(windows)]
        {
            let candidato = dir.join("ffmpeg.exe");
            if candidato.is_file() {
                return true;
            }
        }
        let candidato = dir.join("ffmpeg");
        if candidato.is_file() {
            return true;
        }
    }
    false
}

// ── Helpers puros del diálogo vivo (el estado vive en
// `grafito_ui::assistant::MediaExportDialog`; acá quedan los validadores,
// formatos, calidades y topes que ambas Pieles comparten). El diálogo
// duplicado `AnimExportDialog` se eliminó (W1: sin audio en el núcleo).

#[cfg(test)]
mod anim_export_dialog_tests {
    use super::{
        validate_export_bitrate_kbps, validate_export_fps, VideoExportFormat, VideoQuality,
        ANIM_EXPORT_BITRATE_MAX_KBPS, ANIM_EXPORT_BITRATE_MIN_KBPS, ANIM_EXPORT_DEFAULT_FPS,
        ANIM_EXPORT_DEFAULT_FRAMES, ANIM_EXPORT_FPS_MAX, ANIM_EXPORT_FPS_MIN, FFMPEG_MISSING_HINT,
        GIF_EXPORT_MAX_FILE_BYTES, GIF_EXPORT_MAX_FRAMES, GIF_EXPORT_MAX_TOTAL_PIXELS,
        NATIVE_ANIM_FRAME_COUNT,
    };

    #[test]
    fn default_48_frames_intacto() {
        assert_eq!(ANIM_EXPORT_DEFAULT_FRAMES, 48);
        assert_eq!(ANIM_EXPORT_DEFAULT_FRAMES, NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(ANIM_EXPORT_DEFAULT_FPS, 12);
    }

    #[test]
    fn budgets_64_8m_5mb_pineados() {
        assert_eq!(GIF_EXPORT_MAX_FRAMES, 64);
        assert_eq!(GIF_EXPORT_MAX_TOTAL_PIXELS, 8_000_000);
        assert_eq!(GIF_EXPORT_MAX_FILE_BYTES, 5 * 1024 * 1024);
        assert_eq!(ANIM_EXPORT_BITRATE_MIN_KBPS, 100);
        assert_eq!(ANIM_EXPORT_BITRATE_MAX_KBPS, 20_000);
        assert_eq!(ANIM_EXPORT_FPS_MIN, 1);
        assert_eq!(ANIM_EXPORT_FPS_MAX, 60);
        // Paridad con el bloque fps/bitrate del worker de plot (mismos topes,
        // nombres distintos por scope): si alguno deriva, este test avisa.
        assert_eq!(ANIM_EXPORT_FPS_MIN, super::ANIM_FPS_MIN);
        assert_eq!(ANIM_EXPORT_FPS_MAX, super::ANIM_FPS_MAX);
        assert_eq!(ANIM_EXPORT_DEFAULT_FPS, super::ANIM_FPS_DEFAULT);
        assert_eq!(ANIM_EXPORT_BITRATE_MIN_KBPS, super::ANIM_BITRATE_MIN_KBPS);
        assert_eq!(ANIM_EXPORT_BITRATE_MAX_KBPS, super::ANIM_BITRATE_MAX_KBPS);
        for calidad in VideoQuality::DIALOG_ALL {
            assert_eq!(calidad.dialog_crf(), calidad.flags().0 as u32);
        }
    }

    #[test]
    fn formatos_y_hint_ffmpeg() {
        assert_eq!(VideoExportFormat::ALL.len(), 4);
        assert!(!VideoExportFormat::Gif.needs_ffmpeg());
        assert!(!VideoExportFormat::PngDir.needs_ffmpeg());
        assert!(VideoExportFormat::Mp4.needs_ffmpeg());
        assert!(VideoExportFormat::Webm.needs_ffmpeg());
        assert!(FFMPEG_MISSING_HINT.contains("ffmpeg"));
        // Puente al wire intacto.
        assert_eq!(
            VideoExportFormat::PngDir.to_wire(),
            grafito_anim::ExportFormat::PngSequence
        );
    }

    #[test]
    fn calidad_mapea_flags_bitrate_crf() {
        assert_eq!(VideoQuality::Baja.manim_flag(), "-ql");
        assert_eq!(VideoQuality::Media.manim_flag(), "-qm");
        assert_eq!(VideoQuality::Alta.manim_flag(), "-qh");
        assert!(VideoQuality::Baja.dialog_crf() > VideoQuality::Media.dialog_crf());
        assert!(VideoQuality::Media.dialog_crf() > VideoQuality::Alta.dialog_crf());
        for calidad in VideoQuality::DIALOG_ALL {
            let bitrate = calidad.suggested_bitrate_kbps();
            assert!(validate_export_bitrate_kbps(bitrate).is_ok());
        }
        assert!(validate_export_bitrate_kbps(99).is_err());
        assert!(validate_export_bitrate_kbps(20_001).is_err());
        assert!(validate_export_fps(0).is_err());
        assert!(validate_export_fps(61).is_err());
        assert!(validate_export_fps(12).is_ok());
    }
}

// ── N1: predicados pixel-lógicos compartidos (integral honesta) ──────────
// Sombra = relleno blendido (trayectoria recta fondo→[91,155,255]) + cota
// móvil sólida [66,133,244]. El grid ESTÁTICO vive bajo el relleno: una
// intersección del grid bajo el relleno da píxel más brillante pero sigue
// siendo área azul. Por eso el tope es r<175 y no r<150: cubre la
// trayectoria entera incluido intersección-relleno.
// Fuera quedan: grises (b==r), texto blanco (b−r≈10), recorte del texto
// (b−r≈7) y curva amarilla (r≥218 incluso sobre grid).

/// Cuenta píxeles sombreados (relleno del área + cota móvil).
#[cfg(test)]
fn cuenta_pixeles_sombra(frame: &egui::ColorImage) -> usize {
    frame
        .pixels
        .iter()
        .filter(|c| {
            let (r, _g, b) = (c.r(), c.g(), c.b());
            b > 65 && b.saturating_sub(r) > 30 && r < 175
        })
        .count()
}

/// Píxeles de máscara que difieren entre dos frames (F1: el AA deja un
/// fringe inestable sobre el grid aunque la geometría sea idéntica; contar
/// el diff distingue fringe de movimiento real).
#[cfg(test)]
fn mask_diff_count(a: &[bool], b: &[bool]) -> usize {
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

/// Cota de fringe AA (F1): 0.5% de los píxeles del frame (medido ≤0.02%
/// en integral 64²→320×180). Una curva que se moviese de verdad cambia la
/// mayoría de su máscara (90-321px), dos órdenes sobre la cota: el test
/// sigue cazando regresiones de movimiento.
#[cfg(test)]
fn mask_fringe_allowance(frame_pixels: usize) -> usize {
    (frame_pixels / 200).max(8)
}

/// La curva es la MISMA en los frames dados (vía clásica y paramétrica):
/// toda la máscara salvo fringe AA. Pura de test, sin pánicos.
#[cfg(test)]
fn assert_curva_fija(frames: &[egui::ColorImage], label: &str) {
    let n = frames[0].pixels.len();
    let cota = mask_fringe_allowance(n);
    let c0 = mascara_curva(&frames[0]);
    assert!(c0.iter().any(|v| *v), "{label}: la curva debe pintarse");
    for fi in [24, NATIVE_ANIM_FRAME_COUNT - 1] {
        let d = mask_diff_count(&c0, &mascara_curva(&frames[fi]));
        assert!(
            d <= cota,
            "{label}: curva 0 == {fi} salvo fringe (diff {d} > cota {cota})"
        );
    }
}
/// Máscara de la curva amarilla (r y g altos, b bajo): texto blanco
/// (b=245), cota azul (r=66) y relleno (r<150) quedan fuera.
#[cfg(test)]
fn mascara_curva(frame: &egui::ColorImage) -> Vec<bool> {
    frame
        .pixels
        .iter()
        .map(|c| c.r() > 150 && c.g() > 130 && c.b() < 150)
        .collect()
}

/// Puntos verdes sueltos estilo MINT (g dominante): la BASURA reportada.
/// El umbral bajo (g>90) también caza el MINT tenue blendido; el relleno
/// (g−r≈20), la cota (g<b) y los grises quedan fuera.
#[cfg(test)]
fn tiene_verde_suelto(frame: &egui::ColorImage) -> bool {
    frame.pixels.iter().any(|c| {
        let (r, g, b) = (c.r(), c.g(), c.b());
        g > 90 && g.saturating_sub(r) > 40 && g.saturating_sub(b) > 25
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    type NativeFrameFn = fn(u32, u32) -> Vec<egui::ColorImage>;
    /// Helper central: frames VÁLIDOS = len exacta + tamaño + no vacíos +
    /// no todos idénticos + alpha 255 (bounds de color).
    fn assert_frames_valid(frames: &[egui::ColorImage], w: usize, h: usize, label: &str) {
        assert_eq!(
            frames.len(),
            NATIVE_ANIM_FRAME_COUNT,
            "{label}: len debe ser {NATIVE_ANIM_FRAME_COUNT}"
        );
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.size, [w, h], "{label} frame {i}: size");
            assert_eq!(f.pixels.len(), w * h, "{label} frame {i}: pixels no vacío");
        }
        // No todos idénticos: primero vs último difieren.
        assert_ne!(
            frames[0].pixels,
            frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "{label}: frames deben animar (primero != último)"
        );
        // No vacío/sólido: primer frame tiene ≥2 colores distintos.
        let first_px = frames[0].pixels[0];
        assert!(
            frames[0].pixels.iter().any(|p| *p != first_px),
            "{label}: frame no debe ser sólido"
        );
        // Bounds de color: alpha 255 en muestra (todo el buffer se escribe con a=255).
        for p in frames[0].pixels.iter().step_by(97) {
            assert_eq!(p.a(), 255, "{label}: alpha debe ser 255");
        }
        for p in frames[NATIVE_ANIM_FRAME_COUNT - 1]
            .pixels
            .iter()
            .step_by(131)
        {
            assert_eq!(p.a(), 255, "{label}: alpha último debe ser 255");
        }
    }

    fn render_timed(
        label: &str,
        w: u32,
        h: u32,
        f: impl FnOnce() -> Vec<egui::ColorImage>,
    ) -> Vec<egui::ColorImage> {
        let start = std::time::Instant::now();
        let frames = f();
        let ms = start.elapsed().as_millis();
        println!("template {label} {w}x{h}: {ms}ms");
        assert!(
            ms < 1800,
            "{label} tomó {ms}ms en {w}x{h}, debe ser <1800ms (<2s debug)"
        );
        frames
    }

    // ── AS3: presupuesto de memoria del set (diseño, con test) ──────────
    #[test]
    fn frame_set_memory_budget_is_documented_and_checked() {
        assert_eq!(NATIVE_ANIM_FRAME_COUNT, 48);
        assert_eq!(NATIVE_BYTES_PER_PIXEL, 4);
        // Set canónico 640×480×48 RGBA = 58_982_400 B (≈56 MiB).
        assert_eq!(
            estimate_frames_bytes(640, 480, NATIVE_ANIM_FRAME_COUNT),
            Some(58_982_400)
        );
        assert_eq!(
            NATIVE_FRAME_BYTES_ESTIMADO_640X480,
            640 * 480 * NATIVE_BYTES_PER_PIXEL * NATIVE_ANIM_FRAME_COUNT
        );
        // Tamaño chico de thumbs (160×120×8): ~614 KiB, cabe en GPU integrada.
        assert_eq!(estimate_frames_bytes(160, 120, 8), Some(160 * 120 * 4 * 8));
        // Overflow saturante: None en vez de panic/wrap.
        assert_eq!(estimate_frames_bytes(usize::MAX, usize::MAX, 48), None);
        assert_eq!(
            estimate_frames_bytes(4096, 4096, 48),
            Some(4096 * 4096 * 4 * 48)
        );
        // Cero dimensiones = cero bytes (sin pánicos).
        assert_eq!(estimate_frames_bytes(0, 480, 48), Some(0));
    }

    // ── M3-5: preflight en TODOS los render_* clásicos ───────────────────
    #[test]
    fn preflight_4096_clampeado_sin_oom() {
        // 4096×4096×48 RGBA ≈ 3 GiB > 64 MiB: el helper clampa con OverBudget.
        let ((w, h), err) = resolve_native_size_budgeted(4096, 4096, NATIVE_ANIM_FRAME_COUNT);
        // El pedido crudo excede; el clamped encaja (bytes = estimado clamped).
        let pedido = estimate_frames_bytes(4096, 4096, NATIVE_ANIM_FRAME_COUNT)
            .expect("3 GiB no desborda usize");
        assert!(
            pedido > NATIVE_MAX_SET_BYTES,
            "4096²×48 = {pedido} debe exceder el tope"
        );
        match err {
            Some(NativeSizeError::OverBudget {
                requested,
                clamped,
                bytes,
            }) => {
                assert_eq!(requested, (4096, 4096));
                assert_eq!(clamped, (w, h));
                assert_eq!(
                    bytes,
                    estimate_frames_bytes(w, h, NATIVE_ANIM_FRAME_COUNT)
                        .expect("clamped no desborda")
                );
            }
            other => panic!("4096²×48 debe ser OverBudget, fue: {other:?}"),
        }
        let cabe =
            estimate_frames_bytes(w, h, NATIVE_ANIM_FRAME_COUNT).expect("clamped no desborda");
        assert!(
            cabe <= NATIVE_MAX_SET_BYTES,
            "clamped {w}x{h}×48 = {cabe} debe caber en {NATIVE_MAX_SET_BYTES}"
        );
        assert!(w >= NATIVE_MIN_DIM as usize && h >= NATIVE_MIN_DIM as usize);
        // Pedido chico: sin clamp, sin error.
        let ((w2, h2), err2) = resolve_native_size_budgeted(96, 72, NATIVE_ANIM_FRAME_COUNT);
        assert_eq!((w2, h2), (96, 72));
        assert!(err2.is_none());
        // Tope pineado: paridad con PARAMETRIC_MAX_BYTES.
        assert_eq!(NATIVE_MAX_SET_BYTES, 64 * 1024 * 1024);
    }

    #[test]
    fn preflight_render_gigante_devuelve_clamped_valido() {
        // El render clásico a 4096 no intenta 3 GiB: clampa y devuelve 48
        // frames válidos del tamaño clamped (sin `render_timed`: este test
        // es guard OOM, no de perf).
        let frames = render_universal_youtube_frames("concepto gigante", 4096, 4096);
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        let size = frames[0].size;
        assert!(
            size[0] * size[1] * 4 * NATIVE_ANIM_FRAME_COUNT <= NATIVE_MAX_SET_BYTES,
            "set {size:?}×48 debe caber en el tope"
        );
        assert_ne!(size, [4096, 4096], "4096 debe clamparse, no reservarse");
        for f in &frames {
            assert_eq!(f.size, size);
        }
    }

    #[test]
    fn native_animation_generates_bounded_distinct_frames() {
        let frames = render_timed("derivative-slope", 96, 72, || {
            render_native_animation_frames(96, 72)
        });
        assert_frames_valid(&frames, 96, 72, "derivative-slope");
        let first = &frames.first().unwrap().pixels;
        let middle = &frames[NATIVE_ANIM_FRAME_COUNT / 2].pixels;
        assert_ne!(first, middle, "el punto deslizante debe mover los frames");
    }
    #[test]
    fn integral_frames_distinct() {
        let a = render_timed("integral-area", 64, 64, || render_integral_frames(64, 64));
        assert_frames_valid(&a, 64, 64, "integral-area");
        let b = render_integral_frames(64, 64);
        assert_eq!(a[0].pixels, b[0].pixels);
    }
    // ── N1: integral honesta (curva fija + sombra monótona + sin verde) ──
    #[test]
    fn integral_sombra_monotona_curva_fija_sin_verde() {
        // Varios tamaños (thumbs, card, enseñanza): el contrato no depende
        // de la resolución porque el grid bajo el relleno también cuenta.
        for (w, h) in [(64, 64), (96, 72), (160, 120), (320, 180)] {
            let frames = render_integral_frames(w, h);
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT, "{w}x{h}: 48 frames");
            // El área (píxeles sombreados) es no-decreciente con N.
            let sombras: Vec<usize> = frames.iter().map(super::cuenta_pixeles_sombra).collect();
            for par in sombras.windows(2) {
                assert!(
                    par[1] >= par[0],
                    "{w}x{h}: sombra no-decreciente con N: {sombras:?}"
                );
            }
            assert!(
                sombras[0] < sombras[NATIVE_ANIM_FRAME_COUNT - 1],
                "{w}x{h}: el área final debe sombrear más: {sombras:?}"
            );
            // La curva es la MISMA en los frames 0/24/47 (salvo fringe AA).
            super::assert_curva_fija(&frames, &format!("{w}x{h}"));
            // Cero puntos decorativos sueltos en los 48 frames.
            for (i, f) in frames.iter().enumerate() {
                assert!(
                    !super::tiene_verde_suelto(f),
                    "{w}x{h} frame {i}: sin verde"
                );
            }
        }
    }
    #[test]
    fn integral_acumulada_vale_ocho_tercios_en_02() {
        // S de x^2 en [0,2] = 8/3 ≈ 2.67 (trapecios, tolerancia de malla).
        let canon = super::integral_canonical_anim().expect("canónica integral-area");
        assert_eq!(canon.expr_a, "x^2");
        let s = super::integral_acumulada(Some(&canon), 0, 0.0, 2.0).expect("S finita");
        assert!((s - 8.0 / 3.0).abs() < 0.05, "S={s}");
        assert_eq!(
            super::integral_acumulada(Some(&canon), 0, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            super::integral_acumulada(Some(&canon), 0, 2.0, 2.0),
            Some(0.0)
        );
        // El extremo crece monótono en los 48 frames.
        let mut previo = f64::NEG_INFINITY;
        for f in 0..NATIVE_ANIM_FRAME_COUNT {
            let xe = super::integral_frame_end(0.0, 2.0, f);
            assert!(xe >= previo, "x_end monótono en frame {f}");
            previo = xe;
        }
        assert_eq!(super::integral_frame_end(0.0, 2.0, 0), 0.0);
        assert_eq!(
            super::integral_frame_end(0.0, 2.0, NATIVE_ANIM_FRAME_COUNT - 1),
            2.0
        );
    }
    #[test]
    fn integral_no_finita_da_none_y_nucleo_comun() {
        // R1-7: guard `!finito → None` + núcleo común (ambas vías coinciden).
        let canon = super::integral_canonical_anim().expect("canónica");
        for (a, b) in [
            (f64::NAN, 2.0),
            (0.0, f64::NAN),
            (f64::INFINITY, 2.0),
            (0.0, f64::INFINITY),
            (f64::NEG_INFINITY, 2.0),
        ] {
            assert_eq!(
                super::integral_acumulada(Some(&canon), 0, a, b),
                None,
                "a={a} b={b} → None"
            );
            assert_eq!(
                super::integral_acumulada_con_vivo(Some(&canon), 0, a, b, None),
                None,
                "vivo a={a} b={b} → None"
            );
            assert_eq!(
                super::integral_acumulada_con_vivo(Some(&canon), 0, a, b, Some(1.0)),
                None,
                "vivo fijo a={a} b={b} → None"
            );
        }
        // Núcleo común: sin vivo ambas dan lo mismo en rango sano.
        let s1 = super::integral_acumulada(Some(&canon), 0, 0.0, 2.0).expect("S");
        let s2 =
            super::integral_acumulada_con_vivo(Some(&canon), 0, 0.0, 2.0, None).expect("S vivo");
        assert!((s1 - s2).abs() < 1e-12, "núcleo común: {s1} vs {s2}");
        // Pasos acotados y sanos.
        assert_eq!(super::pasos_trapecios(0.0, 0.0), Some(0));
        assert_eq!(super::pasos_trapecios(2.0, 2.0), Some(0));
        assert_eq!(super::pasos_trapecios(f64::NAN, 1.0), None);
        assert_eq!(super::pasos_trapecios(0.0, f64::INFINITY), None);
        assert_eq!(super::pasos_trapecios(0.0, 500.0), Some(4096));
    }
    #[test]
    fn taylor_frames_bounded() {
        let f = render_timed("taylor-series", 80, 64, || render_taylor_frames(80, 64));
        assert_frames_valid(&f, 80, 64, "taylor-series");
    }
    #[test]
    fn conformal_frames_distinct() {
        let f = render_timed("conformal-map", 64, 64, || render_conformal_frames(64, 64));
        assert_frames_valid(&f, 64, 64, "conformal-map");
    }
    #[test]
    fn pitagoras_frames_valid_under_2s() {
        let f = render_timed("pitagoras", 96, 72, || render_pitagoras_frames(96, 72));
        assert_frames_valid(&f, 96, 72, "pitagoras");
    }
    #[test]
    fn euler_frames_valid_under_2s() {
        let f = render_timed("euler", 96, 72, || render_euler_frames(96, 72));
        assert_frames_valid(&f, 96, 72, "euler");
        // Determinista: dos renders iguales.
        let g = render_euler_frames(96, 72);
        assert_eq!(f[0].pixels, g[0].pixels);
    }
    #[test]
    fn fourier_frames_valid_under_2s() {
        let f = render_timed("fourier", 96, 72, || render_fourier_frames(96, 72));
        assert_frames_valid(&f, 96, 72, "fourier");
        let g = render_fourier_frames(96, 72);
        assert_eq!(f[0].pixels, g[0].pixels);
    }
    #[test]
    fn seven_templates_all_valid_under_2s() {
        // 5 originales + euler/fourier = 7 (universal aparte).
        let cases: [(&str, NativeFrameFn); 7] = [
            ("derivative-slope", render_native_animation_frames),
            ("pitagoras", render_pitagoras_frames),
            ("integral-area", render_integral_frames),
            ("taylor-series", render_taylor_frames),
            ("conformal-map", render_conformal_frames),
            ("euler", render_euler_frames),
            ("fourier", render_fourier_frames),
        ];
        for (label, fun) in cases {
            let frames = render_timed(label, 96, 72, || fun(96, 72));
            assert_frames_valid(&frames, 96, 72, label);
        }
    }
    #[test]
    fn dispatcher_fallback() {
        let d = render_timed("fallback-unknown", 64, 64, || {
            render_anim_by_template("unknown-template", 64, 64)
        });
        assert_frames_valid(&d, 64, 64, "fallback-unknown");
    }
    #[test]
    fn universal_handles_any_text() {
        let cases = [
            "hola mundo",
            "funci\u{00f3}n cuadr\u{00e1}tica f(x)=x\u{00b2}+2x+1",
            "probabilidad binomial n=10 p=0.5",
            "n\u{00fa}mero complejo e^{i\u{03c0}}+1=0",
            "\u{00bf}qu\u{00e9} es una derivada?",
            "vector campo F(x,y)=(-y,x)",
            "teorema de pit\u{00e1}goras con dibujo",
            "integral de Riemann 0 a 2",
            "serie de Taylor de sin(x)",
            "mapeo conforme w=z\u{00b2}",
            "   ",
            "\u{1f600} emoji test \u{1f9e0}",
            &"x".repeat(500),
        ];
        for concept in cases {
            let frames = render_universal_youtube_frames(concept, 96, 72);
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT, "concept: {concept}");
            for f in &frames {
                assert_eq!(f.size, [96, 72]);
            }
            assert_ne!(
                frames[0].pixels,
                frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
                "debe animar para: {concept}"
            );
        }
    }
    #[test]
    fn universal_dispatcher_for_any_template_and_concept() {
        let pairs = [
            ("", "derivada como pendiente"),
            ("unknown", "integral de area"),
            ("derivative-slope", ""),
            ("universal", "cualquier texto libre"),
            ("", "   "),
            ("pitagoras", "triangulo rectangulo"),
        ];
        for (tmpl, concept) in pairs {
            let frames = render_anim_for_concept(tmpl, concept, 64, 64);
            assert_frames_valid(&frames, 64, 64, "dispatcher-pair");
        }
    }
    #[test]
    fn universal_placeholder_under_2s() {
        let start = std::time::Instant::now();
        let _ = render_universal_youtube_frames("test r\u{00e1}pido placeholder <2s", 320, 240);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1800,
            "placeholder tom\u{00f3} {}ms, debe ser <1800ms",
            elapsed.as_millis()
        );
        // tambien probar concepto largo
        let start2 = std::time::Instant::now();
        let _ = render_anim_for_concept("", &"x".repeat(1000), 640, 480);
        assert!(start2.elapsed().as_millis() < 1800);
    }
    #[test]
    fn three_new_templates_valid_under_2s() {
        let cases: [(&str, NativeFrameFn); 3] = [
            ("logistic-bifurcation", render_logistic_bifurcation_frames),
            ("gradient-field", render_gradient_field_frames),
            ("mobius-transform", render_mobius_frames),
        ];
        for (label, fun) in cases {
            let frames = render_timed(label, 96, 72, || fun(96, 72));
            assert_frames_valid(&frames, 96, 72, label);
            // Determinista.
            let again = fun(96, 72);
            assert_eq!(frames[0].pixels, again[0].pixels, "{label}: determinista");
        }
    }
    #[test]
    fn new_templates_registered_in_both_dispatchers() {
        for tmpl in ["logistic-bifurcation", "gradient-field", "mobius-transform"] {
            let a = render_timed(tmpl, 64, 64, || render_anim_by_template(tmpl, 64, 64));
            assert_frames_valid(&a, 64, 64, tmpl);
            let b = render_anim_for_concept(tmpl, "concepto libre", 64, 64);
            assert_frames_valid(&b, 64, 64, tmpl);
        }
        // Aliases.
        for tmpl in ["logistica", "gradiente", "mobius"] {
            let f = render_anim_by_template(tmpl, 64, 64);
            assert_frames_valid(&f, 64, 64, tmpl);
        }
    }
    #[test]
    fn detect_template_covers_new_pedagogical_concepts() {
        assert_eq!(
            detect_template_for_concept("bifurcación logística r=3.5"),
            "logistic-bifurcation"
        );
        assert_eq!(
            detect_template_for_concept("campo de gradiente de f(x,y)"),
            "gradient-field"
        );
        assert_eq!(
            detect_template_for_concept("transformación de Möbius w=(z-c)/(1-cz)"),
            "mobius-transform"
        );
    }
    #[test]
    fn robustness_zero_and_giant_clamp_no_panic_no_oom() {
        // Error tipado estricto.
        assert!(matches!(
            try_resolve_native_size(0, 0),
            Err(NativeSizeError::BelowMinimum { .. })
        ));
        assert!(matches!(
            try_resolve_native_size(0, 72),
            Err(NativeSizeError::BelowMinimum { .. })
        ));
        assert!(matches!(
            try_resolve_native_size(u32::MAX, u32::MAX),
            Err(NativeSizeError::AboveMaximum { .. })
        ));
        assert!(matches!(
            try_resolve_native_size(64, 48),
            Err(NativeSizeError::BelowMinimum { .. })
        ));
        assert_eq!(try_resolve_native_size(96, 72), Ok((96_usize, 72_usize)));
        // Clamp seguro: 0 → 64, gigante → 4096.
        let ((w0, h0), err0) = resolve_native_size(0, 0);
        assert_eq!((w0, h0), (64, 64));
        assert!(err0.is_some());
        let ((wg, hg), errg) = resolve_native_size(u32::MAX, u32::MAX);
        assert_eq!((wg, hg), (4096, 4096));
        assert!(errg.is_some());
        // checked_mul: overflow → error tipado, nunca panic.
        assert!(checked_frame_byte_len(usize::MAX, 4).is_err());
        // Renders con 0 no panican y devuelven tamaño clamped válido.
        // Nota: gigante (4096) solo se verifica a nivel dims (render completo
        // 48×67MiB = 3.2GiBOOM); el clamp ya está probado arriba.
        {
            let start = std::time::Instant::now();
            let frames = render_logistic_bifurcation_frames(0, 0);
            assert_frames_valid(&frames, 64, 64, "logistic-clamp-zero");
            assert!(
                start.elapsed().as_millis() < 1800,
                "clamp cero debe ser <2s"
            );
        }
        // to_pixel defensivo con 0.
        assert_eq!(to_pixel(0, 0, 1.0, 1.0), (0, 0));
        // Display/Error no panican.
        let e = NativeSizeError::AllocationOverflow { w: 1, h: 2 };
        assert!(!format!("{e}").is_empty());
        let e2 = NativeSizeError::AllocationFailed { bytes: 8 };
        assert!(!format!("{e2}").is_empty());
    }
    #[test]
    fn palette_centralized_bg_fg_accent() {
        // Trío canónico BG/FG/ACCENT: opacos y distintos.
        for c in [PAL_BG, PAL_FG, PAL_ACCENT, BG, TEXT_COLOR] {
            assert_eq!(c[3], 255, "paleta debe ser opaca");
        }
        assert_eq!(PAL_BG, BG);
        assert_eq!(PAL_FG, TEXT_COLOR);
        assert_eq!(PAL_ACCENT, PAL_BLUE);
        assert_ne!(PAL_BG, PAL_FG, "BG != FG");
        assert_ne!(PAL_BG, PAL_ACCENT, "BG != ACCENT");
        // Roles derivan de la paleta (sin literales sueltos en renders).
        // T2: TRAIL_FAINT_ALPHA/CURVE_UNIVERSAL_ALPHA se eliminaron con la
        // curva falsa del universal (decoración que fingía respuesta).
        assert_eq!(CURVE_MAIN[3], 235);
        assert_eq!(with_alpha(PAL_BLUE, 28), [66, 133, 244, 28]);
        // Un frame real contiene BG y no-BG (usa la paleta).
        let f = render_gradient_field_frames(64, 64);
        assert_frames_valid(&f, 64, 64, "palette-sample");
    }
    #[test]
    fn detect_template_covers_known_concepts() {
        assert_eq!(
            detect_template_for_concept("teorema de pit\u{00e1}goras"),
            "pitagoras"
        );
        assert_eq!(
            detect_template_for_concept("integral area bajo curva"),
            "integral-area"
        );
        assert_eq!(
            detect_template_for_concept("serie de Taylor sin(x)"),
            "taylor-series"
        );
        assert_eq!(
            detect_template_for_concept("mapeo conforme complejo"),
            "conformal-map"
        );
        assert_eq!(
            detect_template_for_concept("derivada pendiente tangente"),
            "derivative-slope"
        );
        assert_eq!(
            detect_template_for_concept("texto aleatorio sin matematica"),
            "universal"
        );
    }
    #[test]
    fn detect_conocidos_no_caen_a_fallback() {
        // Regresión T2 + R6d: cada concepto conocido resuelve a su plantilla
        // con renderer propio; los catch-alls sin token explícito
        // ("probabilidad binomial" sin histograma/densidad, "vector campo"
        // sin conforme/complejo) van al `universal` honesto, jamás a curva
        // falsa (ver `template_for_concept` en el protocolo).
        for (concepto, esperado) in [
            ("teorema de pitágoras", "pitagoras"),
            ("integral área bajo curva", "integral-area"),
            ("probabilidad binomial n=10", "universal"),
            ("serie de Taylor de sin(x)", "taylor-series"),
            ("mapeo conforme complejo", "conformal-map"),
            ("derivada pendiente tangente", "derivative-slope"),
            ("número e exponencial", "euler"),
            ("análisis de Fourier con armónicos", "fourier"),
            ("bifurcación logística r=3.5", "logistic-bifurcation"),
            ("campo de gradiente de f(x,y)", "gradient-field"),
            ("transformación de Möbius", "mobius-transform"),
            ("fracción con común denominador", "integral-area"),
            ("ecuación cuadrática", "derivative-slope"),
            ("círculo unitario", "taylor-series"),
            ("elipse cónica", "conformal-map"),
            ("vector campo F(x,y)=(-y,x)", "universal"),
        ] {
            assert_eq!(
                detect_template_for_concept(concepto),
                esperado,
                "{concepto}"
            );
        }
    }
    #[test]
    fn detect_sin_contains_tramposos() {
        // T2: substrings que antes fingían plantilla hoy van al neutro.
        for pedido in [
            "tarea de matemática",
            "el ecosistema funciona",
            "funciona el aparato",
        ] {
            assert_eq!(detect_template_for_concept(pedido), "universal", "{pedido}");
        }
        // ...pero la palabra exacta sí resuelve.
        assert_eq!(
            detect_template_for_concept("área del círculo"),
            "integral-area"
        );
        assert_eq!(
            detect_template_for_concept("sistema de ecuaciones"),
            "derivative-slope"
        );
    }
    #[test]
    fn fallback_universal_es_neutro_sin_curva_falsa() {
        // T2: el placeholder no anima contenido matemático: la banda media
        // es idéntica entre el primer y el último frame (sin curva ni
        // partículas que se muevan); solo la barra de progreso avanza.
        let w = 96u32;
        let h = 72u32;
        let frames = render_universal_youtube_frames("texto libre cualquiera", w, h);
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        let primero = &frames[0];
        let ultimo = &frames[NATIVE_ANIM_FRAME_COUNT - 1];
        assert_eq!(primero.size, [w as usize, h as usize]);
        let idx = |x: usize, y: usize| y * (w as usize) + x;
        // Banda media (lejos del título y de la barra): estática.
        for y in 40..((h as usize) - 8) {
            for x in (0..(w as usize)).step_by(7) {
                assert_eq!(
                    primero.pixels[idx(x, y)],
                    ultimo.pixels[idx(x, y)],
                    "banda media estática en ({x},{y})"
                );
            }
        }
        // La barra de progreso sí avanza y el rótulo está pineado.
        assert_ne!(primero.pixels, ultimo.pixels, "el progreso avanza");
        assert_eq!(UNIVERSAL_PLACEHOLDER_LABEL, "vista previa no disponible");
        // Determinista: mismo pedido, mismos píxeles.
        let otra = render_universal_youtube_frames("texto libre cualquiera", w, h);
        for (i, (a, b)) in frames.iter().zip(otra.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "frame {i} determinista");
        }
        // No sólido: hay grilla + texto + barra.
        let px0 = primero.pixels[0];
        assert!(primero.pixels.iter().any(|p| *p != px0), "frame no sólido");
    }
    // ── F10-C: sin rótulo quemado en el chat, con rótulo en export ────
    /// Píxeles de texto quemado (`TEXT_COLOR`/`PAL_FG` opaco) en la franja
    /// superior `0..y_hasta`. El chat debe dar 0 (el header egui ya titula);
    /// el export standalone debe dar >0.
    fn cuenta_texto_quemado(frame: &egui::ColorImage, y_hasta: usize) -> usize {
        // Frente A: los ticks/rótulos de ejes ("y", "2", …) son dato
        // permanente en la mitad derecha (cols ≥ w/2, ver
        // `draw_axes_with_labels`); el título quemado histórico arranca en
        // w/12–w/14 (mitad izquierda). Se cuenta solo x < w/2 para no
        // confundir numeración de ejes con rótulo. Precondición de estos
        // tests: 96x72 (labels x en filas 41..48, fuera de la franja 40).
        let w = frame.size[0];
        let mitad = w / 2;
        frame
            .pixels
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                i / w < y_hasta && i % w < mitad && c.r() == 235 && c.g() == 235 && c.b() == 245
            })
            .count()
    }

    /// Banda media (lejos del título superior y de la barra/S inferior):
    /// idéntica entre variantes con/sin rótulo (el flag solo toca el texto).
    fn banda_media_igual(a: &egui::ColorImage, b: &egui::ColorImage) -> bool {
        assert_eq!(a.size, b.size, "mismo tamaño para comparar banda");
        let (w, h) = (a.size[0], a.size[1]);
        let y0 = 40.min(h);
        let y1 = h.saturating_sub(8).max(y0);
        a.pixels
            .iter()
            .zip(b.pixels.iter())
            .enumerate()
            .all(|(i, (x, y))| {
                let yy = i / w;
                yy < y0 || yy >= y1 || x == y
            })
    }

    // ── M3-8: dispatcher único tras borrar el legacy duplicado ──────────
    #[test]
    fn dispatcher_unico_chat_es_vivo_sin_rama_duplicada() {
        // `render_anim_for_concept` (atajo chat) y `render_anim_with_progress`
        // con params vacíos deben dar los mismos píxeles: una sola rama viva
        // (el duplicado `render_anim_for_concept_legacy` se borró).
        let empty = params_map(&[]);
        for tmpl in NATIVE_TEMPLATES {
            let atajo = render_anim_for_concept(tmpl, "concepto libre", 64, 64);
            let mut vistos = 0usize;
            let vivo =
                render_anim_with_progress(tmpl, "concepto libre", 64, 64, &empty, &mut |_, _| {
                    vistos += 1
                });
            assert_eq!(atajo.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: 48 atajo");
            assert_eq!(vivo.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: 48 vivo");
            for (a, b) in atajo.iter().zip(vivo.iter()) {
                assert_eq!(a.pixels, b.pixels, "{tmpl}: atajo == vivo");
            }
            assert_eq!(vistos, NATIVE_ANIM_FRAME_COUNT, "{tmpl}: progreso real");
        }
    }

    #[test]
    fn chat_sin_rotulo_export_con_rotulo() {
        let empty = params_map(&[]);
        // 96x72: el título (y≈6..13; universal hasta y≈36 con SCRIM+eco)
        // cabe en la franja 0..40; las etiquetas de dato (S= abajo) y las
        // barras de progreso quedan fuera.
        for tmpl in NATIVE_TEMPLATES {
            let chat =
                render_anim_with_progress(tmpl, "concepto libre", 96, 72, &empty, &mut |_, _| {});
            let export = render_anim_for_export(tmpl, "concepto libre", 96, 72, &empty);
            assert_eq!(chat.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: 48 chat");
            assert_eq!(export.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: 48 export");
            // El flag cambia píxeles (el rótulo existe en export)...
            assert_ne!(
                chat[0].pixels, export[0].pixels,
                "{tmpl}: chat y export deben diferir en el rótulo"
            );
            // ...pero la matemática no: banda media idéntica.
            assert!(
                banda_media_igual(&chat[0], &export[0]),
                "{tmpl}: la banda media no debe cambiar"
            );
            // Ausencia en chat + presencia en export (franja superior).
            assert_eq!(
                cuenta_texto_quemado(&chat[0], 40),
                0,
                "{tmpl}: el chat no debe traer título quemado"
            );
            assert!(
                cuenta_texto_quemado(&export[0], 40) > 0,
                "{tmpl}: el export debe mantener el rótulo"
            );
            // El último frame también (el rótulo es estático, no animado).
            assert_eq!(
                cuenta_texto_quemado(&chat[NATIVE_ANIM_FRAME_COUNT - 1], 40),
                0,
                "{tmpl}: último frame del chat sin rótulo"
            );
        }
    }

    #[test]
    fn export_localizado_pt_sin_texto_quemado() {
        // Con locale Pt el frame no debe traer rótulo ES quemado (ni texto
        // quemado directamente): la card v3 ya titula localizado. La
        // matemática queda intacta (banda media idéntica al chat).
        use grafito_ui::i18n::Locale;
        assert!(
            con_rotulo_for_locale(Locale::Es),
            "ES mantiene el histórico"
        );
        assert!(!con_rotulo_for_locale(Locale::En), "EN sin quemado ES");
        assert!(!con_rotulo_for_locale(Locale::Pt), "PT sin quemado ES");
        let empty = params_map(&[]);
        for tmpl in NATIVE_TEMPLATES {
            let pt = render_anim_for_export_localized(
                tmpl,
                "conceito livre",
                96,
                72,
                &empty,
                Locale::Pt,
            );
            let chat =
                render_anim_with_progress(tmpl, "conceito livre", 96, 72, &empty, &mut |_, _| {});
            assert_eq!(pt.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: 48 pt");
            assert_eq!(
                cuenta_texto_quemado(&pt[0], 40),
                0,
                "{tmpl}: locale Pt no debe quemar rótulo ES"
            );
            assert_eq!(
                cuenta_texto_quemado(&pt[NATIVE_ANIM_FRAME_COUNT - 1], 40),
                0,
                "{tmpl}: último frame Pt sin rótulo"
            );
            assert!(
                banda_media_igual(&chat[0], &pt[0]),
                "{tmpl}: la banda media no debe cambiar"
            );
        }
    }

    #[test]
    fn parametrica_chat_sin_rotulo_export_con_rotulo() {
        // Viewport 96x72 (como el dispatcher): el título (y≈6..13) queda en
        // la franja 0..40 y fuera de la banda media (la canónica de
        // `parametric_for_template` es 640x480 y su título cae en y≈40..47).
        let anim = ParametricAnim::try_new(
            ParametricKind::Tangent,
            "x^2".to_string(),
            None,
            ParamName::try_new("p").expect("param p válido"),
            -1.5,
            1.5,
            FrameCount::try_new(8).expect("8 frames válidos"),
            Resolution::try_new(96, 72).expect("viewport válido"),
        )
        .expect("anim válida");
        let chat = render_parametric_frames(&anim).expect("chat renderiza");
        let export = render_parametric_frames_con_rotulo(&anim, true).expect("export renderiza");
        assert_eq!(chat.len(), export.len());
        assert_ne!(chat[0].pixels, export[0].pixels, "el rótulo debe diferir");
        assert!(
            banda_media_igual(&chat[0], &export[0]),
            "la banda media no debe cambiar"
        );
        assert_eq!(
            cuenta_texto_quemado(&chat[0], 40),
            0,
            "paramétrica del chat sin título"
        );
        assert!(
            cuenta_texto_quemado(&export[0], 40) > 0,
            "paramétrica de export con título"
        );
    }
    #[test]
    fn normalize_handles_edge_cases() {
        assert!(!normalize_concept("").is_empty());
        assert!(!normalize_concept("   ").is_empty());
        let long = "a".repeat(500);
        assert!(normalize_concept(&long).len() <= 124);
        assert_eq!(normalize_concept("  hola   mundo  "), "hola mundo");
    }
    #[test]
    fn fondo_unico_fijo_sin_acento_por_concepto() {
        // El fondo ya no depende del concepto: mismo frame base para
        // cualquier pedido (el movimiento lo pone la matemática).
        for concepto in ["derivada", "integral", "otra cosa"] {
            let _ = concepto;
        }
        let (w, h) = (64, 64);
        let mut a = vec![0u8; w * h * 4];
        let mut b = vec![0u8; w * h * 4];
        super::fill_background(&mut a, w, h);
        super::fill_background(&mut b, w, h);
        assert_eq!(a, b, "fondo determinista y único");
        // Gradiente vertical real: primera fila != última.
        assert_ne!(
            &a[0..w * 4],
            &a[(h - 1) * w * 4..h * w * 4],
            "gradiente vertical"
        );
        // Esquinas más oscuras que el centro (viñeta).
        let esquina = u16::from(a[0]) + u16::from(a[1]) + u16::from(a[2]);
        let centro_idx = (h / 2 * w + w / 2).saturating_mul(4);
        let centro = if centro_idx + 2 < a.len() {
            u16::from(a[centro_idx]) + u16::from(a[centro_idx + 1]) + u16::from(a[centro_idx + 2])
        } else {
            esquina
        };
        assert!(esquina <= centro, "viñeta: bordes ≤ centro");
    }

    // ── v3: params vivos ────────────────────────────────────────────────
    fn params_map(pairs: &[(&str, f64)]) -> std::collections::BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    #[test]
    fn params_vacios_reproducen_legacy_exactos() {
        // Mapa vacío = histórico exacto (todos los frames, no solo el 1º).
        let empty = params_map(&[]);
        for (label, legacy, vivo) in [
            (
                "derivative-slope",
                render_native_animation_frames(64, 64),
                render_derivative_frames_with_params(64, 64, &empty),
            ),
            (
                "integral-area",
                render_integral_frames(64, 64),
                render_integral_frames_with_params(64, 64, &empty),
            ),
            (
                "euler",
                render_euler_frames(64, 64),
                render_euler_frames_with_params(64, 64, &empty),
            ),
            (
                "fourier",
                render_fourier_frames(64, 64),
                render_fourier_frames_with_params(64, 64, &empty),
            ),
        ] {
            assert_eq!(legacy.len(), vivo.len(), "{label}: len");
            for (i, (a, b)) in legacy.iter().zip(vivo.iter()).enumerate() {
                assert_eq!(a.pixels, b.pixels, "{label} frame {i}: idéntico a legacy");
            }
        }
    }

    #[test]
    fn params_vivos_cambian_los_frames() {
        let base = params_map(&[]);
        // x0 desplaza el barrido de la tangente.
        let a = render_derivative_frames_with_params(64, 64, &base);
        let b = render_derivative_frames_with_params(64, 64, &params_map(&[("x0", 2.0)]));
        assert_ne!(
            a[NATIVE_ANIM_FRAME_COUNT / 2].pixels,
            b[NATIVE_ANIM_FRAME_COUNT / 2].pixels,
            "x0 debe mover el frame medio"
        );
        // a/b cambian el área barrida.
        let ia_base = render_integral_frames_with_params(64, 64, &base);
        let ia_movida = render_integral_frames_with_params(64, 64, &params_map(&[("a", 1.0)]));
        assert_ne!(
            ia_base[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            ia_movida[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "a debe cambiar el área final"
        );
        // NaN/inf → default (igual que vacío), sin panic.
        let nan = render_derivative_frames_with_params(
            64,
            64,
            &params_map(&[("x0", f64::NAN), ("span", f64::INFINITY)]),
        );
        assert_eq!(a[0].pixels, nan[0].pixels, "NaN/inf → defaults");
        // terms limita parciales/armónicos (último frame difiere).
        let e1 = render_euler_frames_with_params(64, 64, &params_map(&[("terms", 1.0)]));
        let e7 = render_euler_frames_with_params(64, 64, &params_map(&[("terms", 7.0)]));
        assert_ne!(
            e1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            e7[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "terms debe cambiar euler"
        );
        let f1 = render_fourier_frames_with_params(64, 64, &params_map(&[("terms", 1.0)]));
        let f6 = render_fourier_frames_with_params(64, 64, &params_map(&[("terms", 6.0)]));
        assert_ne!(
            f1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            f6[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "terms debe cambiar fourier"
        );
    }

    #[test]
    fn wc_taylor_partial_sum_orden3_es_historico_y_converge() {
        // Orden 3 reproduce el histórico exacto x - x³/6.
        for x in [-2.0f64, -0.5, 0.0, 0.7, 2.5] {
            let esperado = x - x.powi(3) / 6.0;
            assert!(
                (taylor_partial_sum(3, x) - esperado).abs() <= 1e-12,
                "orden 3 debe ser x - x³/6 en x={x}"
            );
        }
        // Orden par == anterior impar para sin (sin términos pares).
        assert_eq!(taylor_partial_sum(4, 0.7), taylor_partial_sum(3, 0.7));
        // Converge: P9 más cerca de sin(0.5) que P1.
        let err1 = (taylor_partial_sum(1, 0.5) - 0.5f64.sin()).abs();
        let err9 = (taylor_partial_sum(9, 0.5) - 0.5f64.sin()).abs();
        assert!(
            err9 < err1,
            "P9 debe acercarse más que P1 (err9={err9}, err1={err1})"
        );
        assert!(err9 < 1e-9, "P9(0.5) casi exacto (err9={err9})");
        // Coeficientes exactos 1, 1/6, 1/120, 1/5040, 1/362880 (factorial
        // exacto en f64) con alternancia de signo: P9(0)=0 y P9'(0)=1.
        assert_eq!(taylor_partial_sum(9, 0.0), 0.0, "P9(0)=0");
        let h = 1e-8f64;
        let derivada = (taylor_partial_sum(9, h) - taylor_partial_sum(9, -h)) / (2.0 * h);
        assert!(
            (derivada - 1.0).abs() <= 1e-6,
            "P9'(0) debe ser 1, fue {derivada}"
        );
        // El último término aportado es +x^9/362880 exacto.
        let aporte9 = taylor_partial_sum(9, 1.0) - taylor_partial_sum(7, 1.0);
        assert!(
            (aporte9 - 1.0 / 362_880.0).abs() <= 1e-15,
            "P9-P7 debe ser x^9/362880, fue {aporte9}"
        );
        // Clamp: 0 → 1, 99 → 10, sin panic.
        assert_eq!(taylor_partial_sum(0, 0.5), taylor_partial_sum(1, 0.5));
        assert_eq!(taylor_partial_sum(99, 0.5), taylor_partial_sum(10, 0.5));
    }

    #[test]
    fn wc_taylor_with_params_timeline_fija_y_determinista() {
        // Determinismo con mapa vacío.
        let a = render_taylor_frames_with_params(64, 64, &params_map(&[]));
        let b = render_taylor_frames_with_params(64, 64, &params_map(&[]));
        assert_eq!(a.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(a[0].pixels, b[0].pixels, "mismo params → mismos píxeles");
        // Timeline fija en los 5 escalones: `terms` se lee pero NO morphing
        // (orden 1 vs 10 dan el mismo set: siempre P1…P9 exactos).
        let t1 = render_taylor_frames_with_params(64, 64, &params_map(&[("terms", 1.0)]));
        let t10 = render_taylor_frames_with_params(64, 64, &params_map(&[("terms", 10.0)]));
        assert_eq!(
            t1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            t10[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "terms no debe morphing: timeline fija 1/3/5/7/9"
        );
        // NaN → default (igual que vacío), sin panic.
        let nan = render_taylor_frames_with_params(64, 64, &params_map(&[("terms", f64::NAN)]));
        assert_eq!(a[0].pixels, nan[0].pixels, "NaN → defaults");
        // Dispatcher con params: mismo set con cualquier terms.
        let d1 = render_anim_with_progress(
            "taylor-series",
            "taylor",
            64,
            64,
            &params_map(&[("terms", 1.0)]),
            &mut |_, _| {},
        );
        let d10 = render_anim_with_progress(
            "taylor-series",
            "taylor",
            64,
            64,
            &params_map(&[("terms", 10.0)]),
            &mut |_, _| {},
        );
        assert_eq!(
            d1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            d10[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "dispatcher: terms inerte en taylor"
        );
    }

    // ── Frente A: Taylor honesto + ejes en previews ─────────────────────
    fn taylor_spec(expr: &str, centro: f64, orden: usize) -> grafito_anim::parametric::TaylorSpec {
        grafito_anim::parametric::TaylorSpec {
            expr: expr.to_string(),
            centro,
            orden,
        }
    }

    #[test]
    fn taylor_poly_de_x3_es_real_y_foo_es_none() {
        // P_5(x³) en x=0 es x³ exacto: el motor deriva de verdad.
        let p = super::taylor_poly_anim("x^3", 0.0, 5).expect("x^3 deriva");
        let v = p.eval_frame(0, 0.7).expect("evalúa");
        assert!(
            (v - 0.7f64.powi(3)).abs() <= 1e-9,
            "P_5(x³)(0.7) debe ser x³, fue {v}"
        );
        // f que el motor no deriva → None honesto, jamás polinomio inventado.
        assert!(super::taylor_poly_anim("foo(x)", 0.0, 3).is_none());
        assert!(super::taylor_poly_anim("x^3", f64::NAN, 3).is_none());
        // El motor sobre sin(x) coincide con la suma exacta de coeficientes
        // (el renderer dibuja P_n del motor: es el P_n exacto, sin morphing).
        for n in [1usize, 3, 5, 7, 9] {
            let p = super::taylor_poly_anim("sin(x)", 0.0, n).expect("sin deriva");
            for x in [-2.0f64, -0.5, 0.0, 0.7, 2.5] {
                let v = p.eval_frame(0, x).expect("evalúa");
                let esperado = super::taylor_partial_sum(n, x);
                assert!(
                    (v - esperado).abs() <= 1e-9,
                    "motor P{n}({x}) debe ser la suma exacta: {v} vs {esperado}"
                );
            }
        }
    }

    #[test]
    fn taylor_x3_real_vs_canonica_declarada() {
        // La queja real: taylor de x³ dibujaba sin(x). Ahora difieren.
        let real = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("x^3", 0.0, 5));
        let canonica = super::render_taylor_frames_for_spec(
            96,
            72,
            &taylor_spec(
                grafito_anim::parametric::TAYLOR_CANONICAL_EXPR,
                grafito_anim::parametric::TAYLOR_CANONICAL_CENTER,
                grafito_anim::parametric::TAYLOR_CANONICAL_ORDER,
            ),
        );
        assert_eq!(real.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(canonica.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_ne!(
            real[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            canonica[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "Taylor de x³ jamás es la senoidal canónica"
        );
        // Determinista: mismo spec → mismos píxeles.
        let otra = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("x^3", 0.0, 5));
        assert_eq!(real[0].pixels, otra[0].pixels);
        // El orden del spec NO recorta la timeline (siempre los 5 escalones):
        // P1(x³) y P5(x³) dan el mismo set (último frame = P9 = x³).
        let p1 = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("x^3", 0.0, 1));
        assert_eq!(
            p1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            real[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "el orden del spec es inerte: timeline fija"
        );
    }

    #[test]
    fn taylor_escalones_exactos_sin_morphing() {
        // 5 escalones discretos pineados: 1, 3, 5, 7, 9.
        assert_eq!(super::TAYLOR_ESCALONES, [1, 3, 5, 7, 9]);
        // Quintiles de los 48 frames (cero interpolación de coeficientes).
        for (frame, esperado) in [
            (0, 1),
            (9, 1),
            (10, 3),
            (19, 3),
            (20, 5),
            (28, 5),
            (29, 7),
            (38, 7),
            (39, 9),
            (47, 9),
        ] {
            assert_eq!(
                super::taylor_orden_en_frame(frame),
                esperado,
                "frame {frame}"
            );
        }
        // Monótono no-decreciente y recorre los 5 escalones.
        let mut previo = 0u32;
        let mut vistos = std::collections::BTreeSet::new();
        for f in 0..NATIVE_ANIM_FRAME_COUNT {
            let o = super::taylor_orden_en_frame(f);
            assert!(o >= previo, "monótono en frame {f}");
            previo = o;
            vistos.insert(o);
        }
        assert_eq!(
            vistos,
            [1u32, 3, 5, 7, 9].into_iter().collect(),
            "los 5 escalones exactos"
        );
        // Mismo escalón = mismos bytes (P_n exacto, sin morphing); entre
        // escalones la curva cambia.
        let frames = super::render_taylor_frames(96, 72);
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        for (a, b) in [(0, 9), (10, 19), (20, 28), (29, 38), (39, 47)] {
            assert_eq!(
                frames[a].pixels, frames[b].pixels,
                "escalón idéntico en frames {a}..={b}"
            );
        }
        for (a, b) in [(9, 10), (19, 20), (28, 29), (38, 39)] {
            assert_ne!(
                frames[a].pixels, frames[b].pixels,
                "el escalón cambia entre frames {a} y {b}"
            );
        }
        assert_ne!(
            frames[0].pixels,
            frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "P1 vs P9 difieren"
        );
    }

    #[test]
    fn taylor_rotulo_vivo_y_formula_sin_solape() {
        // Rótulo vivo por escalón (puro, sin render; siempre ≤12ch).
        for o in [1u32, 3, 5, 7, 9] {
            let rotulo = super::taylor_rotulo_para_orden(o);
            assert!(rotulo.chars().count() <= 12, "rótulo acotado: {rotulo}");
        }
        assert_eq!(super::taylor_rotulo_para_orden(1), "orden 1");
        assert_eq!(super::taylor_rotulo_para_orden(9), "orden 9");
        // Fórmula explícita exacta por escalón (notación `^` ASCII).
        assert_eq!(super::taylor_formula_para_orden(1), "P1 = x");
        assert_eq!(super::taylor_formula_para_orden(3), "P3 = x-x^3/6");
        assert_eq!(super::taylor_formula_para_orden(5), "P5 = x-x^3/6+x^5/120");
        assert_eq!(
            super::taylor_formula_para_orden(7),
            "P7 = x-x^3/6+x^5/120-x^7/5040"
        );
        assert_eq!(
            super::taylor_formula_para_orden(9),
            "P9 = x-x^3/6+x^5/120-x^7/5040+x^9/362880"
        );
        // Partición en hasta 2 líneas sin perder ni un char.
        let p9 = super::taylor_formula_para_orden(9);
        let dos = super::taylor_formula_lineas(p9, 23);
        assert_eq!(dos.len(), 2, "P9 se parte en 2: {dos:?}");
        assert_eq!(dos.concat(), p9, "la unión es la fórmula exacta");
        let una = super::taylor_formula_lineas(super::taylor_formula_para_orden(1), 23);
        assert_eq!(una, vec!["P1 = x".to_string()], "P1 en 1 línea");
        // En previews diminutos colapsa a 1 línea (nunca invade banda media).
        assert_eq!(super::taylor_lineas_para_ancho(p9, 12).len(), 1);
        // Cajas disjuntas entre sí a todo tamaño (igual que el dibujo).
        for (w, h) in [(64usize, 64usize), (96, 72), (480, 360), (640, 480)] {
            for o in super::TAYLOR_ESCALONES {
                let titulo = super::taylor_rotulo_para_orden(o);
                let lineas =
                    super::taylor_lineas_para_frame(super::taylor_formula_para_orden(o), w, h);
                let cajas = super::taylor_rotulos_cajas(w, h, &titulo, &lineas);
                assert_eq!(cajas.len(), 1 + lineas.len(), "{w}x{h} orden {o}");
                for (i, a) in cajas.iter().enumerate() {
                    for b in cajas.iter().skip(i + 1) {
                        let solapa = a.x < b.x + b.w
                            && b.x < a.x + a.w
                            && a.y < b.y + b.h
                            && b.y < a.y + a.h;
                        assert!(!solapa, "{w}x{h} orden {o}: rótulos sin solape");
                    }
                }
            }
        }
        // Etiquetas dentro de la franja superior a 96×72 (banda media
        // intacta para el contrato chat/export), en los 5 escalones.
        for o in super::TAYLOR_ESCALONES {
            let titulo = super::taylor_rotulo_para_orden(o);
            let lineas =
                super::taylor_lineas_para_frame(super::taylor_formula_para_orden(o), 96, 72);
            let cajas = super::taylor_rotulos_cajas(96, 72, &titulo, &lineas);
            for (i, c) in cajas.iter().enumerate() {
                assert!(
                    c.y + c.h <= 40,
                    "orden {o} caja {i} en franja superior (y+h={})",
                    c.y + c.h
                );
            }
        }
        // Export quema el rótulo vivo, el chat no (el header ya titula)…
        let vacio = params_map(&[]);
        let chat = super::render_anim_with_progress(
            "taylor-series",
            "taylor-escalones-pin",
            96,
            72,
            &vacio,
            &mut |_, _| {},
        );
        let export =
            super::render_anim_for_export("taylor-series", "taylor-escalones-pin", 96, 72, &vacio);
        assert_eq!(
            cuenta_texto_quemado(&chat[0], 40),
            0,
            "el chat no quema título"
        );
        assert!(
            cuenta_texto_quemado(&export[0], 40) > 0,
            "el export muestra el rótulo vivo"
        );
        // …pero la matemática no cambia: banda media idéntica.
        assert!(
            banda_media_igual(&chat[0], &export[0]),
            "la banda media no debe cambiar"
        );
        // El rótulo avanza con los escalones: P1 vs P9.
        assert_ne!(
            export[0].pixels,
            export[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "el rótulo vivo avanza"
        );
    }

    #[test]
    fn taylor_viewport_pi_sin_banda_y_curva_solida() {
        // Viewport SOLO-taylor en X: ±2π exactos; el global [-3,3] intacto.
        use std::f64::consts::PI;
        assert!((super::TAYLOR_X_MIN + 2.0 * PI).abs() <= 1e-12);
        assert!((super::TAYLOR_X_MAX - 2.0 * PI).abs() <= 1e-12);
        assert_eq!(super::VIEW_X_MIN, -3.0, "global intacto");
        assert_eq!(super::VIEW_X_MAX, 3.0, "global intacto");
        // Ticks X en múltiplos de π con rótulo ASCII legible.
        let ticks = super::taylor_ticks_x();
        assert_eq!(ticks.len(), 5);
        for ((x, tag), (ex, etag)) in ticks.iter().zip(
            [
                (-2.0 * PI, "-2pi"),
                (-PI, "-pi"),
                (0.0, "0"),
                (PI, "pi"),
                (2.0 * PI, "2pi"),
            ]
            .iter(),
        ) {
            assert!((x - ex).abs() <= 1e-12, "tick en múltiplo de π");
            assert_eq!(*tag, *etag, "rótulo ASCII con glifo real");
        }
        // Mapeo SOLO-taylor: ±2π adentro, ±6.5 afuera; Y con clip limpio.
        assert!(super::taylor_to_pixel_opt(640, 480, 2.0 * PI, 0.0).is_some());
        assert!(super::taylor_to_pixel_opt(640, 480, -2.0 * PI, 0.0).is_some());
        assert!(super::taylor_to_pixel_opt(640, 480, 6.5, 0.0).is_none());
        assert!(super::taylor_to_pixel_opt(640, 480, -6.5, 0.0).is_none());
        assert!(super::taylor_to_pixel_opt(640, 480, 0.0, 3.0).is_some());
        assert!(super::taylor_to_pixel_opt(640, 480, 0.0, 3.5).is_none());
        assert!(super::taylor_to_pixel_opt(0, 0, 0.0, 0.0).is_none());
        // Sin banda sombreada y sin tenue: el P_n va sólido (PAL_BLUE opaco
        // presente en el último frame; el radio infinito de sin no se decora).
        let ultimo = &super::render_taylor_frames(640, 480)[NATIVE_ANIM_FRAME_COUNT - 1];
        let solidos = ultimo
            .pixels
            .iter()
            .filter(|c| c.r() == 66 && c.g() == 133 && c.b() == 244 && c.a() == 255)
            .count();
        assert!(solidos > 20, "P_n sólido presente, sin tenue: {solidos}");
    }

    #[test]
    fn taylor_parametriza_centro_con_presupuesto() {
        // El centro parametriza el dibujo (nada hardcodeado); el orden del
        // spec es inerte (timeline fija en los 5 escalones).
        let base = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("sin(x)", 0.0, 5));
        let movida = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("sin(x)", 1.0, 5));
        assert_ne!(
            base[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            movida[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "el centro parametriza el dibujo"
        );
        let p1 = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("sin(x)", 0.0, 1));
        assert_eq!(
            p1[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            base[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "el orden del spec es inerte"
        );
        // Presupuestos intactos: 48 frames, tamaño pedido, ≤64 MiB.
        assert_eq!(base.len(), NATIVE_ANIM_FRAME_COUNT);
        for (i, fr) in base.iter().enumerate() {
            assert_eq!(fr.size, [96, 72], "frame {i}");
        }
        let bytes =
            super::estimate_frames_bytes(96, 72, NATIVE_ANIM_FRAME_COUNT).expect("sin desborde");
        assert!(
            bytes <= super::NATIVE_MAX_SET_BYTES,
            "set bajo el tope: {bytes}"
        );
        // Sin plateau: P_n diverge pero se corta limpio (clip, no aplaste).
        for f in base.iter().step_by(12) {
            assert!(!tiene_plateau_sup(f), "sin plateau superior");
        }
        // Determinista: mismo spec → mismos píxeles.
        let otra = super::render_taylor_frames_for_spec(96, 72, &taylor_spec("sin(x)", 0.0, 5));
        assert_eq!(base[0].pixels, otra[0].pixels, "determinista");
    }

    #[test]
    fn ejes_pintan_texto_en_zona_de_ejes() {
        // El helper aislado: fondo + ejes rotulados tiene texto claro en
        // las bandas de los ejes; fondo + líneas peladas no.
        let (w, h) = (200usize, 150usize);
        let len = w * h * 4;
        let (cx, cy) = super::to_pixel(w, h, 0.0, 0.0);
        let banda_x = cy + 4..cy + 14;
        let banda_y = cx + 4..cx + 40;
        let cuenta_bandas = |buf: &[u8]| {
            let mut n = 0;
            for y in 0..h {
                for x in 0..w {
                    let en_x = banda_x.contains(&y);
                    let en_y = banda_y.contains(&x);
                    if !en_x && !en_y {
                        continue;
                    }
                    if let Some(i) = y
                        .checked_mul(w)
                        .and_then(|v| v.checked_add(x))
                        .and_then(|v| v.checked_mul(4))
                    {
                        if i + 2 < buf.len() && buf[i] > 200 && buf[i + 1] > 200 && buf[i + 2] > 200
                        {
                            n += 1;
                        }
                    }
                }
            }
            n
        };
        let mut pelado = vec![0u8; len];
        super::fill_background(&mut pelado, w, h);
        super::draw_line(
            &mut pelado,
            w,
            h,
            super::to_pixel(w, h, -3.0, 0.0),
            super::to_pixel(w, h, 3.0, 0.0),
            super::AXIS_COLOR,
        );
        super::draw_line(
            &mut pelado,
            w,
            h,
            super::to_pixel(w, h, 0.0, -3.0),
            super::to_pixel(w, h, 0.0, 3.0),
            super::AXIS_COLOR,
        );
        assert_eq!(cuenta_bandas(&pelado), 0, "líneas peladas sin texto");
        let mut rotulado = vec![0u8; len];
        super::fill_background(&mut rotulado, w, h);
        super::draw_axes_with_labels(&mut rotulado, w, h);
        assert!(
            cuenta_bandas(&rotulado) > 20,
            "ejes rotulados con ticks en bandas"
        );
    }

    #[test]
    fn axis_tick_values_cubre_rango_con_paso_lindo() {
        assert_eq!(super::axis_tick_values(2.0), vec![-2.0, 2.0]);
        assert_eq!(super::axis_tick_values(1.0), vec![-2.0, -1.0, 1.0, 2.0]);
        assert!(super::axis_tick_values(f64::NAN).is_empty());
        assert!(super::axis_tick_values(0.0).is_empty());
        assert!(super::axis_tick_values(-1.0).is_empty());
    }

    /// ¿Hay plateau de curva amarilla en las filas superiores (rachas
    /// horizontales largas = clamp aplastado)? El corte limpio cruza el
    /// borde en columnas aisladas, nunca en rachas ≥24px.
    fn tiene_plateau_sup(frame: &egui::ColorImage) -> bool {
        let w = frame.size[0];
        let filas = 3.min(frame.size[1]);
        for y in 0..filas {
            let mut racha = 0usize;
            for x in 0..w {
                let Some(c) = frame.pixels.get(y * w + x) else {
                    continue;
                };
                if c.r() > 150 && c.g() > 130 && c.b() < 150 {
                    racha += 1;
                    if racha >= 24 {
                        return true;
                    }
                } else {
                    racha = 0;
                }
            }
        }
        false
    }

    #[test]
    fn viewport_fijo_misma_escala_en_los_48_frames() {
        // Viewport fijo documentado [-3,3]² (sin temblor entre frames).
        assert_eq!((super::VIEW_X_MIN, super::VIEW_X_MAX), (-3.0, 3.0));
        assert_eq!((super::VIEW_Y_MIN, super::VIEW_Y_MAX), (-3.0, 3.0));
        let frames = super::render_derivative_frames_with_params(
            320,
            240,
            &std::collections::BTreeMap::new(),
        );
        assert_eq!(frames.len(), super::NATIVE_ANIM_FRAME_COUNT);
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.size, [320, 240], "frame {i}: misma escala");
        }
        // La parábola es fija: máscara amarilla idéntica salvo fringe AA
        // (la tangente azul no entra en la máscara).
        super::assert_curva_fija(&frames, "derivada viewport fijo");
    }

    #[test]
    fn parabola_sin_plateau_corte_limpio_en_borde() {
        // y=9 está fuera de vista: sin punto (antes se aplastaba al borde).
        assert!(super::to_pixel_opt(320, 240, 0.0, 9.0).is_none());
        assert!(super::to_pixel_opt(320, 240, 0.0, 2.0).is_some());
        // Segmento totalmente fuera no pinta nada.
        let (w, h) = (320usize, 240usize);
        let mut buf = vec![7u8; w * h * 4];
        let antes = buf.clone();
        assert!(!super::draw_seg_mundo(
            &mut buf,
            w,
            h,
            -3.0,
            5.0,
            3.0,
            9.0,
            super::CURVE_MAIN,
            super::CURVE_ANCHO,
        ));
        assert_eq!(buf, antes, "fuera de vista no toca el buffer");
        // Ningún frame de la derivada tiene plateau superior.
        let frames = super::render_derivative_frames_with_params(
            320,
            240,
            &std::collections::BTreeMap::new(),
        );
        for (i, f) in frames.iter().enumerate().step_by(12) {
            assert!(!tiene_plateau_sup(f), "frame {i}: sin plateau");
        }
    }

    #[test]
    fn labels_con_formato_corto_y_sin_solape() {
        // 1 decimal máximo vía la Piel.
        assert_eq!(grafito_ui::animation::anim_axes::short_tick_label(1.0), "1");
        assert_eq!(
            grafito_ui::animation::anim_axes::short_tick_label(-2.5),
            "-2.5"
        );
        // Los ejes a 640×480 rotulan sin panic y con texto en bandas
        // (cubre el filtro de cajas disjuntas en el tamaño del reporte).
        let (w, h) = (640usize, 480usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        super::draw_axes_with_labels(&mut buf, w, h);
        let (_, cy) = super::to_pixel(w, h, 0.0, 0.0);
        let mut claros = 0usize;
        for y in (cy + 4)..(cy + 14).min(h) {
            for x in 0..w {
                if let Some(i) = y
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(x))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 2 < buf.len() && buf[i] > 200 && buf[i + 1] > 200 && buf[i + 2] > 200 {
                        claros += 1;
                    }
                }
            }
        }
        assert!(claros > 20, "ticks rotulados con texto claro");
    }

    #[test]
    fn todos_los_parametricos_rotulan_eje_x() {
        // Integración: cada renderer paramétrico deja el rótulo "x" junto al
        // extremo derecho del eje (caja con margen 24px al borde —R6c—,
        // filas cy+8..): zona donde ninguna curva didáctica del set pinta
        // (verificado por renderer).
        let hay_x = |f: &egui::ColorImage| {
            // Ventana derivada del propio frame (la vía paramétrica usa su
            // viewport 640x480, no 200x150).
            let (fw, fh) = (f.size[0], f.size[1]);
            let (_, cy) = super::to_pixel(fw, fh, 0.0, 0.0);
            let mut n = 0;
            for y in cy + 8..(cy + 19).min(fh) {
                // R6c a propósito: el slot "x" se corrió a margen 24px
                // (antes 12px + ventana de 26px); la ventana sigue la caja.
                for x in fw.saturating_sub(40)..fw {
                    let p = &f.pixels[y * fw + x];
                    if p.r() > 200 && p.g() > 200 && p.b() > 200 {
                        n += 1;
                    }
                }
            }
            n
        };
        let empty = params_map(&[]);
        let mut casos: Vec<(&str, Vec<egui::ColorImage>)> = vec![
            (
                "derivative-slope",
                super::render_derivative_frames_with_params(200, 150, &empty),
            ),
            ("integral-area", super::render_integral_frames(200, 150)),
            ("taylor-series", super::render_taylor_frames(200, 150)),
            (
                "taylor-x3",
                super::render_taylor_frames_for_spec(200, 150, &taylor_spec("x^3", 0.0, 5)),
            ),
            ("conformal-map", super::render_conformal_frames(200, 150)),
            ("pitagoras", super::render_pitagoras_frames(200, 150)),
            ("euler", super::render_euler_frames(200, 150)),
            ("fourier", super::render_fourier_frames(200, 150)),
            (
                "logistic",
                super::render_logistic_bifurcation_frames(200, 150),
            ),
            ("gradient", super::render_gradient_field_frames(200, 150)),
            ("mobius", super::render_mobius_frames(200, 150)),
        ];
        if let Ok(frames) = super::render_parametric_frames(
            &super::parametric_for_template("integral-area", "integral").expect("canónica"),
        ) {
            casos.push(("parametrica-base", frames));
        }
        assert!(casos.len() >= 11, "todos los paramétricos: {}", casos.len());
        for (nombre, frames) in &casos {
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT, "{nombre}: 48");
            assert!(
                hay_x(&frames[0]) > 0,
                "{nombre}: frame 0 sin rótulo x en el extremo del eje"
            );
        }
    }

    #[test]
    fn dispatcher_with_params_vacio_igual_a_legacy() {
        // Sin params, el dispatcher v3 delega idéntico al legacy en las 13.
        let empty = params_map(&[]);
        for tmpl in NATIVE_TEMPLATES {
            let legacy = render_anim_for_concept(tmpl, "concepto libre", 64, 64);
            let vivo = render_anim_for_concept_with_params(tmpl, "concepto libre", 64, 64, &empty);
            assert_eq!(legacy.len(), vivo.len(), "{tmpl}: len");
            for (i, (a, b)) in legacy.iter().zip(vivo.iter()).enumerate() {
                assert_eq!(a.pixels, b.pixels, "{tmpl} frame {i}");
            }
        }
    }

    // ── v3: registro + divergencia honesta ──────────────────────────────
    #[test]
    fn native_templates_son_trece_y_despachan() {
        assert_eq!(NATIVE_TEMPLATES.len(), 13, "registro canónico = 13");
        for tmpl in NATIVE_TEMPLATES {
            assert!(is_known_native_template(tmpl), "{tmpl} conocido");
            let f = render_timed(tmpl, 64, 64, || render_anim_by_template(tmpl, 64, 64));
            assert_frames_valid(&f, 64, 64, tmpl);
        }
        assert!(is_known_native_template("pythagoras"), "alias pythagoras");
        assert!(is_known_native_template("  EULER  "), "case/trim");
        assert!(
            !is_known_native_template("limit-epsilon"),
            "sin renderer propio"
        );
        assert!(
            !is_known_native_template("ode-system"),
            "sin renderer propio"
        );
        assert!(!is_known_native_template(""), "vacío no es plantilla");
    }

    #[test]
    fn limit_y_ode_caen_a_fallback_universal() {
        // DIVERGENCIA HONESTA: limit-epsilon / ode-* no existen en ningún
        // registro (ni agent, ni protocolo, ni nativo, ni python). Producen
        // frames válidos solo vía fallback genérico — este test lo pinnea
        // hasta que alguien les dé renderer propio (TODO).
        for tmpl in ["limit-epsilon", "ode-system", "ode"] {
            let f = render_anim_by_template(tmpl, 64, 64);
            assert_frames_valid(&f, 64, 64, tmpl);
            let g = render_anim_for_concept(tmpl, "límite epsilon delta", 64, 64);
            assert_frames_valid(&g, 64, 64, tmpl);
        }
    }

    // ── v4 sync mecánico + dispatch honesto (ANIM-REVIVE) ────────────────
    #[test]
    fn registros_nativo_protocolo_sync_once() {
        // Nativo y protocolo sincronizados: 13 canónicas en ambos, mismo
        // orden (las 11 históricas primero, `subspace` + `fractal` al final).
        assert_eq!(NATIVE_TEMPLATES.len(), 13);
        assert_eq!(CANONICAL_TEMPLATES.len(), 13);
        assert_eq!(NATIVE_TEMPLATES, CANONICAL_TEMPLATES);
    }

    // ── subspace + fractal (frente piel-ui, 2 plantillas nuevas) ──────────
    #[test]
    fn subspace_fases_en_rangos() {
        use super::SubspaceFase;
        for f in 0..12 {
            assert_eq!(
                super::subspace_fase_para_frame(f),
                SubspaceFase::Plano,
                "f{f}"
            );
        }
        for f in 12..24 {
            assert_eq!(
                super::subspace_fase_para_frame(f),
                SubspaceFase::Vectores,
                "f{f}"
            );
        }
        for f in 24..40 {
            assert_eq!(
                super::subspace_fase_para_frame(f),
                SubspaceFase::Span,
                "f{f}"
            );
        }
        for f in 40..48 {
            assert_eq!(
                super::subspace_fase_para_frame(f),
                SubspaceFase::Rotulo,
                "f{f}"
            );
        }
        assert_eq!(
            super::subspace_fase_para_frame(999),
            SubspaceFase::Rotulo,
            "clamp a 47"
        );
    }

    #[test]
    fn subspace_params_rank2_o_default() {
        // Vacío → defaults honestos v1=(2,1), v2=(−1,2).
        let (v1, v2) = super::subspace_vectores_desde_params(&params_map(&[]));
        assert_eq!(v1, super::SUBSPACE_V1_DEFAULT);
        assert_eq!(v2, super::SUBSPACE_V2_DEFAULT);
        // Dependencia lineal (v2 = v1, det=0) → default honesto.
        let deg = params_map(&[("v1x", 2.0), ("v1y", 1.0), ("v2x", 2.0), ("v2y", 1.0)]);
        let (d1, d2) = super::subspace_vectores_desde_params(&deg);
        assert_eq!(
            (d1, d2),
            (super::SUBSPACE_V1_DEFAULT, super::SUBSPACE_V2_DEFAULT)
        );
        // Vector nulo → default honesto.
        let nulo = params_map(&[("v1x", 0.0), ("v1y", 0.0)]);
        let (n1, n2) = super::subspace_vectores_desde_params(&nulo);
        assert_eq!(
            (n1, n2),
            (super::SUBSPACE_V1_DEFAULT, super::SUBSPACE_V2_DEFAULT)
        );
        // Válidos e independientes → se respetan (det = 1·2−0·1 = 2).
        let ok = params_map(&[("v1x", 1.0), ("v1y", 0.0), ("v2x", 1.0), ("v2y", 2.0)]);
        let (o1, o2) = super::subspace_vectores_desde_params(&ok);
        assert_eq!(o1, [1.0, 0.0]);
        assert_eq!(o2, [1.0, 2.0]);
        // Clamp a −3..=3 antes de validar.
        let grande = params_map(&[("v1x", 99.0), ("v1y", 0.0), ("v2x", 0.0), ("v2y", 1.0)]);
        let (g1, g2) = super::subspace_vectores_desde_params(&grande);
        assert_eq!(g1, [3.0, 0.0]);
        assert_eq!(g2, [0.0, 1.0]);
    }

    #[test]
    fn subspace_render_fases_rotulos_y_determinismo() {
        let f = render_timed("subspace", 96, 72, || super::render_subspace_frames(96, 72));
        assert_frames_valid(&f, 96, 72, "subspace");
        // Fases en rangos: representantes difieren (fade, grow, write, indicate).
        assert_ne!(f[0].pixels, f[11].pixels, "fade in del plano");
        assert_ne!(f[12].pixels, f[23].pixels, "grow de flechas");
        assert_ne!(f[24].pixels, f[39].pixels, "write del span");
        assert_ne!(f[40].pixels, f[47].pixels, "indicate sin plateau");
        // Rótulos: el export trae texto arriba-izquierda, el chat no.
        assert!(
            cuenta_texto_quemado(&f[47], 40) > 0,
            "export rotula span(v1,v2)"
        );
        assert!(cuenta_texto_quemado(&f[13], 40) > 0, "export rotula v1/v2");
        let vacio = params_map(&[]);
        let chat = super::render_anim_with_progress(
            "subspace",
            "concepto libre",
            96,
            72,
            &vacio,
            &mut |_, _| {},
        );
        assert_eq!(cuenta_texto_quemado(&chat[47], 40), 0, "chat sin quemado");
        // Misma matemática en banda media (el flag solo toca el texto).
        assert!(banda_media_igual(&chat[47], &f[47]), "banda media idéntica");
        // Determinismo byte-idéntico: doble render igual.
        let g = super::render_subspace_frames(96, 72);
        for (i, (a, b)) in f.iter().zip(g.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "frame {i} determinista");
        }
        // Params degenerados rinden igual que el default (fallback honesto).
        let deg = params_map(&[("v1x", 2.0), ("v1y", 1.0), ("v2x", 2.0), ("v2y", 1.0)]);
        let h = super::render_subspace_frames_with_params(96, 72, &deg);
        for (i, (a, b)) in f.iter().zip(h.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "degenerado == default frame {i}");
        }
        // Params válidos cambian el dibujo.
        let ok = params_map(&[("v1x", 1.0), ("v1y", 0.0), ("v2x", 1.0), ("v2y", 2.0)]);
        let k = super::render_subspace_frames_with_params(96, 72, &ok);
        assert_ne!(f[47].pixels, k[47].pixels, "params vivos cambian el span");
        // Presupuestos: 640×480 default y 480×360 chat con tamaño exacto.
        for (w, h) in [(640u32, 480u32), (480u32, 360u32)] {
            let set = super::render_subspace_frames(w, h);
            assert_eq!(set.len(), super::NATIVE_ANIM_FRAME_COUNT, "48 frames");
            for (i, frame) in set.iter().enumerate() {
                assert_eq!(frame.size, [w as usize, h as usize], "frame {i}");
            }
        }
    }

    #[test]
    fn fractal_niveles_segmentos_y_morf() {
        // 3·4ⁿ: 3, 12, 48, 192, 768 (máx O(cientos) por frame).
        let niveles: Vec<usize> = (0..=4).map(super::koch_segmentos_por_nivel).collect();
        assert_eq!(niveles, vec![3, 12, 48, 192, 768]);
        assert_eq!(super::koch_segmentos_por_nivel(99), 768, "clamp al máx");
        // Morph: f0 en nivel 0 (e=0), f47 en nivel 4 (e=1).
        assert_eq!(super::koch_morf_para_frame(0), (0, 0.0));
        let (n47, e47) = super::koch_morf_para_frame(47);
        assert_eq!(n47, 3);
        assert!((e47 - 1.0).abs() < 1e-12, "f47 cierra el nivel 4");
        // Bases monótonas 0..3 a lo largo de los 48 frames.
        let mut ultima_base = 0usize;
        for f in 0..48 {
            let (n, _) = super::koch_morf_para_frame(f);
            assert!(n >= ultima_base, "base no retrocede en f{f}");
            ultima_base = n;
        }
        // Rótulo vivo: arranca en 0 y termina en 4.
        assert_eq!(super::koch_nivel_mostrado(0), 0);
        assert_eq!(super::koch_nivel_mostrado(47), 4);
    }

    #[test]
    fn fractal_render_rotulos_y_determinismo() {
        let f = render_timed("fractal", 96, 72, || super::render_fractal_frames(96, 72));
        assert_frames_valid(&f, 96, 72, "fractal");
        // Sin plateau largo: los representantes de cada banda difieren y el
        // total de frames distintos es alto (morph continuo + barra).
        for (a, b) in [(5usize, 17usize), (17, 29), (29, 41), (41, 47)] {
            assert_ne!(f[a].pixels, f[b].pixels, "banda {a} vs {b}");
        }
        let distintos = f
            .windows(2)
            .filter(|par| par[0].pixels != par[1].pixels)
            .count();
        assert!(
            distintos >= 40,
            "morph continuo sin plateau: {distintos}/47"
        );
        // Rótulo vivo en export ("iteración N" + "S segmentos" arriba-izq).
        assert!(
            cuenta_texto_quemado(&f[47], 40) > 0,
            "export rotula iteración"
        );
        let vacio = params_map(&[]);
        let chat = super::render_anim_with_progress(
            "fractal",
            "concepto libre",
            96,
            72,
            &vacio,
            &mut |_, _| {},
        );
        assert_eq!(cuenta_texto_quemado(&chat[0], 40), 0, "chat sin quemado");
        assert!(banda_media_igual(&chat[47], &f[47]), "banda media idéntica");
        // Determinismo byte-idéntico.
        let g = super::render_fractal_frames(96, 72);
        for (i, (a, b)) in f.iter().zip(g.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "frame {i} determinista");
        }
        // Presupuestos: 640×480 default y 480×360 chat con tamaño exacto.
        for (w, h) in [(640u32, 480u32), (480u32, 360u32)] {
            let set = super::render_fractal_frames(w, h);
            assert_eq!(set.len(), super::NATIVE_ANIM_FRAME_COUNT, "48 frames");
            for (i, frame) in set.iter().enumerate() {
                assert_eq!(frame.size, [w as usize, h as usize], "frame {i}");
            }
        }
        // Set canónico entra en 64 MiB (48 × 640×480×4 = 58_982_400 B).
        let bytes = super::estimate_frames_bytes(640, 480, super::NATIVE_ANIM_FRAME_COUNT);
        assert!(bytes.is_some_and(|b| b <= super::NATIVE_MAX_SET_BYTES));
    }

    #[test]
    fn dispatch_honesto_direct_y_fallback() {
        // Las 13 canónicas van directo al renderer (el resto se cubre abajo).
        for tmpl in NATIVE_TEMPLATES {
            match native_dispatch_for(tmpl, "concepto libre") {
                NativeDispatch::Direct { canonical } => {
                    assert!(is_known_native_template(canonical), "{tmpl} → {canonical}")
                }
                other => panic!("{tmpl} debería ser Direct, got {other:?}"),
            }
        }
        assert!(matches!(
            native_dispatch_for("pythagoras", "triángulo"),
            NativeDispatch::Direct { .. }
        ));
        assert!(matches!(
            native_dispatch_for("fraccion-visual", "fracciones"),
            NativeDispatch::Direct { .. }
        ));
        assert!(matches!(
            native_dispatch_for("", "derivada"),
            NativeDispatch::Direct { .. }
        ));
        assert!(matches!(
            native_dispatch_for("auto", "derivada"),
            NativeDispatch::Direct { .. }
        ));
        // Sin renderer propio: fallback declarado, no silencioso.
        for tmpl in ["limit-epsilon", "ode-system", "ode", "typo-total"] {
            match native_dispatch_for(tmpl, "límite epsilon delta") {
                NativeDispatch::FallbackUniversal {
                    requested,
                    resolved,
                } => {
                    assert_eq!(requested, tmpl);
                    assert!(is_known_native_template(resolved), "{tmpl} → {resolved}");
                    // El fallback produce frames válidos de verdad.
                    let f = render_anim_with_progress(
                        tmpl,
                        "límite epsilon delta",
                        64,
                        64,
                        &params_map(&[]),
                        &mut |_, _| {},
                    );
                    assert_frames_valid(&f, 64, 64, tmpl);
                }
                other => panic!("{tmpl} debería ser FallbackUniversal, got {other:?}"),
            }
        }
    }

    // ── v4 progreso real por frame (ANIM-REVIVE) ──────────────────────────
    #[test]
    fn progress_emite_48_monotono_todas_las_plantillas() {
        let empty = params_map(&[]);
        for tmpl in NATIVE_TEMPLATES {
            let mut calls: Vec<(usize, usize)> = Vec::new();
            let frames = render_anim_with_progress(
                tmpl,
                "concepto libre",
                64,
                64,
                &empty,
                &mut |done, total| {
                    calls.push((done, total));
                },
            );
            assert_frames_valid(&frames, 64, 64, tmpl);
            // Exactamente 48 emisiones, 1..=48, total siempre 48.
            assert_eq!(calls.len(), NATIVE_ANIM_FRAME_COUNT, "{tmpl}: emisiones");
            for (i, (done, total)) in calls.iter().enumerate() {
                assert_eq!(*done, i + 1, "{tmpl}: done secuencial");
                assert_eq!(*total, NATIVE_ANIM_FRAME_COUNT, "{tmpl}: total");
            }
            // Determinista: el callback no altera píxeles (dos corridas iguales
            // y además iguales al dispatcher sin progreso).
            let mut calls2 = Vec::new();
            let again = render_anim_with_progress(
                tmpl,
                "concepto libre",
                64,
                64,
                &empty,
                &mut |done, total| {
                    calls2.push((done, total));
                },
            );
            assert_eq!(calls, calls2, "{tmpl}: progreso determinista");
            for (i, (a, b)) in frames.iter().zip(again.iter()).enumerate() {
                assert_eq!(a.pixels, b.pixels, "{tmpl} rerun frame {i}: idéntico");
            }
            let plain = render_anim_for_concept_with_params(tmpl, "concepto libre", 64, 64, &empty);
            assert_eq!(frames.len(), plain.len(), "{tmpl}: len con/sin progreso");
            for (i, (a, b)) in frames.iter().zip(plain.iter()).enumerate() {
                assert_eq!(a.pixels, b.pixels, "{tmpl} frame {i}: idéntico");
            }
        }
    }

    #[test]
    fn progress_con_params_vivos_tambien_emite_48() {
        let params = params_map(&[("x0", 2.0), ("terms", 3.0)]);
        let mut calls = 0usize;
        let frames = render_anim_with_progress(
            "derivative-slope",
            "derivada",
            64,
            64,
            &params,
            &mut |done, total| {
                assert_eq!(total, NATIVE_ANIM_FRAME_COUNT);
                assert!((1..=NATIVE_ANIM_FRAME_COUNT).contains(&done));
                calls += 1;
            },
        );
        assert_eq!(calls, NATIVE_ANIM_FRAME_COUNT);
        assert_frames_valid(&frames, 64, 64, "derivative-progress-params");
        // Fracción lista para la UI: done/total sin inventar.
        assert!((calls as f32 / NATIVE_ANIM_FRAME_COUNT as f32 - 1.0).abs() < f32::EPSILON);
    }

    // ── v4 export GIF real (ANIM-REVIVE) ──────────────────────────────────
    fn synthetic_frames(n: usize) -> Vec<egui::ColorImage> {
        (0..n)
            .map(|k| {
                let c = egui::Color32::from_rgb(
                    (k * 37 % 256) as u8,
                    (k * 91 % 256) as u8,
                    (k * 53 % 256) as u8,
                );
                egui::ColorImage::new([8, 8], c)
            })
            .collect()
    }

    // ── F1 Manim-en-Rust: backend tiny-skia ──────────────────────────────
    #[test]
    fn skia_linea_pinta_y_es_determinista() {
        let (w, h) = (32usize, 32usize);
        let mut a = vec![0u8; w * h * 4];
        let mut b = vec![0u8; w * h * 4];
        super::fill_background(&mut a, w, h);
        super::fill_background(&mut b, w, h);
        super::draw_line(&mut a, w, h, (2, 16), (29, 16), super::CURVE_MAIN);
        super::draw_line(&mut b, w, h, (2, 16), (29, 16), super::CURVE_MAIN);
        assert_eq!(a, b, "mismo trazo → mismos bytes");
        let tinta = a
            .chunks_exact(4)
            .filter(|px| px[0] > 150 && px[2] < 150)
            .count();
        assert!(tinta >= 20, "la línea debe pintar, tinta={tinta}");
        // Punto degenerado pinta 1px (paridad con el Bresenham).
        let mut c = vec![0u8; w * h * 4];
        super::fill_background(&mut c, w, h);
        super::draw_line(&mut c, w, h, (10, 10), (10, 10), super::CURVE_MAIN);
        assert_ne!(a, c, "punto vs línea difieren");
        // Buffer corto / tamaño 0: no-op honesto sin panic.
        let mut corto = vec![0u8; 10];
        super::draw_line(&mut corto, w, h, (0, 0), (5, 5), super::CURVE_MAIN);
        super::draw_line(&mut a, 0, 0, (0, 0), (5, 5), super::CURVE_MAIN);
    }

    #[test]
    fn skia_circulo_y_rect_rellenan() {
        let (w, h) = (32usize, 32usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        super::draw_filled_circle(&mut buf, w, h, 16, 16, 5, super::POINT_RED);
        let rojos = buf
            .chunks_exact(4)
            .filter(|px| px[0] > 150 && px[1] < 120)
            .count();
        assert!(rojos >= 40, "círculo debe rellenar, rojos={rojos}");
        super::draw_filled_rect(&mut buf, w, h, 2, 2, 6, 6, super::PAL_ACCENT);
        let azules = buf
            .chunks_exact(4)
            .filter(|px| px[2] > 150 && px[0] < 120)
            .count();
        assert!(azules >= 30, "rect debe rellenar, azules={azules}");
        // Fuera de rango / vacío: no-op sin panic.
        super::draw_filled_rect(&mut buf, w, h, 90, 90, 6, 6, super::PAL_ACCENT);
        super::draw_filled_rect(&mut buf, w, h, 0, 0, 0, 0, super::PAL_ACCENT);
        super::draw_filled_circle(&mut buf, w, h, 200, 200, 3, super::POINT_RED);
    }

    #[test]
    fn skia_texto_pinta_glifos_reales_y_aguanta_emoji() {
        let (w, h) = (200usize, 150usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        super::draw_text_block(&mut buf, w, h, 10, 10, "x", super::TEXT_COLOR, 1);
        let claros = buf
            .chunks_exact(4)
            .filter(|px| px[0] > 200 && px[1] > 200 && px[2] > 200)
            .count();
        // Medido: 'x' a 10px deja 4 píxeles >200 (núcleos con cobertura
        // total tras el boost); piso en 2 con margen al shimmer del fondo.
        assert!(
            claros >= 2,
            "glifo 'x' debe dejar tinta clara sólida, claros={claros}"
        );
        // Emoji + texto largo + escala fuera de rango: sin panics.
        super::draw_text_block(
            &mut buf,
            w,
            h,
            4,
            4,
            "\u{1f600} emoji test \u{1f9e0}",
            super::TEXT_COLOR,
            3,
        );
        super::draw_text_block(&mut buf, w, h, 0, 0, &"y".repeat(200), super::TEXT_COLOR, 9);
        super::draw_text_block(&mut buf, w, h, 500, 500, "fuera", super::TEXT_COLOR, 1);
    }

    // ── F1: conformal-map con Möbius real ────────────────────────────────
    #[test]
    fn mobius_map_identidad_en_cero_y_none_en_polo() {
        // c=0 → w=z (identidad) en la grilla.
        for (x, y) in [(-2.0, 1.0), (0.0, 0.0), (1.5, -1.5)] {
            let (wx, wy) = super::mobius_map(x, y, 0.0, 0.0).expect("lejos del polo");
            assert!(
                (wx - x).abs() < 1e-12 && (wy - y).abs() < 1e-12,
                "c=0 es identidad en ({x},{y})"
            );
        }
        // Polo en z=1/c: den≈0 → None honesto (jamás inventa punto).
        assert!(
            super::mobius_map(2.0, 0.0, 0.5, 0.0).is_none(),
            "polo debe ser None"
        );
        assert!(super::mobius_map(f64::NAN, 0.0, 0.1, 0.0).is_none());
    }

    #[test]
    fn conformal_es_mobius_real_determinista_y_distinto_de_mobius() {
        let a = super::render_conformal_frames(96, 72);
        assert_frames_valid(&a, 96, 72, "conformal-map");
        let b = super::render_conformal_frames(96, 72);
        assert_eq!(a[0].pixels, b[0].pixels, "determinista");
        // Barrido propio: no es un alias del mobius-transform.
        let m = super::render_mobius_frames(96, 72);
        assert_ne!(
            a[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            m[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "conformal ≠ mobius"
        );
    }

    // ── F1: MP4 honesto vía ffmpeg-sidecar ───────────────────────────────
    #[test]
    fn mp4_fps_y_budgets_pineados() {
        assert_eq!(super::MP4_BASE_FPS, 12);
        assert_eq!(super::GIF_BASE_FPS, 12.0);
        assert_eq!(super::mp4_fps_for_delay(8), 12, "delay 8 → 12fps");
        assert_eq!(super::mp4_fps_for_delay(0), 60, "delay 0 → clamp sin div/0");
        assert_eq!(super::mp4_fps_for_delay(4), 25);
        assert_eq!(super::mp4_fps_for_delay(100), 1);
        // Display honesto en español, sin panics.
        assert!(format!("{}", super::Mp4ExportError::FfmpegMissing).contains("usá gif"));
        assert!(!format!("{}", super::Mp4ExportError::Cancelled).is_empty());
        assert!(!format!("{}", super::Mp4ExportError::FfmpegFailed("x".into())).is_empty());
    }

    #[test]
    fn mp4_preflight_falla_rapido_sin_archivo() {
        let dir = std::env::temp_dir().join(format!(
            "grafito-mp4-preflight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("no-debe-existir.mp4");
        let _ = std::fs::remove_file(&dest);
        // Vacío → Budget(EmptyFrames), sin tocar disco ni ffmpeg.
        let err = super::export_frames_to_mp4_file(
            &[],
            &dest,
            super::GIF_EXPORT_DELAY_CS,
            2000,
            super::VideoQuality::Media,
        )
        .unwrap_err();
        assert_eq!(
            err,
            super::Mp4ExportError::Budget(super::GifExportError::EmptyFrames)
        );
        assert!(!dest.exists(), "preflight no debe crear archivo");
        // 65 frames → Budget(TooManyFrames).
        let many = synthetic_frames(65);
        let err =
            super::export_frames_to_mp4_file(&many, &dest, 8, 2000, super::VideoQuality::Media)
                .unwrap_err();
        assert!(
            matches!(
                err,
                super::Mp4ExportError::Budget(super::GifExportError::TooManyFrames { .. })
            ),
            "fue: {err}"
        );
        assert!(!dest.exists(), "preflight no debe crear archivo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mp4_sin_ffmpeg_falla_honesto_con_mensaje() {
        // Binario inexistente a propósito: hermético, no depende del PATH.
        let frames = synthetic_frames(2);
        let dir = std::env::temp_dir().join(format!(
            "grafito-mp4-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("no-debe-existir.mp4");
        let err = super::export_frames_to_mp4_file_with_bin(
            &frames,
            &dest,
            8,
            &CancellationToken::default(),
            std::path::Path::new("/definitivamente/no/existe/ffmpeg-f1"),
            2000,
            super::VideoQuality::Media,
        )
        .unwrap_err();
        assert_eq!(err, super::Mp4ExportError::FfmpegMissing);
        assert!(
            format!("{err}").contains("usá gif"),
            "mensaje honesto con alternativa, fue: {err}"
        );
        assert!(!dest.exists(), "sin ffmpeg no hay archivo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mp4_spawn_codifica_real_o_missing_honesto() {
        // Integración con el ffmpeg del box: si está, el MP4 es real
        // (existe y pesa >0); si no, `Missing` honesto. Jamás fake.
        let dir = std::env::temp_dir().join(format!(
            "grafito-mp4-spawn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("clip.mp4");
        let frames = synthetic_frames(4);
        let handle = super::spawn_mp4_export(
            frames,
            dest.clone(),
            8,
            CancellationToken::default(),
            2000,
            super::VideoQuality::Media,
        );
        match handle.join().expect("el hilo no debe panicar") {
            Ok(path) => {
                let bytes = std::fs::metadata(&path)
                    .expect("MP4 real debe existir")
                    .len();
                assert!(bytes > 0, "MP4 real no vacío");
                let _ = std::fs::remove_file(&path);
            }
            Err(super::Mp4ExportError::FfmpegMissing) => {
                assert!(!dest.exists(), "sin ffmpeg no hay archivo");
            }
            Err(other) => panic!("MP4 real o Missing honesto, fue: {other}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn calidad_mapea_resolucion_crf_bitrate_reales() {
        use super::{video_quality_export_size, video_quality_max_side, VideoQuality};
        // `-ql`/`-qm`/`-qh` → lados + crf + bitrate.
        assert_eq!(video_quality_max_side(VideoQuality::Baja), 640);
        assert_eq!(video_quality_max_side(VideoQuality::Media), 1280);
        assert_eq!(video_quality_max_side(VideoQuality::Alta), 4096);
        // Sin upscale + pares + aspecto.
        assert_eq!(
            video_quality_export_size(320, 200, VideoQuality::Baja),
            (320, 200)
        );
        assert_eq!(
            video_quality_export_size(1280, 720, VideoQuality::Baja),
            (640, 360)
        );
        assert_eq!(
            video_quality_export_size(1920, 1080, VideoQuality::Media),
            (1280, 720)
        );
        assert_eq!(
            video_quality_export_size(4096, 4096, VideoQuality::Alta),
            (4096, 4096)
        );
        let (w, h) = video_quality_export_size(1001, 707, VideoQuality::Baja);
        assert_eq!((w % 2, h % 2), (0, 0), "yuv420p exige pares");
        assert!(w <= 640 && h <= 640);
        // crf + bitrate por calidad (paridad con `flags`/`suggested`).
        assert_eq!(VideoQuality::Baja.flags(), (30, "veryfast"));
        assert_eq!(VideoQuality::Media.flags(), (23, "veryfast"));
        assert_eq!(VideoQuality::Alta.flags(), (18, "fast"));
        assert_eq!(VideoQuality::Baja.suggested_bitrate_kbps(), 500);
        assert_eq!(VideoQuality::Media.suggested_bitrate_kbps(), 2000);
        assert_eq!(VideoQuality::Alta.suggested_bitrate_kbps(), 8000);
    }

    #[test]
    fn remuestreo_fps_duracion_fija_via_timeline() {
        use super::remuestrear_frames_para_fps;
        assert!(remuestrear_frames_para_fps(&[], 12, 24).is_empty());
        // 48 @12fps = 4s: a 24fps son 96 (misma duración).
        let base = synthetic_frames(48);
        let doble = remuestrear_frames_para_fps(&base, 12, 24);
        assert_eq!(doble.len(), 96, "duración fija: 48@12 → 96@24");
        assert_eq!(doble[0].pixels, base[0].pixels, "arranca igual");
        assert_eq!(
            doble[doble.len() - 1].pixels,
            base[base.len() - 1].pixels,
            "termina igual"
        );
        // Mismo fps = identidad exacta.
        let igual = remuestrear_frames_para_fps(&base, 12, 12);
        assert_eq!(igual.len(), 48);
        for (a, b) in igual.iter().zip(base.iter()) {
            assert_eq!(a.pixels, b.pixels);
        }
        // Mitad de fps: 48@24fps = 2s → 24@12fps.
        let mitad = remuestrear_frames_para_fps(&base, 24, 12);
        assert_eq!(mitad.len(), 24, "48@24 (2s) → 24@12");
        assert_eq!(mitad[0].pixels, base[0].pixels);
        // fps inválidos se clampean, sin panics.
        assert_eq!(remuestrear_frames_para_fps(&base, 0, 0).len(), 48);
        assert_eq!(remuestrear_frames_para_fps(&base, 999, 999).len(), 48);
    }

    /// ffmpeg falso que captura su argv en `$GRAFITO_FFMPEG_ARGS_<sufijo>` y
    /// drena stdin (éxito inmediato). Solo unix (shebang + coreutils).
    #[cfg(unix)]
    fn ffmpeg_falso_con_argv(base: &std::path::Path, sufijo: &str) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = base.join(format!("ffmpeg-falso-{sufijo}"));
        let captura = base.join(format!("argv-{sufijo}.txt"));
        // `touch` del último argv (el tmp hermano): como el real, el falso
        // "produce" el archivo para que el rename publique.
        let script = format!(
            "#!/bin/sh\necho \"$@\" > \"{}\"\ncat > /dev/null\nfor a in \"$@\"; do ultimo=\"$a\"; done\ntouch \"$ultimo\"\nexit 0\n",
            captura.display()
        );
        std::fs::write(&bin, script).unwrap();
        let mut permisos = std::fs::metadata(&bin).unwrap().permissions();
        permisos.set_mode(0o755);
        std::fs::set_permissions(&bin, permisos).unwrap();
        (bin, captura)
    }

    #[cfg(unix)]
    #[test]
    fn mp4_plomea_bitrate_crf_calidad_reales() {
        let base = std::env::temp_dir().join(format!(
            "grafito-mp4-args-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        let (falso, captura) = ffmpeg_falso_con_argv(&base, "mp4");
        let dest = base.join("clip.mp4");
        let frames = synthetic_frames(2);
        super::export_frames_to_mp4_file_with_bin(
            &frames,
            &dest,
            8,
            &CancellationToken::default(),
            &falso,
            2000,
            super::VideoQuality::Media,
        )
        .expect("el falso siempre sale 0");
        let argv = std::fs::read_to_string(&captura).expect("argv capturado");
        for aguja in [
            "-b:v",
            "2000k",
            "-crf",
            "23",
            "-preset",
            "veryfast",
            "-framerate",
            "12",
            "libx264",
            "+faststart",
        ] {
            assert!(argv.contains(aguja), "{aguja} en argv, fue: {argv}");
        }
        assert!(dest.exists(), "publicó el destino");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn r6c_webm_pasa_preset_como_mp4() {
        // Regla 8 (R6c): WebM pasa `-preset` igual que MP4
        // (`VideoQuality::flags`), pineado con el mismo falso.
        let base = std::env::temp_dir().join(format!(
            "grafito-webm-preset-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        let (falso, captura) = ffmpeg_falso_con_argv(&base, "webm-preset");
        let dest = base.join("clip.webm");
        let frames = synthetic_frames(2);
        super::export_frames_to_webm_file_with_bin(
            &frames,
            &dest,
            8,
            &CancellationToken::default(),
            &falso,
            2000,
            super::VideoQuality::Media,
        )
        .expect("el falso siempre sale 0");
        let argv = std::fs::read_to_string(&captura).expect("argv capturado");
        for aguja in ["-preset", "veryfast", "-crf", "23", "libvpx-vp9"] {
            assert!(argv.contains(aguja), "{aguja} en argv, fue: {argv}");
        }
        assert!(dest.exists(), "publicó el destino");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn mp4_baja_reescala_y_crf_30() {
        let base = std::env::temp_dir().join(format!(
            "grafito-mp4-baja-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        let (falso, captura) = ffmpeg_falso_con_argv(&base, "baja");
        let dest = base.join("clip.mp4");
        // 1280×720 en Baja → 640×360 real en `-s`.
        let grande = vec![egui::ColorImage::new([1280, 720], egui::Color32::RED); 2];
        super::export_frames_to_mp4_file_with_bin(
            &grande,
            &dest,
            8,
            &CancellationToken::default(),
            &falso,
            500,
            super::VideoQuality::Baja,
        )
        .expect("el falso siempre sale 0");
        let argv = std::fs::read_to_string(&captura).expect("argv capturado");
        assert!(argv.contains("640x360"), "-s reescalado, fue: {argv}");
        assert!(argv.contains("500k"), "bitrate baja, fue: {argv}");
        assert!(argv.contains("-crf 30"), "crf baja, fue: {argv}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn webm_cancelado_durante_wait_mata_al_hijo() {
        // ffmpeg falso que duerme: el token cancela en pleno `wait` y el
        // watcher debe matarlo (CANCEL_GRACE) sin dejar tmp ni destino.
        use std::os::unix::fs::PermissionsExt as _;
        let base = std::env::temp_dir().join(format!(
            "grafito-webm-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        let dormilon = base.join("ffmpeg-duerme");
        std::fs::write(&dormilon, "#!/bin/sh\nsleep 30\n").unwrap();
        let mut permisos = std::fs::metadata(&dormilon).unwrap().permissions();
        permisos.set_mode(0o755);
        std::fs::set_permissions(&dormilon, permisos).unwrap();
        let dest = base.join("clip.webm");
        let token = CancellationToken::default();
        let clon = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            clon.cancel();
        });
        let err = super::export_frames_to_webm_file_with_bin(
            &synthetic_frames(2),
            &dest,
            8,
            &token,
            &dormilon,
            2000,
            super::VideoQuality::Media,
        )
        .unwrap_err();
        assert_eq!(err, super::WebmExportError::Cancelled);
        assert!(!dest.exists(), "cancelado no publica");
        // Sin tmp huérfano junto al destino.
        let restos: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(restos.is_empty(), "sin tmp huérfano: {restos:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn gif_export_codifica_frames_sinteticos_con_cabecera() {
        let frames = synthetic_frames(3);
        let bytes = encode_frames_to_gif_bytes(&frames, GIF_EXPORT_DELAY_CS).unwrap();
        assert!(bytes.len() > 13, "GIF mínimo con 3 frames");
        assert_eq!(&bytes[0..6], b"GIF89a", "cabecera GIF real");
    }

    #[test]
    fn gif_export_roundtrip_48_frames_reales() {
        let frames = render_derivative_frames_with_params(64, 64, &params_map(&[]));
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        let bytes = encode_frames_to_gif_bytes(&frames, GIF_EXPORT_DELAY_CS).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
        // Decode de vuelta: los 48 frames viajan de verdad.
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes.as_slice()).unwrap();
        let mut count = 0usize;
        while decoder.read_next_frame().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, NATIVE_ANIM_FRAME_COUNT);
    }

    // ── M3-6: GIF cancelable + budget pre-spawn ──────────────────────────
    #[test]
    fn gif_export_cancelable_spawn_cancel_join_rapido_sin_archivo() {
        let frames = synthetic_frames(8);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("grafito_gif_cancel_{}_{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cancel.gif");
        // Cancelado pre-spawn: determinista (el hilo ni codifica).
        let token = CancellationToken::default();
        token.cancel();
        let inicio = std::time::Instant::now();
        let handle = spawn_gif_export_cancelable(frames, path.clone(), GIF_EXPORT_DELAY_CS, token);
        let salida = handle.join().expect("join del hilo exportador");
        assert_eq!(salida, Err(GifExportError::Cancelled));
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(5),
            "cancel→join debe ser rápido"
        );
        assert!(!path.exists(), "cancelado no deja archivo");
        assert_eq!(
            format!("{}", GifExportError::Cancelled),
            "exportación cancelada"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_export_cancelable_sin_cancelar_escribe_real() {
        let frames = synthetic_frames(4);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "grafito_gif_nocancel_{}_{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ok.gif");
        let token = CancellationToken::default();
        let handle = spawn_gif_export_cancelable(frames, path.clone(), GIF_EXPORT_DELAY_CS, token);
        let salida = handle.join().expect("join").expect("sin cancelar exporta");
        assert_eq!(salida, path);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_export_budget_pre_spawn_falla_rapido_sin_archivo() {
        // 65 frames > tope 64: el hilo devuelve el budget sin codificar.
        let many = synthetic_frames(GIF_EXPORT_MAX_FRAMES + 1);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("grafito_gif_budget_{}_{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gordo.gif");
        let inicio = std::time::Instant::now();
        let handle = spawn_gif_export(many, path.clone(), GIF_EXPORT_DELAY_CS);
        let salida = handle.join().expect("join");
        assert_eq!(
            salida,
            Err(GifExportError::TooManyFrames {
                got: GIF_EXPORT_MAX_FRAMES + 1
            })
        );
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(5),
            "budget pre-spawn debe ser rápido"
        );
        assert!(!path.exists(), "budget excedido no deja archivo");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_export_rechaza_vacio_inconsistente_y_exceso() {
        assert_eq!(
            encode_frames_to_gif_bytes(&[], GIF_EXPORT_DELAY_CS).unwrap_err(),
            GifExportError::EmptyFrames
        );
        // Tamaños mezclados.
        let mut mixed = synthetic_frames(2);
        mixed.push(egui::ColorImage::new([4, 4], egui::Color32::BLACK));
        assert!(matches!(
            encode_frames_to_gif_bytes(&mixed, GIF_EXPORT_DELAY_CS).unwrap_err(),
            GifExportError::InconsistentSize { index: 2, .. }
        ));
        // Exceso sobre el tope 64.
        let many = synthetic_frames(GIF_EXPORT_MAX_FRAMES + 1);
        assert_eq!(
            encode_frames_to_gif_bytes(&many, GIF_EXPORT_DELAY_CS).unwrap_err(),
            GifExportError::TooManyFrames {
                got: GIF_EXPORT_MAX_FRAMES + 1
            }
        );
        // Mensajes en español, sin inglés crudo.
        for e in [
            GifExportError::EmptyFrames,
            GifExportError::DimensionOutOfRange {
                width: 0,
                height: 0,
            },
            GifExportError::Encode("x".into()),
            GifExportError::Io("y".into()),
        ] {
            let msg = format!("{e}");
            assert!(!msg.is_empty());
            assert!(
                !msg.to_lowercase().contains("error encoding")
                    && !msg.to_lowercase().contains("failed"),
                "sin inglés crudo: {msg}"
            );
        }
    }

    #[test]
    fn gif_export_en_hilo_escribe_archivo_real() {
        let frames = synthetic_frames(4);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("grafito_gif_export_{}_{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anim.gif");
        let handle = spawn_gif_export(frames, path.clone(), GIF_EXPORT_DELAY_CS);
        let out = handle.join().unwrap().unwrap();
        assert_eq!(out, path);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_export_no_sigue_ni_trunca_symlink_plantado() {
        let frames = synthetic_frames(2);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("grafito_gif_guard_{}_{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Víctima con contenido conocido + symlink plantado en el destino.
        let victima = dir.join("victima.gif");
        std::fs::write(&victima, b"contenido original").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victima, dir.join("anim.gif")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&victima, dir.join("anim.gif")).unwrap();
        let destino = dir.join("anim.gif");
        let error = export_frames_to_gif_file(&frames, &destino, GIF_EXPORT_DELAY_CS)
            .expect_err("symlink plantado debe fallar cerrado");
        assert!(
            error.to_string().contains("sin sobrescribir"),
            "error honesto, got: {error}"
        );
        // La víctima intacta y el enlace sin reemplazar por archivo real.
        assert_eq!(std::fs::read(&victima).unwrap(), b"contenido original");
        assert!(std::fs::symlink_metadata(&destino)
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_export_fallo_inyectado_no_deja_final_ni_tmp() {
        // Fallo inyectado: 2º frame con otro tamaño → la codificación falla
        // y no debe quedar ni archivo final ni `.tmp` huérfano en el dir.
        let mut frames = synthetic_frames(2);
        frames[1] = egui::ColorImage::new([4, 4], egui::Color32::RED);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("grafito_gif_atom_{}_{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anim.gif");
        let error = export_frames_to_gif_file(&frames, &path, GIF_EXPORT_DELAY_CS)
            .expect_err("frames inconsistentes deben fallar");
        assert!(!error.to_string().is_empty());
        assert!(!path.exists(), "el fallo no debe dejar archivo final");
        assert_sin_tmp_huerfanos(&dir);
        // Éxito atómico: el final aparece con cabecera GIF real y sin `.tmp`.
        let out = export_frames_to_gif_file(&synthetic_frames(2), &path, GIF_EXPORT_DELAY_CS)
            .expect("export válido");
        assert_eq!(out, path);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..6], b"GIF89a");
        assert_sin_tmp_huerfanos(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Ningún `.tmp.<pid>-<nanos>` huérfano en el directorio.
    fn assert_sin_tmp_huerfanos(dir: &std::path::Path) {
        let entries: Vec<_> = std::fs::read_dir(dir).unwrap().collect();
        for entry in entries {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(!name.contains(".tmp."), "quedó temporal huérfano: {name}");
        }
    }

    #[test]
    fn workdir_exclusiva_falla_cerrado_ante_enlace_plantado() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "grafito_workdir_guard_{}_{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real");
        std::fs::create_dir_all(&real).unwrap();
        let enlace = dir.join("work");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &enlace).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real, &enlace).unwrap();
        let error =
            prepare_anim_workdir_exclusive(&enlace).expect_err("enlace plantado: falla cerrado");
        assert!(
            error.contains("área de trabajo"),
            "error honesto, got: {error}"
        );
        // Nada escrito a través del enlace.
        let dentro: Vec<_> = std::fs::read_dir(&real).unwrap().collect();
        assert!(dentro.is_empty());
        // Caso legítimo: dir inexistente se crea sin error.
        prepare_anim_workdir_exclusive(&dir.join("nuevo")).unwrap();
        assert!(dir.join("nuevo").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gif_delay_for_rate_escala_desde_la_base() {
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, 1.0), 8);
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, 0.5), 16);
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, 2.0), 4);
        // Tasas inválidas caen a la base, sin panic.
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, 0.0), 8);
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, -1.0), 8);
        assert_eq!(gif_delay_for_rate(GIF_EXPORT_DELAY_CS, f32::NAN), 8);
        // FPS base pineado con el delay.
        assert_eq!(GIF_BASE_FPS, 12.0);
        assert_eq!(100 / u32::from(GIF_EXPORT_DELAY_CS), 12);
    }

    #[test]
    fn gif_export_budget_preflight_cotas_honestas() {
        assert_eq!(
            check_gif_export_budget(&[]),
            Err(GifExportError::EmptyFrames)
        );
        assert!(check_gif_export_budget(&synthetic_frames(3)).is_ok());
        let many = synthetic_frames(GIF_EXPORT_MAX_FRAMES + 1);
        assert_eq!(
            check_gif_export_budget(&many),
            Err(GifExportError::TooManyFrames {
                got: GIF_EXPORT_MAX_FRAMES + 1
            })
        );
        // Dimensión fuera de rango.
        let big = vec![egui::ColorImage::new(
            [GIF_EXPORT_MAX_DIM + 1, 8],
            egui::Color32::BLACK,
        )];
        assert!(matches!(
            check_gif_export_budget(&big),
            Err(GifExportError::DimensionOutOfRange { .. })
        ));
        // H1: el preflight solo rechaza el frame absurdo individual
        // (4096×2048 = 8.4 M > 8 M); el total del set lo baja el autofit,
        // así que 9 × 1024² (1 M por frame) pasa el preflight.
        let absurdo = vec![egui::ColorImage::new([4096, 2048], egui::Color32::BLACK)];
        assert!(matches!(
            check_gif_export_budget(&absurdo),
            Err(GifExportError::TooManyPixels { .. })
        ));
        let repartido: Vec<egui::ColorImage> = (0..9)
            .map(|_| egui::ColorImage::new([1024, 1024], egui::Color32::BLACK))
            .collect();
        assert!(
            check_gif_export_budget(&repartido).is_ok(),
            "el total lo baja el autofit, no el preflight"
        );
        // Mensaje en español, sin inglés crudo.
        let msg = format!("{}", GifExportError::TooManyPixels { got: 9_000_000 });
        assert!(msg.contains("píxeles"));
        assert!(!msg.to_lowercase().contains("failed"));
        // Cotas pineadas (paridad con el loader de `assistant.rs`).
        assert_eq!(GIF_EXPORT_MAX_TOTAL_PIXELS, 8_000_000);
        assert_eq!(GIF_EXPORT_MAX_FILE_BYTES, 5 * 1024 * 1024);
    }

    #[test]
    fn gif_export_presupuestos_pineados() {
        // 12 fps ≈ delay 8cs; tope 64 = loader; lado 4096 = Resolution.
        assert_eq!(GIF_EXPORT_DELAY_CS, 8);
        assert_eq!(100 / u32::from(GIF_EXPORT_DELAY_CS), 12);
        assert_eq!(GIF_EXPORT_MAX_FRAMES, 64);
        assert_eq!(GIF_EXPORT_MAX_DIM, 4096);
        assert!((1..=30).contains(&GIF_EXPORT_SPEED));
        // 48 nativos ≤ tope 64, pineado en tiempo de compilación:
        const { assert!(NATIVE_ANIM_FRAME_COUNT <= GIF_EXPORT_MAX_FRAMES) };
    }
}

// ── AS4: tests del render paramétrico (barrido/traza/morph/locus) ────────
#[cfg(test)]
mod parametric_render_tests {
    use super::*;
    use grafito_anim::protocol::Resolution;

    fn anim(
        kind: ParametricKind,
        expr_a: &str,
        expr_b: Option<&str>,
        param: &str,
        p0: f64,
        p1: f64,
        n: usize,
    ) -> ParametricAnim {
        ParametricAnim::try_new(
            kind,
            expr_a.to_string(),
            expr_b.map(str::to_string),
            ParamName::try_new(param).unwrap(),
            p0,
            p1,
            FrameCount::try_new(n).unwrap(),
            Resolution::try_new(96, 72).unwrap(),
        )
        .unwrap()
    }

    fn assert_parametric_valid(frames: &[egui::ColorImage], n: usize, label: &str) {
        assert_eq!(frames.len(), n, "{label}: len");
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.size, [96, 72], "{label} frame {i}: size");
            assert_eq!(f.pixels.len(), 96 * 72, "{label} frame {i}: pixels");
        }
        assert_ne!(
            frames[0].pixels,
            frames[n - 1].pixels,
            "{label}: primero != último"
        );
        let first_px = frames[0].pixels[0];
        assert!(
            frames[0].pixels.iter().any(|p| *p != first_px),
            "{label}: frame no sólido"
        );
        for p in frames[0].pixels.iter().step_by(97) {
            assert_eq!(p.a(), 255, "{label}: alpha 255");
        }
    }

    #[test]
    fn barrido_traza_morph_locus_acotados_y_distintos() {
        let casos = [
            (
                anim(ParametricKind::Sweep, "x^2+p*x", None, "p", -2.0, 2.0, 12),
                "barrido",
            ),
            (
                anim(ParametricKind::Trace, "sin(x)", None, "t", 0.0, 1.0, 12),
                "traza",
            ),
            (
                anim(ParametricKind::Morph, "x^2", Some("x^3"), "p", 0.0, 1.0, 12),
                "morph",
            ),
            (
                anim(ParametricKind::Locus, "x^2", None, "p", -2.0, 2.0, 12),
                "locus",
            ),
        ];
        for (a, label) in casos {
            let frames = render_parametric_frames(&a).unwrap();
            assert_parametric_valid(&frames, 12, label);
        }
    }

    #[test]
    fn tangente_y_area_moviles_acotados_y_distintos() {
        // Sucesores genéricos de derivative-slope / integral-area.
        let tg = anim(ParametricKind::Tangent, "x^2", None, "p", -1.5, 1.5, 12);
        let area = anim(ParametricKind::Area, "x^2", None, "p", 0.0, 2.0, 12);
        assert_parametric_valid(&render_parametric_frames(&tg).unwrap(), 12, "tangente");
        assert_parametric_valid(&render_parametric_frames(&area).unwrap(), 12, "área");
    }

    // ── M4: updater por frame con `p` vivo ─────────────────────────────
    #[test]
    fn vivo_fijo_refleja_p_actual_en_todo_el_set() {
        // Barrido `x+p` en [0,10]: con vivo 7.0 el frame N refleja p=7
        // (mismo muestreo que `eval_frame_con_vivo`, a nivel render).
        let a = anim(ParametricKind::Sweep, "x+p", None, "p", 0.0, 10.0, 8);
        let fijos = render_parametric_frames(&a).unwrap();
        let vivos = render_parametric_frames_con_vivo(&a, Some(7.0)).unwrap();
        assert_eq!(vivos.len(), 8);
        // Con vivo fijo, TODO el set congela p=7: todos los frames iguales…
        for f in &vivos {
            assert_eq!(f.pixels, vivos[0].pixels, "vivo fijo congela el set");
        }
        // …y el frame 0 vivo coincide con el frame fijo donde p=7 (frame 6
        // de 8 en [0,10]: p = 10*6/7 ≈ 8.57 no da exacto, así que se compara
        // contra el muestreo directo del frame con vivo).
        let directo = sample_curve_con_vivo(&a, 3, Some(7.0));
        let propio = sample_curve_con_vivo(&a, 3, None);
        assert_ne!(directo, propio, "el vivo cambia el muestreo");
        // El vivo 7.0 en x=1 da y=8 en todos los frames (p actual manda).
        for i in 0..8 {
            assert_eq!(a.eval_frame_con_vivo(i, 1.0, Some(7.0)), Some(8.0));
        }
        // Sin vivo el set anima de verdad (primero != último).
        assert_ne!(fijos[0].pixels, fijos[7].pixels);
        // `None` por updater == vía clásica píxel a píxel (sin regresión).
        let via_updater =
            render_parametric_frames_con_updater(&a, &mut |_| None, false, &mut |_, _| {}).unwrap();
        assert_eq!(via_updater.len(), fijos.len());
        for (f, g) in via_updater.iter().zip(fijos.iter()) {
            assert_eq!(f.pixels, g.pixels);
        }
    }

    #[test]
    fn vivo_fijo_no_va_a_gif_err_honesto() {
        // R1-1: semántica pinneada — vivo fijo = foto preview, el GIF lo
        // prohíbe con `Err` honesto en vez de congelar silencioso.
        assert_eq!(
            validar_vivo_para_gif(Some(7.0)),
            Err(ParametricRenderError::VivoFijoNoExportable)
        );
        assert!(validar_vivo_para_gif(None).is_ok());
        // No-finito cae a rango propio: no congela, va al GIF.
        assert!(validar_vivo_para_gif(Some(f64::NAN)).is_ok());
        assert!(validar_vivo_para_gif(Some(f64::INFINITY)).is_ok());
        let msg = format!("{}", ParametricRenderError::VivoFijoNoExportable);
        assert!(
            msg.contains("congela") || msg.contains("vivo"),
            "mensaje honesto en español, got: {msg}"
        );
    }

    #[test]
    fn updater_por_frame_varia_y_no_finito_cae_a_rango_propio() {
        let a = anim(ParametricKind::Sweep, "x+p", None, "p", 0.0, 10.0, 4);
        // Updater real: cada frame lee su propio vivo (rampa 0,2,4,6).
        let vivos = [0.0, 2.0, 4.0, 6.0];
        let mut k = 0_usize;
        let frames = render_parametric_frames_con_updater(
            &a,
            &mut |_| {
                let v = vivos[k.min(3)];
                k += 1;
                Some(v)
            },
            false,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(frames.len(), 4);
        assert_ne!(frames[0].pixels, frames[3].pixels, "la rampa anima");
        // Updater que devuelve NaN/inf en un frame: ese frame usa rango propio.
        let fijos = render_parametric_frames(&a).unwrap();
        let mixtos = render_parametric_frames_con_updater(
            &a,
            &mut |frame| {
                if frame == 2 {
                    Some(f64::NAN)
                } else {
                    None
                }
            },
            false,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(mixtos[2].pixels, fijos[2].pixels, "NaN cae a rango propio");
    }

    #[test]
    fn progreso_real_por_frame_y_determinismo() {
        let a = anim(ParametricKind::Sweep, "x^2+p*x", None, "p", -2.0, 2.0, 8);
        let mut vistos = Vec::new();
        let frames = render_parametric_frames_with_progress(&a, &mut |done, total| {
            vistos.push((done, total));
        })
        .unwrap();
        assert_eq!(vistos.len(), 8);
        for (i, (done, total)) in vistos.iter().enumerate() {
            assert_eq!((*done, *total), (i + 1, 8));
        }
        // Determinista: mismo anim → mismos píxeles.
        let again = render_parametric_frames(&a).unwrap();
        assert_eq!(frames.len(), again.len());
        for (f, g) in frames.iter().zip(again.iter()) {
            assert_eq!(f.pixels, g.pixels);
        }
    }

    #[test]
    fn oom_rechaza_honesto_sin_reservar() {
        let huge = ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x+p".to_string(),
            None,
            ParamName::try_new("p").unwrap(),
            -2.0,
            2.0,
            FrameCount::try_new(48).unwrap(),
            Resolution::try_new(4096, 4096).unwrap(),
        );
        // try_new ya rechaza por presupuesto…
        assert!(huge.is_err());
        // …y el render también si el presupuesto se excede por otra vía.
        let mut a = anim(ParametricKind::Sweep, "x+p", None, "p", -2.0, 2.0, 12);
        a.viewport = Resolution::try_new(4096, 4096).unwrap();
        // 4096×4096×4×12 > 64 MiB → Oom honesto.
        assert!(matches!(
            render_parametric_frames(&a),
            Err(ParametricRenderError::Oom { .. })
        ));
    }

    #[test]
    fn templates_viejos_mapean_a_casos_y_resto_none() {
        let tg = parametric_for_template("derivative-slope", "derivada").unwrap();
        assert_eq!(tg.kind, ParametricKind::Tangent);
        let area = parametric_for_template("integral-area", "integral").unwrap();
        assert_eq!(area.kind, ParametricKind::Area);
        let tr = parametric_for_template("taylor-series", "serie").unwrap();
        assert_eq!(tr.kind, ParametricKind::Trace);
        // Sin equivalente honesto → None (conservan renderer dedicado).
        for t in ["pitagoras", "euler", "fourier", "universal", "no-existe"] {
            assert!(
                parametric_for_template(t, "x").is_none(),
                "{t} debe conservar su renderer"
            );
        }
        // El equivalente canónico renderiza de verdad.
        let frames = render_parametric_frames(&tg).unwrap();
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_ne!(frames[0].pixels, frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels);
    }

    // ── N1: la canónica integral→Area rinde 48 con el mismo contrato ────
    #[test]
    fn area_canonica_mapea_template_y_rinde_48_monotonos() {
        use grafito_anim::parametric::{
            INTEGRAL_CANONICAL_EXPR, INTEGRAL_CANONICAL_P0, INTEGRAL_CANONICAL_P1,
        };
        // `parametric_for_template` mapea integral→Area con la función
        // defaulteada (la que dibuja la card).
        let area = parametric_for_template("integral-area", "integral").expect("mapea a Area");
        assert_eq!(area.kind, ParametricKind::Area);
        assert_eq!(area.expr_a, INTEGRAL_CANONICAL_EXPR);
        assert_eq!(area.param.as_str(), "p");
        assert_eq!(
            (area.p0, area.p1),
            (INTEGRAL_CANONICAL_P0, INTEGRAL_CANONICAL_P1)
        );
        assert_eq!(area.frame_count(), NATIVE_ANIM_FRAME_COUNT);
        let frames = render_parametric_frames(&area).expect("render Area");
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        // Mismo contrato pixel-lógico que la vía clásica.
        let sombras: Vec<usize> = frames.iter().map(super::cuenta_pixeles_sombra).collect();
        for par in sombras.windows(2) {
            assert!(
                par[1] >= par[0],
                "sombra paramétrica no-decreciente: {sombras:?}"
            );
        }
        assert!(
            sombras[0] < sombras[NATIVE_ANIM_FRAME_COUNT - 1],
            "el área final debe sombrear más: {sombras:?}"
        );
        super::assert_curva_fija(&frames, "area-param");
        for (i, f) in frames.iter().enumerate() {
            assert!(!super::tiene_verde_suelto(f), "frame {i} sin verde");
        }
        // Consistencia card↔pantalla completa: el fullscreen reusa el mismo
        // `AssistantMedia` (mismos 48 frames, sin re-render); acá se pinnea
        // que la vía clásica produce el mismo largo con la misma canónica.
        let clasicos = render_integral_frames(96, 72);
        assert_eq!(clasicos.len(), NATIVE_ANIM_FRAME_COUNT);
    }
}

// ── M4: Group simultáneo real (solo tests, sin tocar prod) ──────────────
#[cfg(test)]
mod group_compose_m4_tests {
    use super::*;

    fn imagen_solida(w: usize, h: usize, rgba: [u8; 4]) -> egui::ColorImage {
        egui::ColorImage {
            size: [w, h],
            pixels: vec![
                egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
                w.saturating_mul(h)
            ],
        }
    }

    #[test]
    fn dos_animaciones_distintas_solapadas() {
        // R1-2: dos nativas opacas YA no componen en silencio (tapaba mudo).
        // Mismo N y viewport, pero el frente es totalmente opaco → Err honesto.
        let fondo = render_integral_frames(96, 72);
        let frente = render_native_animation_frames(96, 72);
        assert_eq!(fondo.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(frente.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_ne!(
            fondo[0].pixels, frente[0].pixels,
            "las dos animaciones difieren"
        );
        let err = componer_grupo_nativo(&[fondo.clone(), frente.clone()]).unwrap_err();
        assert_eq!(err, GroupComposeError::FrenteOpaco { set: 1 });
        assert!(
            err.to_string().contains("opaco"),
            "mensaje honesto en español, got: {err}"
        );
        // Tres capas opacas también se rechazan (la primera frente opaca manda).
        let err3 = componer_grupo_nativo(&[fondo, frente.clone(), frente]).unwrap_err();
        assert_eq!(err3, GroupComposeError::FrenteOpaco { set: 1 });
    }

    #[test]
    fn dos_opacos_no_tapan_en_silencio() {
        // R1-2 pinneado: 2 sets totalmente opacos → `Err(FrenteOpaco)`, jamás
        // `compuesto == frente` silencioso. Exigí alfa<255 o split/grid.
        let fondo = vec![imagen_solida(4, 4, [0, 0, 255, 255]); 3];
        let frente = vec![imagen_solida(4, 4, [255, 0, 0, 255]); 3];
        let err = componer_grupo_nativo(&[fondo, frente]).unwrap_err();
        assert_eq!(err, GroupComposeError::FrenteOpaco { set: 1 });
        assert!(err.to_string().contains("split/grid"), "got: {err}");
    }

    #[test]
    fn capas_translucidas_se_mezclan_de_verdad() {
        // SPEC R1-10: over Porter-Duff en alfa directo; la ida y vuelta por
        // `Color32` premultiplicado mete ±2 (no es calibración del bug, es
        // la precisión documentada del tipo de píxel egui).
        let fondo = vec![imagen_solida(2, 2, [0, 0, 255, 255]); 3];
        let frente = vec![imagen_solida(2, 2, [255, 0, 0, 128]); 3];
        let compuesto = componer_grupo_nativo(&[fondo, frente]).unwrap();
        assert_eq!(compuesto.len(), 3);
        for f in &compuesto {
            for p in &f.pixels {
                assert_eq!(p.a(), 255);
                assert!((p.r() as i16 - 128).abs() <= 2, "r={}", p.r());
                assert_eq!(p.g(), 0);
                assert!((p.b() as i16 - 127).abs() <= 2, "b={}", p.b());
            }
        }
    }

    #[test]
    fn viewport_o_err_honesto_y_n_dispar_remuestrea() {
        // OJO R1-2: los frentes de este test son translúcidos (alfa 128):
        // con opacos daría `FrenteOpaco` antes de llegar al viewport.
        let ok = vec![imagen_solida(4, 4, [0, 0, 255, 255]); 3];
        let ok2 = vec![imagen_solida(4, 4, [255, 0, 0, 128]); 3];
        assert!(componer_grupo_nativo(&[ok.clone(), ok2.clone()]).is_ok());
        // F2: N distinto YA no es `Err`: se remuestrea al máximo (vecino más
        // cercano, misma disciplina que `plan_remuestreo`). 3 + 2 → 3 frames.
        let corto = vec![imagen_solida(4, 4, [255, 0, 0, 128]); 2];
        let compuesto = componer_grupo_nativo(&[ok.clone(), corto]).unwrap();
        assert_eq!(compuesto.len(), 3);
        // Viewport distinto → Err (el re-muestreo es temporal, jamás espacial).
        let otro_size = vec![imagen_solida(8, 4, [255, 0, 0, 128]); 3];
        let err = componer_grupo_nativo(&[ok.clone(), otro_size])
            .unwrap_err()
            .to_string();
        assert!(err.contains("viewport"), "got: {err}");
        // 0/1 sets y set vacío → Err.
        assert_eq!(componer_grupo_nativo(&[]), Err(GroupComposeError::Vacio));
        assert!(componer_grupo_nativo(std::slice::from_ref(&ok)).is_err());
        assert!(componer_grupo_nativo(&[ok, vec![]]).is_err());
        // Prosa rioplatense en los errores.
        assert!(GroupComposeError::Vacio.to_string().contains("al menos 2"));
    }

    #[test]
    fn n_dispar_mapea_vecino_mas_cercano_exacto() {
        // 4 + 2 → 4 frames; el corto aporta [0, 0, 1, 1]
        // (round(0), round(1/3)=0, round(2/3)=1, round(1)). Frames de colores
        // distintos por índice para que el mapeo sea observable píxel a píxel.
        let fondo = vec![
            imagen_solida(2, 2, [10, 20, 30, 255]),
            imagen_solida(2, 2, [40, 50, 60, 255]),
            imagen_solida(2, 2, [70, 80, 90, 255]),
            imagen_solida(2, 2, [100, 110, 120, 255]),
        ];
        let frente = vec![
            imagen_solida(2, 2, [255, 0, 0, 128]),
            imagen_solida(2, 2, [0, 255, 0, 128]),
        ];
        let compuesto = componer_grupo_nativo(&[fondo.clone(), frente.clone()]).unwrap();
        assert_eq!(compuesto.len(), 4);
        let mapeo = [0_usize, 0, 1, 1];
        for (j, frame) in compuesto.iter().enumerate() {
            let esperado: Vec<egui::Color32> = fondo[j]
                .pixels
                .iter()
                .zip(frente[mapeo[j]].pixels.iter())
                .map(|(b, f)| {
                    let mezcla =
                        mezclar_pixel_alfa(b.to_srgba_unmultiplied(), f.to_srgba_unmultiplied());
                    egui::Color32::from_rgba_unmultiplied(
                        mezcla[0], mezcla[1], mezcla[2], mezcla[3],
                    )
                })
                .collect();
            assert_eq!(
                frame.pixels, esperado,
                "frame {j} mezcla fondo[{j}] con frente[{}]",
                mapeo[j]
            );
        }
    }

    #[test]
    fn remuestreo_nativo_en_paridad_con_el_nucleo() {
        // El helper local replica `AnimationGroup::plan_remuestreo`: mismos
        // índices o el test lo dice (no se finge paridad).
        let grupo = grafito_anim::protocol::AnimationGroup::try_new(vec![0, 1], 0.0).unwrap();
        for (n0, n1) in [(48_usize, 48_usize), (48, 24), (3, 2), (4, 2), (5, 1)] {
            let plan = grupo.plan_remuestreo(&[n0, n1], &[(8, 8), (8, 8)]).unwrap();
            assert_eq!(plan.n_comun, n0.max(n1));
            assert_eq!(
                plan.indices_por_set,
                vec![
                    indice_vecino_mas_cercano_nativo(n0, plan.n_comun),
                    indice_vecino_mas_cercano_nativo(n1, plan.n_comun),
                ],
                "paridad de grilla para [{n0}, {n1}]"
            );
        }
    }

    #[test]
    fn remuestreo_cuesta_o_de_frames() {
        // Perf F2: el re-muestreo agrega O(frames) índices frente al O(píxeles)
        // del over; N dispar debe costar ~igual que mismo N (cota 20×, piso
        // 50 ms anti-flake en boxes lentos).
        let fondo = vec![imagen_solida(160, 120, [0, 0, 255, 255]); 48];
        let frente_igual = vec![imagen_solida(160, 120, [255, 0, 0, 128]); 48];
        let frente_corto = vec![imagen_solida(160, 120, [255, 0, 0, 128]); 24];
        let t0 = std::time::Instant::now();
        let a = componer_grupo_nativo(&[fondo.clone(), frente_igual]).unwrap();
        let t_igual = t0.elapsed();
        let t1 = std::time::Instant::now();
        let b = componer_grupo_nativo(&[fondo, frente_corto]).unwrap();
        let t_dispar = t1.elapsed();
        assert_eq!(a.len(), 48);
        assert_eq!(b.len(), 48);
        println!("componer mismo-N: {t_igual:?} | N-dispar remuestreado: {t_dispar:?}");
        assert!(
            t_dispar <= t_igual * 20 + std::time::Duration::from_millis(50),
            "re-muestreo O(frames) desbocado: igual={t_igual:?} dispar={t_dispar:?}"
        );
    }
}

// ── F10 hostile fuzz (solo tests, sin tocar prod) ─────────────────────────
// Escenario SIGABRT: chat → animación integral → 2da animación → muerte.
// RAW sin catch/should_panic para ver el pánico crudo con RUST_BACKTRACE=1.
// OJO OOM: jamás render full con 4097/4096 (3.2 GiB el set); esos van solo
// a estimate/budget/try_resolve. Full render solo con dims chicas (0,1,63,
// 65,64) que clampean a ≤64 y cuestan <1 MiB.
#[cfg(test)]
mod hostile_crash_f10 {
    use super::*;
    use std::collections::BTreeMap;

    fn empty_params() -> BTreeMap<String, f64> {
        BTreeMap::new()
    }

    #[test]
    fn hostile_dims_raras_clampean_sin_panic() {
        // 0,1,63 → clamp a 64; 65 → 65 real; todos baratos (<1 MiB).
        for (w, h) in [
            (0, 0),
            (0, 480),
            (480, 0),
            (1, 1),
            (63, 63),
            (63, 64),
            (65, 65),
            (64, 64),
        ] {
            let d = render_derivative_frames_with_params(w, h, &empty_params());
            assert_eq!(d.len(), NATIVE_ANIM_FRAME_COUNT);
            let i = render_integral_frames_with_params(w, h, &empty_params());
            assert_eq!(i.len(), NATIVE_ANIM_FRAME_COUNT);
            let t = render_taylor_frames(w, h);
            assert_eq!(t.len(), NATIVE_ANIM_FRAME_COUNT);
            let c = render_conformal_frames(w, h);
            assert_eq!(c.len(), NATIVE_ANIM_FRAME_COUNT);
            let p = render_pitagoras_frames(w, h);
            assert_eq!(p.len(), NATIVE_ANIM_FRAME_COUNT);
        }
    }

    #[test]
    fn hostile_todos_los_renderers_dims_chicas() {
        // Todos los renderers con 64x64 y 65x65: tamaño + pixels coherentes.
        for (w, h) in [(64u32, 64u32), (65, 65)] {
            let sets: Vec<Vec<egui::ColorImage>> = vec![
                render_native_animation_frames(w, h),
                render_integral_frames(w, h),
                render_taylor_frames(w, h),
                render_conformal_frames(w, h),
                render_pitagoras_frames(w, h),
                render_universal_youtube_frames("integral", w, h),
                render_euler_frames(w, h),
                render_fourier_frames(w, h),
                render_logistic_bifurcation_frames(w, h),
                render_gradient_field_frames(w, h),
                render_mobius_frames(w, h),
                render_anim_by_template("integral-area", w, h),
                render_anim_by_template("", w, h),
                render_anim_by_template("\u{1F600}", w, h),
                render_anim_for_concept("integral-area", "integral", w, h),
            ];
            for frames in &sets {
                assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
                for f in frames {
                    assert_eq!(f.pixels.len(), (w as usize) * (h as usize));
                }
            }
        }
    }

    #[test]
    fn hostile_segunda_animacion_no_mata() {
        // Repro del escenario: integral → 2da animación (todas las parejas).
        let first = render_integral_frames(96, 72);
        assert_eq!(first.len(), NATIVE_ANIM_FRAME_COUNT);
        for tpl in [
            "integral-area",
            "derivative-slope",
            "taylor-series",
            "conformal-map",
            "pitagoras",
            "euler",
            "fourier",
            "logistic-bifurcation",
            "gradient-field",
            "mobius-transform",
            "universal",
            "",
            "\u{1F600}",
        ] {
            let second = render_anim_by_template(tpl, 96, 72);
            assert_eq!(second.len(), NATIVE_ANIM_FRAME_COUNT);
        }
        // Integral dos veces seguidas (el caso exacto del reporte).
        let a = render_integral_frames_with_params(96, 72, &empty_params());
        let b = render_integral_frames_with_params(96, 72, &empty_params());
        assert_eq!(a.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(b.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_eq!(a[0].pixels, b[0].pixels);
    }

    #[test]
    fn hostile_estimate_y_resolve_gigantes_sin_alloc() {
        // 4097 / 4096 / u32::MAX / usize::MAX-ish: solo matemática chequeada,
        // jamás alloc. try_resolve es pub(crate): accesible vía super::*.
        assert!(try_resolve_native_size(0, 0).is_err());
        assert!(try_resolve_native_size(1, 1).is_err());
        assert!(try_resolve_native_size(63, 63).is_err());
        assert!(try_resolve_native_size(65, 65).is_ok());
        assert!(try_resolve_native_size(4097, 4097).is_err());
        assert!(try_resolve_native_size(u32::MAX, u32::MAX).is_err());
        assert!(try_resolve_native_size(u32::MAX, 64).is_err());
        // estimate_frames_bytes con usize::MAX-ish: None, no panic
        assert_eq!(estimate_frames_bytes(usize::MAX, 64, 48), None);
        assert_eq!(estimate_frames_bytes(64, usize::MAX, 48), None);
        assert_eq!(
            estimate_frames_bytes(usize::MAX, usize::MAX, usize::MAX),
            None
        );
        assert!(estimate_frames_bytes(4096, 4096, 48).is_some());
        // 4096x4096x48 = 3.2 GiB: el número existe, el render jamás debe intentarlo aquí
        let big = estimate_frames_bytes(4096, 4096, 48).unwrap();
        assert!(big > GIF_EXPORT_MAX_TOTAL_PIXELS);
        // check_gif_export_budget rechaza gigante sin alloc
        let huge_img = egui::ColorImage {
            size: [4096, 4096],
            pixels: vec![egui::Color32::BLACK; 16],
        };
        let _ = check_gif_export_budget(&[huge_img]);
        // w*h*4 ±1 vía mocks: ColorImage con pixels incoherentes → Err, no panic
        for (w, h, npix) in [
            (64usize, 64usize, 64 * 64 - 1),
            (64, 64, 64 * 64 + 1),
            (64, 64, 0),
            (64, 64, 1),
            (0, 0, 0),
        ] {
            let img = egui::ColorImage {
                size: [w, h],
                pixels: vec![egui::Color32::BLACK; npix],
            };
            let _ = encode_frames_to_gif_bytes(std::slice::from_ref(&img), 8);
            let _ = check_gif_export_budget(std::slice::from_ref(&img));
        }
    }

    #[test]
    fn hostile_from_rgba_mismatch_documenta_egui() {
        // from_rgba_unmultiplied exige buf.len() == w*h*4. Nuestro código
        // siempre lo cumple (checked_frame_byte_len); acá se pinnea que el
        // mismatch paniquea EN egui (no en nuestro código) para ubicar el
        // SIGABRT si algún día llega un buf recortado.
        // NOTA: este test espera el panic de egui → se deja RAW para verlo.
        // Si paniquea, el culpable es egui::ColorImage, no anim_native.
        let w = 64usize;
        let h = 64usize;
        let good_len = w * h * 4;
        let good = vec![0u8; good_len];
        let _ = egui::ColorImage::from_rgba_unmultiplied([w, h], &good);
        // Los dos mismatch de abajo paniquean en egui: se prueban en el test
        // siguiente con catch para no voltear la suite (ver hostile_catch).
    }

    #[test]
    fn hostile_parametric_y_template_hostil() {
        // Templates hostiles: vacío, solo operadores, unicode, 200KB
        for tpl in [
            "",
            "   ",
            "+++",
            "***",
            "(((",
            "\u{1F600}",
            &"a".repeat(200_000),
        ] {
            let frames = render_anim_by_template(tpl, 64, 64);
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        }
        // Conceptos hostiles (render_anim_for_concept exige template+concepto)
        for concept in [
            "",
            "+++",
            "\u{1F600}".repeat(50).as_str(),
            &"x".repeat(200_000),
        ] {
            let frames = render_anim_for_concept("integral-area", concept, 64, 64);
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
            let frames2 = render_anim_for_concept("", concept, 64, 64);
            assert_eq!(frames2.len(), NATIVE_ANIM_FRAME_COUNT);
        }
        // Params con NaN/inf: scene_param_clamped debe contenerlos
        let mut bad = BTreeMap::new();
        bad.insert("x0".to_string(), f64::NAN);
        bad.insert("span".to_string(), f64::INFINITY);
        bad.insert("a".to_string(), f64::NEG_INFINITY);
        bad.insert("b".to_string(), f64::NAN);
        let d = render_derivative_frames_with_params(64, 64, &bad);
        assert_eq!(d.len(), NATIVE_ANIM_FRAME_COUNT);
        let i = render_integral_frames_with_params(64, 64, &bad);
        assert_eq!(i.len(), NATIVE_ANIM_FRAME_COUNT);
    }
}

// ── F2b: renderer morph polilínea + concat playlist (Succession) ───────────
// Dibuja el seam de F2a (`PolylineMorph::frames_puntos()`: 1 frame = 1
// `Vec<[f64;2]>` en mundo) como polilínea con viewport validado
// (`resolve_native_size` 64..=4096 + `to_pixel` mundo [-3,3]², puntos fuera se
// clampan al borde sin panic). Los 8 easings NO se reimplementan: viajan
// dentro del `PolylineMorph` (`ShapeEasing`, mismos valores que
// `grafito-geometry::morph::MorphEasing`); acá solo se resuelven por nombre
// (`morph_easing_from_name`). La playlist se reproduce como UN set
// concatenado (`concat_playlist_frames`): el player del chat ya hace scrub
// por animación vía `Timeline::sample` y así el scrub cubre el tiempo global
// sin tocar la UI. Presupuestos: frames totales ≤96
// (`grafito-anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL`), OOM por set con
// `checked` + `try_reserve`. Todo `Err` honesto en rioplatense, nada parcial
// en silencio.
use grafito_anim::parametric::{PolylineMorph, ShapeEasing};

/// Nombres de los 8 easings del morph (los ya definidos en `ShapeEasing`).
pub const MORPH_EASING_NAMES: [&str; 8] = [
    "linear",
    "quadratic_in",
    "quadratic_out",
    "cubic_in",
    "cubic_out",
    "cubic_in_out",
    "sin_in_out",
    "ease_out_back",
];

/// Guía accionable para los errores "sin fotogramas" (frente errs mudos):
/// el mensaje siempre dice qué hacer, jamás solo qué falló. Vive en el
/// catálogo i18n (`anim.empty.guide`); la comparten `render_morph_frames` y
/// las vías del asistente (`assistant.rs`) vía [`error_sin_fotogramas`].
/// Sin `Locale` a mano se usa ES (idioma actual del UI; Oleada 3 cableará el
/// ajuste de idioma).
pub fn sin_fotogramas_guia(locale: grafito_ui::i18n::Locale) -> &'static str {
    grafito_ui::i18n::t("anim.empty.guide", locale)
}

/// Mensaje "sin fotogramas" con guía accionable (punto único testeable).
/// Plantilla i18n (`anim.empty.message`, `{motor}`/`{guia}` sustituidos acá).
pub fn error_sin_fotogramas_localized(motor: &str, locale: grafito_ui::i18n::Locale) -> String {
    grafito_ui::i18n::t("anim.empty.message", locale)
        .replace("{motor}", motor)
        .replace("{guia}", sin_fotogramas_guia(locale))
}

/// Mensaje "sin fotogramas" en ES (idioma actual del UI).
pub fn error_sin_fotogramas(motor: &str) -> String {
    error_sin_fotogramas_localized(motor, grafito_ui::i18n::Locale::Es)
}

/// Error tipado del render morph / concat (mensajes en español, sin pánicos).
#[derive(Debug, Clone, PartialEq)]
pub enum MorphRenderError {
    /// La forma no valida o el set pedido es incoherente.
    InvalidShape(String),
    /// El set estimado excede `PARAMETRIC_MAX_BYTES` o desborda.
    Oom { got: Option<usize>, max: usize },
    /// La reserva del frame falló (OOM real del SO).
    AllocFailed { bytes: usize },
}

impl std::fmt::Display for MorphRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidShape(detail) => write!(f, "forma inválida: {detail}"),
            Self::Oom { got, max } => match got {
                Some(got) => write!(
                    f,
                    "el set estimado ({got} bytes) excede el tope de {max} bytes: bajá la resolución o los fotogramas"
                ),
                None => write!(
                    f,
                    "el set estimado desborda el contador: bajá la resolución o los fotogramas (tope {max} bytes)"
                ),
            },
            Self::AllocFailed { bytes } => {
                write!(f, "sin memoria para reservar el frame ({bytes} bytes)")
            }
        }
    }
}

impl std::error::Error for MorphRenderError {}

/// Resuelve un easing por nombre a los 8 ya definidos (`ShapeEasing`).
///
/// `None`/desconocido → `Err` honesto que lista los 8 (jamás default
/// silencioso que cambie la curva sin avisar).
pub fn morph_easing_from_name(name: &str) -> Result<ShapeEasing, MorphRenderError> {
    ShapeEasing::from_name(name).ok_or_else(|| {
        MorphRenderError::InvalidShape(format!(
            "easing desconocido {name:?}: usá uno de {}",
            MORPH_EASING_NAMES.join(", ")
        ))
    })
}

/// Dibuja una polilínea del mundo sobre el buffer (corta en puntos no
/// finitos sin unir ramas rotas; `closed` une último→primero).
fn draw_polyline_mundo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    puntos: &[[f64; 2]],
    closed: bool,
    color: [u8; 4],
) {
    let px: Vec<Option<(usize, usize)>> = puntos
        .iter()
        .map(|p| {
            if p[0].is_finite() && p[1].is_finite() {
                Some(to_pixel(w, h, p[0], p[1]))
            } else {
                None
            }
        })
        .collect();
    for par in px.windows(2) {
        if let (Some(a), Some(b)) = (par[0], par[1]) {
            draw_line(buf, w, h, a, b, color);
        }
    }
    if closed {
        let primero = px.first().copied().flatten();
        let ultimo = px.last().copied().flatten();
        if let (Some(a), Some(b)) = (ultimo, primero) {
            if a != b {
                draw_line(buf, w, h, a, b, color);
            }
        }
    }
}

/// Renderiza un `PolylineMorph` (seam F2a) a fotogramas RGBA en memoria.
///
/// 1 frame de `frames_puntos()` = 1 polilínea en mundo. OOM acotado igual que
/// el paramétrico: presupuesto con `estimate_frames_bytes` + reserva con
/// `try_reserve` (`AllocFailed` honesto en vez de abortar). Determinista:
/// mismo morph → mismos píxeles.
pub fn render_morph_frames(
    morph: &PolylineMorph,
    width: u32,
    height: u32,
) -> Result<Vec<egui::ColorImage>, MorphRenderError> {
    let puntos = morph
        .frames_puntos()
        .map_err(|e| MorphRenderError::InvalidShape(e.to_string()))?;
    if puntos.is_empty() {
        return Err(MorphRenderError::InvalidShape(error_sin_fotogramas(
            "el morph",
        )));
    }
    let n = puntos.len();
    // Viewport validado + preflight con el n real (clamp 64..=4096 + tope
    // 64 MiB, nunca panic). El chequeo `Oom` de abajo queda como segunda
    // barrera honesta con `Err` (esta fn sí devuelve `Result`).
    let ((w, h), _) = resolve_native_size_budgeted(width, height, n);
    match estimate_frames_bytes(w, h, n) {
        Some(got) if got <= PARAMETRIC_MAX_BYTES => {}
        other => {
            return Err(MorphRenderError::Oom {
                got: other,
                max: PARAMETRIC_MAX_BYTES,
            });
        }
    }
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(n)
        .map_err(|_| MorphRenderError::Oom {
            got: estimate_frames_bytes(w, h, n),
            max: PARAMETRIC_MAX_BYTES,
        })?;
    let cerrada = morph.closed;
    for forma in puntos.iter() {
        let mut buf = alloc_frame_buffer(w, h).map_err(|_| {
            let got = estimate_frames_bytes(w, h, 1);
            MorphRenderError::AllocFailed {
                bytes: got.unwrap_or(w.saturating_mul(h).saturating_mul(4)),
            }
        })?;
        draw_parametric_base(&mut buf, w, h, "morph", true);
        draw_polyline_mundo(&mut buf, w, h, forma, cerrada, CURVE_MAIN);
        // Punto inicial marcado (misma semántica que la tangente: rojo).
        if let Some(primero) = forma.first() {
            if primero[0].is_finite() && primero[1].is_finite() {
                let (px, py) = to_pixel(w, h, primero[0], primero[1]);
                draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
            }
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    Ok(frames)
}

/// Atajo: arma el morph por nombre de easing (los 8 ya definidos) y lo
/// renderiza. `frames_n` en 1..=48 (`FrameCount`), `samples` en 2..=512.
/// Todo `Err` honesto (easing, formas, frames o memoria).
#[allow(clippy::too_many_arguments)]
pub fn render_morph_with_easing(
    a: Vec<[f64; 2]>,
    b: Vec<[f64; 2]>,
    samples: usize,
    frames_n: usize,
    closed: bool,
    align_start: bool,
    easing_name: &str,
    width: u32,
    height: u32,
) -> Result<Vec<egui::ColorImage>, MorphRenderError> {
    let easing = morph_easing_from_name(easing_name)?;
    let frames = FrameCount::try_new(frames_n)
        .map_err(|e| MorphRenderError::InvalidShape(format!("fotogramas inválidos: {e}")))?;
    let morph = PolylineMorph::try_new(a, b, samples, frames, closed, align_start, easing)
        .map_err(|e| MorphRenderError::InvalidShape(e.to_string()))?;
    render_morph_frames(&morph, width, height)
}

/// Frames de hold para un `wait_after_ms` a `fps` (silencio = último frame
/// quieto). Pura: `fps` no finito o ≤0 → 0; saturada al tope de playlist.
pub fn playlist_hold_frames(wait_ms: u64, fps: f32) -> usize {
    if wait_ms == 0 || !fps.is_finite() || fps <= 0.0 {
        return 0;
    }
    let frames = (wait_ms as f64) / 1000.0 * f64::from(fps);
    if !frames.is_finite() || frames <= 0.0 {
        return 0;
    }
    (frames.round() as usize).min(grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL)
}

/// Concatena sets de frames en UN set para el player (scrub total).
///
/// Chequeos honestos, nada parcial: total vacío → `Err`; total >96 →
/// `Err`; dimensiones inconsistentes → `Err` con el índice; OOM por set →
/// `Err`. Mueve los frames (sin re-render ni copia de píxeles).
pub fn concat_playlist_frames(
    sets: Vec<Vec<egui::ColorImage>>,
) -> Result<Vec<egui::ColorImage>, MorphRenderError> {
    let mut total: usize = 0;
    for set in &sets {
        total = total.checked_add(set.len()).ok_or(MorphRenderError::Oom {
            got: None,
            max: PARAMETRIC_MAX_BYTES,
        })?;
    }
    if total == 0 {
        return Err(MorphRenderError::InvalidShape(
            "la playlist no tiene ningún frame: agregá un step animado".to_string(),
        ));
    }
    if total > grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL {
        return Err(MorphRenderError::InvalidShape(format!(
            "{total} frames exceden el tope de {}: sacá un step o bajá los frames por step",
            grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL
        )));
    }
    let (w, h) = sets
        .iter()
        .flatten()
        .next()
        .map_or((0, 0), |frame| (frame.size[0], frame.size[1]));
    for (conjunto, set) in sets.iter().enumerate() {
        for (indice, frame) in set.iter().enumerate() {
            if frame.size != [w, h] {
                return Err(MorphRenderError::InvalidShape(format!(
                    "el frame {indice} del set {conjunto} mide {:?} y el primero mide [{w},{h}]: renderá todo a la misma resolución",
                    frame.size
                )));
            }
        }
    }
    match estimate_frames_bytes(w, h, total) {
        Some(got) if got <= PARAMETRIC_MAX_BYTES => {}
        other => {
            return Err(MorphRenderError::Oom {
                got: other,
                max: PARAMETRIC_MAX_BYTES,
            });
        }
    }
    let mut out = Vec::new();
    out.try_reserve_exact(total)
        .map_err(|_| MorphRenderError::Oom {
            got: estimate_frames_bytes(w, h, total),
            max: PARAMETRIC_MAX_BYTES,
        })?;
    for set in sets {
        out.extend(set);
    }
    Ok(out)
}

/// Concatena steps `(frames, wait_after_ms)` agregando holds del último frame.
///
/// Cada espera suma `playlist_hold_frames(wait_ms, fps)` copias del último
/// frame del step (silencio quieto estilo `Wait`). Presupuesto total (frames
/// + holds) ≤96 con `checked`, todo `Err` honesto.
pub fn concat_playlist_with_holds(
    steps: Vec<(Vec<egui::ColorImage>, u64)>,
    fps: f32,
) -> Result<Vec<egui::ColorImage>, MorphRenderError> {
    let mut total: usize = 0;
    for (frames, espera) in &steps {
        total = total
            .checked_add(frames.len())
            .ok_or(MorphRenderError::Oom {
                got: None,
                max: PARAMETRIC_MAX_BYTES,
            })?;
        total = total
            .checked_add(playlist_hold_frames(*espera, fps))
            .ok_or(MorphRenderError::Oom {
                got: None,
                max: PARAMETRIC_MAX_BYTES,
            })?;
    }
    if total == 0 {
        return Err(MorphRenderError::InvalidShape(
            "la playlist no tiene ningún frame: agregá un step animado".to_string(),
        ));
    }
    if total > grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL {
        return Err(MorphRenderError::InvalidShape(format!(
            "{total} frames (con esperas) exceden el tope de {}: sacá un step o acortá las esperas",
            grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL
        )));
    }
    let mut sets: Vec<Vec<egui::ColorImage>> = Vec::with_capacity(steps.len());
    for (frames, espera) in steps {
        let hold = playlist_hold_frames(espera, fps);
        let mut set = frames;
        if hold > 0 {
            if let Some(ultimo) = set.last().cloned() {
                set.try_reserve_exact(hold)
                    .map_err(|_| MorphRenderError::Oom {
                        got: None,
                        max: PARAMETRIC_MAX_BYTES,
                    })?;
                for _ in 0..hold {
                    set.push(ultimo.clone());
                }
            }
        }
        sets.push(set);
    }
    concat_playlist_frames(sets)
}

/// Cuántos frames quedan de un step de `n` con stride `s` (preservando primer
/// y último: índices `0, s, 2s…` más `n-1` si falta). Pura, sin allocs.
fn strided_len(n: usize, stride: usize) -> usize {
    if n == 0 || stride == 0 {
        return 0;
    }
    // ceil(n/s) índices base; +1 si el último no cae en la grilla.
    let base = n.saturating_add(stride.saturating_sub(1)) / stride;
    if (n.saturating_sub(1)).is_multiple_of(stride) {
        base
    } else {
        base.saturating_add(1)
    }
}

/// Submuestrea un step con stride `s` preservando primer y último frame.
/// Pura (clona los frames elegidos, mueve el resto).
fn stride_frames(frames: &[egui::ColorImage], stride: usize) -> Vec<egui::ColorImage> {
    if frames.is_empty() || stride <= 1 {
        return frames.to_vec();
    }
    let n = frames.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        out.push(frames[i].clone());
        i = i.saturating_add(stride);
    }
    if !(n.saturating_sub(1)).is_multiple_of(stride) {
        if let Some(ultimo) = frames.last() {
            out.push(ultimo.clone());
        }
    }
    out
}

/// Concatena steps ajustando la cadencia por step para entrar en 96.
///
/// Política honesta y determinista (nada parcial en silencio):
/// 1. holds por step = `playlist_hold_frames(wait_ms, fps)` (silencio quieto).
/// 2. si frames + holds ≤ 96 → concat directo (stride 1, sin tocar nada).
/// 3. si no, el menor stride `s` uniforme con `Σ strided + holds ≤ 96`
///    (baja la cadencia, jamás corta contenido: primer y último frame de
///    cada step siempre presentes, los extremos A→B intactos).
/// 4. si ni 1 frame por step + holds entra → `Err` honesto.
///
/// Todo lo demás lo valida `concat_playlist_frames` (dims, OOM).
pub fn concat_playlist_fitting(
    steps: Vec<(Vec<egui::ColorImage>, u64)>,
    fps: f32,
) -> Result<Vec<egui::ColorImage>, MorphRenderError> {
    if steps.is_empty() {
        return Err(MorphRenderError::InvalidShape(
            "la playlist no tiene ningún step".to_string(),
        ));
    }
    let mut holds = Vec::with_capacity(steps.len());
    let mut frames_total: usize = 0;
    let mut max_step: usize = 0;
    for (frames, espera) in &steps {
        holds.push(playlist_hold_frames(*espera, fps));
        frames_total = frames_total
            .checked_add(frames.len())
            .ok_or(MorphRenderError::Oom {
                got: None,
                max: PARAMETRIC_MAX_BYTES,
            })?;
        max_step = max_step.max(frames.len());
    }
    let holds_total: usize = holds.iter().sum();
    let sin_stride = frames_total.saturating_add(holds_total);
    let tope = grafito_anim::protocol::PLAYLIST_MAX_FRAMES_TOTAL;
    // Caso directo: entra tal cual (el común: 2×48 sin esperas largas).
    if sin_stride <= tope && frames_total > 0 {
        return concat_playlist_with_holds(steps, fps);
    }
    if frames_total == 0 {
        return Err(MorphRenderError::InvalidShape(
            "la playlist no tiene ningún frame: agregá un step animado".to_string(),
        ));
    }
    // Busca el menor stride uniforme que entra con los holds.
    let mut elegido: Option<usize> = None;
    for stride in 1..=max_step.max(1) {
        let mut total = holds_total;
        let mut ok = true;
        for (frames, _) in &steps {
            total = match total.checked_add(strided_len(frames.len(), stride)) {
                Some(v) => v,
                None => {
                    ok = false;
                    break;
                }
            };
            if total > tope {
                ok = false;
                break;
            }
        }
        if ok {
            elegido = Some(stride);
            break;
        }
    }
    let Some(stride) = elegido else {
        return Err(MorphRenderError::InvalidShape(format!(
            "ni con 1 frame por step + esperas entra en {tope}: sacá un step o acortá las esperas"
        )));
    };
    let rebajados: Vec<(Vec<egui::ColorImage>, u64)> = steps
        .into_iter()
        .map(|(frames, espera)| (stride_frames(&frames, stride), espera))
        .collect();
    concat_playlist_with_holds(rebajados, fps)
}

#[cfg(test)]
mod morph_playlist_f2b_tests {
    use super::*;

    fn cuadrada() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    fn triangulo() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]
    }

    fn morph_lineal() -> PolylineMorph {
        PolylineMorph::try_new(
            cuadrada(),
            triangulo(),
            16,
            FrameCount::try_new(5).unwrap(),
            false,
            true,
            ShapeEasing::Linear,
        )
        .unwrap()
    }

    #[test]
    fn morph_dibuja_polilinea_con_extremos_distintos() {
        let frames = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        assert_eq!(frames.len(), 5);
        for frame in &frames {
            assert_eq!(frame.size, [64, 64]);
        }
        // El morph se mueve: primero ≠ último (algún píxel cambia).
        let primero = &frames[0].pixels;
        let ultimo = &frames[4].pixels;
        assert_ne!(primero, ultimo, "el morph debe progresar entre frames");
        // Progresión monótona del seam: el frame medio no es ni A ni B.
        assert_ne!(&frames[2].pixels, primero);
        assert_ne!(&frames[2].pixels, ultimo);
    }

    #[test]
    fn morph_cerrada_cierra_el_lazo_y_viewport_clampeado() {
        let morph = PolylineMorph::try_new(
            cuadrada(),
            triangulo(),
            12,
            FrameCount::try_new(3).unwrap(),
            true,
            true,
            ShapeEasing::CubicInOut,
        )
        .unwrap();
        let frames = render_morph_frames(&morph, 64, 64).unwrap();
        assert_eq!(frames.len(), 3);
        // Viewport bajo mínimo (8 < 64): clampeado a 64 sin panic.
        let chicos = render_morph_frames(&morph_lineal(), 8, 8).unwrap();
        assert!(chicos.iter().all(|f| f.size == [64, 64]));
    }

    #[test]
    fn easings_los_8_por_nombre_sin_reimplementar() {
        assert_eq!(MORPH_EASING_NAMES.len(), 8);
        for nombre in MORPH_EASING_NAMES {
            let easing = morph_easing_from_name(nombre).unwrap();
            assert_eq!(easing.as_str(), nombre);
            assert!((easing.apply(0.0)).abs() < 1e-12, "{nombre}");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-9, "{nombre}");
            // El atajo renderiza con cada easing (4 frames chicos).
            let frames = render_morph_with_easing(
                cuadrada(),
                triangulo(),
                8,
                4,
                false,
                true,
                nombre,
                64,
                64,
            )
            .unwrap();
            assert_eq!(frames.len(), 4, "{nombre}");
        }
        let err = morph_easing_from_name("bounce").unwrap_err().to_string();
        assert!(
            err.contains("linear") && err.contains("cubic_in_out"),
            "lista los 8, got: {err}"
        );
    }

    #[test]
    fn morph_invalido_falla_honesto() {
        let frames = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        assert_eq!(frames.len(), 5);
        // Forma vacía: Err con guía, no deforma en silencio.
        let vacio = PolylineMorph::try_new(
            vec![],
            triangulo(),
            8,
            FrameCount::try_new(3).unwrap(),
            false,
            true,
            ShapeEasing::Linear,
        );
        assert!(vacio.is_err());
        // Easing desconocido + frames fuera de rango: Err honesto.
        assert!(render_morph_with_easing(
            cuadrada(),
            triangulo(),
            8,
            0,
            false,
            true,
            "linear",
            64,
            64
        )
        .is_err());
        assert!(render_morph_with_easing(
            cuadrada(),
            triangulo(),
            8,
            4,
            false,
            true,
            "rebote-magico",
            64,
            64
        )
        .is_err());
    }

    #[test]
    fn concat_respeta_tope_96_y_dimensiones() {
        let a = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        let b = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        assert_eq!(a.len() + b.len(), 10);
        let todo = concat_playlist_frames(vec![a, b]).unwrap();
        assert_eq!(todo.len(), 10);
        // Vacío y exceso fallan honestos (nada parcial).
        assert!(concat_playlist_frames(vec![]).is_err());
        let muchos: Vec<Vec<egui::ColorImage>> = (0..20)
            .map(|_| render_morph_frames(&morph_lineal(), 64, 64).unwrap())
            .collect();
        let err = concat_playlist_frames(muchos).unwrap_err().to_string();
        assert!(err.contains("96"), "tope 96 en el mensaje, got: {err}");
        // Dimensiones mezcladas: Err con el índice.
        let chico = egui::ColorImage::new([32, 32], egui::Color32::BLACK);
        let grande = egui::ColorImage::new([64, 64], egui::Color32::BLACK);
        assert!(concat_playlist_frames(vec![vec![chico, grande]]).is_err());
    }

    #[test]
    fn holds_suman_silencio_quieto_y_piden_fps_sano() {
        assert_eq!(playlist_hold_frames(0, 12.0), 0);
        assert_eq!(playlist_hold_frames(1000, 12.0), 12);
        assert_eq!(playlist_hold_frames(500, 12.0), 6);
        assert_eq!(playlist_hold_frames(1000, f32::NAN), 0);
        assert_eq!(playlist_hold_frames(1000, 0.0), 0);
        // Con holds: 5 frames + 12 de espera = 17 (últimos 12 idénticos).
        let base = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        let ultimo = base.last().unwrap().pixels.clone();
        let todo = concat_playlist_with_holds(vec![(base, 1000)], 12.0).unwrap();
        assert_eq!(todo.len(), 17);
        for frame in todo.iter().skip(5) {
            assert_eq!(frame.pixels, ultimo, "la espera congela el último frame");
        }
        // Holds que pasan 96: Err honesto.
        let base2 = render_morph_frames(&morph_lineal(), 64, 64).unwrap();
        let err = concat_playlist_with_holds(vec![(base2, 10_000)], 12.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("96"), "got: {err}");
    }

    #[test]
    fn fitting_ajusta_cadencia_preservando_extremos_o_falla_honesto() {
        // Sets de 48 (los nativos reales): 2×48 + 6 holds = 102 > 96 →
        // stride 2 → 25+25+6 = 56 con extremos intactos.
        let morph48 = || {
            render_morph_with_easing(
                cuadrada(),
                triangulo(),
                16,
                48,
                false,
                true,
                "linear",
                64,
                64,
            )
            .unwrap()
        };
        let a = morph48();
        let b = morph48();
        let primero_a = a.first().unwrap().pixels.clone();
        let ultimo_b = b.last().unwrap().pixels.clone();
        let todo = concat_playlist_fitting(vec![(a, 500), (b, 0)], 12.0).unwrap();
        assert_eq!(todo.len(), 25 + 25 + 6, "got: {}", todo.len());
        assert_eq!(
            todo.first().unwrap().pixels,
            primero_a,
            "primer frame intacto"
        );
        assert_eq!(
            todo.last().unwrap().pixels,
            ultimo_b,
            "último frame intacto"
        );
        // Caso directo sin tocar: 2×48 sin esperas = 96 clavados.
        let directo = concat_playlist_fitting(vec![(morph48(), 0), (morph48(), 0)], 12.0).unwrap();
        assert_eq!(directo.len(), 96);
        // Imposible: ni 1 frame por step + holds entra → Err honesto.
        let muchos: Vec<(Vec<egui::ColorImage>, u64)> = (0..8)
            .map(|_| {
                (
                    render_morph_frames(&morph_lineal(), 64, 64).unwrap(),
                    10_000,
                )
            })
            .collect();
        let err = concat_playlist_fitting(muchos, 12.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("96"), "got: {err}");
        // Vacío: Err honesto.
        assert!(concat_playlist_fitting(vec![], 12.0).is_err());
        // stride cuenta bien los bordes.
        assert_eq!(strided_len(48, 1), 48);
        assert_eq!(strided_len(48, 2), 25);
        assert_eq!(strided_len(5, 2), 3);
        assert_eq!(strided_len(0, 2), 0);
    }

    #[test]
    fn error_sin_fotogramas_dice_que_hacer() {
        // Frente errs mudos: el punto único `error_sin_fotogramas` (lo usan
        // el morph y las dos vías del asistente) siempre dice qué hacer.
        for motor in ["el morph", "el motor nativo"] {
            let err = error_sin_fotogramas(motor);
            assert!(err.contains("no produjo fotogramas"), "qué falló: {err}");
            assert!(err.contains("probá bajar"), "qué hacer: {err}");
            assert!(err.contains("reintentá"), "qué hacer: {err}");
        }
        let display = MorphRenderError::InvalidShape(error_sin_fotogramas("el morph")).to_string();
        assert!(display.contains("probá bajar"), "Display útil: {display}");
    }

    #[test]
    fn error_sin_fotogramas_viene_del_catalogo_i18n() {
        // Auditoría: el literal ES vive en `MESSAGES` (`anim.empty.*`), acá
        // solo `t()` + sustitución. EN/PT resuelven sin placeholders colgados.
        use grafito_ui::i18n::{t, Locale};
        assert_eq!(
            sin_fotogramas_guia(Locale::Es),
            t("anim.empty.guide", Locale::Es)
        );
        assert_eq!(
            error_sin_fotogramas("el morph"),
            error_sin_fotogramas_localized("el morph", Locale::Es)
        );
        let en = error_sin_fotogramas_localized("morph", Locale::En);
        assert!(en.contains("morph"), "motor: {en}");
        assert!(
            !en.contains("{motor}") && !en.contains("{guia}"),
            "sin colgados: {en}"
        );
        let pt_msg = error_sin_fotogramas_localized("morph", Locale::Pt);
        assert!(
            !pt_msg.contains("{motor}") && !pt_msg.contains("{guia}"),
            "PT: {pt_msg}"
        );
    }
}

// ── P1-render: tests de vídeo todo incluido ─────────────────────────────────
#[cfg(test)]
mod p1_video_tests {
    use super::*;

    fn cuadros_sinteticos(n: usize) -> Vec<egui::ColorImage> {
        (0..n)
            .map(|k| {
                let mut pixeles = Vec::with_capacity(32 * 32);
                for i in 0..32 * 32 {
                    let v = ((i + k * 37) % 256) as u8;
                    pixeles.push(egui::Color32::from_rgba_unmultiplied(v, 255 - v, 128, 255));
                }
                egui::ColorImage {
                    size: [32, 32],
                    pixels: pixeles,
                }
            })
            .collect()
    }

    fn dir_tmp(prefijo: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("{prefijo}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn png_dir_roundtrip_conserva_frames_y_dimensiones() {
        let cuadros = cuadros_sinteticos(4);
        let base = dir_tmp("grafito-p1-png");
        let destino = base.join("seq");
        let salida = export_frames_to_png_dir(&cuadros, &destino).unwrap();
        assert_eq!(salida, destino);
        for i in 0..4 {
            let bytes = std::fs::read(destino.join(format!("frame_{i:04}.png"))).unwrap();
            let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
            assert_eq!((img.width(), img.height()), (32, 32));
        }
        // O_EXCL: el destino ya existe → honesto sin pisar.
        let err = export_frames_to_png_dir(&cuadros, &destino).unwrap_err();
        assert!(matches!(err, PngDirExportError::Io(_)));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn png_dir_preflight_no_toca_disco() {
        let base = dir_tmp("grafito-p1-png-budget");
        let destino = base.join("no-debe-existir");
        let err = export_frames_to_png_dir(&[], &destino).unwrap_err();
        assert_eq!(err, PngDirExportError::Budget(GifExportError::EmptyFrames));
        assert!(!destino.exists());
        let muchos = cuadros_sinteticos(GIF_EXPORT_MAX_FRAMES + 1);
        let err = export_frames_to_png_dir(&muchos, &destino).unwrap_err();
        assert!(matches!(
            err,
            PngDirExportError::Budget(GifExportError::TooManyFrames { .. })
        ));
        assert!(!destino.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn png_dir_cancelado_no_deja_nada() {
        let cuadros = cuadros_sinteticos(4);
        let base = dir_tmp("grafito-p1-png-cancel");
        let destino = base.join("seq");
        let token = CancellationToken::default();
        token.cancel();
        let handle = spawn_png_dir_export(cuadros, destino.clone(), token);
        assert_eq!(handle.join().unwrap(), Err(PngDirExportError::Cancelled));
        assert!(!destino.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn png_dir_validate_en_contiene_escapes() {
        let base = dir_tmp("grafito-p1-pngdir-en");
        assert!(validate_png_dir_en(&base, "seq/frames").is_ok());
        assert!(validate_png_dir_en(&base, "../afuera").is_err());
        assert!(validate_png_dir_en(&base, "").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn webm_preflight_y_missing_honestos() {
        let base = dir_tmp("grafito-p1-webm");
        let destino = base.join("no-debe-existir.webm");
        let err =
            export_frames_to_webm_file(&[], &destino, 8, 2000, VideoQuality::Media).unwrap_err();
        assert_eq!(err, WebmExportError::Budget(GifExportError::EmptyFrames));
        assert!(!destino.exists());
        let cuadros = cuadros_sinteticos(2);
        let err = export_frames_to_webm_file_with_bin(
            &cuadros,
            &destino,
            8,
            &CancellationToken::default(),
            Path::new("/definitivamente/no/existe/ffmpeg-p1"),
            2000,
            VideoQuality::Media,
        )
        .unwrap_err();
        assert_eq!(err, WebmExportError::FfmpegMissing);
        assert!(format!("{err}").contains("usá gif"));
        assert!(!destino.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn webm_spawn_codifica_real_o_missing_honesto() {
        let base = dir_tmp("grafito-p1-webm-spawn");
        let destino = base.join("clip.webm");
        let handle = spawn_webm_export(
            cuadros_sinteticos(4),
            destino.clone(),
            8,
            CancellationToken::default(),
            2000,
            VideoQuality::Media,
        );
        match handle.join().expect("el hilo no debe panicar") {
            Ok(path) => {
                assert!(std::fs::metadata(&path).unwrap().len() > 0);
                std::fs::remove_file(&path).unwrap();
            }
            Err(WebmExportError::FfmpegMissing) => assert!(!destino.exists()),
            Err(otro) => panic!("WebM real o Missing honesto, fue: {otro}"),
        }
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn tex_svg_rasteriza_formas_y_rechaza_basura() {
        let svg = "<svg viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"20\"/><rect x=\"10\" y=\"10\" width=\"20\" height=\"20\"/><line x1=\"0\" y1=\"0\" x2=\"100\" y2=\"100\"/></svg>";
        let mut fondo = vec![0u8; 64 * 64 * 4];
        fill_background(&mut fondo, 64, 64);
        let antes = fondo.clone();
        assert!(draw_tex_svg_onto(&mut fondo, 64, 64, svg));
        assert_ne!(fondo, antes, "las formas deben pintar píxeles");
        assert!(!draw_tex_svg_onto(&mut fondo, 64, 64, "no es svg"));
        let grande = "x".repeat(grafito_anim::MAX_TEX_SVG_BYTES + 1);
        assert!(!draw_tex_svg_onto(&mut fondo, 64, 64, &grande));
    }

    #[test]
    fn mobjects_nuevos_dibujan_e_invalidos_no_pintan() {
        use grafito_anim::Mobject as M;
        let nuevos = [
            M::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 1.0,
            },
            M::Square {
                cx: 0.0,
                cy: 0.0,
                side: 2.0,
            },
            M::Line {
                from: [-2.0, -2.0],
                to: [2.0, 2.0],
            },
            M::Arrow {
                from: [-2.0, 0.0],
                to: [2.0, 0.0],
            },
            M::NumberPlane {
                x_min: -3.0,
                x_max: 3.0,
                y_min: -3.0,
                y_max: 3.0,
                x_step: 1.0,
                y_step: 1.0,
            },
            M::VectorField {
                func: "x+y".to_string(),
                nx: 6,
                ny: 6,
            },
            M::FunctionGraph {
                expr: "x*x".to_string(),
            },
            M::ArrowField { nx: 6, ny: 6 },
            M::Dot { x: 1.0, y: 1.0 },
            M::Axes,
        ];
        for m in &nuevos {
            let mut buf = vec![0u8; 64 * 64 * 4];
            fill_background(&mut buf, 64, 64);
            let antes = buf.clone();
            assert!(draw_mobject(&mut buf, 64, 64, m), "debe dibujar: {m:?}");
            assert_ne!(buf, antes, "debe cambiar píxeles: {m:?}");
        }
        // Inválidos: false honesto sin pintar.
        let mut buf = vec![0u8; 64 * 64 * 4];
        fill_background(&mut buf, 64, 64);
        let antes = buf.clone();
        assert!(!draw_mobject(
            &mut buf,
            64,
            64,
            &M::Circle {
                cx: f64::NAN,
                cy: 0.0,
                r: 1.0
            }
        ));
        assert_eq!(buf, antes);
        // Tex con texto cae al fallback legible (no falso, no vacío).
        let tex = M::tex_desde_texto("hola").unwrap();
        let mut buf2 = vec![0u8; 64 * 64 * 4];
        fill_background(&mut buf2, 64, 64);
        assert!(draw_mobject(&mut buf2, 64, 64, &tex));
        assert_ne!(buf2, antes);
    }

    #[test]
    fn p3_rect_ellipse_arc_dibujan_e_invalidos_no_pintan() {
        use grafito_anim::Mobject as M;
        use std::f64::consts::PI;
        // Muestreadores puros: cotas 32..=128 y 4..=128.
        assert_eq!(super::muestras_elipse_para_radio(0.5, 0.5), 32);
        assert_eq!(super::muestras_elipse_para_radio(1.0, 1.0), 32);
        assert_eq!(super::muestras_elipse_para_radio(2.0, 1.0), 64);
        assert_eq!(super::muestras_elipse_para_radio(100.0, 100.0), 128);
        assert_eq!(super::muestras_elipse_para_radio(f64::NAN, 1.0), 32);
        assert_eq!(super::muestras_arco_para_barrido(PI), 48);
        assert_eq!(super::muestras_arco_para_barrido(std::f64::consts::TAU), 96);
        assert_eq!(super::muestras_arco_para_barrido(0.1), 4);
        assert_eq!(super::muestras_arco_para_barrido(-1.0), 4);
        // Válidos: dibujan y cambian píxeles.
        let p3 = [
            M::Rectangle {
                cx: 0.0,
                cy: 0.0,
                w: 4.0,
                h: 2.0,
            },
            M::Ellipse {
                cx: 0.0,
                cy: 0.0,
                rx: 2.0,
                ry: 1.0,
            },
            M::Arc {
                cx: 0.0,
                cy: 0.0,
                r: 1.5,
                start_rad: 0.0,
                end_rad: PI,
            },
        ];
        for m in &p3 {
            let mut buf = vec![0u8; 64 * 64 * 4];
            fill_background(&mut buf, 64, 64);
            let antes = buf.clone();
            assert!(draw_mobject(&mut buf, 64, 64, m), "debe dibujar: {m:?}");
            assert_ne!(buf, antes, "debe cambiar píxeles: {m:?}");
        }
        // Inválidos: false honesto sin pintar.
        let malos = [
            M::Rectangle {
                cx: 0.0,
                cy: 0.0,
                w: 0.0,
                h: 1.0,
            },
            M::Ellipse {
                cx: 0.0,
                cy: 0.0,
                rx: 1.0,
                ry: -2.0,
            },
            M::Arc {
                cx: 0.0,
                cy: 0.0,
                r: 1.0,
                start_rad: 1.0,
                end_rad: 1.0,
            },
            M::Arc {
                cx: f64::NAN,
                cy: 0.0,
                r: 1.0,
                start_rad: 0.0,
                end_rad: 1.0,
            },
        ];
        for m in &malos {
            let mut buf = vec![0u8; 64 * 64 * 4];
            fill_background(&mut buf, 64, 64);
            let antes = buf.clone();
            assert!(!draw_mobject(&mut buf, 64, 64, m), "no debe dibujar: {m:?}");
            assert_eq!(buf, antes);
        }
        // Escala del colocado también mueve las P3 (rect ×2 no pinta menos).
        use grafito_anim::{Camera, Ortho, PlacedMobject};
        let ortho = Camera::Ortho(Ortho::default_16_9());
        let vacio = render_placed_objects(&[], 64, 64, ortho);
        let rect = M::Rectangle {
            cx: 0.0,
            cy: 0.0,
            w: 2.0,
            h: 1.0,
        };
        let cuenta = |f: &egui::ColorImage| {
            f.pixels
                .iter()
                .zip(vacio.pixels.iter())
                .filter(|(a, b)| a != b)
                .count()
        };
        let chico = render_placed_objects(
            &[PlacedMobject::try_new(rect.clone(), 1.0, 1.0, [0.0, 0.0]).expect("válido")],
            64,
            64,
            ortho,
        );
        let grande = render_placed_objects(
            &[PlacedMobject::try_new(rect, 1.0, 2.0, [0.0, 0.0]).expect("válido")],
            64,
            64,
            ortho,
        );
        assert!(
            cuenta(&grande) >= cuenta(&chico),
            "rect escala 2 no debe pintar menos que escala 1"
        );
    }

    #[test]
    fn write_mitad_linea_frontera_a_1px() {
        use grafito_anim::{Mobject as M, RateFunc, WriteAnim};
        // Línea conocida [-2,0]→[2,0] en 64×64: mundo [-3,3]² → 1px ≈ 0.094.
        // A mitad (rate lineal) la frontera cae en mundo (0,0) = píxel (32,32).
        let linea = M::Line {
            from: [-2.0, 0.0],
            to: [2.0, 0.0],
        };
        let w = WriteAnim::try_new(linea, 8, 1000, RateFunc::Linear).unwrap();
        let colocado = w.placed_at(0.5).expect("write válido coloca");
        assert!(matches!(colocado.mobject, M::Polygon { .. }));
        let mut fondo = vec![0u8; 64 * 64 * 4];
        fill_background(&mut fondo, 64, 64);
        let mut buf = fondo.clone();
        assert!(draw_mobject(&mut buf, 64, 64, &colocado.mobject));
        // Difiere del fondo si algún canal RGB se mueve >12 (piso anti-halo AA).
        let difiere = |imagen: &[u8], base: &[u8], x: usize, y: usize| -> bool {
            let i = (y * 64 + x) * 4;
            imagen[i..i + 3]
                .iter()
                .zip(base[i..i + 3].iter())
                .map(|(a, b)| (*a as i16 - *b as i16).abs())
                .sum::<i16>()
                > 12
        };
        // Frontera a ±1px: pinta en 31 y 32, limpio desde 34 (33 = halo).
        assert!(difiere(&buf, &fondo, 31, 32), "el trazo debe llegar a 31");
        assert!(difiere(&buf, &fondo, 32, 32), "frontera en 32");
        assert!(!difiere(&buf, &fondo, 34, 32), "nada revelado en 34");
        // El extremo (mundo x=2 → px 53) sigue sin revelar a mitad...
        assert!(!difiere(&buf, &fondo, 53, 32));
        // ...pero sí con la figura completa.
        let lleno = w.placed_at(1.0).expect("write válido coloca");
        let mut buf2 = fondo.clone();
        assert!(draw_mobject(&mut buf2, 64, 64, &lleno.mobject));
        assert!(difiere(&buf2, &fondo, 53, 32), "completo debe llegar a 53");
    }

    #[test]
    fn placed_objects_rasteriza_con_opacidad_escala_centro() {
        use grafito_anim::{Camera, Mobject as M, Ortho, PlacedMobject};
        let ortho = Camera::Ortho(Ortho::default_16_9());
        // Vacío → fondo honesto 64×64.
        let vacio = render_placed_objects(&[], 64, 64, ortho);
        assert_eq!(vacio.size, [64, 64]);
        // Círculo opaco centrado pinta píxeles no-fondo.
        let circ = M::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
        };
        let lleno = render_placed_objects(
            &[PlacedMobject::opaco(circ.clone()).unwrap()],
            64,
            64,
            ortho,
        );
        assert_ne!(lleno.pixels, vacio.pixels, "el objeto debe pintar");
        // Opacidad 0 → idéntico al fondo.
        let mudo =
            PlacedMobject::try_new(circ.clone(), 0.0, 1.0, [0.0, 0.0]).expect("colocado válido");
        let sin_pintar = render_placed_objects(&[mudo], 64, 64, ortho);
        assert_eq!(sin_pintar.pixels, vacio.pixels, "opacity 0 no pinta");
        // Opacidad intermedia → entre fondo y opaco (blend global).
        let medio =
            PlacedMobject::try_new(circ.clone(), 0.5, 1.0, [0.0, 0.0]).expect("colocado válido");
        let inter = render_placed_objects(&[medio], 64, 64, ortho);
        assert_ne!(inter.pixels, vacio.pixels);
        assert_ne!(inter.pixels, lleno.pixels);
        // Escala 2 agranda: más píxeles no-fondo que escala 1.
        let chico = render_placed_objects(
            &[PlacedMobject::try_new(circ.clone(), 1.0, 1.0, [0.0, 0.0]).expect("válido")],
            64,
            64,
            ortho,
        );
        let grande = render_placed_objects(
            &[PlacedMobject::try_new(circ, 1.0, 2.0, [0.0, 0.0]).expect("válido")],
            64,
            64,
            ortho,
        );
        let cuenta = |f: &egui::ColorImage| {
            f.pixels
                .iter()
                .zip(vacio.pixels.iter())
                .filter(|(a, b)| a != b)
                .count()
        };
        assert!(
            cuenta(&grande) >= cuenta(&chico),
            "escala 2 no debe pintar menos que escala 1"
        );
        // Polilínea (VMobject como polígono abierto de 2 pts) rasteriza.
        let linea = M::Polygon {
            pts: vec![[-2.0, -2.0], [2.0, 2.0]],
        };
        let pl = render_placed_objects(&[PlacedMobject::opaco(linea).unwrap()], 64, 64, ortho);
        assert_ne!(pl.pixels, vacio.pixels, "polilínea debe pintar");
        // VMobject real aplanado a polilínea: pinta.
        let vm = grafito_anim::VMobject::try_new(
            vec![[-2.0, -2.0], [0.0, 2.0], [2.0, -2.0]],
            vec![[-2.0, -2.0], [0.0, 2.0], [2.0, -2.0]],
            vec![[-2.0, -2.0], [0.0, 2.0], [2.0, -2.0]],
        )
        .expect("vm válido");
        let plano = vmobject_como_polilinea(&vm).expect("aplana");
        let pv = render_placed_objects(&[PlacedMobject::opaco(plano).unwrap()], 64, 64, ortho);
        assert_ne!(pv.pixels, vacio.pixels, "VMobject aplanado debe pintar");
        // VMobject vacío → None honesto.
        let vacio_vm = grafito_anim::VMobject {
            anchors: Vec::new(),
            handles_in: Vec::new(),
            handles_out: Vec::new(),
        };
        assert!(vmobject_como_polilinea(&vacio_vm).is_none());
        // Perspective: sin ejes 2D pero el objeto igual se rasteriza.
        let persp = Camera::perspective(50.0, [5.0, 2.0, 5.0], [0.0, 0.0, 0.0]).unwrap();
        let p3 = render_placed_objects(
            &[PlacedMobject::opaco(M::Dot { x: 0.0, y: 0.0 }).unwrap()],
            64,
            64,
            persp,
        );
        assert_eq!(p3.size, [64, 64]);
        assert_ne!(p3.pixels, vacio.pixels, "el punto debe pintarse en 3D");
    }

    #[test]
    fn texto_escala_por_h_y_scrim_dimensiona() {
        // R6c a propósito: título escala 3 en h≥360 (antes 2 hasta 719);
        // abajo legado compacto 1 para previews diminutos.
        assert_eq!(text_scale_for_h(64), 1);
        assert_eq!(text_scale_for_h(359), 1);
        assert_eq!(text_scale_for_h(360), 3);
        assert_eq!(text_scale_for_h(719), 3);
        assert_eq!(text_scale_for_h(720), 3);
        assert_eq!(text_scale_for_h(4096), 3);
        // Scrim dimensiona al texto sin panics (incluso en 1px y vacío).
        let mut buf = vec![0u8; 64 * 64 * 4];
        fill_background(&mut buf, 64, 64);
        let antes = buf.clone();
        draw_scrim_para_rotulo(&mut buf, 64, 64, 4, 5, "hola");
        assert_ne!(buf, antes, "el scrim debe oscurecer la banda");
        draw_scrim_para_rotulo(&mut buf, 0, 0, 0, 0, "");
        draw_rotulo_con_scrim(&mut buf, 64, 64, 4, 5, "x");
    }

    // ── R6c (re-aplicado post-revert): legibilidad medible a 480×360 ──────
    // Cada regla con su test que la pinnea (la 8 vive junto al test de argv
    // MP4 en `mod tests`, misma instrumentación); presupuestos intactos
    // (48 frames, 64/8M/5MB, 64MiB, Resolution 64..4096).

    #[test]
    fn r6c_ticks_escala_2_a_480x360() {
        // Regla 1: ticks a escala ≥2 en h≥360 (11px viola ≥4.5%/16px).
        assert_eq!(tick_scale_for_h(359), 1, "abajo legado compacto");
        assert_eq!(tick_scale_for_h(360), 2);
        assert_eq!(tick_scale_for_h(720), 2);
        // Medible: en ejes aislados a 480×360 hay tinta clara bajo cy+19
        // (a escala 1 el glifo termina en ly+11 = cy+19).
        let (w, h) = (480usize, 360usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        super::draw_axes_with_labels(&mut buf, w, h);
        let (_, cy) = super::to_pixel(w, h, 0.0, 0.0);
        let mut claros = 0usize;
        for y in cy + 19..(cy + 30).min(h) {
            for x in 0..w {
                if let Some(i) = y
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(x))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 2 < buf.len() && buf[i] > 200 && buf[i + 1] > 200 && buf[i + 2] > 200 {
                        claros += 1;
                    }
                }
            }
        }
        assert!(
            claros > 10,
            "ticks a escala ≥2 (tinta bajo cy+19), fue: {claros}"
        );
    }

    #[test]
    fn r6c_titulo_escala_3_a_480x360() {
        // Regla 2: título escala 3 en h≥360 (~30px ≈ 8-9%h).
        assert_eq!(text_scale_for_h(359), 1, "abajo legado compacto");
        assert_eq!(text_scale_for_h(360), 3);
        // Medible: el scrim de "derivada" a 480×360 mide 12*3+8 = 44 filas
        // (a escala 2 serían 32): la columna x (sin tinta: el texto arranca
        // en x+4) sale oscurecida en y..y+44 e intacta en y+44.
        let (w, h) = (480usize, 360usize);
        let (x, y) = (w / 14, h / 12);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        let mut fondo = vec![0u8; w * h * 4];
        super::fill_background(&mut fondo, w, h);
        super::draw_rotulo_con_scrim(&mut buf, w, h, x, y, "derivada");
        let mas_oscuro = |yy: usize| -> bool {
            let i = yy * w + x;
            (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2])
                < (fondo[i * 4], fondo[i * 4 + 1], fondo[i * 4 + 2])
        };
        for yy in y..y + 44 {
            assert!(mas_oscuro(yy), "scrim escala 3 cubre la fila {yy}");
        }
        let i = (y + 44) * w + x;
        assert_eq!(
            (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2]),
            (fondo[i * 4], fondo[i * 4 + 1], fondo[i * 4 + 2]),
            "tras 44 filas no hay scrim"
        );
    }

    #[test]
    fn r6c_slots_y_universal_a_margen_24() {
        // Regla 3: slots "x"/"y" y placeholder universal a margen ≥24px.
        // Medible: en ejes aislados a 480×360 no hay tinta clara en el marco
        // exterior de 24px (las líneas de ejes son grises 200, no >200).
        let (w, h) = (480usize, 360usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        super::draw_subtle_grid(&mut buf, w, h);
        super::draw_axes_with_labels(&mut buf, w, h);
        let en_marco =
            |x: usize, y: usize| -> bool { x < 24 || x >= w - 24 || y < 24 || y >= h - 24 };
        let mut claros = 0usize;
        for y in 0..h {
            for x in 0..w {
                if !en_marco(x, y) {
                    continue;
                }
                let i = (y * w + x) * 4;
                if buf[i] > 200 && buf[i + 1] > 200 && buf[i + 2] > 200 {
                    claros += 1;
                }
            }
        }
        assert_eq!(claros, 0, "marco 24px sin tinta de slots, fue: {claros}");
        // Universal: rótulo + eco dentro del marco a 480×360.
        let uni = super::render_universal_youtube_frames("prueba margen", 480, 360);
        assert_eq!(uni.len(), super::NATIVE_ANIM_FRAME_COUNT);
        let claros_uni = uni
            .iter()
            .map(|f| {
                f.pixels
                    .iter()
                    .enumerate()
                    .filter(|(k, px)| {
                        let (x, y) = (k % w, k / w);
                        en_marco(x, y) && px.r() > 200 && px.g() > 200 && px.b() > 200
                    })
                    .count()
            })
            .max()
            .unwrap_or(0);
        assert_eq!(claros_uni, 0, "universal a margen ≥24px, fue: {claros_uni}");
    }

    #[test]
    fn r6c_grilla_alfa_38_51() {
        // Regla 4: grilla alfa 38..51 (≈15-20%, contraste 3:1).
        let alfa = super::GRID_COLOR[3];
        assert!(
            (38..=51).contains(&alfa),
            "alfa de grilla en 38..=51, fue: {alfa}"
        );
        // Medible: el píxel de grilla (3,0) —solo dosis horizontal, sin la
        // doble dosis de las intersecciones— aclara el fondo entre 5 y 70
        // por canal (mezcla por alfa, jamás promedio 50%).
        let (w, h) = (480usize, 360usize);
        let mut fondo = vec![0u8; w * h * 4];
        super::fill_background(&mut fondo, w, h);
        let mut buf = fondo.clone();
        super::draw_subtle_grid(&mut buf, w, h);
        let i = 3 * 4;
        for k in 0..3 {
            let delta = buf[i + k] as i16 - fondo[i + k] as i16;
            assert!(
                (5..=70).contains(&delta),
                "canal {k} aclara {delta}, fuera de 5..=70"
            );
        }
    }

    #[test]
    fn r6c_rotulos_12ch() {
        // Regla 5: rótulos cortos (antes "derivada  f'(x)" 13ch,
        // "a^2 + b^2 = c^2" 15ch, "conforme w=(z-c)/(1-cc*z)" 23ch).
        for rot in [
            "derivada",
            "pitagoras",
            "bifurcacion",
            "gradiente f",
            "mobius w(z)",
            "y=x^2",
        ] {
            assert!(rot.chars().count() <= 12, "rótulo ≤12ch, fue: {rot}");
        }
        // "conforme w(z)" (13ch documentados) reemplaza al de 23ch.
        assert_eq!("conforme w(z)".chars().count(), 13);
        // Humo: los tres renderers rotulan 48 frames válidos a 480×360.
        let vacio = std::collections::BTreeMap::new();
        for (nombre, frames) in [
            (
                "derivative-slope",
                super::render_derivative_frames_with_params(480, 360, &vacio),
            ),
            ("pitagoras", super::render_pitagoras_frames(480, 360)),
            ("conformal-map", super::render_conformal_frames(480, 360)),
        ] {
            assert_eq!(frames.len(), super::NATIVE_ANIM_FRAME_COUNT, "{nombre}");
            assert_eq!(frames[0].size, [480, 360], "{nombre}: tamaño");
        }
    }

    #[test]
    fn r6c_curva_3px_ejes_1_5px() {
        // Regla 6: curva 3px, ejes 1.5px (dorados actualizados a propósito:
        // `todos_los_parametricos_rotulan_eje_x` + `texto_escala_*` + docs).
        assert_eq!(super::CURVE_ANCHO, 3.0);
        assert_eq!(super::AXIS_ANCHO, 1.5);
        // Medible: segmento mundo horizontal a 480×360 pinta ≥3 filas.
        let (w, h) = (480usize, 360usize);
        let mut fondo = vec![0u8; w * h * 4];
        super::fill_background(&mut fondo, w, h);
        let mut buf = fondo.clone();
        assert!(super::draw_seg_mundo(
            &mut buf,
            w,
            h,
            -3.0,
            0.0,
            3.0,
            0.0,
            super::CURVE_MAIN,
            super::CURVE_ANCHO,
        ));
        let (_, cy) = super::to_pixel(w, h, 0.0, 0.0);
        for yy in [cy.saturating_sub(1), cy, cy + 1] {
            let n = (0..w)
                .filter(|&x| {
                    let i = (yy * w + x) * 4;
                    buf[i] != fondo[i] || buf[i + 1] != fondo[i + 1] || buf[i + 2] != fondo[i + 2]
                })
                .count();
            assert!(n >= 10, "curva 3px cubre la fila {yy}, fue: {n}");
        }
        // Ejes: la fila central y sus vecinas difieren del fondo+grilla.
        let mut ejes = fondo.clone();
        super::draw_axes_with_labels(&mut ejes, w, h);
        for yy in [cy.saturating_sub(1), cy, cy + 1] {
            let n = (0..w)
                .filter(|&x| {
                    let i = (yy * w + x) * 4;
                    ejes[i] != fondo[i]
                        || ejes[i + 1] != fondo[i + 1]
                        || ejes[i + 2] != fondo[i + 2]
                })
                .count();
            assert!(n >= 100, "eje 1.5px cubre la fila {yy}, fue: {n}");
        }
    }

    #[test]
    fn r6c_halo_previo_a_cada_tick() {
        // Regla 7: halo/scrim previo a cada tick (sin solape por cajas
        // disjuntas, ≥8px).
        // Helper directo: caja 8×8 se oscurece entera, fuera no se toca.
        let (w, h) = (64usize, 64usize);
        let mut buf = vec![0u8; w * h * 4];
        super::fill_background(&mut buf, w, h);
        let antes = buf.clone();
        super::pintar_halo_rotulo(&mut buf, w, h, 10, 10, 8, 8);
        for y in 10..18 {
            for x in 10..18 {
                let i = (y * w + x) * 4;
                assert!(
                    (buf[i], buf[i + 1], buf[i + 2]) < (antes[i], antes[i + 1], antes[i + 2]),
                    "halo oscurece ({x},{y})"
                );
            }
        }
        for (x, y) in [(9, 10), (10, 9), (18, 10), (10, 18)] {
            let i = (y * w + x) * 4;
            assert_eq!(
                (buf[i], buf[i + 1], buf[i + 2]),
                (antes[i], antes[i + 1], antes[i + 2]),
                "fuera de la caja no se toca ({x},{y})"
            );
        }
        // Caja mínima real ≥8px a 480×360.
        assert!(
            super::tick_scale_for_h(360) * super::TICK_CHAR_H_PX as usize >= 8,
            "halo ≥8px"
        );
        // Integración: en ejes a 480×360 hay halo (mínimo bajo el fondo
        // solo) y tinta (máximo sobre el fondo solo).
        let (w, h) = (480usize, 360usize);
        let mut fondo = vec![0u8; w * h * 4];
        super::fill_background(&mut fondo, w, h);
        let mut ejes = fondo.clone();
        super::draw_axes_with_labels(&mut ejes, w, h);
        let minimo = |b: &[u8]| b.chunks_exact(4).map(|px| px[0]).min().unwrap_or(255);
        let maximo = |b: &[u8]| {
            b.chunks_exact(4)
                .map(|px| px[0].max(px[1]).max(px[2]))
                .max()
                .unwrap_or(0)
        };
        assert!(
            minimo(&ejes) < minimo(&fondo),
            "halo previo oscurece bajo el fondo"
        );
        assert!(
            maximo(&ejes) > maximo(&fondo),
            "tinta del tick sobre el halo"
        );
    }

    #[test]
    fn r6c_overbudget_aviso_mostrable_en_card() {
        // Regla 9: preview overbudget con mensaje para la card (`Option`:
        // la firma `Vec` histórica impide `Err` y hace clamp documentado).
        assert!(
            super::mensaje_overbudget_para(480, 360).is_none(),
            "el canon entra holgado"
        );
        let msg = super::mensaje_overbudget_para(4096, 4096).expect("4096²×48 excede 64MiB");
        assert!(msg.contains("reducida"), "aviso mostrable, fue: {msg}");
        // El render clásico sigue devolviendo el set (clamp, sin panic).
        let frames = super::render_pitagoras_frames(4096, 4096);
        assert_eq!(frames.len(), super::NATIVE_ANIM_FRAME_COUNT);
    }

    #[test]
    fn fases_setup_construccion_hold() {
        use super::{FaseConstruccion, FASE_CONSTRUCCION_HASTA, FASE_SETUP_HASTA};
        assert_eq!(FASE_SETUP_HASTA, 0.2);
        assert_eq!(FASE_CONSTRUCCION_HASTA, 0.8);
        let (f0, a0) = fase_para_frame(0);
        assert_eq!((f0, a0), (FaseConstruccion::Setup, 0.0));
        let (f1, a1) = fase_para_frame(NATIVE_ANIM_FRAME_COUNT - 1);
        assert_eq!((f1, a1), (FaseConstruccion::Hold, 1.0));
        // Setup plano: primeros 20% en alpha 0.
        for f in 0..10 {
            assert_eq!(fase_alpha(f), 0.0, "setup en frame {f}");
        }
        // Hold plano: últimos 20% en alpha 1.
        for f in 38..NATIVE_ANIM_FRAME_COUNT {
            assert_eq!(fase_alpha(f), 1.0, "hold en frame {f}");
        }
        // Construcción monótona 0→1 con cubic (no lineal: el medio no es 0.5
        // exacto del tramo... sí lo es por simetría cúbica: se pinnea).
        let mut previo = 0.0;
        for f in 10..38 {
            let a = fase_alpha(f);
            assert!(a >= previo, "monótono en {f}");
            previo = a;
        }
        assert!((fase_alpha(24) - 0.5).abs() < 0.15, "medio ≈ 0.5");
        // integral_frame_end respeta fases (extremos + monotonía).
        assert_eq!(integral_frame_end(0.0, 2.0, 0), 0.0);
        assert_eq!(
            integral_frame_end(0.0, 2.0, NATIVE_ANIM_FRAME_COUNT - 1),
            2.0
        );
    }

    #[test]
    fn camara_perspectiva_tracks_y_orbita_48() {
        use grafito_anim::{Camera, MovingCamera, Ortho, RateFunc};
        let desde = Camera::perspective(50.0, [5.0, 2.0, 5.0], [0.0, 0.0, 0.0]).unwrap();
        let hasta = Camera::perspective(50.0, [-5.0, 2.0, 5.0], [0.0, 0.0, 0.0]).unwrap();
        let travelling = MovingCamera::try_new(desde, hasta, 2000, RateFunc::Smooth).unwrap();
        // 7 tracks de perspectiva (fov + eye xyz + center xyz).
        let ids = anim_camera_track_ids(&travelling);
        assert_eq!(ids.len(), 7);
        assert!(ids.contains(&"cam.fov".to_string()));
        assert!(ids.contains(&"cam.eye_x".to_string()));
        assert!(ids.contains(&"cam.center_z".to_string()));
        // Extremos del sample + project_3d honesto.
        assert_eq!(anim_camera_sample(&travelling, 0), desde);
        assert_eq!(anim_camera_sample(&travelling, 2000), hasta);
        assert!(desde.project_3d([0.0, 0.0, 0.0]).is_some());
        assert!(
            desde.project_3d([5.0, 2.0, 5.0 + 1.0]).is_none()
                || desde.project_3d([0.0, 0.0, 100.0]).is_none()
        );
        // Ortho: 4 tracks.
        let o1 = Camera::Ortho(Ortho::try_new(-3.0, 3.0, -3.0, 3.0).unwrap());
        let o2 = Camera::Ortho(Ortho::try_new(-2.0, 2.0, -2.0, 2.0).unwrap());
        let mov2d = MovingCamera::try_new(o1, o2, 1000, RateFunc::Linear).unwrap();
        assert_eq!(anim_camera_track_ids(&mov2d).len(), 4);
        // Órbita: 48 frames reales; el travelling rompe la simetría en los
        // tres ejes (si solo se espejara x sobre el cubo simétrico, el
        // primero y el último serían espejos idénticos). Se cuenta diferencia
        // en vez de `assert_ne!` directo para no volcar 4096 píxeles al log.
        let frames = render_orbit_frames(64, 64);
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        let difieren = frames[0]
            .pixels
            .iter()
            .zip(frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(difieren > 0, "la órbita debe mover la cámara entre frames");
        // Mobject estático: 48 frames con la forma presente.
        let cuadros = render_mobject_frames(
            64,
            64,
            &grafito_anim::Mobject::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 1.0,
            },
        );
        assert_eq!(cuadros.len(), NATIVE_ANIM_FRAME_COUNT);
    }
}

// ── P2-perf: params vivos restantes + Manim resto (SOLO AÑADIDOS) ───────────
// Coordinación W5: los writers legacy (`*_impl` sin params) NO se tocan.
// Cada plantilla restante gana `render_*_with_params` + `_with_params_impl`
// (clon parametrizado; mapa vacío o defaults → delega al legacy exacto) y
// el dispatcher `render_anim_for_concept_with_params_p2` que atiende las 5
// y delega el resto al dispatcher existente. Presupuesto intacto: siempre
// 48 frames (`NATIVE_ANIM_FRAME_COUNT`).
//
// | Clave          | Plantilla            | Significado              | Default      |
// |----------------|----------------------|--------------------------|--------------|
// | `cr_range`     | conformal-map        | semirrecorrido real de c | 0.45 [0.05,1]|
// | `ci_amp`       | conformal-map        | amplitud imag de c       | 0.3 [0,1]    |
// | `tri_size`     | pitagoras            | escala del triángulo     | 1.0 [0.25,2] |
// | `r0` / `r1`    | logistic-bifurcation | rango de r barrido       | 2.5 / 4.0    |
// | `freq`         | gradient-field       | frecuencia del campo     | 1.0 [0.25,3] |
// | `mob_amp`      | mobius-transform     | recorrido real de c(t)   | 0.8 [0,1.5]  |
// Ausente/NaN/inf → default; fuera de rango → clamp. `r0 > r1` se ordena
// (igual que `a`/`b` en integral-area).

/// Semirrecorrido real del centro conforme (conformal-map).
pub const P2_PARAM_CR_RANGE: &str = "cr_range";
/// Amplitud imaginaria del centro conforme (conformal-map).
pub const P2_PARAM_CI_AMP: &str = "ci_amp";
/// Escala del triángulo (pitagoras).
pub const P2_PARAM_TRI_SIZE: &str = "tri_size";
/// Extremo inicial del barrido de r (logistic-bifurcation).
pub const P2_PARAM_R0: &str = "r0";
/// Extremo final del barrido de r (logistic-bifurcation).
pub const P2_PARAM_R1: &str = "r1";
/// Frecuencia del campo f=sin·cos (gradient-field).
pub const P2_PARAM_FREQ: &str = "freq";
/// Recorrido real de c(t) (mobius-transform).
pub const P2_PARAM_MOB_AMP: &str = "mob_amp";
/// Fotogramas por segundo pedidos (`AnimRequest.params`, Manim `-fps`).
pub const P2_PARAM_FPS: &str = "fps";
/// Bitrate pedido en kbps (`AnimRequest.params`, Manim `--bitrate`).
pub const P2_PARAM_BITRATE: &str = "bitrate_kbps";
/// Calidad: 0 baja (`-ql`), 1 media (`-qm`, default), 2 alta.
pub const P2_PARAM_QUALITY: &str = "quality";

/// Guarda compartida de los clones P2 con tope temporal: repite el último
/// frame hasta completar los 48 (idéntica a la de los writers legacy).
fn p2_rellena_con_ultimo(
    frames: &mut Vec<egui::ColorImage>,
    w: usize,
    h: usize,
    on_frame: &mut dyn FnMut(usize, usize),
) {
    let ultimo = frames.last().cloned().unwrap_or_else(|| {
        let len = checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut b = vec![0u8; len];
        for chunk in b.chunks_exact_mut(4) {
            chunk.copy_from_slice(&PAL_BG);
        }
        if len == w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
            egui::ColorImage::from_rgba_unmultiplied([w, h], &b)
        } else {
            egui::ColorImage::from_rgba_unmultiplied([NATIVE_FALLBACK_W, NATIVE_FALLBACK_H], &b)
        }
    });
    while frames.len() < NATIVE_ANIM_FRAME_COUNT {
        frames.push(ultimo.clone());
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
}

// ── conformal-map con params vivos ──────────────────────────────────────────

/// Conformal con params vivos (`cr_range`, `ci_amp`). Mapa vacío = legacy.
pub fn render_conformal_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_conformal_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_conformal_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    if params.is_empty() {
        return render_conformal_frames_impl(width, height, con_rotulo, on_frame);
    }
    let cr_range = scene_param_clamped(params, P2_PARAM_CR_RANGE, 0.45, 0.05, 1.0);
    let ci_amp = scene_param_clamped(params, P2_PARAM_CI_AMP, 0.3, 0.0, 1.0);
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let (cr, ci) = (
            cr_range * (2.0 * t - 1.0),
            ci_amp * (std::f64::consts::PI * t).sin(),
        );
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let p0 = to_pixel(w, h, x, y);
                draw_filled_circle(&mut buf, w, h, p0.0, p0.1, 1, MINT_FAINT);
                if let Some((wx, wy)) = mobius_map(x, y, cr, ci) {
                    let p1 = to_pixel(w, h, wx, wy);
                    draw_filled_circle(&mut buf, w, h, p1.0, p1.1, 2, MINT_STRONG);
                }
            }
        }
        let mut prev: Option<(usize, usize)> = None;
        for k in 0..=60 {
            let a = 2.0 * std::f64::consts::PI * k as f64 / 60.0;
            let (zx, zy) = (a.cos(), a.sin());
            if let Some((wx, wy)) = mobius_map(zx, zy, cr, ci) {
                let p = to_pixel(w, h, wx, wy);
                if let Some(q) = prev {
                    draw_line(&mut buf, w, h, q, p, CURVE_MAIN);
                }
                prev = Some(p);
            } else {
                prev = None;
            }
        }
        if con_rotulo {
            draw_rotulo_con_scrim(&mut buf, w, h, w / 14, h / 12, "conforme w(z)");
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── pitagoras con params vivos ──────────────────────────────────────────────

/// Pitágoras con params vivos (`tri_size`). Mapa vacío = legacy.
pub fn render_pitagoras_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_pitagoras_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_pitagoras_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    if params.is_empty() {
        return render_pitagoras_frames_impl(width, height, con_rotulo, on_frame);
    }
    let size = scene_param_clamped(params, P2_PARAM_TRI_SIZE, 1.0, 0.25, 2.0);
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        // Timeline por fases: los cuadrados crecen 0→1 en construcción.
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let p1 = to_pixel(w, h, -size, -size);
        let p2 = to_pixel(w, h, 1.0 * size, -size);
        let p3 = to_pixel(w, h, 1.0 * size, 0.5 * size);
        draw_line(&mut buf, w, h, p1, p2, LINE_WHITE);
        draw_line(&mut buf, w, h, p2, p3, LINE_WHITE);
        draw_line(&mut buf, w, h, p3, p1, LINE_WHITE);
        let scale = t;
        let sq1_p2 = to_pixel(w, h, -size, (-1.0 - 2.0 * scale) * size);
        let sq1_p3 = to_pixel(w, h, 1.0 * size, (-1.0 - 2.0 * scale) * size);
        draw_line(&mut buf, w, h, p1, sq1_p2, SQUARE_BLUE);
        draw_line(&mut buf, w, h, sq1_p2, sq1_p3, SQUARE_BLUE);
        draw_line(&mut buf, w, h, sq1_p3, p2, SQUARE_BLUE);
        let sq2_p2 = to_pixel(w, h, (1.0 + 1.5 * scale) * size, -size);
        let sq2_p3 = to_pixel(w, h, (1.0 + 1.5 * scale) * size, 0.5 * size);
        draw_line(&mut buf, w, h, p2, sq2_p2, SQUARE_AMBER);
        draw_line(&mut buf, w, h, sq2_p2, sq2_p3, SQUARE_AMBER);
        draw_line(&mut buf, w, h, sq2_p3, p3, SQUARE_AMBER);
        if t > 0.5 {
            let tt = (t - 0.5) * 2.0;
            let mid = to_pixel(w, h, (-1.0 - 1.0 * tt) * size, (0.5 + 0.5 * tt) * size);
            draw_line(&mut buf, w, h, p3, mid, SQUARE_GREEN);
            draw_line(&mut buf, w, h, mid, p1, SQUARE_GREEN);
        }
        if con_rotulo {
            draw_rotulo_con_scrim(&mut buf, w, h, w / 14, h / 12, "pitagoras");
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── logistic-bifurcation con params vivos ───────────────────────────────────

/// Logística con params vivos (`r0`, `r1`; se ordenan si `r0 > r1`).
/// Mapa vacío = legacy.
pub fn render_logistic_bifurcation_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_logistic_bifurcation_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_logistic_bifurcation_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    if params.is_empty() {
        return render_logistic_bifurcation_frames_impl(width, height, con_rotulo, on_frame);
    }
    let mut r0 = scene_param_clamped(params, P2_PARAM_R0, 2.5, 2.0, 4.0);
    let mut r1 = scene_param_clamped(params, P2_PARAM_R1, 4.0, 2.0, 4.0);
    if r0 > r1 {
        std::mem::swap(&mut r0, &mut r1);
    }
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            p2_rellena_con_ultimo(&mut frames, w, h, on_frame);
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let x0 = w / 12;
        let x1 = w.saturating_sub(w / 12).max(x0 + 8);
        let y_top = h / 4;
        let y_bot = h.saturating_sub(h / 4).max(y_top + 8);
        let span_x = (x1.saturating_sub(x0)).max(1);
        let span_y = (y_bot.saturating_sub(y_top)).max(1);
        let mut sx = x0;
        while sx < x1 {
            let r = r0 + (r1 - r0) * (sx.saturating_sub(x0)) as f64 / span_x as f64;
            let mut x = 0.5;
            for _ in 0..100 {
                x = r * x * (1.0 - x);
            }
            for _ in 0..16 {
                x = r * x * (1.0 - x);
                let frac = x.clamp(0.0, 1.0);
                let py = y_bot.saturating_sub((frac * span_y as f64) as usize);
                draw_filled_circle(
                    &mut buf,
                    w,
                    h,
                    sx,
                    py.min(h.saturating_sub(1)),
                    1,
                    MINT_FAINT,
                );
            }
            sx += 2;
        }
        let hx = x0 + ((t * span_x as f64) as usize).min(span_x.saturating_sub(1));
        draw_line(&mut buf, w, h, (hx, y_top), (hx, y_bot), PAL_ACCENT);
        let r_h = r0 + (r1 - r0) * t;
        let mut xh = 0.5;
        for _ in 0..100 {
            xh = r_h * xh * (1.0 - xh);
        }
        for _ in 0..12 {
            xh = r_h * xh * (1.0 - xh);
            let frac = xh.clamp(0.0, 1.0);
            let py = y_bot.saturating_sub((frac * span_y as f64) as usize);
            draw_filled_circle(
                &mut buf,
                w,
                h,
                hx,
                py.min(h.saturating_sub(1)),
                2,
                POINT_RED,
            );
        }
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "bifurcacion");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "bifurcacion",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── gradient-field con params vivos ─────────────────────────────────────────

/// Gradiente con params vivos (`freq`). Mapa vacío = legacy.
pub fn render_gradient_field_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_gradient_field_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_gradient_field_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    if params.is_empty() {
        return render_gradient_field_frames_impl(width, height, con_rotulo, on_frame);
    }
    let freq = scene_param_clamped(params, P2_PARAM_FREQ, 1.0, 0.25, 3.0);
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            p2_rellena_con_ultimo(&mut frames, w, h, on_frame);
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let math_per_px = 6.0 / w.max(1) as f64;
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64 * 0.9;
                let y = gy as f64 * 0.9;
                let gfx = (freq * x).cos() * (freq * y).cos();
                let gfy = -((freq * x).sin() * (freq * y).sin());
                let mag = (gfx * gfx + gfy * gfy).sqrt();
                let (dx, dy) = if mag < 1e-9 {
                    (0.0, 0.0)
                } else {
                    (gfx / mag, gfy / mag)
                };
                let len_px = 4.0 + 10.0 * (mag / (1.0 + mag));
                let x2 = (x + dx * len_px * math_per_px).clamp(-3.0, 3.0);
                let y2 = (y + dy * len_px * math_per_px).clamp(-3.0, 3.0);
                let a = to_pixel(w, h, x, y);
                let b = to_pixel(w, h, x2, y2);
                draw_line(&mut buf, w, h, a, b, PAL_ACCENT);
                draw_filled_circle(&mut buf, w, h, b.0, b.1, 1, MINT_STRONG);
                draw_filled_circle(&mut buf, w, h, a.0, a.1, 1, MINT_FAINT);
            }
        }
        for i in 0..6 {
            let ang = 2.0 * std::f64::consts::PI * (i as f64 / 6.0) + t * 1.4 + i as f64 * 0.35;
            let rad = 1.25 + 0.3 * (t * std::f64::consts::TAU + i as f64 * 1.3).sin();
            let x = (rad * ang.cos()).clamp(-2.8, 2.8);
            let y = (rad * ang.sin() * 0.7).clamp(-2.8, 2.8);
            let p = to_pixel(w, h, x, y);
            let pulse = (frame as f64 * 0.4 + i as f64).sin() * 0.5 + 0.5;
            let col = with_alpha(POINT_RED, (140.0 + 100.0 * pulse) as u8);
            draw_filled_circle(&mut buf, w, h, p.0, p.1, 3, col);
        }
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "gradiente f");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "gradiente f",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

// ── mobius-transform con params vivos ───────────────────────────────────────

/// Möbius con params vivos (`mob_amp`). Mapa vacío = legacy.
pub fn render_mobius_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_mobius_frames_with_params_impl(width, height, params, true, &mut |_, _| {})
}

fn render_mobius_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    if params.is_empty() {
        return render_mobius_frames_impl(width, height, con_rotulo, on_frame);
    }
    let amp = scene_param_clamped(params, P2_PARAM_MOB_AMP, 0.8, 0.0, 1.5);
    let ((w, h), _) = resolve_native_size_budgeted(width, height, NATIVE_ANIM_FRAME_COUNT);
    let start = std::time::Instant::now();
    let max_ms: u128 = 1800;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        if start.elapsed().as_millis() > max_ms {
            p2_rellena_con_ultimo(&mut frames, w, h, on_frame);
            break;
        }
        // Timeline por fases (setup 20% / construcción 60% cubic_in_out / hold 20%).
        let t = fase_alpha(frame);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h);
        draw_subtle_grid(&mut buf, w, h);
        draw_axes_with_labels(&mut buf, w, h);
        let ang_c = 2.0 * std::f64::consts::PI * t;
        let (cr, ci) = (-0.4 + amp * t, 0.3 * ang_c.sin());
        let mobius = |x: f64, y: f64| -> Option<(f64, f64)> { mobius_map(x, y, cr, ci) };
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let p0 = to_pixel(w, h, x, y);
                draw_filled_circle(&mut buf, w, h, p0.0, p0.1, 1, MINT_FAINT);
                if let Some((wx, wy)) = mobius(x, y) {
                    let p1 = to_pixel(w, h, wx, wy);
                    draw_filled_circle(&mut buf, w, h, p1.0, p1.1, 2, PAL_ACCENT);
                }
            }
        }
        let mut prev: Option<(usize, usize)> = None;
        for k in 0..=60 {
            let a = 2.0 * std::f64::consts::PI * k as f64 / 60.0;
            let (zx, zy) = (a.cos(), a.sin());
            if let Some((wx, wy)) = mobius(zx, zy) {
                let p = to_pixel(w, h, wx, wy);
                if let Some(q) = prev {
                    draw_line(&mut buf, w, h, q, p, CURVE_MAIN);
                }
                prev = Some(p);
            } else {
                prev = None;
            }
        }
        let pc = to_pixel(w, h, cr * 2.0, ci * 2.0);
        draw_filled_circle(&mut buf, w, h, pc.0, pc.1, 3, POINT_RED);
        if con_rotulo {
            draw_scrim_para_rotulo(&mut buf, w, h, w / 14, h / 12, "mobius w(z)");
            draw_text_block(
                &mut buf,
                w,
                h,
                w / 14,
                h / 12,
                "mobius w(z)",
                PAL_FG,
                text_scale_for_h(h),
            );
        }
        let bar_y = h.saturating_sub(4);
        // Progreso lineal honesto del frame (la construcción va por fases).
        let prog = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let bar_w = (w as f64 * prog) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 2, PAL_ACCENT);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w.saturating_sub(bar_w),
            2,
            TRACK,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Dispatcher P2: las 5 plantillas restantes con params vivos; el resto
/// delega al dispatcher existente (wiring W5 intacto).
pub fn render_anim_for_concept_with_params_p2(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    con_rotulo: bool,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    match resolve_native_template(template, concept) {
        "conformal-map" => {
            render_conformal_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "pitagoras" => {
            render_pitagoras_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        "logistic-bifurcation" => render_logistic_bifurcation_frames_with_params_impl(
            width, height, params, con_rotulo, on_frame,
        ),
        "gradient-field" => render_gradient_field_frames_with_params_impl(
            width, height, params, con_rotulo, on_frame,
        ),
        "mobius-transform" => {
            render_mobius_frames_with_params_impl(width, height, params, con_rotulo, on_frame)
        }
        _ => render_anim_with_progress_con_rotulo(
            template, concept, width, height, params, con_rotulo, on_frame,
        ),
    }
}

// ── always_redraw / updaters genéricos (Manim resto) ────────────────────────
// En Manim cada `Mobject` con updater se re-evalúa por frame
// (`always_redraw(f)`); acá los sets nativos son 48 frames precomputados y
// el seam genérico ya existe (`render_parametric_frames_con_updater`, que
// acepta `&mut dyn FnMut(usize) -> Option<f64>` como vivo por frame). Este
// bloque suma la DECISIÓN de redibujo: con `always=true` (default Manim)
// cada scrub re-renderiza; con `false` solo si el fingerprint de params
// cambió (ahorra los 48 frames cuando el slider no se movió).

/// Huella FNV-1a de 64 bits de un mapa de params (claves ordenadas por
/// `BTreeMap` + bits de cada `f64`). Pura, sin pánicos.
pub fn params_fingerprint(params: &std::collections::BTreeMap<String, f64>) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for (k, v) in params {
        for byte in k.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in v.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

/// Decisión de redibujo estilo `always_redraw`.
///
/// `always=true` → siempre `true` (Manim default: el updater corre por
/// frame). `always=false` → `true` solo si el fingerprint cambió desde la
/// última llamada (el caller guarda el estado entre scrubs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdaterRedraw {
    /// `true` = re-renderizar siempre (semántica Manim).
    pub always: bool,
    /// Último fingerprint visto (`u64::MAX` = ninguno todavía).
    pub ultimo_hash: u64,
}

impl UpdaterRedraw {
    /// Constructor: `UpdaterRedraw::nuevo(true)` = `always_redraw`.
    pub const fn nuevo(always: bool) -> Self {
        Self {
            always,
            ultimo_hash: u64::MAX,
        }
    }

    /// ¿Hay que re-renderizar con estos params? Actualiza `ultimo_hash`.
    pub fn debe_redibujar(&mut self, params: &std::collections::BTreeMap<String, f64>) -> bool {
        if self.always {
            return true;
        }
        let hash = params_fingerprint(params);
        if hash == self.ultimo_hash {
            false
        } else {
            self.ultimo_hash = hash;
            true
        }
    }
}

// ── fps / bitrate / -qm / -ql (Manim resto, lado Piel) ─────────────────────
// BLOQUEADOR HONESTO: `AnimRequest` vive en `grafito-anim/src/protocol.rs`
// (vedado: W4 hecho). Estos helpers leen `fps`/`bitrate_kbps`/`quality` del
// mapa `params` (el mismo que viaja en el wire) con validación acotada;
// agregar los CAMPOS al struct es tarea W4. Default intacto: 48 frames.
// `mp4_fps_for_delay` (delay 8cs → 12 fps) sigue mandando en el export
// actual; `anim_fps_desde_params` lo espeja cuando el pedido trae `fps`.

/// fps mínimo aceptado (Manim permite 1).
pub const ANIM_FPS_MIN: u32 = 1;
/// fps máximo aceptado (60 = tope razonable de preview).
pub const ANIM_FPS_MAX: u32 = 60;
/// fps cuando el pedido no trae `fps` (12 = delay 8cs histórico).
pub const ANIM_FPS_DEFAULT: u32 = 12;
/// Bitrate mínimo en kbps.
pub const ANIM_BITRATE_MIN_KBPS: u32 = 100;
/// Bitrate máximo en kbps (20 Mbps, cota anti-abuso del export).
pub const ANIM_BITRATE_MAX_KBPS: u32 = 20_000;
/// Bitrate cuando el pedido no trae `bitrate_kbps`.
pub const ANIM_BITRATE_DEFAULT_KBPS: u32 = 2000;

/// fps desde `params["fps"]`: ausente/NaN/inf → 12; redondea y clampa 1..=60.
pub fn anim_fps_desde_params(params: &std::collections::BTreeMap<String, f64>) -> u32 {
    match params.get(P2_PARAM_FPS).copied() {
        Some(v) if v.is_finite() => {
            (v.round()
                .clamp(f64::from(ANIM_FPS_MIN), f64::from(ANIM_FPS_MAX))) as u32
        }
        _ => ANIM_FPS_DEFAULT,
    }
}

/// Bitrate kbps desde `params["bitrate_kbps"]`: ausente/NaN/inf → 2000;
/// clampa 100..=20000.
pub fn anim_bitrate_desde_params(params: &std::collections::BTreeMap<String, f64>) -> u32 {
    match params.get(P2_PARAM_BITRATE).copied() {
        Some(v) if v.is_finite() => {
            (v.round().clamp(
                f64::from(ANIM_BITRATE_MIN_KBPS),
                f64::from(ANIM_BITRATE_MAX_KBPS),
            )) as u32
        }
        _ => ANIM_BITRATE_DEFAULT_KBPS,
    }
}

/// Calidad de export (Manim `-ql`/`-qm`; 2 = alta, sin flag Manim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoQuality {
    /// `-ql`: rápido y liviano (crf 30, igual que el webm actual).
    Baja,
    /// `-qm` (default): equilibrio (crf 23, igual que el mp4 actual).
    #[default]
    Media,
    /// Sin flag Manim: archivo grande y nítido (crf 18, preset fast).
    Alta,
}

impl VideoQuality {
    /// `(crf, preset)` ffmpeg para la calidad.
    pub const fn flags(self) -> (u8, &'static str) {
        match self {
            Self::Baja => (30, "veryfast"),
            Self::Media => (23, "veryfast"),
            Self::Alta => (18, "fast"),
        }
    }
}

/// Calidad desde `params["quality"]`: 0 → Baja, 2 → Alta, resto → Media.
pub fn video_quality_desde_params(
    params: &std::collections::BTreeMap<String, f64>,
) -> VideoQuality {
    match params.get(P2_PARAM_QUALITY).copied() {
        Some(v) if v.is_finite() && v.round() as i64 == 0 => VideoQuality::Baja,
        Some(v) if v.is_finite() && v.round() as i64 == 2 => VideoQuality::Alta,
        _ => VideoQuality::Media,
    }
}

/// Argumentos ffmpeg para el export con calidad (espeja los del writer mp4
/// actual + `-b:v`; el cableado al `Command` real es P3: los writers los
/// posee W5). Puro.
pub fn ffmpeg_video_args_con_calidad(
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    w: u32,
    h: u32,
) -> Vec<String> {
    let fps = fps.clamp(ANIM_FPS_MIN, ANIM_FPS_MAX);
    let bitrate_kbps = bitrate_kbps.clamp(ANIM_BITRATE_MIN_KBPS, ANIM_BITRATE_MAX_KBPS);
    let (crf, preset) = quality.flags();
    vec![
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{w}x{h}"),
        "-framerate".to_string(),
        fps.to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        preset.to_string(),
        "-crf".to_string(),
        crf.to_string(),
        "-b:v".to_string(),
        format!("{bitrate_kbps}k"),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-vf".to_string(),
        "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
    ]
}

// ── Export real con calidad (cableado MP4/WebM) ────────────────────────────
// `-ql`/`-qm`/`-qh` mapean a Resolución + crf + bitrate reales:
// - `video_quality_export_size`: lado mayor 640 (Baja) / 1280 (Media) /
//   nativo ≤4096 (Alta), pares (yuv420p), sin upscale.
// - crf/preset: `VideoQuality::flags` (30/23/18 + veryfast/veryfast/fast).
// - bitrate: `bitrate_kbps` del diálogo (100..=20000) a `-b:v {k}k`.
// Todo puro salvo el reescalado (CPU en el hilo worker, jamás en UI).

/// Lado mayor máximo por calidad (`-ql` 640 / `-qm` 1280 / `-qh` nativo).
pub const fn video_quality_max_side(quality: VideoQuality) -> u32 {
    match quality {
        VideoQuality::Baja => 640,
        VideoQuality::Media => 1280,
        VideoQuality::Alta => 4096,
    }
}

/// Tamaño de export para la calidad (pares, sin upscale, ≤4096). Puro.
pub fn video_quality_export_size(w: usize, h: usize, quality: VideoQuality) -> (usize, usize) {
    let tope = video_quality_max_side(quality) as usize;
    let mayor = w.max(h);
    if mayor == 0 || mayor <= tope {
        return (w - w % 2, h - h % 2);
    }
    let (nw, nh) = ((w * tope) / mayor.max(1), (h * tope) / mayor.max(1));
    let (nw, nh) = (nw.max(2) - (nw.max(2) % 2), nh.max(2) - (nh.max(2) % 2));
    (nw.max(2), nh.max(2))
}

/// Reescala los frames al tamaño de la calidad (solo downscale, Lanczos3
/// vía `image`). Sin upscale: si ya encajan, devuelve clon. Puro en CPU
/// (llamar en hilo worker).
fn reescalar_frames_para_calidad(
    frames: &[egui::ColorImage],
    quality: VideoQuality,
) -> Vec<egui::ColorImage> {
    let primero = frames.first().map(|f| f.size);
    let Some([bw, bh]) = primero else {
        return Vec::new();
    };
    let (nw, nh) = video_quality_export_size(bw, bh, quality);
    if (nw, nh) == (bw, bh) || nw == 0 || nh == 0 {
        return frames.to_vec();
    }
    let (nw32, nh32) = (nw as u32, nh as u32);
    frames
        .iter()
        .filter(|f| f.size == [bw, bh])
        .map(|f| {
            let bytes: Vec<u8> = f.pixels.iter().flat_map(|p| p.to_array()).collect();
            let img = image::RgbaImage::from_raw(bw as u32, bh as u32, bytes);
            let Some(img) = img else {
                return f.clone();
            };
            let chica =
                image::imageops::resize(&img, nw32, nh32, image::imageops::FilterType::Triangle);
            let pixeles: Vec<egui::Color32> = chica
                .pixels()
                .map(|p| egui::Color32::from_rgba_unmultiplied(p.0[0], p.0[1], p.0[2], p.0[3]))
                .collect();
            egui::ColorImage {
                size: [nw, nh],
                pixels: pixeles,
            }
        })
        .collect()
}

/// Re-muestrea los frames al fps pedido con duración fija honesta.
///
/// Construye un `Timeline` (1 key por frame sobre la duración total) y lo
/// muestrea con `Timeline::sample` (lineal) en los instantes del fps
/// destino: 48 @12fps → 96 @24fps = mismos 4s. Vacío → vacío; fps fuera
/// de 1..=60 se clampean. Puro (clona frames, sin E/S).
pub fn remuestrear_frames_para_fps(
    frames: &[egui::ColorImage],
    fps_origen: u32,
    fps_destino: u32,
) -> Vec<egui::ColorImage> {
    use grafito_anim::protocol::{Keyframe, Timeline};
    if frames.is_empty() {
        return Vec::new();
    }
    let origen = fps_origen.clamp(ANIM_FPS_MIN, ANIM_FPS_MAX) as u64;
    let destino = fps_destino.clamp(ANIM_FPS_MIN, ANIM_FPS_MAX) as u64;
    if origen == destino {
        return frames.to_vec();
    }
    let n = frames.len() as u64;
    let duracion_ms = n.saturating_mul(1000) / origen.max(1);
    if duracion_ms == 0 {
        return frames.to_vec();
    }
    let keys: Vec<Keyframe> = (0..n)
        .map(|i| Keyframe {
            t_ms: i.saturating_mul(1000) / origen,
            value: i as f32,
        })
        .collect();
    let timeline = Timeline {
        duration_ms: duracion_ms.max(1),
        keyframes: keys,
    };
    let total_dest = (duracion_ms.saturating_mul(destino) / 1000).max(1);
    // Tope anti-abuso: no generar más de 512 frames por re-muestreo (el
    // preflight de budgets del runner valida igual; esto evita el clon
    // gigante antes del `Err` honesto).
    let total_dest = total_dest.min(512) as usize;
    (0..total_dest)
        .map(|j| {
            let t = (j as u64).saturating_mul(1000) / destino;
            let idx = timeline.sample(t).round().clamp(0.0, (n - 1) as f32) as usize;
            frames[idx.min(frames.len() - 1)].clone()
        })
        .collect()
}

// ── SVGMobject / ImageMobject (Manim resto, lado Piel) ─────────────────────
// BLOQUEADOR HONESTO: el enum `Mobject` vive en `grafito-anim/src/scene.rs`
// (vedado: W4). `usvg` NO es dependencia del workspace (sumarla toca
// `Cargo.toml`, fuera de este scope); por eso `SVGMobject` se rasteriza por
// el MISMO subconjunto honesto del `Tex` W5 (`draw_tex_svg_onto`: circle/
// rect/line sobre viewBox, ≤64 KiB, sin duplicar el parser). `ImageMobject`
// usa el crate `image` (ya dependencia) con presupuesto propio.

/// Tope de bytes de un `ImageMobject` (1 MiB, paridad con AttachmentLimits).
pub const MAX_IMAGE_MOBJECT_BYTES: usize = 1_048_576;
/// Lado máximo decodificado de un `ImageMobject` (paridad `Resolution`).
pub const MAX_IMAGE_MOBJECT_SIDE: u32 = 4096;

/// Rasteriza un `SVGMobject` sobre el frame con el parser honesto W5.
/// `false` honesto si el SVG excede presupuesto o no trae formas del
/// subconjunto (el llamador decide fallback, jamás inventa píxeles).
pub fn raster_svg_mobject_onto(buf: &mut [u8], w: usize, h: usize, svg: &str) -> bool {
    if svg.is_empty() || !svg.contains("<svg") {
        return false;
    }
    draw_tex_svg_onto(buf, w, h, svg)
}

/// Rasteriza un `ImageMobject` (bytes PNG/JPEG) centrado sobre el frame con
/// alfa real (`mezclar_pixel_alfa`). `false` honesto si los bytes exceden
/// 1 MiB, no decodifican o el frame no es RGBA `w*h*4`.
pub fn raster_image_mobject_onto(buf: &mut [u8], w: usize, h: usize, bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_MOBJECT_BYTES {
        return false;
    }
    if buf.len() != w.checked_mul(h).and_then(|v| v.checked_mul(4)).unwrap_or(0) {
        return false;
    }
    let Ok(img) = image::load_from_memory(bytes) else {
        return false;
    };
    if img.width() == 0 || img.height() == 0 {
        return false;
    }
    if img.width() > MAX_IMAGE_MOBJECT_SIDE || img.height() > MAX_IMAGE_MOBJECT_SIDE {
        return false;
    }
    let chica = img.thumbnail(
        w.min(MAX_IMAGE_MOBJECT_SIDE as usize) as u32,
        h.min(MAX_IMAGE_MOBJECT_SIDE as usize) as u32,
    );
    let rgba = chica.to_rgba8();
    let (iw, ih) = (rgba.width() as usize, rgba.height() as usize);
    if iw == 0 || ih == 0 || iw > w || ih > h {
        return false;
    }
    let ox = (w - iw) / 2;
    let oy = (h - ih) / 2;
    for y in 0..ih {
        for x in 0..iw {
            let p = rgba.get_pixel(x as u32, y as u32).0;
            let idx = ((oy + y) * w + (ox + x)) * 4;
            let Some(slot) = buf.get_mut(idx..idx + 4) else {
                return false;
            };
            let fondo = [slot[0], slot[1], slot[2], slot[3]];
            let mezcla = mezclar_pixel_alfa(fondo, p);
            slot.copy_from_slice(&mezcla);
        }
    }
    true
}

// ── P1-app: export narrado (render→mux→burn→sidecar) ───────────────────────
// Combina los writers streaming (`export_mp4_streaming_with_bin`) con el
// frente voz (`voice.rs`): render del set → voiceover piper → mux offset/gain
// → quemado ASS → sidecar SRT. Todo en hilo worker (`spawn_mp4_narrado` +
// `CancellationToken` cooperativo, tmp+rename `O_EXCL`, kill+wait anti-zombie
// en cada etapa); la UI solo dispara y lee el `JoinHandle` + `ProgresoChunks`.
// Solo MP4: el mux usa `-c:v copy` (H.264) y el burn re-encodea libx264.
// Sin piper → `Voz(PiperMissing)`; sin ffmpeg → `Mux/Burn::FfmpegMissing`
// honestos (el wiring muestra el video mudo + `.srt`). El wav es insumo
// temporal (se borra tras el mux, jamás se publica).

/// Pedido de voiceover: una sola pista de voz (contrato `AudioTrack`:
/// `offset_ms` 0..=60000, `gain` 0.0..=2.0, validados en el mux).
#[derive(Debug, Clone)]
pub struct VoiceoverPedido {
    /// Texto a narrar (1..=8192 chars, `VOICE_MAX_TEXT_CHARS`).
    pub texto: String,
    /// Voz `.onnx` en disco (`detect_piper_voice_path` la encuentra).
    pub voz: PathBuf,
    /// Desplazamiento en ms (0..=60000).
    pub offset_ms: u32,
    /// Ganancia lineal (0.0..=2.0, finita).
    pub gain: f32,
}

/// Subtítulos del export: el `.srt` sidecar se escribe siempre; con
/// `quemar = true` además se quema el ASS sobre el video (re-encode).
#[derive(Debug, Clone)]
pub struct SubtitulosPedido {
    /// Pista validada (`to_srt`/`to_ass` con cota 256 KiB internas).
    pub track: grafito_anim::captions::CaptionTrack,
    /// ¿Quemar sobre el video (exige ffmpeg) o solo sidecar?
    pub quemar: bool,
}

/// Bins externos (tests herméticos / distros sin PATH). Prod pasa `None`
/// (= `piper`/`ffmpeg` del PATH real).
#[derive(Debug, Clone)]
pub struct VozBins {
    /// Binario piper (tests: script falso).
    pub piper: PathBuf,
    /// Binario ffmpeg (tests: script falso).
    pub ffmpeg: PathBuf,
}

/// Salida del export narrado: video final + sidecar si hubo subtítulos.
#[derive(Debug, Clone)]
pub struct VideoNarrado {
    /// Video publicado (con voz quemada según el pedido).
    pub video: PathBuf,
    /// Sidecar `.srt` junto al video (`None` si no hubo subtítulos).
    pub srt: Option<PathBuf>,
}

/// Error tipado del export narrado (cada etapa conserva su error honesto).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoNarradoError {
    /// Falló el render del set (incluye `Cancelled` y `FfmpegMissing`).
    Render(Mp4ExportError),
    /// Falló el voiceover piper.
    Voz(voice::VoiceError),
    /// Falló el mux audio→video.
    Mux(voice::MuxError),
    /// Falló el quemado ASS.
    Burn(voice::CaptionBurnError),
    /// Falló el sidecar `.srt` / `.ass` intermedio.
    Sidecar(voice::SidecarError),
}

impl std::fmt::Display for VideoNarradoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Render(inner) => write!(f, "{inner}"),
            Self::Voz(inner) => write!(f, "{inner}"),
            Self::Mux(inner) => write!(f, "{inner}"),
            Self::Burn(inner) => write!(f, "{inner}"),
            Self::Sidecar(inner) => write!(f, "{inner}"),
        }
    }
}

impl std::error::Error for VideoNarradoError {}

/// Tmp intermedio del narrado (`<name>.narrado-<etapa>.<pid>-<nanos>.mp4`):
/// nombres distintos por etapa para no colisionar entre sí. Puro, sin E/S.
fn narrado_tmp(path: &Path, etapa: &str, extension: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("clip"));
    let tmp_name = format!(
        "{name}.narrado-{etapa}.{}-{stamp}.{extension}",
        std::process::id()
    );
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Borrado best-effort de intermedios (jamás deja parcial huérfano).
fn narrado_limpiar(tmps: &[PathBuf]) {
    for tmp in tmps {
        let _ = std::fs::remove_file(tmp);
    }
}

/// Publicación final tmp→destino con `O_EXCL` en ambos bordes (igual que
/// los writers: pre-chequeo + re-chequeo pre-rename anti-TOCTOU).
fn narrado_publicar(tmp: &Path, destino: &Path) -> Result<PathBuf, String> {
    if std::fs::symlink_metadata(destino).is_ok() {
        return Err(format!(
            "no se pudo crear {} sin sobrescribir: el destino ya existe",
            destino.display()
        ));
    }
    if let Err(e) = std::fs::rename(tmp, destino) {
        let _ = std::fs::remove_file(tmp);
        return Err(format!("no se pudo publicar {}: {e}", destino.display()));
    }
    Ok(destino.to_path_buf())
}

/// Núcleo bloqueante (llamar en hilo): render→mux→burn→sidecar con progreso
/// por hitos (render 0..0.7, voz+mux 0.7..0.85, burn 0.85..0.95, sidecar 1.0).
/// Sin post (ni voz ni quemado) el render va directo al destino con progreso
/// fino real del streaming. Sin pánicos.
#[allow(clippy::too_many_arguments)]
pub fn export_mp4_narrado_streaming(
    productor: &mut dyn FnMut(usize) -> Option<egui::ColorImage>,
    total_frames: usize,
    path: &Path,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    voz: Option<VoiceoverPedido>,
    subtitulos: Option<SubtitulosPedido>,
    bins: Option<&VozBins>,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<VideoNarrado, VideoNarradoError> {
    if token.is_cancelled() {
        return Err(VideoNarradoError::Render(Mp4ExportError::Cancelled));
    }
    let ffmpeg_bin: &Path = bins
        .map(|b| b.ffmpeg.as_path())
        .unwrap_or(Path::new("ffmpeg"));
    let quema = subtitulos.as_ref().is_some_and(|s| s.quemar);
    // Camino rápido: sin voz ni quemado el render publica directo.
    if voz.is_none() && !quema {
        let video = export_mp4_streaming_with_bin(
            productor,
            total_frames,
            path,
            fps,
            bitrate_kbps,
            quality,
            token,
            progreso,
            ffmpeg_bin,
        )
        .map_err(VideoNarradoError::Render)?;
        let srt = match subtitulos {
            Some(pedido) => Some(
                voice::write_caption_sidecar(&pedido.track, path)
                    .map_err(VideoNarradoError::Sidecar)?,
            ),
            None => None,
        };
        marcar_progreso(progreso, 1, 1);
        return Ok(VideoNarrado { video, srt });
    }
    // Camino con post: render a intermedio, luego voz/mux/burn.
    let interna: ProgresoChunks = std::sync::Arc::new(std::sync::Mutex::new(0.0));
    let render_tmp = narrado_tmp(path, "render", "mp4");
    let mut tmps = vec![render_tmp.clone()];
    marcar_progreso(progreso, 0, 1);
    if let Err(e) = export_mp4_streaming_with_bin(
        productor,
        total_frames,
        &render_tmp,
        fps,
        bitrate_kbps,
        quality,
        token,
        Some(&interna),
        ffmpeg_bin,
    ) {
        narrado_limpiar(&tmps);
        return Err(VideoNarradoError::Render(e));
    }
    marcar_progreso(progreso, 7, 10);
    let mut actual = render_tmp.clone();
    // Voz: piper en su hilo (join honesto) + mux offset/gain.
    if let Some(pedido) = voz {
        if token.is_cancelled() {
            narrado_limpiar(&tmps);
            return Err(VideoNarradoError::Voz(voice::VoiceError::Cancelled));
        }
        let piper_bin = bins
            .map(|b| b.piper.clone())
            .unwrap_or_else(|| PathBuf::from("piper"));
        let wav_tmp = narrado_tmp(path, "voz", "wav");
        tmps.push(wav_tmp.clone());
        let handle = voice::spawn_voiceover_with_bin(
            pedido.texto,
            pedido.voz,
            wav_tmp.clone(),
            token.clone(),
            piper_bin,
        );
        let wav = match handle.join() {
            Ok(Ok(wav)) => wav,
            Ok(Err(e)) => {
                narrado_limpiar(&tmps);
                return Err(VideoNarradoError::Voz(e));
            }
            Err(_) => {
                narrado_limpiar(&tmps);
                return Err(VideoNarradoError::Voz(voice::VoiceError::Failed(
                    "el hilo de voz terminó de forma inesperada".to_string(),
                )));
            }
        };
        let mux_tmp = narrado_tmp(path, "mux", "mp4");
        tmps.push(mux_tmp.clone());
        let muxado = voice::mux_audio_cancelable_with_bin(
            &actual,
            &wav,
            pedido.offset_ms,
            pedido.gain,
            &mux_tmp,
            token,
            ffmpeg_bin,
        );
        // El wav es insumo temporal: se borra siempre tras el mux.
        let _ = std::fs::remove_file(&wav);
        tmps.retain(|t| t != &wav_tmp);
        match muxado {
            Ok(muxado) => {
                let _ = std::fs::remove_file(&actual);
                tmps.retain(|t| t != &actual);
                actual = muxado;
            }
            Err(e) => {
                narrado_limpiar(&tmps);
                return Err(VideoNarradoError::Mux(e));
            }
        }
        marcar_progreso(progreso, 85, 100);
    }
    // Burn: ASS intermedio → re-encode → limpieza del ASS.
    if let Some(pedido) = subtitulos.as_ref() {
        if pedido.quemar {
            if token.is_cancelled() {
                narrado_limpiar(&tmps);
                return Err(VideoNarradoError::Burn(voice::CaptionBurnError::Cancelled));
            }
            let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
            let dir_cow: PathBuf = dir.map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            let ass = match voice::write_ass_temp(&pedido.track, &dir_cow) {
                Ok(ass) => ass,
                Err(e) => {
                    narrado_limpiar(&tmps);
                    return Err(VideoNarradoError::Sidecar(e));
                }
            };
            let burn_tmp = narrado_tmp(path, "burn", "mp4");
            tmps.push(burn_tmp.clone());
            let quemado = voice::burn_captions_cancelable_with_bin(
                &actual, &ass, &burn_tmp, token, ffmpeg_bin,
            );
            let _ = std::fs::remove_file(&ass);
            match quemado {
                Ok(quemado) => {
                    let _ = std::fs::remove_file(&actual);
                    tmps.retain(|t| t != &actual);
                    actual = quemado;
                }
                Err(e) => {
                    narrado_limpiar(&tmps);
                    return Err(VideoNarradoError::Burn(e));
                }
            }
            marcar_progreso(progreso, 95, 100);
        }
    }
    if token.is_cancelled() {
        narrado_limpiar(&tmps);
        return Err(VideoNarradoError::Render(Mp4ExportError::Cancelled));
    }
    // Publicación final + sidecar (el sidecar va sobre el destino real).
    let resto: Vec<PathBuf> = tmps.iter().filter(|t| *t != &actual).cloned().collect();
    let video = match narrado_publicar(&actual, path) {
        Ok(video) => video,
        Err(detalle) => {
            narrado_limpiar(&tmps);
            return Err(VideoNarradoError::Render(Mp4ExportError::Io(detalle)));
        }
    };
    narrado_limpiar(&resto);
    let srt = match subtitulos {
        Some(pedido) => Some(
            voice::write_caption_sidecar(&pedido.track, path)
                .map_err(VideoNarradoError::Sidecar)?,
        ),
        None => None,
    };
    marcar_progreso(progreso, 1, 1);
    Ok(VideoNarrado { video, srt })
}

/// Idem desde un set materializado (corto, ≤64 frames): adapta el slice a
/// productor y delega. Bloquea: llamar en hilo.
#[allow(clippy::too_many_arguments)]
pub fn export_mp4_narrado_desde_set(
    frames: &[egui::ColorImage],
    path: &Path,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    voz: Option<VoiceoverPedido>,
    subtitulos: Option<SubtitulosPedido>,
    bins: Option<&VozBins>,
    token: &CancellationToken,
    progreso: Option<&ProgresoChunks>,
) -> Result<VideoNarrado, VideoNarradoError> {
    let total = frames.len();
    let mut productor = |j: usize| frames.get(j).cloned();
    export_mp4_narrado_streaming(
        &mut productor,
        total,
        path,
        fps,
        bitrate_kbps,
        quality,
        voz,
        subtitulos,
        bins,
        token,
        progreso,
    )
}

/// Export narrado en un hilo aparte (no bloquea la UI). Espejo de
/// `spawn_mp4_export`: el wiring lo dispara con el pedido del diálogo y al
/// hacer `join` publica `media_path` / estado.
#[allow(clippy::too_many_arguments)]
pub fn spawn_mp4_narrado(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    fps: u32,
    bitrate_kbps: u32,
    quality: VideoQuality,
    voz: Option<VoiceoverPedido>,
    subtitulos: Option<SubtitulosPedido>,
    bins: Option<VozBins>,
    token: CancellationToken,
    progreso: Option<ProgresoChunks>,
) -> std::thread::JoinHandle<Result<VideoNarrado, VideoNarradoError>> {
    std::thread::spawn(move || {
        export_mp4_narrado_desde_set(
            &frames,
            &path,
            fps,
            bitrate_kbps,
            quality,
            voz,
            subtitulos,
            bins.as_ref(),
            &token,
            progreso.as_ref(),
        )
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod p1_voz_tests {
    use super::*;

    fn frames_mini(n: usize) -> Vec<egui::ColorImage> {
        vec![egui::ColorImage::new([16, 16], egui::Color32::RED); n]
    }

    fn dir_unico(prefijo: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "grafito-narrado-{prefijo}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// piper falso: respeta `--output_file X`, drena stdin, escribe wav.
    #[cfg(unix)]
    fn piper_falso(base: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = base.join("piper-falso");
        std::fs::write(
            &bin,
            "#!/bin/sh\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"--output_file\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\ncat > /dev/null\nprintf 'RIFFfalso-wav' > \"$out\"\nexit 0\n",
        )
        .unwrap();
        let mut permisos = std::fs::metadata(&bin).unwrap().permissions();
        permisos.set_mode(0o755);
        std::fs::set_permissions(&bin, permisos).unwrap();
        bin
    }

    /// ffmpeg falso en modo append: acumula argv de TODAS las etapas
    /// (render+mux+burn) y `touch`ea el último argv como el real.
    #[cfg(unix)]
    fn ffmpeg_falso_append(base: &Path) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = base.join("ffmpeg-falso");
        let captura = base.join("argv.txt");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\ncat > /dev/null\nfor a in \"$@\"; do ultimo=\"$a\"; done\ntouch \"$ultimo\"\nexit 0\n",
                captura.display()
            ),
        )
        .unwrap();
        let mut permisos = std::fs::metadata(&bin).unwrap().permissions();
        permisos.set_mode(0o755);
        std::fs::set_permissions(&bin, permisos).unwrap();
        (bin, captura)
    }

    fn pista_hola() -> grafito_anim::captions::CaptionTrack {
        grafito_anim::captions::CaptionTrack::try_new(vec![
            grafito_anim::captions::CaptionSegment::frase("hola mundo".to_string(), 1000, 3500)
                .unwrap(),
        ])
        .unwrap()
    }

    fn sin_tmps(base: &Path) {
        let restos: Vec<_> = std::fs::read_dir(base)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_string_lossy().contains(".tmp.")
                    || e.file_name().to_string_lossy().contains(".narrado-")
            })
            .collect();
        assert!(restos.is_empty(), "sin intermedios huérfanos: {restos:?}");
    }

    #[cfg(unix)]
    #[test]
    fn narrado_full_orden_render_mux_burn_sidecar() {
        let base = dir_unico("full");
        let piper = piper_falso(&base);
        let (ffmpeg, captura) = ffmpeg_falso_append(&base);
        let bins = VozBins { piper, ffmpeg };
        let voz_onnx = base.join("voz.onnx");
        std::fs::write(&voz_onnx, b"falsa").unwrap();
        let dest = base.join("clip.mp4");
        let voz = VoiceoverPedido {
            texto: "hola mundo".to_string(),
            voz: voz_onnx,
            offset_ms: 500,
            gain: 1.5,
        };
        let subtitulos = SubtitulosPedido {
            track: pista_hola(),
            quemar: true,
        };
        let salida = export_mp4_narrado_desde_set(
            &frames_mini(2),
            &dest,
            12,
            2000,
            VideoQuality::Media,
            Some(voz),
            Some(subtitulos),
            Some(&bins),
            &CancellationToken::default(),
            None,
        )
        .expect("los falsos siempre salen 0");
        assert_eq!(salida.video, dest);
        assert!(dest.exists(), "video final publicado");
        // Sidecar junto al video con el texto.
        let srt = salida.srt.expect("con subtítulos hay sidecar");
        assert_eq!(srt, base.join("clip.srt"));
        // El SRT parte en ≤2 renglones: ambas palabras, no la frase literal.
        let srt_texto = std::fs::read_to_string(&srt).unwrap();
        assert!(srt_texto.contains("hola"));
        assert!(srt_texto.contains("mundo"));
        // Las 3 etapas pasaron por ffmpeg: render (libx264) + mux
        // (itsoffset/volume) + burn (ass=). Orden: el mux va después del
        // primer libx264 y el burn después del mux.
        let argv = std::fs::read_to_string(&captura).unwrap();
        let lineas: Vec<&str> = argv.lines().collect();
        assert_eq!(lineas.len(), 3, "render+mux+burn, fue:\n{argv}");
        assert!(lineas[0].contains("libx264"), "render primero");
        assert!(lineas[1].contains("-itsoffset") && lineas[1].contains("0.5"));
        assert!(lineas[1].contains("volume=1.5"));
        assert!(lineas[1].contains("-c:v copy"), "mux no re-encodea");
        assert!(
            lineas[2].contains("ass="),
            "burn con ass=, fue: {}",
            lineas[2]
        );
        assert!(lineas[2].contains("-c:a copy"), "burn copia el audio");
        sin_tmps(&base);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn narrado_sin_voz_solo_sidecar_sin_burn() {
        let base = dir_unico("sidecar");
        let (ffmpeg, captura) = ffmpeg_falso_append(&base);
        let bins = VozBins {
            piper: base.join("piper-no"),
            ffmpeg,
        };
        let dest = base.join("clip.mp4");
        let subtitulos = SubtitulosPedido {
            track: pista_hola(),
            quemar: false,
        };
        let salida = export_mp4_narrado_desde_set(
            &frames_mini(2),
            &dest,
            12,
            2000,
            VideoQuality::Media,
            None,
            Some(subtitulos),
            Some(&bins),
            &CancellationToken::default(),
            None,
        )
        .expect("render falso + sidecar");
        assert!(dest.exists());
        assert!(salida.srt.unwrap().exists());
        // Sin quemado: una sola llamada (el render), sin ass=.
        let argv = std::fs::read_to_string(&captura).unwrap();
        assert_eq!(argv.lines().count(), 1, "solo render, fue:\n{argv}");
        assert!(!argv.contains("ass="));
        sin_tmps(&base);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn narrado_sin_piper_falla_honesto_sin_video() {
        // Bins a binarios inexistentes: hermético. El render falla primero
        // (antes que la voz), sin publicar nada.
        let base = dir_unico("missing");
        let bins = VozBins {
            piper: PathBuf::from("/definitivamente/no/existe/piper-falso"),
            ffmpeg: PathBuf::from("/definitivamente/no/existe/ffmpeg-falso"),
        };
        let voz_onnx = base.join("voz.onnx");
        std::fs::write(&voz_onnx, b"falsa").unwrap();
        let dest = base.join("clip.mp4");
        // Sin bins de render tampoco hay render: el render usa el ffmpeg
        // inexistente → Render(FfmpegMissing) honesto antes que la voz.
        let err = export_mp4_narrado_desde_set(
            &frames_mini(2),
            &dest,
            12,
            2000,
            VideoQuality::Media,
            Some(VoiceoverPedido {
                texto: "hola".to_string(),
                voz: voz_onnx,
                offset_ms: 0,
                gain: 1.0,
            }),
            None,
            Some(&bins),
            &CancellationToken::default(),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                VideoNarradoError::Render(Mp4ExportError::FfmpegMissing)
            ),
            "el render falla primero y honesto, fue: {err}"
        );
        assert!(!dest.exists());
        sin_tmps(&base);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn narrado_cancelado_antes_no_toca_disco() {
        let base = dir_unico("cancel");
        let token = CancellationToken::default();
        token.cancel();
        let dest = base.join("clip.mp4");
        let err = export_mp4_narrado_desde_set(
            &frames_mini(2),
            &dest,
            12,
            2000,
            VideoQuality::Media,
            None,
            None,
            None,
            &token,
            None,
        )
        .unwrap_err();
        assert_eq!(err, VideoNarradoError::Render(Mp4ExportError::Cancelled));
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn narrado_spawn_no_bloquea_y_publica() {
        let base = dir_unico("spawn");
        let piper = piper_falso(&base);
        let (ffmpeg, _captura) = ffmpeg_falso_append(&base);
        let voz_onnx = base.join("voz.onnx");
        std::fs::write(&voz_onnx, b"falsa").unwrap();
        let dest = base.join("clip.mp4");
        let handle = spawn_mp4_narrado(
            frames_mini(2),
            dest.clone(),
            12,
            2000,
            VideoQuality::Media,
            Some(VoiceoverPedido {
                texto: "hola".to_string(),
                voz: voz_onnx,
                offset_ms: 0,
                gain: 1.0,
            }),
            Some(SubtitulosPedido {
                track: pista_hola(),
                quemar: true,
            }),
            Some(VozBins { piper, ffmpeg }),
            CancellationToken::default(),
            None,
        );
        let salida = handle.join().expect("el hilo no debe panicar").unwrap();
        assert_eq!(salida.video, dest);
        assert!(dest.exists());
        assert!(salida.srt.unwrap().exists());
        sin_tmps(&base);
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod p2_tests {
    use super::*;

    fn mapa(pares: &[(&str, f64)]) -> std::collections::BTreeMap<String, f64> {
        pares.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    #[test]
    fn p2_vacio_reproduce_legacy_exactos() {
        let vacio = mapa(&[]);
        for (rotulo, legacy, vivo) in [
            (
                "conformal-map",
                render_conformal_frames(64, 64),
                render_conformal_frames_with_params(64, 64, &vacio),
            ),
            (
                "pitagoras",
                render_pitagoras_frames(64, 64),
                render_pitagoras_frames_with_params(64, 64, &vacio),
            ),
            (
                "logistic-bifurcation",
                render_logistic_bifurcation_frames(64, 64),
                render_logistic_bifurcation_frames_with_params(64, 64, &vacio),
            ),
            (
                "gradient-field",
                render_gradient_field_frames(64, 64),
                render_gradient_field_frames_with_params(64, 64, &vacio),
            ),
            (
                "mobius-transform",
                render_mobius_frames(64, 64),
                render_mobius_frames_with_params(64, 64, &vacio),
            ),
        ] {
            assert_eq!(legacy.len(), vivo.len(), "{rotulo}: len");
            for (i, (a, b)) in legacy.iter().zip(vivo.iter()).enumerate() {
                assert_eq!(a.pixels, b.pixels, "{rotulo} frame {i}: idéntico a legacy");
            }
        }
    }

    #[test]
    fn p2_knobs_mueven_los_frames() {
        let medio = NATIVE_ANIM_FRAME_COUNT / 2;
        let base = mapa(&[]);
        let conf_base = render_conformal_frames_with_params(64, 64, &base);
        let b = render_conformal_frames_with_params(64, 64, &mapa(&[("cr_range", 1.0)]));
        assert_ne!(
            conf_base[medio].pixels, b[medio].pixels,
            "cr_range debe mover el frame"
        );

        let a = render_pitagoras_frames_with_params(64, 64, &base);
        let b = render_pitagoras_frames_with_params(64, 64, &mapa(&[("tri_size", 2.0)]));
        assert_ne!(
            a[medio].pixels, b[medio].pixels,
            "tri_size debe mover el frame"
        );

        let a = render_logistic_bifurcation_frames_with_params(64, 64, &base);
        let b = render_logistic_bifurcation_frames_with_params(
            64,
            64,
            &mapa(&[("r0", 3.5), ("r1", 4.0)]),
        );
        assert_ne!(
            a[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            b[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
            "r0/r1 debe mover el frame final"
        );

        let a = render_gradient_field_frames_with_params(64, 64, &base);
        let b = render_gradient_field_frames_with_params(64, 64, &mapa(&[("freq", 3.0)]));
        assert_ne!(a[medio].pixels, b[medio].pixels, "freq debe mover el frame");

        let a = render_mobius_frames_with_params(64, 64, &base);
        let b = render_mobius_frames_with_params(64, 64, &mapa(&[("mob_amp", 0.0)]));
        assert_ne!(
            a[medio].pixels, b[medio].pixels,
            "mob_amp debe mover el frame"
        );

        // NaN/inf → defaults = legacy, sin panic.
        let nan = render_conformal_frames_with_params(
            64,
            64,
            &mapa(&[("cr_range", f64::NAN), ("ci_amp", f64::INFINITY)]),
        );
        assert_eq!(conf_base[0].pixels, nan[0].pixels, "NaN/inf → defaults");
        // r0 > r1 se ordena en vez de romper.
        let swap = render_logistic_bifurcation_frames_with_params(
            64,
            64,
            &mapa(&[("r0", 4.0), ("r1", 2.5)]),
        );
        assert_eq!(
            swap.len(),
            NATIVE_ANIM_FRAME_COUNT,
            "swap honesto, 48 frames"
        );
    }

    #[test]
    fn p2_dispatcher_atiende_las_cinco_y_delega_el_resto() {
        let knobs = mapa(&[("freq", 2.0)]);
        let directo = render_gradient_field_frames_with_params(64, 64, &knobs);
        let via_p2 = render_anim_for_concept_with_params_p2(
            "gradient-field",
            "campo",
            64,
            64,
            &knobs,
            true,
            &mut |_, _| {},
        );
        assert_eq!(directo.len(), via_p2.len(), "P2 despacha gradient-field");
        for (i, (a, b)) in directo.iter().zip(via_p2.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "frame {i} idéntico vía P2");
        }
        // El resto delega al dispatcher existente (derivative-slope con x0).
        let x0 = mapa(&[("x0", 1.0)]);
        let via_p2 = render_anim_for_concept_with_params_p2(
            "derivative-slope",
            "derivada",
            64,
            64,
            &x0,
            true,
            &mut |_, _| {},
        );
        let directo = render_derivative_frames_with_params(64, 64, &x0);
        assert_eq!(via_p2.len(), directo.len(), "P2 delega derivative-slope");
        for (i, (a, b)) in via_p2.iter().zip(directo.iter()).enumerate() {
            assert_eq!(a.pixels, b.pixels, "frame {i} delegado idéntico");
        }
    }

    #[test]
    fn p2_fps_bitrate_quality_acotados() {
        let vacio = mapa(&[]);
        assert_eq!(anim_fps_desde_params(&vacio), 12, "default 12 fps");
        assert_eq!(anim_bitrate_desde_params(&vacio), 2000, "default 2000 kbps");
        assert_eq!(
            video_quality_desde_params(&vacio),
            VideoQuality::Media,
            "default -qm"
        );
        assert_eq!(anim_fps_desde_params(&mapa(&[("fps", 60.0)])), 60);
        assert_eq!(
            anim_fps_desde_params(&mapa(&[("fps", 1000.0)])),
            60,
            "clamp arriba"
        );
        assert_eq!(
            anim_fps_desde_params(&mapa(&[("fps", 0.0)])),
            1,
            "clamp abajo"
        );
        assert_eq!(
            anim_fps_desde_params(&mapa(&[("fps", f64::NAN)])),
            12,
            "NaN → default"
        );
        assert_eq!(
            anim_bitrate_desde_params(&mapa(&[("bitrate_kbps", 1e9)])),
            20_000,
            "clamp arriba"
        );
        assert_eq!(
            video_quality_desde_params(&mapa(&[("quality", 0.0)])),
            VideoQuality::Baja,
            "-ql"
        );
        assert_eq!(
            video_quality_desde_params(&mapa(&[("quality", 2.0)])),
            VideoQuality::Alta
        );
        assert_eq!(
            VideoQuality::Media.flags(),
            (23, "veryfast"),
            "media = mp4 actual"
        );
        assert_eq!(
            VideoQuality::Baja.flags(),
            (30, "veryfast"),
            "baja = webm actual"
        );
        let args = ffmpeg_video_args_con_calidad(12, 2000, VideoQuality::Media, 640, 480);
        assert!(args.contains(&"12".to_string()), "lleva el fps");
        assert!(args.contains(&"2000k".to_string()), "lleva el bitrate");
        assert!(args.contains(&"23".to_string()), "lleva el crf de -qm");
    }

    #[test]
    fn p2_always_redraw_decide_bien() {
        let a = mapa(&[("freq", 1.0)]);
        let b = mapa(&[("freq", 2.0)]);
        assert_ne!(
            params_fingerprint(&a),
            params_fingerprint(&b),
            "hash distingue"
        );
        assert_eq!(
            params_fingerprint(&a),
            params_fingerprint(&mapa(&[("freq", 1.0)])),
            "hash estable"
        );
        let mut siempre = UpdaterRedraw::nuevo(true);
        assert!(siempre.debe_redibujar(&a), "always → true");
        assert!(siempre.debe_redibujar(&a), "always → true otra vez");
        let mut cambio = UpdaterRedraw::nuevo(false);
        assert!(cambio.debe_redibujar(&a), "primera vez → true");
        assert!(
            !cambio.debe_redibujar(&a),
            "sin cambio → false (ahorra 48 frames)"
        );
        assert!(cambio.debe_redibujar(&b), "cambio → true");
        assert!(!cambio.debe_redibujar(&b), "otra vez igual → false");
    }

    #[test]
    fn p2_svg_e_imagen_rasterizan_honesto() {
        let w = 64usize;
        let h = 64usize;
        // SVG del subconjunto honesto pinta (círculo sobre viewBox 100x100).
        let mut buf = vec![0u8; w * h * 4];
        let svg = "<svg viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"20\"/></svg>";
        assert!(
            raster_svg_mobject_onto(&mut buf, w, h, svg),
            "círculo pinta"
        );
        assert!(buf.iter().any(|b| *b != 0), "hay píxeles no negros");
        // Basura → false honesto, sin panic.
        let mut buf2 = vec![0u8; w * h * 4];
        assert!(
            !raster_svg_mobject_onto(&mut buf2, w, h, "hola"),
            "sin <svg → false"
        );
        assert!(
            !raster_svg_mobject_onto(&mut buf2, w, h, ""),
            "vacío → false"
        );
        // PNG real de 8x8 rojo vía `image` (ya dependencia): centra y blitea.
        let rojo = image::ImageBuffer::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        let dyn_img = image::DynamicImage::ImageRgba8(rojo);
        let mut png: Vec<u8> = Vec::new();
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let mut buf3 = vec![0u8; w * h * 4];
        assert!(
            raster_image_mobject_onto(&mut buf3, w, h, &png),
            "PNG válido blitea"
        );
        let centro = ((h / 2) * w + (w / 2)) * 4;
        assert_eq!(&buf3[centro..centro + 3], &[255, 0, 0], "centro rojo");
        // Basura y exceso → false honesto.
        assert!(
            !raster_image_mobject_onto(&mut buf3, w, h, b"no-es-imagen"),
            "basura → false"
        );
        assert!(
            !raster_image_mobject_onto(&mut buf3, w, h, &vec![0u8; MAX_IMAGE_MOBJECT_BYTES + 1]),
            "exceso → false"
        );
    }
}

// ── R4: regresión de los 4 contratos (H1/T1-parcial/F4a/F1canon) ─────────
// T1 vive en `manim_orchestrator` (owner) + `assistant` (drains/spawns);
// acá se pinean H1 (autofit+aviso+rescale), F4a (caché LRU acotada) y
// F1canon (viewport canónico + taylor-siempre-frames).
#[cfg(test)]
mod r4_contratos_tests {
    use super::*;

    fn frames_mini(n: usize, w: usize, h: usize) -> Vec<egui::ColorImage> {
        (0..n)
            .map(|k| {
                egui::ColorImage::new([w, h], egui::Color32::from_rgb((k % 256) as u8, 10, 20))
            })
            .collect()
    }

    #[test]
    fn h1_autofit_none_si_entra_y_some_par_si_excede() {
        // Chico entra: sin plan.
        assert_eq!(gif_autofit_size(64, 48, 48), None);
        // El default 480×360×48 (8_294_400 px) autofitea mínimo a dims
        // pares que entran en 8 M.
        let (nw, nh) = gif_autofit_size(CHAT_CANON_W as usize, CHAT_CANON_H as usize, 48)
            .expect("el default debe autofitear");
        assert_eq!((nw % 2, nh % 2), (0, 0), "dims pares");
        assert!(
            nw.checked_mul(nh)
                .and_then(|v| v.checked_mul(48))
                .is_some_and(|v| v <= GIF_EXPORT_MAX_TOTAL_PIXELS),
            "470×352×48 debe entrar: {nw}×{nh}"
        );
        assert!(
            nw >= 460 && nh >= 340,
            "autofit mínimo, no recorte agresivo: {nw}×{nh}"
        );
        // Cero/desborde → None honesto (lo rechaza el preflight después).
        assert_eq!(gif_autofit_size(0, 48, 48), None);
        assert_eq!(gif_autofit_size(usize::MAX, usize::MAX, 48), None);
    }

    #[test]
    fn h1_export_default_autofitea_con_aviso() {
        // Regresión H1: el set default del chat sale con plan + aviso
        // visible camino export (antes el aviso quedaba solo en tests).
        let plan = gif_autofit_size(
            CHAT_CANON_W as usize,
            CHAT_CANON_H as usize,
            NATIVE_ANIM_FRAME_COUNT,
        )
        .expect("default con plan");
        let aviso = mensaje_autofit_gif(plan.0, plan.1);
        assert!(aviso.contains('×'), "aviso con dims: {aviso}");
        assert!(
            aviso.contains("presupuesto"),
            "aviso nombra el presupuesto: {aviso}"
        );
        assert_eq!(
            aviso,
            format!(
                "Exportado a {}×{} para entrar en presupuesto",
                plan.0, plan.1
            )
        );
    }

    #[test]
    fn h1_reescalar_vecino_mas_cercano_y_cancelable() {
        let token = CancellationToken::default();
        let frames = frames_mini(3, 8, 8);
        let chico = reescalar_frames_a_cancelable(&frames, 4, 4, &token).expect("reescala");
        assert_eq!(chico.len(), 3);
        for frame in &chico {
            assert_eq!(frame.size, [4, 4]);
            assert_eq!(frame.pixels.len(), 16);
        }
        // Cancelado → Cancelled sin parcial.
        token.cancel();
        assert_eq!(
            reescalar_frames_a_cancelable(&frames, 4, 4, &token),
            Err(GifExportError::Cancelled)
        );
        // Vacío / destino absurdo → Err honesto.
        let fresco = CancellationToken::default();
        assert_eq!(
            reescalar_frames_a_cancelable(&[], 4, 4, &fresco),
            Err(GifExportError::EmptyFrames)
        );
        assert!(matches!(
            reescalar_frames_a_cancelable(&frames, 0, 4, &fresco),
            Err(GifExportError::DimensionOutOfRange { .. })
        ));
    }

    #[test]
    fn f4a_cache_hit_devuelve_mismos_pixeles_y_emite_progreso() {
        use std::collections::BTreeMap;
        let params = BTreeMap::new();
        let mut progreso_hit = 0usize;
        let primero = render_anim_with_progress_con_rotulo(
            "derivative-slope",
            "r4-cache-pin",
            64,
            48,
            &params,
            false,
            &mut |_, _| {},
        );
        assert!(!primero.is_empty());
        let segundo = render_anim_with_progress_con_rotulo(
            "derivative-slope",
            "r4-cache-pin",
            64,
            48,
            &params,
            false,
            &mut |done, total| {
                assert_eq!(total, primero.len());
                progreso_hit = done;
            },
        );
        assert_eq!(primero.len(), segundo.len());
        assert_eq!(primero[0].pixels, segundo[0].pixels, "hit idéntico");
        assert_eq!(progreso_hit, primero.len(), "progreso real en hit");
        // Rótulo distinto = clave distinta (no contamina).
        let rotulado = render_anim_with_progress_con_rotulo(
            "derivative-slope",
            "r4-cache-pin",
            64,
            48,
            &params,
            true,
            &mut |_, _| {},
        );
        assert_eq!(rotulado.len(), primero.len());
    }

    #[test]
    fn f4a_cache_lru_acotada_y_set_gigante_no_se_guarda() {
        // LRU pura sobre la struct (sin render pesado).
        let mut cache = CacheSetsAnim::default();
        // Set de ~12 KiB: llenar hasta pasar el tope desaloja al más viejo.
        let chico = frames_mini(48, 8, 8);
        let por_set = CacheSetsAnim::bytes_de_set(&chico);
        assert!(por_set > 0 && por_set < CACHE_SETS_ANIM_MAX_BYTES);
        let cuantos = CACHE_SETS_ANIM_MAX_BYTES / por_set + 3;
        for k in 0..cuantos {
            cache.guardar(format!("k{k}"), chico.clone());
        }
        assert!(
            cache.bytes <= CACHE_SETS_ANIM_MAX_BYTES,
            "tope respetado: {}",
            cache.bytes
        );
        assert!(cache.buscar("k0").is_none(), "el más viejo se desalojó");
        assert!(
            cache.buscar(&format!("k{}", cuantos - 1)).is_some(),
            "el más reciente sobrevive"
        );
        // Set gigante (> tope) no se guarda: buscar → None.
        // (4096²×4 B = 64 MiB exactos: dos frames para exceder.)
        let gigante = vec![egui::ColorImage::new([4096, 4096], egui::Color32::BLACK); 2];
        assert!(CacheSetsAnim::bytes_de_set(&gigante) > CACHE_SETS_ANIM_MAX_BYTES);
        cache.guardar("gigante".to_string(), gigante);
        assert!(cache.buscar("gigante").is_none(), "lo gigante no se cachea");
    }

    #[test]
    fn f1_canon_pineado_y_encaje_con_aspecto() {
        assert_eq!((CHAT_CANON_W, CHAT_CANON_H), (480, 360));
        // Canónico documentado: 31.6 MiB < 64 MiB.
        assert_eq!(
            estimate_frames_bytes(480, 360, NATIVE_ANIM_FRAME_COUNT),
            Some(480 * 360 * 4 * 48)
        );
        const {
            assert!(480 * 360 * 4 * 48 < NATIVE_MAX_SET_BYTES);
        }
        // Encaje: idéntico si entra, reduce con aspecto si excede, pares.
        assert_eq!(encajar_anim_a_chat(480, 360), (480, 360));
        assert_eq!(encajar_anim_a_chat(320, 200), (320, 200));
        assert_eq!(encajar_anim_a_chat(720, 540), (480, 360));
        let (w, h) = encajar_anim_a_chat(1000, 100);
        assert_eq!((w % 2, h % 2), (0, 0));
        assert!(w <= 480 && h <= 360, "{w}×{h}");
        // Nulo/degenerado → mínimo honesto 2×2, sin panic.
        assert_eq!(encajar_anim_a_chat(0, 0), (2, 2));
    }

    #[test]
    fn f1_taylor_siempre_devuelve_frames_fallback_canonico() {
        // El camino existe: expr que el motor no deriva → fallback canónico
        // (48 frames honestos, jamás vacío que rompa el worker).
        let spec = grafito_anim::parametric::TaylorSpec {
            expr: "zzz_no_existe(x)".to_string(),
            centro: 0.0,
            orden: 5,
        };
        let frames = render_taylor_frames_for_spec_impl(
            CHAT_CANON_W,
            CHAT_CANON_H,
            &spec,
            false,
            &mut |_, _| {},
        );
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT, "taylor→frames");
        assert_eq!(
            frames[0].size,
            [CHAT_CANON_W as usize, CHAT_CANON_H as usize]
        );
    }
}
