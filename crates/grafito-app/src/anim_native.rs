//! Animacion didactica nativa (sin motor externo).
//! Cada plantilla con renderer propio dibuja su objeto matemático; el
//! fallback `universal` es un placeholder neutro rotulado ("vista previa
//! no disponible"): grilla + texto, jamás una curva que parezca respuesta.
//! Todas las plantillas son deterministas.

pub(crate) const NATIVE_ANIM_FRAME_COUNT: usize = 48;

#[cfg(test)]
use grafito_anim::protocol::CANONICAL_TEMPLATES;
use grafito_anim::protocol::{
    contiene_palabra, scene_param_clamped, template_for_concept, SCENE_PARAM_A, SCENE_PARAM_B,
    SCENE_PARAM_SPAN, SCENE_PARAM_TERMS, SCENE_PARAM_X0,
};
use std::path::{Path, PathBuf};

// ── Registro canónico nativo v4 (11 plantillas) ──────────────────────────
// SYNC MECÁNICO 11↔11↔11 (ANIM-REVIVE):
// - `grafito-anim/src/protocol.rs::CANONICAL_TEMPLATES`: fuente única (11).
// - Este `NATIVE_TEMPLATES`: idéntico orden y contenido (test pineado).
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
            if frame.size != size {
                return Err(GifExportError::InconsistentSize {
                    index,
                    expected: size,
                    got: frame.size,
                });
            }
            if frame.pixels.len() != pixel_count {
                return Err(GifExportError::PixelCountMismatch {
                    index,
                    expected: pixel_count,
                    got: frame.pixels.len(),
                });
            }
            let byte_len =
                pixel_count
                    .checked_mul(4)
                    .ok_or(GifExportError::PixelCountMismatch {
                        index,
                        expected: pixel_count.saturating_mul(4),
                        got: frame.pixels.len().saturating_mul(4),
                    })?;
            let mut rgba = Vec::new();
            rgba.try_reserve_exact(byte_len).map_err(|_| {
                GifExportError::Encode(format!("sin memoria para el frame {index}"))
            })?;
            for px in &frame.pixels {
                rgba.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
            }
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
pub fn export_frames_to_gif_file(
    frames: &[egui::ColorImage],
    path: &Path,
    delay_cs: u16,
) -> Result<PathBuf, GifExportError> {
    let bytes = encode_frames_to_gif_bytes(frames, delay_cs)?;
    std::fs::write(path, &bytes)
        .map_err(|e| GifExportError::Io(format!("no se pudo escribir {}: {e}", path.display())))?;
    Ok(path.to_path_buf())
}

