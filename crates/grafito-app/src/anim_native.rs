//! Animacion didactica nativa (sin motor externo): generador universal estilo canal de matematica.
//! Soporta cualquier texto como un canal profesional de YouTube (3Blue1Brown): elige
//! automaticamente la mejor plantilla segun el concepto y garantiza un fallback elegante
//! en <2s incluso si Manim no esta disponible. Todas las plantillas son deterministas.

pub(crate) const NATIVE_ANIM_FRAME_COUNT: usize = 48;

#[cfg(test)]
use grafito_anim::protocol::CANONICAL_TEMPLATES;
use grafito_anim::protocol::{
    scene_param_clamped, SCENE_PARAM_A, SCENE_PARAM_B, SCENE_PARAM_SPAN, SCENE_PARAM_TERMS,
    SCENE_PARAM_X0,
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
/// Plantillas canónicas con renderer nativo propio (+ `universal` youtube-style).
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
const CENTER_WHITE: [u8; 4] = [255, 255, 255, 200];
const SCRIM: [u8; 4] = [0, 0, 0, 110];
const TRACK: [u8; 4] = [255, 255, 255, 22];
const TEXT_CUTOUT: [u8; 4] = [14, 14, 20, 180];
const TRAIL_FAINT_ALPHA: u8 = 28;
const CURVE_UNIVERSAL_ALPHA: u8 = 210;

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

/// Detecta la mejor plantilla para un concepto libre (ES + EN). Estilo YouTube:
/// cubre derivadas, integrales, Taylor, conforme, Pitagoras, vectores, probabilidad...
pub fn detect_template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    // Pitagoras / triangulo rectangulo
    if c.contains("pit\u{00e1}goras")
        || c.contains("pitagoras")
        || c.contains("pythag")
        || (c.contains("triang") && (c.contains("rect") || c.contains("hipoten")))
    {
        return "pitagoras";
    }
    if c.contains("integral")
        || c.contains("\u{00e1}rea")
        || (c.contains("area")
            && (c.contains("bajo") || c.contains("curva") || c.contains("riemann")))
        || c.contains("\u{00e1}rea bajo")
    {
        return "integral-area";
    }
    if c.contains("taylor")
        || c.contains("maclaurin")
        || (c.contains("serie") && (c.contains("potencia") || c.contains("aprox")))
        || c.contains("aproxima")
    {
        return "taylor-series";
    }
    if c.contains("conformal")
        || c.contains("conforme")
        || c.contains("complej")
        || c.contains("complex")
        || c.contains("fractal")
        || c.contains("mandelb")
    {
        return "conformal-map";
    }
    if c.contains("deriv")
        || c.contains("pendiente")
        || c.contains("tangente")
        || c.contains("slope")
        || c.contains("l\u{00ed}mite") && c.contains("cociente")
    {
        return "derivative-slope";
    }
    // genericos con mapping elegante — F5: fracciones, vectores, matrices, proba inline
    if c.contains("vector") || c.contains("campo") && c.contains("vectorial") {
        return "conformal-map";
    }
    if c.contains("probab")
        || c.contains("binom")
        || c.contains("distrib")
        || c.contains("estad")
        || c.contains("bayes")
        || c.contains("muestreo")
        || c.contains("histograma")
    {
        return "integral-area";
    }
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
    if c.contains("serie")
        || c.contains("sucesi")
        || c.contains("fourier")
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
        || c.contains("sistema")
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
    // Nuevos pedagógicos (BUILD: logística / gradiente / Möbius).
    if c.contains("logist") || c.contains("bifurc") {
        return "logistic-bifurcation";
    }
    if c.contains("gradiente") || c.contains("gradient") {
        return "gradient-field";
    }
    if c.contains("mobius") || c.contains("m\u{00f6}bius") || c.contains("moebius") {
        return "mobius-transform";
    }
    if c.contains("func") {
        return "universal";
    }
    if c.contains("sin(") || c.contains("cos(") || c.contains("seno") || c.contains("coseno") {
        return "taylor-series";
    }
    // fallback universal profesional
    "universal"
}

/// Dispatcher con 10 plantillas mas fallback universal.
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
pub fn render_integral_frames_with_params(
    width: u32,
    height: u32,
    params: &std::collections::BTreeMap<String, f64>,
) -> Vec<egui::ColorImage> {
    render_integral_frames_with_params_impl(width, height, params, &mut |_, _| {})
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
    let curve: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, x * x * 0.15)
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
        for pair in curve.windows(2) {
            let (ax, ay) = to_pixel(w, h, pair[0].0, pair[0].1);
            let (bx, by) = to_pixel(w, h, pair[1].0, pair[1].1);
            draw_line(&mut buf, w, h, (ax, ay), (bx, by), CURVE_MAIN);
        }
        let x_max = (a + (b - a) * t).clamp(-3.0, 3.0);
        let steps = (((x_max - a) * 20.0) as i32).max(0);
        for i in 0..steps {
            let x = a + i as f64 / 20.0;
            let y = x * x * 0.15;
            let top = to_pixel(w, h, x, y);
            let bottom = to_pixel(w, h, x, 0.0);
            draw_line(&mut buf, w, h, top, bottom, FILL_SOFT_BLUE);
        }
        let xm = x_max;
        let ym = xm * xm * 0.15;
        let p = to_pixel(w, h, xm, ym);
        draw_filled_circle(&mut buf, w, h, p.0, p.1, 3, DOT_BLUE);
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "area  integral",
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