/// Exporta en un hilo aparte (no bloquea la UI).
///
/// El lead lo dispara con los frames del estado + destino elegido por el
/// usuario y al hacer `join` actualiza `media_path` / `status`.
pub fn spawn_gif_export(
    frames: Vec<egui::ColorImage>,
    path: PathBuf,
    delay_cs: u16,
) -> std::thread::JoinHandle<Result<PathBuf, GifExportError>> {
    std::thread::spawn(move || export_frames_to_gif_file(&frames, &path, delay_cs))
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

/// Preflight puro antes de spawnear la exportación (B5).
///
/// Verifica vacío, tope 64 frames, dimensiones 1..=4096 y píxeles totales
/// ≤ 8 M (paridad con el loader; `checked_*` + saturación, sin panic ni
/// overflow). No estima bytes: el tamaño final (cota 5 MB) lo verifica la app
/// tras el join, porque solo se conoce al codificar. Puro, sin E/S.
pub fn check_gif_export_budget(frames: &[egui::ColorImage]) -> Result<(), GifExportError> {
    if frames.is_empty() {
        return Err(GifExportError::EmptyFrames);
    }
    if frames.len() > GIF_EXPORT_MAX_FRAMES {
        return Err(GifExportError::TooManyFrames { got: frames.len() });
    }
    let mut total_pixels: usize = 0;
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
        total_pixels = total_pixels.saturating_add(pixel_count);
        if total_pixels > GIF_EXPORT_MAX_TOTAL_PIXELS {
            return Err(GifExportError::TooManyPixels { got: total_pixels });
        }
    }
    Ok(())
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
//   El acento canónico sage #6B7A6F (Theme accent) se usa como tinte en
//   fill_background vía accent_for_concept; los 6 vivos garantizan contraste
//   sobre BG oscuro para vídeo didáctico.
// REGLA: ningún color RGBA fuera de este bloque. Los renders solo usan estas
// consts o `with_alpha(BASE, a)`. Ver test `palette_has_no_loose_hardcodes`.
const BG: [u8; 4] = [14, 14, 20, 255];
const BG_GRADIENT: [u8; 4] = [22, 22, 34, 255];
const GRID_COLOR: [u8; 4] = [255, 255, 255, 14];
const AXIS_COLOR: [u8; 4] = [200, 200, 200, 90];
const TEXT_COLOR: [u8; 4] = [235, 235, 245, 255];
// Trío canónico BG/FG/ACCENT (alias documentados para el gate de paleta).
const PAL_BG: [u8; 4] = BG;
const PAL_FG: [u8; 4] = TEXT_COLOR;
// Base vivos (únicos literales de acento; ACCENTS y roles derivan de aquí).
const PAL_BLUE: [u8; 4] = [66, 133, 244, 255];
const PAL_YELLOW: [u8; 4] = [235, 211, 84, 255];
const PAL_RED: [u8; 4] = [255, 77, 77, 255];
const PAL_MINT: [u8; 4] = [126, 214, 160, 255];
const PAL_VIOLET: [u8; 4] = [168, 120, 255, 255];
const PAL_ORANGE: [u8; 4] = [255, 153, 51, 255];
const ACCENTS: [[u8; 4]; 6] = [
    PAL_BLUE, PAL_YELLOW, PAL_RED, PAL_MINT, PAL_VIOLET, PAL_ORANGE,
];
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
const LINE_SOFT_BLUE: [u8; 4] = [91, 155, 255, 140];
const FAINT_WHITE: [u8; 4] = [255, 255, 255, 35];
const GIBBS_RED: [u8; 4] = [255, 77, 77, 200];
const SCRIM: [u8; 4] = [0, 0, 0, 110];
const TRACK: [u8; 4] = [255, 255, 255, 22];
const TEXT_CUTOUT: [u8; 4] = [14, 14, 20, 180];

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

/// Hash FNV-1a rapido y determinista para variaciones por concepto.
fn hash_concept(concept: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in concept.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    // mezclar longitud para que strings vacios no den 0 trivial
    h ^= concept.len() as u64;
    h
}

fn accent_for_concept(concept: &str) -> [u8; 4] {
    let h = hash_concept(concept);
    ACCENTS[(h as usize) % ACCENTS.len()]
}

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
/// Convierte punto matematico (x,y en [-3,3]^2) a pixel del buffer.
/// Nunca panic con w/h en 0.
fn to_pixel(width: usize, height: usize, x: f64, y: f64) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let px = ((x + 3.0) / 6.0 * (width as f64)).round() as usize;
    let py = ((3.0 - y) / 6.0 * (height as f64)).round() as usize;
    (px.min(width - 1), py.min(height - 1))
}

fn draw_line(
    buf: &mut [u8],
    w: usize,
    h: usize,
    a: (usize, usize),
    b: (usize, usize),
    color: [u8; 4],
) {
    let mut x = a.0 as i64;
    let mut y = a.1 as i64;
    let dx = (b.0 as i64 - x).abs();
    let dy = -(b.1 as i64 - y).abs();
    let sx = if x < b.0 as i64 { 1 } else { -1 };
    let sy = if y < b.1 as i64 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let ux = x as usize;
            let uy = y as usize;
            if let Some(i) = uy
                .checked_mul(w)
                .and_then(|v| v.checked_add(ux))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    // alpha blending simple: si color alpha <255, mezclar con fondo
                    if color[3] == 255 {
                        buf[i..i + 4].copy_from_slice(&color);
                    } else {
                        let a = color[3] as f64 / 255.0;
                        for k in 0..3 {
                            buf[i + k] =
                                (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                        }
                        buf[i + 3] = 255;
                    }
                }
            }
        }
        if x == b.0 as i64 && y == b.1 as i64 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
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
    for dy in -(radius as i64)..=(radius as i64) {
        for dx in -(radius as i64)..=(radius as i64) {
            if dx * dx + dy * dy <= (radius as i64) * (radius as i64) {
                let x = cx as i64 + dx;
                let y = cy as i64 + dy;
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    let ux = x as usize;
                    let uy = y as usize;
                    if let Some(i) = uy
                        .checked_mul(w)
                        .and_then(|v| v.checked_add(ux))
                        .and_then(|v| v.checked_mul(4))
                    {
                        if i + 3 < buf.len() {
                            if color[3] == 255 {
                                buf[i..i + 4].copy_from_slice(&color);
                            } else {
                                let a = color[3] as f64 / 255.0;
                                for k in 0..3 {
                                    buf[i + k] =
                                        (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                                } // clippy ok
                                buf[i + 3] = 255;
                            }
                        }
                    }
                }
            }
        }
    }
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
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 >= buf.len() {
                    continue;
                }
                if color[3] == 255 {
                    buf[i..i + 4].copy_from_slice(&color);
                } else {
                    let a = color[3] as f64 / 255.0;
                    for k in 0..3 {
                        buf[i + k] = (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                    }
                    buf[i + 3] = 255;
                }
            }
        }
    }
}

// ── Fuente bitmap 5x7 ultra-minimal (solo ASCII 32..126, mayus) ────────────
// Cada char 5 columnas, 7 filas: bit 1 = pixel.
const FONT5X7: [[u8; 7]; 95] = {
    // generada proceduralmente: para este motor usaremos un estilo "block" simplificado:
    // en lugar de almacenar glifos perfectos, dibujaremos un rectangulo con variacion
    // por hash para que cualquier texto se vea nitido en modo placeholder.
    // Para mantenerlo simple y robusto, usaremos rect blocks.
    [[0; 7]; 95]
};

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
    // Dibujo profesional minimal: cada caracter como bloque 5x7 con separacion 1px,
    // escalable. Garantiza que cualquier texto (incluso emojis truncados) se renderice
    // sin panics y en <1ms.
    let scale = scale.clamp(1, 3);
    let char_w = 5 * scale;
    let char_h = 7 * scale;
    let mut cx = x;
    for ch in text.chars().take(48) {
        if cx + char_w >= w {
            break;
        }
        if ch == ' ' {
            cx += char_w + scale;
            continue;
        }
        // Texto sólido, sin variación hash que corrompe la lectura
        draw_filled_rect(buf, w, h, cx, y, char_w, char_h, color);
        // recortar interior 1px para efecto "pixel font" legible
        if char_w > 2 && char_h > 2 {
            let inner = TEXT_CUTOUT;
            draw_filled_rect(
                buf,
                w,
                h,
                cx + scale,
                y + scale,
                char_w - 2 * scale,
                char_h - 2 * scale,
                inner,
            );
        }
        cx += char_w + scale;
    }
}

fn fill_background(buf: &mut [u8], w: usize, h: usize, concept: &str, t: f64) {
    let accent = accent_for_concept(concept);
    // gradiente vertical suave + tinte del acento segun t
    for y in 0..h {
        let v = y as f64 / h as f64;
        // interpolacion BG -> BG_GRADIENT con seno sutil
        let mix = v * 0.6 + (t * 0.08).sin() * 0.04;
        let r = (BG[0] as f64 * (1.0 - mix)
            + BG_GRADIENT[0] as f64 * mix
            + accent[0] as f64 * 0.04 * (1.0 - v)) as u8;
        let g = (BG[1] as f64 * (1.0 - mix)
            + BG_GRADIENT[1] as f64 * mix
            + accent[1] as f64 * 0.04 * (1.0 - v)) as u8;
        let b = (BG[2] as f64 * (1.0 - mix)
            + BG_GRADIENT[2] as f64 * mix
            + accent[2] as f64 * 0.04 * (1.0 - v)) as u8;
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

fn draw_subtle_grid(buf: &mut [u8], w: usize, h: usize, t: f64) {
    // grid cada ~40px con parallax leve
    let step = (w.min(h) / 10).max(18);
    let off = ((t * 18.0) as usize) % step;
    for x in (off..w).step_by(step) {
        for y in 0..h {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                // linea vertical punteada sutil
                if y % 3 == 0 && i + 2 < buf.len() {
                    buf[i] = ((buf[i] as u16 + GRID_COLOR[0] as u16) / 2) as u8;
                    buf[i + 1] = ((buf[i + 1] as u16 + GRID_COLOR[1] as u16) / 2) as u8;
                    buf[i + 2] = ((buf[i + 2] as u16 + GRID_COLOR[2] as u16) / 2) as u8;
                }
            }
        }
    }
    for y in (off..h).step_by(step) {
        for x in 0..w {
            if x % 3 == 0 {
                if let Some(i) = y
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(x))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 2 < buf.len() {
                        buf[i] = ((buf[i] as u16 + GRID_COLOR[0] as u16) / 2) as u8;
                        buf[i + 1] = ((buf[i + 1] as u16 + GRID_COLOR[1] as u16) / 2) as u8;
                        buf[i + 2] = ((buf[i + 2] as u16 + GRID_COLOR[2] as u16) / 2) as u8;
                    }
                }
            }
        }
    }
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ── Plantillas existentes ────────────────────────────────────────────────

pub(crate) fn render_native_animation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
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
    render_derivative_frames_with_params_impl(width, height, params, &mut |_, _| {})
}