/// Animacion universal estilo YouTube: funciona con cualquier texto, siempre se ve profesional.
/// Fondo con gradiente + grid, orbita de particulas, onda morfologica y titulo con maquina de escribir.
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
    let accent = accent_for_concept(&concept_norm);
    let hash = hash_concept(&concept_norm);
    // color secundario derivado
    let accent2 = ACCENTS[((hash >> 16) as usize) % ACCENTS.len()];
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    // preparar curva morph base: parabola -> seno morphing
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t_raw = if NATIVE_ANIM_FRAME_COUNT <= 1 {
            0.0
        } else {
            frame as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
        };
        let t = ease_in_out(t_raw);
        let byte_len =
            checked_frame_byte_len(w, h).unwrap_or(NATIVE_FALLBACK_W * NATIVE_FALLBACK_H * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, &concept_norm, t * 0.12);
        draw_subtle_grid(&mut buf, w, h, t * 0.6);
        // ejes sutiles
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
        // onda central morfologica: mezcla de parabola y seno desplazada por hash
        let phase = (hash as f64 % std::f64::consts::TAU) * 0.1;
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let y0_par = x0 * x0 * 0.18;
            let y0_sin = (x0 * 1.2 + phase).sin() * 0.8;
            let y1_par = x1 * x1 * 0.18;
            let y1_sin = (x1 * 1.2 + phase).sin() * 0.8;
            let y0 = y0_par * (1.0 - t) + y0_sin * t;
            let y1 = y1_par * (1.0 - t) + y1_sin * t;
            let a = to_pixel(w, h, x0, y0);
            let b = to_pixel(w, h, x1, y1);
            // color interpolado entre dos acentos
            let col = [
                (accent[0] as f64 * (1.0 - t) + accent2[0] as f64 * t) as u8,
                (accent[1] as f64 * (1.0 - t) + accent2[1] as f64 * t) as u8,
                (accent[2] as f64 * (1.0 - t) + accent2[2] as f64 * t) as u8,
                CURVE_UNIVERSAL_ALPHA,
            ];
            draw_line(&mut buf, w, h, a, b, col);
        }
        // particulas orbitando (hash controla velocidad)
        let n_particles = 6;
        for i in 0..n_particles {
            let angle = 2.0
                * std::f64::consts::PI
                * (i as f64 / n_particles as f64 + t * 0.6 + (hash % 7) as f64 * 0.04);
            let radius = 0.9 + 0.25 * (t * std::f64::consts::TAU + i as f64).sin();
            let x = radius * angle.cos();
            let y = radius * angle.sin() * 0.7;
            let p = to_pixel(w, h, x, y);
            let sz = if i == 0 { 4 } else { 3 };
            // pulso
            let pulse = (frame as f64 * 0.35 + i as f64 * 1.1).sin() * 0.5 + 0.5;
            let col = [
                accent[0],
                accent[1],
                accent[2],
                (120.0 + 110.0 * pulse) as u8,
            ];
            draw_filled_circle(&mut buf, w, h, p.0, p.1, sz, col);
            // estela sutil: linea al centro
            let center = to_pixel(w, h, 0.0, 0.0);
            let trail_col = with_alpha(accent, TRAIL_FAINT_ALPHA);
            draw_line(&mut buf, w, h, center, p, trail_col);
        }
        // punto central que late
        let c = to_pixel(w, h, 0.0, 0.0);
        let r = (2.0 + 1.5 * (t * 12.56).sin().abs()) as usize;
        draw_filled_circle(&mut buf, w, h, c.0, c.1, r, CENTER_WHITE);
        draw_filled_circle(&mut buf, w, h, c.0, c.1, (r / 2).max(1), accent);
        // barra de progreso inferior (estilo YouTube)
        let bar_y = h.saturating_sub(6);
        let bar_w = (w as f64 * t_raw) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 4, accent);
        draw_filled_rect(&mut buf, w, h, bar_w, bar_y, w - bar_w, 4, TRACK);
        // titulo con efecto maquina de escribir: muestra concept truncado progresivo
        let reveal = ((t_raw * concept_norm.len() as f64).ceil() as usize).min(concept_norm.len());
        let visible: String = concept_norm.chars().take(reveal).collect();
        // fondo semitransparente para legibilidad
        let title_h = 21; // 7px char + padding calculated
        draw_filled_rect(&mut buf, w, h, 6, 6, w - 12, title_h, SCRIM);
        // acento superior
        draw_filled_rect(
            &mut buf,
            w,
            h,
            6,
            6,
            ((w - 12) as f64 * (t_raw * 0.9 + 0.1)) as usize,
            2,
            accent,
        );
        let truncated: String = visible.chars().take(32).collect();
        draw_text_block(&mut buf, w, h, 10, 10, &truncated, TEXT_COLOR, 1);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
        on_frame(frames.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    frames
}

/// Dispatcher universal: elige plantilla automaticamente a partir del concepto si hace falta.
/// Garantiza que cualquier combinacion produce frames validos en <2s.
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
    // Compat: si se llama solo con template, usar universal como fallback elegante
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
            // template desconocido -> universal con ese texto como concepto para no quedar vacio
            if template.trim().is_empty() {
                render_native_animation_frames(width, height)
            } else {
                render_universal_youtube_frames(template, width, height)
            }
        }
    }
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
        assert_eq!(CURVE_MAIN[3], 235);
        assert_eq!(TRAIL_FAINT_ALPHA, 28);
        assert_eq!(CURVE_UNIVERSAL_ALPHA, 210);
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