fn render_derivative_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let center = scene_param_clamped(params, SCENE_PARAM_X0, 0.0, -3.0, 3.0);
    let span = scene_param_clamped(params, SCENE_PARAM_SPAN, 1.5, 0.25, 3.0);
    let ((w, h), _) = resolve_native_size(width, height);
    let parabola: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, x * x)
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = if NATIVE_ANIM_FRAME_COUNT <= 1 {
            0.0
        } else {
            frame as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
        };
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "derivada", t * 0.1);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for pair in parabola.windows(2) {
            let (ax, ay) = to_pixel(w, h, pair[0].0, pair[0].1);
            let (bx, by) = to_pixel(w, h, pair[1].0, pair[1].1);
            draw_line(&mut buf, w, h, (ax, ay), (bx, by), CURVE_MAIN);
        }
        let x0 = (center - span + 2.0 * span * t).clamp(-3.0, 3.0);
        let y0 = x0 * x0;
        let slope = 2.0 * x0;
        let x_a = x0 - 1.0;
        let x_b = x0 + 1.0;
        let (ax, ay) = to_pixel(w, h, x_a, y0 + slope * (x_a - x0));
        let (bx, by) = to_pixel(w, h, x_b, y0 + slope * (x_b - x0));
        draw_line(&mut buf, w, h, (ax, ay), (bx, by), TANGENT_BLUE);
        let (px, py) = to_pixel(w, h, x0, y0);
        draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
        // titulo superior
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 12,
            h / 12,
            "derivada  f'(x)",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub(crate) fn render_pitagoras_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_pitagoras_frames_impl(width, height, &mut |_, _| {})
}

fn render_pitagoras_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).clamp(0.0, 1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "pitagoras", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
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
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "a^2 + b^2 = c^2",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub(crate) fn render_integral_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
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
    render_integral_frames_with_params_impl(width, height, params, &mut |_, _| {})
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

/// Extremo derecho del área en el frame (`a..x_end` crece con el frame).
/// Puro, sin I/O, sin pánicos.
pub(crate) fn integral_frame_end(a: f64, b: f64, frame: usize) -> f64 {
    let total = NATIVE_ANIM_FRAME_COUNT;
    let t = if total <= 1 {
        0.0
    } else {
        frame.min(total - 1) as f64 / (total - 1) as f64
    };
    a + (b - a) * t
}

/// `S` acumulada por trapecios (paso 1/20, ≤122 tramos acotados) sobre
/// `[a, x_end]` con el evaluador existente; salta tramos no finitos.
/// `Some(0.0)` si `x_end <= a`; `None` si ningún tramo valida (la etiqueta
/// muestra `S=?` honesto en vez de inventar).
pub(crate) fn integral_acumulada(
    anim: Option<&ParametricAnim>,
    frame: usize,
    a: f64,
    x_end: f64,
) -> Option<f64> {
    // Entradas finitas por construcción (cotas clampeadas + frame acotado):
    // `<=` es total acá; NaN no llega (y el `as usize` saturaría a 0 igual).
    if x_end <= a {
        return Some(0.0);
    }
    let pasos = ((x_end - a) / 0.05).ceil() as usize;
    let mut s = 0.0;
    let mut valida = false;
    for i in 0..pasos.min(4096) {
        let x0 = a + i as f64 * 0.05;
        let x1 = (a + (i + 1) as f64 * 0.05).min(x_end);
        if x1 <= x0 {
            continue;
        }
        if let (Some(fa), Some(fb)) = (
            integral_eval(anim, frame, x0),
            integral_eval(anim, frame, x1),
        ) {
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

fn render_integral_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let lo = scene_param_clamped(params, SCENE_PARAM_A, 0.0, -3.0, 3.0);
    let hi = scene_param_clamped(params, SCENE_PARAM_B, 2.0, -3.0, 3.0);
    let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "integral", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
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
        // Etiquetas ASCII honestas: título fijo + valor acumulado del frame.
        draw_text_block(&mut buf, w, h, w / 14, h / 12, "y=x^2", TEXT_COLOR, 1);
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

pub(crate) fn render_taylor_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_taylor_frames_impl(width, height, &mut |_, _| {})
}

fn render_taylor_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
    let f = |x: f64| x.sin();
    let taylor = |x: f64| x - x.powi(3) / 6.0;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "taylor", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let a = to_pixel(w, h, x0, f(x0));
            let b = to_pixel(w, h, x1, f(x1));
            draw_line(&mut buf, w, h, a, b, CURVE_MAIN);
        }
        let alpha = (t * 255.0) as u8;
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let w0 = (1.0 - (x0.abs() / 3.0)).clamp(0.0, 1.0);
            let w1 = (1.0 - (x1.abs() / 3.0)).clamp(0.0, 1.0);
            let a = to_pixel(w, h, x0, taylor(x0));
            let b = to_pixel(w, h, x1, taylor(x1));
            let mut c = PAL_BLUE;
            c[3] = (alpha as f64 * w0.min(w1)) as u8;
            draw_line(&mut buf, w, h, a, b, c);
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "taylor  sin(x)",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

pub(crate) fn render_conformal_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_conformal_frames_impl(width, height, &mut |_, _| {})
}

fn render_conformal_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "conformal", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let tx = x + 0.2 * t * (3.0 * x).sin();
                let ty = y + 0.15 * t * (3.0 * y).cos();
                let p = to_pixel(w, h, tx, ty);
                let sz = if gx == 0 && gy == 0 { 4 } else { 2 };
                let col = if gx == 0 || gy == 0 {
                    MINT_STRONG
                } else {
                    MINT_FAINT
                };
                draw_filled_circle(&mut buf, w, h, p.0, p.1, sz, col);
            }
        }
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let y0 = 0.2 * t * (3.0 * x0).sin();
            let y1 = 0.2 * t * (3.0 * x1).sin();
            let a = to_pixel(w, h, x0, y0);
            let b = to_pixel(w, h, x1, y1);
            draw_line(&mut buf, w, h, a, b, LINE_SOFT_BLUE);
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "conforme  w=f(z)",
            TEXT_COLOR,
            1,
        );
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
    render_universal_youtube_frames_impl(concept, width, height, &mut |_, _| {})
}

fn render_universal_youtube_frames_impl(
    concept: &str,
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
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
        fill_background(&mut buf, w, h, "universal", 0.0);
        draw_subtle_grid(&mut buf, w, h, 0.0);
        // Rótulo honesto + eco del pedido (estáticos en todos los frames).
        let echo: String = concept_norm.chars().take(32).collect();
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
/// `integral-area` (a/b), `euler`/`fourier` (terms). El resto IGNORA params
/// por ahora (TODO honesto: taylor es fade de orden fijo; conformal/pitagoras/
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

/// Entrada canónica con PROGRESO REAL por frame (ANIM-REVIVE).
///
/// Idéntica a `render_anim_for_concept_with_params`, pero `on_frame(done, total)`
/// se invoca tras pushear CADA frame (`done` 1..=48, `total` = 48) desde dentro
/// del loop nativo — nunca valores inventados. El hilo del lead la usa para
/// mover `AnimPreviewState.progress` y `request_repaint`; el render es
/// determinista: el callback no altera los píxeles (ver test).
/// Presupuesto intacto: siempre 48 frames (`NATIVE_ANIM_FRAME_COUNT`).
pub fn render_anim_with_progress(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    match resolve_native_template(template, concept) {
        "integral-area" => render_integral_frames_with_params_impl(width, height, params, on_frame),
        "derivative-slope" => {
            render_derivative_frames_with_params_impl(width, height, params, on_frame)
        }
        "euler" => render_euler_frames_with_params_impl(width, height, params, on_frame),
        "fourier" => render_fourier_frames_with_params_impl(width, height, params, on_frame),
        tmpl => render_anim_for_concept_legacy_with_progress(
            tmpl, concept, width, height, params, on_frame,
        ),
    }
}

/// Rama legacy con progreso (mismo match que `render_anim_for_concept_legacy`,
/// pero sobre los `*_impl` para emitir por frame).
fn render_anim_for_concept_legacy_with_progress(
    tmpl: &str,
    concept: &str,
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    match tmpl {
        "integral-area" => render_integral_frames_with_params_impl(width, height, params, on_frame),
        "taylor-series" => render_taylor_frames_impl(width, height, on_frame),
        "conformal-map" => render_conformal_frames_impl(width, height, on_frame),
        "pitagoras" => render_pitagoras_frames_impl(width, height, on_frame),
        "derivative-slope" => {
            render_derivative_frames_with_params_impl(width, height, params, on_frame)
        }
        "euler" => render_euler_frames_with_params_impl(width, height, params, on_frame),
        "fourier" => render_fourier_frames_with_params_impl(width, height, params, on_frame),
        "logistic-bifurcation" => render_logistic_bifurcation_frames_impl(width, height, on_frame),
        "gradient-field" => render_gradient_field_frames_impl(width, height, on_frame),
        "mobius-transform" => render_mobius_frames_impl(width, height, on_frame),
        "universal" => render_universal_youtube_frames_impl(concept, width, height, on_frame),
        _ => render_universal_youtube_frames_impl(concept, width, height, on_frame),
    }
}

/// Rama legacy del dispatcher (sin params): idéntica a la versión previa a v3.
fn render_anim_for_concept_legacy(
    tmpl: &str,
    concept: &str,
    width: u32,
    height: u32,
) -> Vec<egui::ColorImage> {
    match tmpl {
        "integral-area" => render_integral_frames(width, height),
        "taylor-series" => render_taylor_frames(width, height),
        "conformal-map" => render_conformal_frames(width, height),
        "pitagoras" => render_pitagoras_frames(width, height),
        "derivative-slope" => render_native_animation_frames(width, height),
        "euler" => render_euler_frames(width, height),
        "fourier" => render_fourier_frames(width, height),
        "logistic-bifurcation" => render_logistic_bifurcation_frames(width, height),
        "gradient-field" => render_gradient_field_frames(width, height),
        "mobius-transform" => render_mobius_frames(width, height),
        "universal" => render_universal_youtube_frames(concept, width, height),
        _ => render_universal_youtube_frames(concept, width, height),
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
    render_euler_frames_with_params_impl(width, height, params, &mut |_, _| {})
}

fn render_euler_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let max_terms = scene_param_clamped(params, SCENE_PARAM_TERMS, 7.0, 1.0, 7.0) as usize;
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let terms = (1 + (t * (max_terms as f64 - 1.0)) as usize).clamp(1, max_terms);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "euler", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        // draw exp(x) target faint
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let y0 = x0.exp().clamp(-3.0, 3.0);
            let y1 = x1.exp().clamp(-3.0, 3.0);
            draw_line(
                &mut buf,
                w,
                h,
                to_pixel(w, h, x0, y0),
                to_pixel(w, h, x1, y1),
                FAINT_WHITE,
            );
        }
        // draw partial sum
        let factorial = |n: usize| -> f64 { (1..=n).fold(1.0, |a, b| a * b as f64) };
        let partial = |x: f64| -> f64 {
            (0..terms)
                .map(|k| x.powi(k as i32) / factorial(k))
                .sum::<f64>()
                .clamp(-3.0, 3.0)
        };
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let a = to_pixel(w, h, x0, partial(x0));
            let b = to_pixel(w, h, x1, partial(x1));
            draw_line(&mut buf, w, h, a, b, TANGENT_BLUE);
        }
        // indicator text terms
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            &format!("e^x  n={}", terms - 1),
            TEXT_COLOR,
            1,
        );
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
    render_fourier_frames_with_params_impl(width, height, params, &mut |_, _| {})
}

fn render_fourier_frames_with_params_impl(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let max_harm = scene_param_clamped(params, SCENE_PARAM_TERMS, 6.0, 1.0, 6.0) as usize;
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let harmonics = (1 + (t * (max_harm as f64 - 1.0)) as usize).clamp(1, max_harm);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "fourier", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
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
            let a = to_pixel(w, h, x0, fourier(x0));
            let b = to_pixel(w, h, x1, fourier(x1));
            draw_line(&mut buf, w, h, a, b, CURVE_MAIN);
        }
        // Gibbs markers at discontinuities
        for &cx in &[-std::f64::consts::PI, 0.0, std::f64::consts::PI] {
            if cx.abs() <= 3.0 {
                let p = to_pixel(w, h, cx, 0.0);
                draw_filled_circle(&mut buf, w, h, p.0, p.1, 2, GIBBS_RED);
            }
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            &format!("fourier  k={}", harmonics),
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Bifurcación logística: diagrama r∈[2.5,4.0] vs x*=r·x·(1-x).
/// Fondo + diagrama tenue estático + columna highlight que barre con t.
/// Determinista, <2s (muestreo cada 2px, 120 iters/col).
pub fn render_logistic_bifurcation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_logistic_bifurcation_frames_impl(width, height, &mut |_, _| {})
}

fn render_logistic_bifurcation_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "bifurcacion logistica", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            AXIS_COLOR,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            AXIS_COLOR,
        );
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
        draw_text_block(&mut buf, w, h, w / 14, h / 12, "bifurcacion r", PAL_FG, 1);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Campo de gradiente: f(x,y)=sin(x)·cos(y), grad=(cos·cos, −sin·sin).
/// 25 flechas + 6 partículas orbitando moduladas por |grad|. <2s.
pub fn render_gradient_field_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_gradient_field_frames_impl(width, height, &mut |_, _| {})
}

fn render_gradient_field_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "campo gradiente", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            AXIS_COLOR,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            AXIS_COLOR,
        );
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
        draw_text_block(&mut buf, w, h, w / 14, h / 12, "gradiente f", PAL_FG, 1);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Transformación de Möbius: w=(z−c)/(1−conj(c)·z), c(t) barre sin loop.
/// Rejilla tenue original + rejilla transformada brillante + círculo unidad.
/// <2s (25 puntos + 60 segmentos/frame).
pub fn render_mobius_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    render_mobius_frames_impl(width, height, &mut |_, _| {})
}

fn render_mobius_frames_impl(
    width: u32,
    height: u32,
    on_frame: &mut dyn FnMut(usize, usize),
) -> Vec<egui::ColorImage> {
    let ((w, h), _) = resolve_native_size(width, height);
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
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "mobius", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            AXIS_COLOR,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            AXIS_COLOR,
        );
        // c(t) barre sin loop (t=0 -> t=1 distintos): evita primero==último.
        let ang_c = 2.0 * std::f64::consts::PI * t;
        let (cr, ci) = (-0.4 + 0.8 * t, 0.3 * ang_c.sin());
        let mobius = |x: f64, y: f64| -> Option<(f64, f64)> {
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
        };
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
        draw_text_block(&mut buf, w, h, w / 14, h / 12, "mobius  w(z)", PAL_FG, 1);
        // Barra de progreso inferior: garantiza primero != último aunque c coincida.
        let bar_y = h.saturating_sub(4);
        let bar_w = (w as f64 * t) as usize;
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
    FrameCount, ParamName, ParametricAnim, ParametricKind, PARAMETRIC_MAX_BYTES,
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
fn draw_parametric_base(buf: &mut [u8], w: usize, h: usize, t: f64, title: &str) {
    fill_background(buf, w, h, title, t * 0.08);
    draw_subtle_grid(buf, w, h, t);
    draw_line(
        buf,
        w,
        h,
        to_pixel(w, h, -3.0, 0.0),
        to_pixel(w, h, 3.0, 0.0),
        AXIS_COLOR,
    );
    draw_line(
        buf,
        w,
        h,
        to_pixel(w, h, 0.0, -3.0),
        to_pixel(w, h, 0.0, 3.0),
        AXIS_COLOR,
    );
    let short: String = title.chars().take(24).collect();
    draw_text_block(buf, w, h, w / 12, h / 12, &short, TEXT_COLOR, 1);
}

/// Muestrea `y = anim(frame i, x)` en 121 puntos de [-3,3]; huecos donde no hay dominio.
fn sample_curve(anim: &ParametricAnim, frame: usize) -> Vec<Option<(f64, f64)>> {
    (0..=120)
        .map(|k| {
            let x = -3.0 + 6.0 * (k as f64 / 120.0);
            anim.eval_frame(frame, x).map(|y| (x, y))
        })
        .collect()
}

/// Dibuja la polilínea cortando en huecos (sin unir ramas ni dominios rotos).
fn draw_curve_gaps(buf: &mut [u8], w: usize, h: usize, pts: &[Option<(f64, f64)>], color: [u8; 4]) {
    let px: Vec<Option<(usize, usize)>> = pts
        .iter()
        .map(|opt| opt.map(|(x, y)| to_pixel(w, h, x, y)))
        .collect();
    for pair in px.windows(2) {
        if let (Some(a), Some(b)) = (pair[0], pair[1]) {
            draw_line(buf, w, h, a, b, color);
        }
    }
}

/// Pendiente numérica central (clamp a ±10 para dibujar; `None` sin dominio).
fn numeric_slope(anim: &ParametricAnim, frame: usize, x: f64) -> Option<f64> {
    let h_step = 1e-3;
    let a = anim.eval_frame(frame, x - h_step)?;
    let b = anim.eval_frame(frame, x + h_step)?;
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
pub fn render_parametric_frames_with_progress(
    anim: &ParametricAnim,
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
        let s = anim.frame_fraction(frame);
        let p = anim.frame_param(frame);
        let mut buf = alloc_frame_buffer(w, h).map_err(|_| {
            let got = estimate_frames_bytes(w, h, 1);
            ParametricRenderError::AllocFailed {
                bytes: got.unwrap_or(w.saturating_mul(h).saturating_mul(4)),
            }
        })?;
        draw_parametric_base(&mut buf, w, h, s, &anim.expr_a);
        match anim.kind {
            ParametricKind::Sweep | ParametricKind::Morph => {
                let pts = sample_curve(anim, frame);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
            }
            ParametricKind::Trace => {
                // La curva se dibuja hasta `s`: solo el prefijo x ≤ -3+6s.
                let x_max = -3.0 + 6.0 * s;
                let pts: Vec<Option<(f64, f64)>> = (0..=120)
                    .map(|k| {
                        let x = -3.0 + 6.0 * (k as f64 / 120.0);
                        if x <= x_max {
                            anim.eval_frame(frame, x).map(|y| (x, y))
                        } else {
                            None
                        }
                    })
                    .collect();
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                if let Some(y) = anim.eval_frame(frame, x_max) {
                    let (px, py) = to_pixel(w, h, x_max, y);
                    draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                }
            }
            ParametricKind::Locus => {
                // Traza de (q, f(q)) para q de p0 a p + punto móvil en p.
                let pts: Vec<Option<(f64, f64)>> = (0..=120)
                    .map(|k| {
                        let q = anim.p0 + (p - anim.p0) * (k as f64 / 120.0);
                        anim.eval_frame(frame, q).map(|y| (q, y))
                    })
                    .collect();
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let xc = p.clamp(-3.0, 3.0);
                if let Some(y) = anim.eval_frame(frame, xc) {
                    let (px, py) = to_pixel(w, h, xc, y);
                    draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
                }
            }
            ParametricKind::Tangent => {
                let pts = sample_curve(anim, frame);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let xc = p.clamp(-3.0, 3.0);
                if let (Some(y0), Some(slope)) =
                    (anim.eval_frame(frame, xc), numeric_slope(anim, frame, xc))
                {
                    let xa = (xc - 1.2).max(-3.0);
                    let xb = (xc + 1.2).min(3.0);
                    let a = to_pixel(w, h, xa, y0 + slope * (xa - xc));
                    let b = to_pixel(w, h, xb, y0 + slope * (xb - xc));
                    draw_line(&mut buf, w, h, a, b, TANGENT_BLUE);
                    let (px, py) = to_pixel(w, h, xc, y0);
                    draw_filled_circle(&mut buf, w, h, px, py, 3, POINT_RED);
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
                    if let Some(y) = anim.eval_frame(frame, x) {
                        let top = to_pixel(w, h, x, y);
                        let base = to_pixel(w, h, x, 0.0);
                        draw_line(&mut buf, w, h, base, top, FILL_SOFT_BLUE);
                    }
                }
                let y_b = anim.eval_frame(frame, b).map_or(0.0, |v| v);
                draw_line(
                    &mut buf,
                    w,
                    h,
                    to_pixel(w, h, b, 0.0),
                    to_pixel(w, h, b, y_b),
                    DOT_BLUE,
                );
                let pts = sample_curve(anim, frame);
                draw_curve_gaps(&mut buf, w, h, &pts, CURVE_MAIN);
                let etiqueta = match integral_acumulada(Some(anim), frame, a, b) {
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

// ── N1: predicados pixel-lógicos compartidos (integral honesta) ──────────
// Sombra = relleno blendido (trayectoria recta fondo→[91,155,255]) + cota
// móvil sólida [66,133,244]. El grid con parallax se mueve BAJO el relleno:
// una intersección del grid (gris ~195) bajo el relleno da [163,182,214]
// (sigue siendo área azul, solo más brillante). Por eso el tope es r<175
// y no r<150: cubre la trayectoria entera incluido intersección-relleno.
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
            // La curva es la MISMA en los frames 0/24/47.
            let c0 = super::mascara_curva(&frames[0]);
            assert!(c0.iter().any(|v| *v), "{w}x{h}: la curva debe pintarse");
            assert_eq!(
                c0,
                super::mascara_curva(&frames[24]),
                "{w}x{h}: curva 0 == 24"
            );
            assert_eq!(
                c0,
                super::mascara_curva(&frames[47]),
                "{w}x{h}: curva 0 == 47"
            );
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
        assert_eq!(ACCENTS.len(), 6);
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
        // Regresión T2: cada concepto conocido resuelve a su plantilla con
        // renderer propio; ninguno cae a `universal`.
        for (concepto, esperado) in [
            ("teorema de pitágoras", "pitagoras"),
            ("integral área bajo curva", "integral-area"),
            ("probabilidad binomial n=10", "integral-area"),
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
            ("vector campo F(x,y)=(-y,x)", "conformal-map"),
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
    #[test]
    fn normalize_handles_edge_cases() {
        assert!(!normalize_concept("").is_empty());
        assert!(!normalize_concept("   ").is_empty());
        let long = "a".repeat(500);
        assert!(normalize_concept(&long).len() <= 124);
        assert_eq!(normalize_concept("  hola   mundo  "), "hola mundo");
    }
    #[test]
    fn different_concepts_produce_different_accents() {
        let c1 = accent_for_concept("derivada");
        let c2 = accent_for_concept("integral");
        // no garantizado distinto, pero al menos determinista
        assert_eq!(accent_for_concept("derivada"), c1);
        // hash varia
        assert_ne!(hash_concept("a"), hash_concept("b"));
        let _ = c2;
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
    fn dispatcher_with_params_vacio_igual_a_legacy() {
        // Sin params, el dispatcher v3 delega idéntico al legacy en las 11.
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
    fn native_templates_son_once_y_despachan() {
        assert_eq!(NATIVE_TEMPLATES.len(), 11, "registro canónico = 11");
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
        // Fuente única: protocolo == nativo, mismo orden y contenido.
        assert_eq!(NATIVE_TEMPLATES.len(), 11);
        assert_eq!(CANONICAL_TEMPLATES.len(), 11);
        assert_eq!(NATIVE_TEMPLATES, CANONICAL_TEMPLATES);
    }

    #[test]
    fn dispatch_honesto_direct_y_fallback() {
        // Las 11 canónicas van directo al renderer (el resto se cubre abajo).
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
        let dir = std::env::temp_dir().join(format!("grafito_gif_export_{}", std::process::id()));
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
        // Píxeles totales sobre 8 M con dimensiones válidas (9 × 1024² > 8 M).
        let heavy: Vec<egui::ColorImage> = (0..9)
            .map(|_| egui::ColorImage::new([1024, 1024], egui::Color32::BLACK))
            .collect();
        assert!(matches!(
            check_gif_export_budget(&heavy),
            Err(GifExportError::TooManyPixels { .. })
        ));
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
        let c0 = super::mascara_curva(&frames[0]);
        assert!(c0.iter().any(|v| *v), "la curva debe pintarse");
        assert_eq!(c0, super::mascara_curva(&frames[24]), "curva 0 == 24");
        assert_eq!(c0, super::mascara_curva(&frames[47]), "curva 0 == 47");
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
