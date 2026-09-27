//! Superficies 3D animadas (nivel 3Blue1Brown, espejo de `ThreeDScene`).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! nuevas (solo `std` + `crate`). La geometría vive en
//! [`crate::scene::surfaces3d`] (`Surface3D`/`Curva3D`/`Ejes3D`), la cámara en
//! [`crate::scene`] (`AmbientOrbit`/`ZoomAnim`/`Camera`), el easing en
//! [`crate::anims`] y los títulos en [`crate::textanim`]: acá se ANIMAN.
//!
//! Escenas (ver [`TEMPLATE_IDS`]):
//!
//! - `sup-paraboloide-tangente`: paraboloide elíptico (`modo = 0`,
//!   `z = a·x² + b·y²`) o hiperbólico (`modo = 1`, `z = a·x² − b·y²`) con el
//!   punto de tangencia que da una vuelta y su plano tangente móvil
//!   (`z = z₀ + 2a·x₀·(x−x₀) ± 2b·y₀·(y−y₀)`, el diferencial honesto).
//! - `sup-toro-rotante`: toro que cabecea (`R_y(θ)`, visible a propósito: el
//!   giro sobre `z` sería simétrico e invisible) `vueltas` veces por clip.
//! - `sup-campo-vectorial`: campo 3D sobre un paraboloide (modo 0 = normales
//!   `[-2ax, -2ay, 1]`, modo 1 = rotacional `[-y, x, 0.3]`), como el vector
//!   sobre la esfera del hilo clásico de `r/manim`.
//! - `sup-interseccion`: esfera de radio `R` + plano `z = c` con su curva de
//!   intersección (`r = √(R²−c²)`, círculo honesto) y zoom `fov` animado.
//! - `sup-onda-3d`: onda radial `z = A·sin(k·r − w·t)` con `r = √(x²+y²)` que
//!   respira un período temporal por clip, más su perfil sobre el eje `x`.
//! - `sup-silla-descenso`: silla `z = x² − y²` con el sendero de descenso por
//!   gradiente (`p −= paso·(2x, −2y)`) revelado con `alpha`.
//!
//! ## Reúso (dispatcher + `scene.rs` / `anims.rs` / `textanim.rs` / `protocol.rs`)
//!
//! - El render productivo vive en `grafito-app/src/anim_native.rs`; el
//!   cableado (`lib.rs`, allowlist de `protocol.rs::CANONICAL_TEMPLATES`)
//!   lo hace la coordinación: este módulo solo expone geometría + samplers
//!   y [`TEMPLATE_IDS`] en kebab-case.
//! - Cámara: [`camara_para`] combina [`AmbientOrbit`] (giro) con el `fov` de
//!   [`ZoomAnim`] (easing propio); [`orbita_por_defecto`] da una vuelta por
//!   clip topada a 1 vuelta/s (más es mareo).
//! - Morph: `alpha` crudo `frame/(total−1)` eased con [`smooth`] de
//!   `anims.rs` ([`linear`] para el giro del toro, velocidad constante).
//! - Reveal 2D: [`a_poligonos`] proyecta las [`Curva3D`] con
//!   [`Curva3D::proyecta`] y normaliza por el [`Scratch`] del llamador.
//! - Títulos con `textanim::Titulo` ([`titulo_para`]); params vivos con
//!   `protocol::scene_param` ([`Params3D`]).
//!
//! ## Presupuestos (acotados, todo lo que excede es `Err` honesto)
//!
//! | Recurso | Tope | Paridad |
//! |---|---|---|
//! | Res de malla por lado | 2..=128 | `SURF3D_MIN/MAX_RES` |
//! | Frames por pedido | 1..=1500 ([`valida_frames`]) | `VIDEO_LONGFORM_MAX_FRAMES` |
//! | Chunk en RAM | 64 MiB ([`chunk_frames`]) | `LONGFORM_CHUNK_MAX_BYTES` |
//! | Canvas | 64..=4096 por lado ([`valida_pedido`]) | `Resolution` |
//! | Duración | 1..=60000 ms | `MAX_TRACK_DURATION_MS` |
//! | Vectores del campo | 1..=16 por lado (≤256 flechas) | `MAX_FIELD_DIVISIONS` (holgura) |
//! | Pasos del descenso | 2..=256 | `CURVA3D_MAX_PUNTOS` (holgura) |
//!
//! El frente compone por chunks ([`chunk_frames`]) y jamás arma el `Vec`
//! total en long-form (contrato P0.1 del protocolo).
//!
//! ## Fuentes (verificadas por búsqueda web en esta sesión)
//!
//! - `Surface` como `func(u, v) -> [x, y, z]` con checkerboard:
//!   docs Manim Community v0.21.0
//!   (<https://docs.manim.community/en/stable/reference/manim.mobject.three_d.three_dimensions.Surface.html>).
//! - `ThreeDScene` + ejes 3D + orientación por `theta`/`phi`/`gamma`:
//!   "Create 3D Earth animation using Manim" (Medium,
//!   <https://medium.com/@kamol.roy08/create-3d-earth-animation-using-manim-892245675f62>)
//!   y "Plotting and 3D Scenes" (slama.dev,
//!   <https://slama.dev/manim/plotting-and-3d-scenes>).
//! - Superficie paramétrica `(u, v)` con `z` como altura y fuente de luz en
//!   `(-10, 10, 10)`: "Visualize 3D Space & Animate Space Time Curvature
//!   with Parametric Surface" (Master Manim 09,
//!   <https://www.youtube.com/watch?v=pUC5a6XNEn4>).
//! - Vector sobre la superficie de una esfera (caso del campo 3D):
//!   hilo "rendering parametric surface" (`r/manim`,
//!   <https://www.reddit.com/r/manim/comments/ey89o3/rendering_parametric_surface>).
//!
//! Sin `unwrap`/`expect` en producción (`unwrap_used = deny`).

use crate::anims::{linear, smooth, Scratch};
use crate::protocol::{
    max_chunk_frames, scene_param, Resolution, LONGFORM_CHUNK_MAX_BYTES, VIDEO_LONGFORM_MAX_FRAMES,
};
use crate::scene::surfaces3d::{
    Curva3D, Ejes3D, Surface3D, CURVA3D_MAX_PUNTOS, SURF3D_MAX_COORD, SURF3D_MAX_RES,
    SURF3D_MIN_RES,
};
use crate::scene::{
    AmbientOrbit, Camera, Mobject, RateFunc, SceneError, SceneResult, ZoomAnim,
    MAX_TRACK_DURATION_MS,
};
use crate::textanim::{TextAnimError, Titulo};
use std::collections::BTreeMap;

// ── Registro ─────────────────────────────────────────────────────────────

/// Plantillas de este módulo, en kebab-case para el dispatcher
/// (`anim_native` las cablea; el wire las registra después en
/// `CANONICAL_TEMPLATES`). Ninguna colisiona con las 13 canónicas.
pub const TEMPLATE_IDS: &[&str] = &[
    "sup-paraboloide-tangente",
    "sup-toro-rotante",
    "sup-campo-vectorial",
    "sup-interseccion",
    "sup-onda-3d",
    "sup-silla-descenso",
    "sup-laplace-3d",
];

/// Título + descripción corta para la UI/preview (sin construir nada).
pub fn describe(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "sup-paraboloide-tangente" => Some((
            "Paraboloide y plano tangente",
            "el punto de tangencia recorre la superficie con su plano móvil",
        )),
        "sup-toro-rotante" => Some(("Toro rotante", "cabeceo continuo con órbita de cámara")),
        "sup-campo-vectorial" => Some((
            "Campo vectorial 3D",
            "flechas normales o rotacionales sobre el paraboloide",
        )),
        "sup-interseccion" => Some((
            "Intersección de superficies",
            "esfera cortada por un plano y su círculo de intersección",
        )),
        "sup-onda-3d" => Some(("Onda 3D", "onda radial que respira sobre la malla")),
        "sup-silla-descenso" => Some((
            "Silla y descenso",
            "bajada por gradiente revelada sobre z = x² − y²",
        )),
        "sup-laplace-3d" => Some((
            "Laplace 3D",
            "|F(s)| sobre el plano complejo con sonda en el eje real",
        )),
        _ => None,
    }
}

/// Título centrado de la escena (reúso `textanim::Titulo`; valida canvas
/// 64..=4096 y fuente 8..=96 px como el resto del crate).
pub fn titulo_para(id: &str, canvas_w: u32, canvas_h: u32) -> Result<Titulo, TextAnimError> {
    match describe(id) {
        Some((titulo, subtitulo)) => {
            Titulo::try_new(titulo, Some(subtitulo), 32.0, canvas_w, canvas_h)
        }
        None => Titulo::try_new(
            "Superficies 3D",
            Some("plantilla desconocida"),
            32.0,
            canvas_w,
            canvas_h,
        ),
    }
}

// ── Presupuestos del protocolo ───────────────────────────────────────────

/// Frames máximos por pedido (paridad `VIDEO_LONGFORM_MAX_FRAMES` = 1500).
pub const TPL3D_MAX_FRAMES: usize = VIDEO_LONGFORM_MAX_FRAMES;
/// Bytes máximos del chunk en RAM (paridad `LONGFORM_CHUNK_MAX_BYTES`).
pub const TPL3D_CHUNK_BYTES: usize = LONGFORM_CHUNK_MAX_BYTES;
/// Lado mínimo del canvas (paridad `Resolution`).
pub const TPL3D_CANVAS_MIN: u32 = 64;
/// Lado máximo del canvas (paridad `Resolution`).
pub const TPL3D_CANVAS_MAX: u32 = 4096;
/// Vectores máximos por lado del campo 3D (16² = 256 flechas).
pub const TPL3D_CAMPO_MAX_N: usize = 16;
/// Pasos máximos del descenso de la silla (holgura bajo 512).
pub const TPL3D_SILLA_MAX_PASOS: usize = 256;
/// Semiancho de las mallas base (`[-2, 2]²`).
pub const TPL3D_SEMI: f64 = 2.0;

/// Valida el conteo total de frames (1..=1500). `0` es `Err` honesto.
pub fn valida_frames(total: usize) -> SceneResult<()> {
    if total == 0 || total > TPL3D_MAX_FRAMES {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("frames {total} fuera de 1..={TPL3D_MAX_FRAMES}: partí el pedido"),
        });
    }
    Ok(())
}

/// Valida el pedido completo: canvas 64..=4096 (vía `Resolution`) + frames
/// 1..=1500. Todo `Err` honesto, sin pánicos.
pub fn valida_pedido(canvas_w: u32, canvas_h: u32, frames: usize) -> SceneResult<()> {
    Resolution::try_new(canvas_w, canvas_h).map_err(|e| SceneError::EscenaInvalida {
        detalle: format!("canvas inválido: {e}"),
    })?;
    if canvas_w < TPL3D_CANVAS_MIN
        || canvas_h < TPL3D_CANVAS_MIN
        || canvas_w > TPL3D_CANVAS_MAX
        || canvas_h > TPL3D_CANVAS_MAX
    {
        return Err(SceneError::EscenaInvalida {
            detalle: format!("canvas {canvas_w}x{canvas_h} fuera de 64..=4096 por lado"),
        });
    }
    valida_frames(frames)
}

/// ¿Cuántos frames de `w`×`h` entran en el chunk de 64 MiB? (P0.1, puro.)
/// Delega en `protocol::max_chunk_frames`; `0` honesto si no entra ninguno.
pub fn chunk_frames(canvas_w: usize, canvas_h: usize) -> usize {
    max_chunk_frames(canvas_w, canvas_h, TPL3D_CHUNK_BYTES)
}

// ── Claves vivas del wire ────────────────────────────────────────────────

/// Res de malla por lado (2..=128).
pub const P3D_RES: &str = "res";
/// Amplitud / coeficiente `a` (según plantilla).
pub const P3D_AMP: &str = "amp";
/// Frecuencia / coeficiente `b` / vueltas (según plantilla).
pub const P3D_FREQ: &str = "freq";
/// Fase inicial en radianes (punto de tangencia, onda).
pub const P3D_FASE: &str = "fase";
/// Radio mayor (toro) / radio de la esfera (intersección).
pub const P3D_MAYOR: &str = "mayor";
/// Radio menor (toro) / corte `c` del plano (intersección).
pub const P3D_MENOR: &str = "menor";
/// Extra: retícula del campo / pasos del descenso / `k` de la onda.
pub const P3D_EXTRA: &str = "extra";
/// Variante: 0/1 según plantilla (elíptico vs hiperbólico, etc.).
pub const P3D_MODO: &str = "modo";

// ── Params ───────────────────────────────────────────────────────────────

/// Params vivos de las 6 plantillas (todo `f64` crudo del wire).
///
/// | Clave | Paraboloide | Toro | Campo | Intersección | Onda | Silla |
/// |---|---|---|---|---|---|---|
/// | `res` | malla 2..=128 | malla 2..=128 | malla 2..=128 | malla 2..=128 | malla 2..=128 | malla 2..=128 |
/// | `amp` | `a` 0.05..=2 | — | — | — | amplitud 0.05..=1 | `x0` −2..=2 |
/// | `freq` | `b` 0.05..=2 | vueltas 0.25..=4 | — | — | `k` 0.5..=4 | `y0` −2..=2 |
/// | `fase` | fase 0..=2π | — | — | — | `w` 0.5..=4 | paso 0.01..=0.3 |
/// | `mayor` | — | mayor 0.5..=4 | — | radio 0.5..=4 | — | — |
/// | `menor` | parche 0.3..=1.5 | menor 0.05..=4 (< mayor) | intensidad 0.1..=1 | corte `|c| < R` | — | — |
/// | `extra` | — | — | retícula 1..=16 | — | — | pasos 2..=256 |
/// | `modo` | 0 elíptico / 1 hiperbólico | — | 0 normales / 1 rotacional | — | — | — |
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params3D {
    /// Res de malla por lado.
    pub res: f64,
    /// Amplitud / `a` / `x0`.
    pub amp: f64,
    /// Frecuencia / `b` / vueltas / `y0` / `k`.
    pub freq: f64,
    /// Fase / `w` / paso del descenso.
    pub fase: f64,
    /// Radio mayor / radio de la esfera.
    pub mayor: f64,
    /// Radio menor / parche tangente / intensidad / corte.
    pub menor: f64,
    /// Retícula del campo / pasos del descenso.
    pub extra: f64,
    /// Variante 0/1.
    pub modo: f64,
}

impl Params3D {
    /// Defaults por plantilla (lo que se ve sin mover sliders).
    pub fn por_defecto(id: &str) -> Self {
        match id {
            "sup-toro-rotante" => Self {
                res: 32.0,
                amp: 0.5,
                freq: 1.0,
                fase: 0.0,
                mayor: 2.0,
                menor: 0.6,
                extra: 8.0,
                modo: 0.0,
            },
            "sup-campo-vectorial" => Self {
                res: 24.0,
                amp: 0.3,
                freq: 1.0,
                fase: 0.0,
                mayor: 2.0,
                menor: 0.5,
                extra: 8.0,
                modo: 0.0,
            },
            "sup-interseccion" => Self {
                res: 32.0,
                amp: 0.5,
                freq: 1.0,
                fase: 0.0,
                mayor: 2.0,
                menor: 0.8,
                extra: 8.0,
                modo: 0.0,
            },
            "sup-onda-3d" => Self {
                res: 32.0,
                amp: 0.4,
                freq: 2.0,
                fase: 1.5,
                mayor: 2.0,
                menor: 0.6,
                extra: 8.0,
                modo: 0.0,
            },
            "sup-silla-descenso" => Self {
                res: 32.0,
                amp: 1.2,
                freq: 0.4,
                fase: 0.12,
                mayor: 2.0,
                menor: 0.6,
                extra: 64.0,
                modo: 0.0,
            },
            "sup-laplace-3d" => Self {
                res: 32.0,
                amp: 1.0,
                freq: 1.0,
                fase: 0.0,
                mayor: 2.0,
                menor: 0.6,
                extra: 8.0,
                modo: 0.0,
            },
            _ => Self {
                res: 32.0,
                amp: 0.5,
                freq: 0.5,
                fase: 0.0,
                mayor: 2.0,
                menor: 0.6,
                extra: 8.0,
                modo: 0.0,
            },
        }
    }

    /// Mezcla el mapa del wire sobre los defaults (ausente/NaN/inf →
    /// default, vía `protocol::scene_param`; el rango lo valida cada escena).
    pub fn desde_mapa(id: &str, params: &BTreeMap<String, f64>) -> Self {
        let base = Self::por_defecto(id);
        Self {
            res: scene_param(params, P3D_RES, base.res),
            amp: scene_param(params, P3D_AMP, base.amp),
            freq: scene_param(params, P3D_FREQ, base.freq),
            fase: scene_param(params, P3D_FASE, base.fase),
            mayor: scene_param(params, P3D_MAYOR, base.mayor),
            menor: scene_param(params, P3D_MENOR, base.menor),
            extra: scene_param(params, P3D_EXTRA, base.extra),
            modo: scene_param(params, P3D_MODO, base.modo),
        }
    }

    /// Res de malla 2..=128 (paridad `SURF3D_MIN/MAX_RES`).
    pub fn res(&self) -> SceneResult<usize> {
        if !self.res.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "Params3D.res",
                detalle: "res no finita: pasame 2..=128".to_string(),
            });
        }
        let n = self.res as usize;
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "Params3D.res",
                detalle: format!(
                    "res {} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}",
                    self.res
                ),
            });
        }
        Ok(n)
    }
}

// ── Utilidades ───────────────────────────────────────────────────────────

/// Guarda finita 0..1 (`NaN`/inf → 0). Pura.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Fracción cruda del frame (`frame < total`; `total == 1` → 0.0). Pura.
pub fn alpha_en(frame: usize, total: usize) -> SceneResult<f64> {
    valida_frames(total)?;
    if frame >= total {
        return Err(SceneError::EscenaInvalida {
            detalle: format!("frame {frame} fuera de 0..{total}"),
        });
    }
    if total <= 1 {
        return Ok(0.0);
    }
    Ok((frame as f64) / ((total - 1) as f64))
}

/// Instante en ms para `alpha` eased con [`smooth`] (morph suave). Puro.
pub fn t_ms_en(alpha_eased: f64, duration_ms: u64) -> SceneResult<u64> {
    if duration_ms == 0 || duration_ms > MAX_TRACK_DURATION_MS {
        return Err(SceneError::EscenaInvalida {
            detalle: format!("duración {duration_ms} fuera de 1..={MAX_TRACK_DURATION_MS}"),
        });
    }
    let a = clamp01(alpha_eased);
    Ok(((a * (duration_ms as f64)).round() as u64).min(duration_ms))
}

/// ¿Punto 3D finito y acotado? (guarda del wire, paridad ±1e6).
fn fin3_acotado(p: &[f64; 3]) -> bool {
    p.iter()
        .all(|v| v.is_finite() && v.abs() <= SURF3D_MAX_COORD)
}

/// Valida un coeficiente finito en `[min, max]`. Puro.
fn coef_en(v: f64, min: f64, max: f64, donde: &'static str, nombre: &str) -> SceneResult<f64> {
    if !v.is_finite() || v < min || v > max {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("{nombre} {v} fuera de {min}..={max}"),
        });
    }
    Ok(v)
}

// ── Cámara: giro + zoom ──────────────────────────────────────────────────

/// Órbita por defecto de la plantilla: una vuelta por clip topada a
/// 1 vuelta/s (más es mareo; clips de <1 s giran más lento, honesto).
/// `duration_ms` 1..=60000, `fov` fijo 55°.
pub fn orbita_por_defecto(id: &str, duration_ms: u64) -> SceneResult<AmbientOrbit> {
    if duration_ms == 0 || duration_ms > MAX_TRACK_DURATION_MS {
        return Err(SceneError::CamaraInvalida {
            detalle: format!("duración {duration_ms} fuera de 1..={MAX_TRACK_DURATION_MS}"),
        });
    }
    let dur_s = (duration_ms as f64) / 1000.0;
    let mut omega = std::f64::consts::TAU / dur_s;
    if !omega.is_finite() || omega <= 0.0 {
        omega = 0.0;
    }
    omega = omega.min(std::f64::consts::TAU);
    let (center, radius, height) = match id {
        "sup-toro-rotante" => ([0.0, 0.0, 0.0], 6.5, 3.0),
        "sup-interseccion" => ([0.0, 0.0, 0.4], 6.0, 2.6),
        "sup-onda-3d" => ([0.0, 0.0, 0.0], 6.0, 3.4),
        "sup-silla-descenso" => ([0.0, 0.0, -0.4], 6.0, 2.8),
        _ => ([0.0, 0.0, 0.5], 6.0, 3.0),
    };
    AmbientOrbit::try_new(center, radius, height, omega, 0.0, 55.0)
}

/// Zoom por defecto: la distancia viaja 7→5.2 y el `fov` 60→46 con
/// `RateFunc::Smooth` (reúso [`ZoomAnim` real, no números sueltos).
pub fn zoom_por_defecto(duration_ms: u64) -> SceneResult<ZoomAnim> {
    ZoomAnim::try_new(
        [0.0, 0.0, 0.0],
        [0.55, 0.45, 0.7],
        7.0,
        5.2,
        60.0,
        46.0,
        duration_ms,
        RateFunc::Smooth,
    )
    .map_err(|e| SceneError::CamaraInvalida {
        detalle: format!("zoom por defecto inválido: {e}"),
    })
}

/// Cámara del frame: ojo de la órbita en `t_ms` + `fov` del zoom en `t_ms`
/// (el easing del travelling vive en el [`ZoomAnim`).
/// Sin zoom → órbita pura. Pura, sin pánicos.
pub fn camara_para(orbita: AmbientOrbit, zoom: Option<ZoomAnim>, t_ms: u64) -> SceneResult<Camera> {
    let base = orbita.camera_at(t_ms)?;
    let Some(z) = zoom else {
        return Ok(base);
    };
    let fov = z.fov_at(t_ms);
    match base {
        Camera::Perspective { eye, center, .. } => Ok(Camera::perspective(fov, eye, center)?),
        Camera::Ortho(_) => Ok(base),
    }
}

// ── 1. Paraboloide / hiperboloide + plano tangente ───────────────────────

/// Paraboloide con punto de tangencia móvil.
///
/// `modo = 0`: elíptico `z = a·x² + b·y²`; `modo = 1`: hiperbólico
/// `z = a·x² − b·y²`. El punto da una vuelta (`fase` + `eased·2π`) sobre el
/// círculo de radio 1.2 y el parche tangente usa el diferencial exacto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParaboloideTangente {
    /// Res por lado (2..=128).
    pub res: usize,
    /// Coeficiente `a` (0.05..=2).
    pub a: f64,
    /// Coeficiente `b` (0.05..=2).
    pub b: f64,
    /// `false` = elíptico, `true` = hiperbólico.
    pub hiperbolico: bool,
    /// Fase inicial en radianes (finita).
    pub fase: f64,
    /// Semilado del parche tangente (0.3..=1.5).
    pub parche: f64,
}

impl ParaboloideTangente {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(
        res: usize,
        a: f64,
        b: f64,
        hiperbolico: bool,
        fase: f64,
        parche: f64,
    ) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "ParaboloideTangente",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        coef_en(a, 0.05, 2.0, "ParaboloideTangente", "a")?;
        coef_en(b, 0.05, 2.0, "ParaboloideTangente", "b")?;
        if !fase.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "ParaboloideTangente",
                detalle: "fase no finita: pasame radianes".to_string(),
            });
        }
        coef_en(parche, 0.3, 1.5, "ParaboloideTangente", "parche")?;
        Ok(Self {
            res,
            a,
            b,
            hiperbolico,
            fase,
            parche,
        })
    }

    /// Desde los params vivos (`amp = a`, `freq = b`, `modo = 0/1`,
    /// `fase` en radianes, `menor` = parche).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        let res = p.res()?;
        if !p.modo.is_finite() || !(0.0..=1.0).contains(&p.modo) {
            return Err(SceneError::MobjectInvalido {
                donde: "ParaboloideTangente",
                detalle: format!("modo {} fuera de 0..=1 (0 elíptico, 1 hiperbólico)", p.modo),
            });
        }
        Self::try_new(res, p.amp, p.freq, p.modo >= 0.5, p.fase, p.menor)
    }

    /// Altura `z(x, y)`. Pura.
    pub fn altura(self, x: f64, y: f64) -> f64 {
        if self.hiperbolico {
            self.a * x * x - self.b * y * y
        } else {
            self.a * x * x + self.b * y * y
        }
    }

    /// Malla base en `[-2, 2]²`. `Err` honesto si un punto degenera.
    pub fn superficie(self) -> SceneResult<Surface3D> {
        let semi = TPL3D_SEMI;
        Surface3D::try_new(self.res, self.res, [-semi, semi], [-semi, semi], |u, v| {
            [u, v, self.altura(u, v)]
        })
    }

    /// Punto de tangencia en `alpha` (crudo 0..1; no finito → 0).
    /// Da una vuelta exacta por clip (`fase + smooth(alpha)·2π`).
    pub fn punto_en(self, alpha: f64) -> [f64; 3] {
        let e = smooth(clamp01(alpha));
        let t = self.fase + e * std::f64::consts::TAU;
        let (s, c) = t.sin_cos();
        let (x, y) = (1.2 * c, 1.2 * s);
        [x, y, self.altura(x, y)]
    }

    /// Parche del plano tangente en `alpha` (diferencial exacto
    /// `z₀ + 2a·x₀·s ± 2b·y₀·t`, malla 8×8 sobre `±parche/2`).
    pub fn tangente_en(self, alpha: f64) -> SceneResult<Surface3D> {
        let q = self.punto_en(alpha);
        if !fin3_acotado(&q) {
            return Err(SceneError::MobjectInvalido {
                donde: "ParaboloideTangente",
                detalle: "punto de tangencia no finito".to_string(),
            });
        }
        let (x0, y0, z0) = (q[0], q[1], q[2]);
        let dx = 2.0 * self.a * x0;
        let dy = if self.hiperbolico {
            -2.0 * self.b * y0
        } else {
            2.0 * self.b * y0
        };
        let h = self.parche / 2.0;
        Surface3D::try_new(8, 8, [-h, h], [-h, h], |s, t| {
            [x0 + s, y0 + t, z0 + dx * s + dy * t]
        })
    }

    /// Trayectoria del punto (`128` muestras en `[0, 2π]`).
    pub fn trayectoria(self) -> SceneResult<Curva3D> {
        Curva3D::traza_en(128, 0.0, std::f64::consts::TAU, |t| {
            let a = self.fase + t;
            let (s, c) = a.sin_cos();
            let (x, y) = (1.2 * c, 1.2 * s);
            [x, y, self.altura(x, y)]
        })
    }
}

// ── 2. Toro rotante ──────────────────────────────────────────────────────

/// Toro que cabecea alrededor del eje `y` (`vueltas` por clip, velocidad
/// constante con [`linear`]). El giro sobre `z` sería invisible por simetría:
/// se usa `R_y` a propósito.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToroRotante {
    /// Res por lado (2..=128).
    pub res: usize,
    /// Radio mayor (0.5..=4).
    pub mayor: f64,
    /// Radio menor (0.05..=4, `< mayor`).
    pub menor: f64,
    /// Vueltas por clip (0.25..=4).
    pub vueltas: f64,
}

impl ToroRotante {
    /// Constructor validado (`mayor > menor`, sin autointersección).
    pub fn try_new(res: usize, mayor: f64, menor: f64, vueltas: f64) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "ToroRotante",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        coef_en(mayor, 0.5, 4.0, "ToroRotante", "mayor")?;
        coef_en(menor, 0.05, 4.0, "ToroRotante", "menor")?;
        if !matches!(menor.partial_cmp(&mayor), Some(std::cmp::Ordering::Less)) {
            return Err(SceneError::MobjectInvalido {
                donde: "ToroRotante",
                detalle: "necesito menor < mayor (si no se autointersecta)".to_string(),
            });
        }
        coef_en(vueltas, 0.25, 4.0, "ToroRotante", "vueltas")?;
        Ok(Self {
            res,
            mayor,
            menor,
            vueltas,
        })
    }

    /// Desde los params vivos (`mayor`, `menor`, `freq` = vueltas).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        Self::try_new(p.res()?, p.mayor, p.menor, p.freq)
    }

    /// Ángulo de cabeceo en `alpha` (crudo; `linear` = velocidad constante).
    pub fn angulo_en(self, alpha: f64) -> f64 {
        linear(clamp01(alpha)) * self.vueltas * std::f64::consts::TAU
    }

    /// Toro rotado `R_y(θ)` en `alpha`. `Err` honesto si degenera.
    pub fn superficie_en(self, alpha: f64) -> SceneResult<Surface3D> {
        let th = self.angulo_en(alpha);
        let (s, c) = th.sin_cos();
        let (mayor, menor) = (self.mayor, self.menor);
        Surface3D::try_new(
            self.res,
            self.res,
            [0.0, std::f64::consts::TAU],
            [0.0, std::f64::consts::TAU],
            |u, v| {
                let (su, cu) = u.sin_cos();
                let (sv, cv) = v.sin_cos();
                let r = mayor + menor * cv;
                let (x, y, z) = (r * cu, r * su, menor * sv);
                [c * x + s * z, y, -s * x + c * z]
            },
        )
    }

    /// Aro central (`radio = mayor`, `64` muestras) rotado en `alpha`.
    pub fn aro_en(self, alpha: f64) -> SceneResult<Curva3D> {
        let th = self.angulo_en(alpha);
        let (s, c) = th.sin_cos();
        let mayor = self.mayor;
        Curva3D::traza_en(64, 0.0, std::f64::consts::TAU, |u| {
            let (su, cu) = u.sin_cos();
            [c * mayor * cu, mayor * su, -s * mayor * cu]
        })
    }
}

// ── 3. Campo vectorial 3D ────────────────────────────────────────────────

/// Campo 3D sobre el paraboloide `z = 0.3·(x²+y²)`.
///
/// `modo = 0`: normales `[-2ax, -2ay, 1]` (el vector "parado" sobre la
/// superficie, como el hilo de `r/manim`); `modo = 1`: rotacional
/// `[-y, x, 0.3]`. Retícula `n × n` en `[-1.5, 1.5]²`, largo `intensidad`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CampoVectorial {
    /// Res de la base por lado (2..=128).
    pub res: usize,
    /// Retícula por lado (1..=16).
    pub n: usize,
    /// Largo de flecha (0.1..=1).
    pub intensidad: f64,
    /// `false` = normales, `true` = rotacional.
    pub rotacional: bool,
}

impl CampoVectorial {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(res: usize, n: usize, intensidad: f64, rotacional: bool) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoVectorial",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        if n == 0 || n > TPL3D_CAMPO_MAX_N {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoVectorial",
                detalle: format!("n {n} fuera de 1..={TPL3D_CAMPO_MAX_N}"),
            });
        }
        coef_en(intensidad, 0.1, 1.0, "CampoVectorial", "intensidad")?;
        Ok(Self {
            res,
            n,
            intensidad,
            rotacional,
        })
    }

    /// Desde los params vivos (`extra` = retícula, `menor` = intensidad,
    /// `modo` = 0/1).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        if !p.extra.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoVectorial",
                detalle: "extra no finita: pasame 1..=16".to_string(),
            });
        }
        let n = p.extra as usize;
        if !p.modo.is_finite() || !(0.0..=1.0).contains(&p.modo) {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoVectorial",
                detalle: format!("modo {} fuera de 0..=1 (0 normales, 1 rotacional)", p.modo),
            });
        }
        Self::try_new(p.res()?, n, p.menor, p.modo >= 0.5)
    }

    /// Base `z = 0.3·(x²+y²)` en `[-2, 2]²`.
    pub fn base(self) -> SceneResult<Surface3D> {
        let semi = TPL3D_SEMI;
        Surface3D::try_new(self.res, self.res, [-semi, semi], [-semi, semi], |u, v| {
            [u, v, 0.3 * (u * u + v * v)]
        })
    }

    /// Dirección del campo en `(x, y)` (sin normalizar; el sampler normaliza).
    /// Pura.
    pub fn direccion(self, x: f64, y: f64) -> [f64; 3] {
        if self.rotacional {
            [-y, x, 0.3]
        } else {
            [-0.6 * x, -0.6 * y, 1.0]
        }
    }

    /// Flechas `[base, punta]` (`n² ≤ 256`, finitas y acotadas).
    pub fn vectores(self) -> SceneResult<Vec<[[f64; 3]; 2]>> {
        let mut out = Vec::with_capacity(self.n * self.n);
        for i in 0..self.n {
            for j in 0..self.n {
                let x = if self.n <= 1 {
                    0.0
                } else {
                    -1.5 + 3.0 * (i as f64) / ((self.n - 1) as f64)
                };
                let y = if self.n <= 1 {
                    0.0
                } else {
                    -1.5 + 3.0 * (j as f64) / ((self.n - 1) as f64)
                };
                let base = [x, y, 0.3 * (x * x + y * y)];
                let d = self.direccion(x, y);
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if !l.is_finite() || l < 1e-9 {
                    continue;
                }
                let punta = [
                    base[0] + self.intensidad * d[0] / l,
                    base[1] + self.intensidad * d[1] / l,
                    base[2] + self.intensidad * d[2] / l,
                ];
                if fin3_acotado(&base) && fin3_acotado(&punta) {
                    out.push([base, punta]);
                }
            }
        }
        if out.is_empty() {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoVectorial",
                detalle: "campo vacío: nada que dibujar".to_string(),
            });
        }
        Ok(out)
    }
}

// ── 4. Intersección esfera–plano ─────────────────────────────────────────

/// Esfera de radio `R` cortada por el plano `z = c` (`|c| < R` estricto, con
/// margen `1e-9` para que el círculo no degenere a punto).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interseccion {
    /// Res por lado (2..=128).
    pub res: usize,
    /// Radio de la esfera (0.5..=4).
    pub radio: f64,
    /// Corte del plano (`|c| ≤ R − 1e-9`).
    pub corte: f64,
}

impl Interseccion {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(res: usize, radio: f64, corte: f64) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "Interseccion",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        coef_en(radio, 0.5, 4.0, "Interseccion", "radio")?;
        if !corte.is_finite() || corte.abs() > radio - 1e-9 {
            return Err(SceneError::MobjectInvalido {
                donde: "Interseccion",
                detalle: format!("corte {corte} fuera de ±({radio} − 1e-9): el círculo degenera"),
            });
        }
        Ok(Self { res, radio, corte })
    }

    /// Desde los params vivos (`mayor` = radio, `menor` = corte).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        Self::try_new(p.res()?, p.mayor, p.menor)
    }

    /// Radio del círculo (`√(R²−c²)`, `> 0` por construcción). Puro.
    pub fn radio_circulo(self) -> f64 {
        (self.radio * self.radio - self.corte * self.corte).sqrt()
    }

    /// Esfera (`Surface3D::esfera`, polos honestos).
    pub fn esfera(self) -> SceneResult<Surface3D> {
        Surface3D::esfera(self.radio, self.res, self.res)
    }

    /// Plano `z = c` en `[-R, R]²`.
    pub fn plano(self) -> SceneResult<Surface3D> {
        let r = self.radio;
        let c = self.corte;
        Surface3D::try_new(self.res, self.res, [-r, r], [-r, r], |u, v| [u, v, c])
    }

    /// Círculo de intersección (`128` muestras en `[0, 2π]`).
    pub fn curva(self) -> SceneResult<Curva3D> {
        let r = self.radio_circulo();
        if !r.is_finite() || r <= 0.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "Interseccion",
                detalle: "radio del círculo degenerado: separá el corte".to_string(),
            });
        }
        let c = self.corte;
        Curva3D::traza_en(128, 0.0, std::f64::consts::TAU, |t| {
            let (s, co) = t.sin_cos();
            [r * co, r * s, c]
        })
    }
}

// ── 5. Onda 3D ───────────────────────────────────────────────────────────

/// Onda radial `z = A·sin(k·r − w·t)` con `r = √(x²+y²)` sobre `[-2, 2]²`.
/// El tiempo animado cubre un período exacto (`T = 2π/w`): el clip cierra.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onda3D {
    /// Res por lado (2..=128).
    pub res: usize,
    /// Amplitud (0.05..=1).
    pub amplitud: f64,
    /// Número de onda (0.5..=4).
    pub k: f64,
    /// Frecuencia temporal (0.5..=4).
    pub w: f64,
}

impl Onda3D {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(res: usize, amplitud: f64, k: f64, w: f64) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "Onda3D",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        coef_en(amplitud, 0.05, 1.0, "Onda3D", "amplitud")?;
        coef_en(k, 0.5, 4.0, "Onda3D", "k")?;
        coef_en(w, 0.5, 4.0, "Onda3D", "w")?;
        Ok(Self {
            res,
            amplitud,
            k,
            w,
        })
    }

    /// Desde los params vivos (`amp` = amplitud, `freq` = `k`, `fase` = `w`).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        Self::try_new(p.res()?, p.amp, p.freq, p.fase)
    }

    /// Instante animado en `alpha` (crudo; un período `2π/w` por clip). Puro.
    pub fn tiempo_en(self, alpha: f64) -> f64 {
        smooth(clamp01(alpha)) * std::f64::consts::TAU / self.w
    }

    /// Altura `z(x, y, t)`. Pura.
    pub fn altura(self, x: f64, y: f64, t: f64) -> f64 {
        let r = (x * x + y * y).sqrt();
        self.amplitud * (self.k * r - self.w * t).sin()
    }

    /// Malla en `alpha` (el morph regenera la malla: Manim `Surface` por
    /// frame, acotado a 128²). `Err` honesto si degenera.
    pub fn superficie_en(self, alpha: f64) -> SceneResult<Surface3D> {
        let t = self.tiempo_en(alpha);
        if !t.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "Onda3D",
                detalle: "instante no finito".to_string(),
            });
        }
        let semi = TPL3D_SEMI;
        Surface3D::try_new(self.res, self.res, [-semi, semi], [-semi, semi], |u, v| {
            [u, v, self.altura(u, v, t)]
        })
    }

    /// Perfil sobre el eje `x` en `alpha` (`128` muestras en `[-2, 2]`).
    pub fn perfil_en(self, alpha: f64) -> SceneResult<Curva3D> {
        let t = self.tiempo_en(alpha);
        Curva3D::traza_en(128, -TPL3D_SEMI, TPL3D_SEMI, |x| {
            [x, 0.0, self.altura(x, 0.0, t)]
        })
    }
}

// ── 6. Silla + descenso ──────────────────────────────────────────────────

/// Silla `z = x² − y²` con sendero de descenso por gradiente desde
/// `(x0, y0)`: `p −= paso·(2x, −2y)`, `pasos` puntos en `[-2, 2]²`.
/// (Silla honesta: el descenso sigue la dirección de máxima bajada local y
/// puede frenarse en la silla; el sampler corta donde deja de bajar.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SillaDescenso {
    /// Res por lado (2..=128).
    pub res: usize,
    /// Puntos del sendero (2..=256).
    pub pasos: usize,
    /// `x0` inicial (−2..=2).
    pub x0: f64,
    /// `y0` inicial (−2..=2).
    pub y0: f64,
    /// Paso del gradiente (0.01..=0.3).
    pub paso: f64,
}

impl SillaDescenso {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(res: usize, pasos: usize, x0: f64, y0: f64, paso: f64) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "SillaDescenso",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        if !(2..=TPL3D_SILLA_MAX_PASOS).contains(&pasos) {
            return Err(SceneError::MobjectInvalido {
                donde: "SillaDescenso",
                detalle: format!("pasos {pasos} fuera de 2..={TPL3D_SILLA_MAX_PASOS}"),
            });
        }
        coef_en(x0, -2.0, 2.0, "SillaDescenso", "x0")?;
        coef_en(y0, -2.0, 2.0, "SillaDescenso", "y0")?;
        coef_en(paso, 0.01, 0.3, "SillaDescenso", "paso")?;
        if x0 == 0.0 && y0 == 0.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "SillaDescenso",
                detalle: "el origen es punto silla (gradiente nulo): arrancá descentrado"
                    .to_string(),
            });
        }
        Ok(Self {
            res,
            pasos,
            x0,
            y0,
            paso,
        })
    }

    /// Desde los params vivos (`amp` = `x0`, `freq` = `y0`, `fase` = paso,
    /// `extra` = pasos).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        if !p.extra.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "SillaDescenso",
                detalle: "extra no finita: pasame 2..=256".to_string(),
            });
        }
        Self::try_new(p.res()?, p.extra as usize, p.amp, p.freq, p.fase)
    }

    /// Silla en `[-2, 2]²`.
    pub fn superficie(self) -> SceneResult<Surface3D> {
        let semi = TPL3D_SEMI;
        Surface3D::try_new(self.res, self.res, [-semi, semi], [-semi, semi], |u, v| {
            [u, v, u * u - v * v]
        })
    }

    /// Sendero completo (corta honesto si sale de `[-2, 2]²` o deja de ser
    /// finito; `< 2` puntos es `Err`: nada que dibujar).
    pub fn sendero(self) -> SceneResult<Curva3D> {
        let mut puntos = Vec::with_capacity(self.pasos);
        let (mut x, mut y) = (self.x0, self.y0);
        puntos.push([x, y, x * x - y * y]);
        for _ in 1..self.pasos {
            let nx = x - self.paso * 2.0 * x;
            let ny = y + self.paso * 2.0 * y;
            if !nx.is_finite() || !ny.is_finite() {
                break;
            }
            if nx.abs() > TPL3D_SEMI || ny.abs() > TPL3D_SEMI {
                break;
            }
            x = nx;
            y = ny;
            puntos.push([x, y, x * x - y * y]);
        }
        if puntos.len() < 2 {
            return Err(SceneError::MobjectInvalido {
                donde: "SillaDescenso",
                detalle: "el sendero muere en el primer paso: probá otro arranque".to_string(),
            });
        }
        for (k, p) in puntos.iter().enumerate() {
            if !fin3_acotado(p) {
                return Err(SceneError::MobjectInvalido {
                    donde: "SillaDescenso",
                    detalle: format!("punto no finito en la muestra {k}"),
                });
            }
        }
        if puntos.len() > CURVA3D_MAX_PUNTOS {
            return Err(SceneError::PresupuestoExcedido {
                detalle: format!(
                    "sendero de {} puntos (tope {CURVA3D_MAX_PUNTOS})",
                    puntos.len()
                ),
            });
        }
        Ok(Curva3D {
            puntos,
            t0: 0.0,
            t1: 1.0,
        })
    }

/// Punto del descenso en `alpha` (crudo; índice eased sobre el sendero).
    pub fn punto_en(self, alpha: f64) -> SceneResult<[f64; 3]> {
        let s = self.sendero()?;
        let e = smooth(clamp01(alpha));
        let k = ((e * ((s.puntos.len() - 1) as f64)).round() as usize).min(s.puntos.len() - 1);
        s.puntos.get(k).copied().ok_or(SceneError::MobjectInvalido {
            donde: "SillaDescenso",
            detalle: "índice del sendero fuera de rango".to_string(),
        })
    }
}

// ── Laplace 3D: |F(s)| sobre el plano complejo ────────────────────────────
// f(t) = e^(−a·t) → F(s) = 1/(s+a). z = min(|F(σ+iω)|, tope): la campana
// con el polo recortado honestamente (sin inf en la malla). La curva real
// (ω=0) y la sonda móvil viven sobre la superficie.

/// Tope de z (el polo en s=−a diverge; se recorta, no se miente).
pub const LAPLACE3D_ZMAX: f64 = 4.0;
/// Rango σ (parte real de s).
pub const LAPLACE3D_SIG_MIN: f64 = -3.0;
/// Rango σ (parte real de s).
pub const LAPLACE3D_SIG_MAX: f64 = 1.5;
/// Rango ω (parte imaginaria de s).
pub const LAPLACE3D_W_MAX: f64 = 4.0;
/// Decaimiento `a` válido.
pub const LAPLACE3D_A_MIN: f64 = 0.05;
/// Decaimiento `a` válido.
pub const LAPLACE3D_A_MAX: f64 = 5.0;

/// Superficie |F(s)| de Laplace con sonda sobre el eje real.
pub struct Laplace3D {
    res: usize,
    a: f64,
}

impl Laplace3D {
    /// Constructor validado. Todo `Err` honesto.
    pub fn try_new(res: usize, a: f64) -> SceneResult<Self> {
        if !(SURF3D_MIN_RES..=SURF3D_MAX_RES).contains(&res) {
            return Err(SceneError::MobjectInvalido {
                donde: "Laplace3D",
                detalle: format!("res {res} fuera de {SURF3D_MIN_RES}..={SURF3D_MAX_RES}"),
            });
        }
        coef_en(a, LAPLACE3D_A_MIN, LAPLACE3D_A_MAX, "Laplace3D", "a")?;
        Ok(Self { res, a })
    }

    /// Desde los params vivos (`amp` = decaimiento `a`).
    pub fn desde_params(p: &Params3D) -> SceneResult<Self> {
        Self::try_new(p.res()?, p.amp)
    }

    /// |F(σ+iω)| = 1/|σ+a+iω| para f(t) = e^(−a·t). Pura.
    pub fn modulo(&self, sigma: f64, omega: f64) -> f64 {
        let m = (self.a + sigma).hypot(omega);
        if m <= 0.0 || !m.is_finite() {
            return LAPLACE3D_ZMAX;
        }
        (1.0 / m).min(LAPLACE3D_ZMAX)
    }

    /// Malla de la superficie sobre σ×ω. `Err` honesto si degenera.
    pub fn superficie(&self) -> SceneResult<Surface3D> {
        let a = self.a;
        Surface3D::try_new(
            self.res,
            self.res,
            [LAPLACE3D_SIG_MIN, LAPLACE3D_SIG_MAX],
            [-LAPLACE3D_W_MAX, LAPLACE3D_W_MAX],
            |u, v| {
                let m = (a + u).hypot(v);
                let z = if m <= 0.0 || !m.is_finite() {
                    LAPLACE3D_ZMAX
                } else {
                    (1.0 / m).min(LAPLACE3D_ZMAX)
                };
                [u, v, z]
            },
        )
    }

    /// Curva real F(σ) sobre ω=0, σ ∈ [0.05, 3.0]. Pura.
    pub fn curva_real(&self) -> SceneResult<Curva3D> {
        let mut puntos = Vec::with_capacity(32);
        for k in 0..32 {
            let s = 0.05 + 2.95 * f64::from(k) / 31.0;
            let z = 1.0 / (s + self.a);
            if !z.is_finite() {
                return Err(SceneError::MobjectInvalido {
                    donde: "Laplace3D",
                    detalle: "curva real no finita".to_string(),
                });
            }
            puntos.push([s, 0.0, z]);
        }
        Ok(Curva3D {
            puntos,
            t0: 0.0,
            t1: 1.0,
        })
    }

    /// Sonda sobre el eje real en `alpha` crudo 0..1. Pura.
    pub fn sonda_en(&self, alpha: f64) -> [f64; 3] {
        let e = smooth(clamp01(alpha));
        let s = 0.05 + e * 2.95;
        [s, 0.0, 1.0 / (s + self.a)]
    }
}

// ── Muestra del frame (giro + morph) ─────────────────────────────────────

/// Fotograma 3D muestreado: cámara del giro + geometría del morph.
/// Lo que cablea el dispatcher por frame (el frente lo proyecta a 2D con
/// [`a_poligonos`] o lo tesela con `solido`/`alambre`).
#[derive(Debug, Clone, PartialEq)]
pub struct Muestra3D {
    /// Cámara del giro en `t_ms` (órbita + `fov` del zoom).
    pub camara: Camera,
    /// `alpha` crudo 0..1 (`frame/(total−1)`).
    pub alpha: f64,
    /// `alpha` eased con [`smooth`] (el que mueve el morph).
    pub eased: f64,
    /// Instante en ms (`eased·duration`).
    pub t_ms: u64,
    /// Superficies del frame (base + overlays según plantilla).
    pub superficies: Vec<Surface3D>,
    /// Curvas del frame (trayectorias, intersección, perfiles, sendero).
    pub curvas: Vec<Curva3D>,
    /// Flechas del campo (`[base, punta]`; solo `sup-campo-vectorial`).
    pub vectores: Vec<[[f64; 3]; 2]>,
    /// Ejes de referencia (siempre presentes).
    pub ejes: Ejes3D,
}

/// Ejes de referencia por defecto (centro origen, semi `[3, 3, 2]`,
/// ticks `[7, 7, 5]`). `Err` honesto (inaccesible, pero total).
pub fn ejes_por_defecto() -> SceneResult<Ejes3D> {
    Ejes3D::try_new([0.0, 0.0, 0.0], [3.0, 3.0, 2.0], [7, 7, 5])
}

/// Sampler del frame `frame` de `total` (`duration_ms` 1..=60000):
/// giro de cámara ([`orbita_por_defecto`] + `fov` de [`zoom_por_defecto`])
/// + morph eased con [`smooth`]. Desconocido → `Err` honesto.
pub fn muestra_frame(
    id: &str,
    params: &Params3D,
    frame: usize,
    total: usize,
    duration_ms: u64,
) -> SceneResult<Muestra3D> {
    let alpha = alpha_en(frame, total)?;
    let eased = smooth(alpha);
    let t_ms = t_ms_en(eased, duration_ms)?;
    let orbita = orbita_por_defecto(id, duration_ms)?;
    let zoom = zoom_por_defecto(duration_ms).ok();
    let camara = camara_para(orbita, zoom, t_ms)?;
    let ejes = ejes_por_defecto()?;
    let mut superficies = Vec::new();
    let mut curvas = Vec::new();
    let mut vectores = Vec::new();
    match id {
        "sup-paraboloide-tangente" => {
            let esc = ParaboloideTangente::desde_params(params)?;
            superficies.push(esc.superficie()?);
            superficies.push(esc.tangente_en(alpha)?);
            curvas.push(esc.trayectoria()?);
        }
        "sup-toro-rotante" => {
            let esc = ToroRotante::desde_params(params)?;
            superficies.push(esc.superficie_en(alpha)?);
            curvas.push(esc.aro_en(alpha)?);
        }
        "sup-campo-vectorial" => {
            let esc = CampoVectorial::desde_params(params)?;
            superficies.push(esc.base()?);
            vectores = esc.vectores()?;
        }
        "sup-interseccion" => {
            let esc = Interseccion::desde_params(params)?;
            superficies.push(esc.esfera()?);
            superficies.push(esc.plano()?);
            curvas.push(esc.curva()?);
        }
        "sup-onda-3d" => {
            let esc = Onda3D::desde_params(params)?;
            superficies.push(esc.superficie_en(alpha)?);
            curvas.push(esc.perfil_en(alpha)?);
        }
        "sup-silla-descenso" => {
            let esc = SillaDescenso::desde_params(params)?;
            superficies.push(esc.superficie()?);
            let s = esc.sendero()?;
            let k = ((eased * ((s.puntos.len() - 1) as f64)).round() as usize)
                .min(s.puntos.len() - 1)
                .max(1);
            let tramo = s.puntos[..=k].to_vec();
            curvas.push(Curva3D {
                puntos: tramo,
                t0: 0.0,
                t1: 1.0,
            });
        }
        "sup-laplace-3d" => {
            let esc = Laplace3D::desde_params(params)?;
            superficies.push(esc.superficie()?);
            curvas.push(esc.curva_real()?);
        }
        otro => {
            return Err(SceneError::EscenaInvalida {
                detalle: format!("plantilla {otro:?} desconocida: elegí una de {TEMPLATE_IDS:?}"),
            });
        }
    }
    Ok(Muestra3D {
        camara,
        alpha,
        eased,
        t_ms,
        superficies,
        curvas,
        vectores,
        ejes,
    })
}

/// Proyecta la muestra a 2D de mundo para el renderer: cada [`Curva3D`]
/// visible va como `Polygon` (corta donde cae detrás, vía
/// [`Curva3D::proyecta`); cada flecha visible como `Line`; los ejes como
/// `Line`s. Pasa por el [`Scratch`] del llamador (cero allocs intermedios
/// no acotados; la salida reserva lo justo). Nunca inventa puntos.
pub fn a_poligonos(muestra: &Muestra3D, scratch: &mut Scratch) -> Vec<Mobject> {
    let mut out = Vec::new();
    for curva in &muestra.curvas {
        for tramo in curva.proyecta(muestra.camara) {
            if tramo.len() < 2 || tramo.len() > crate::scene::MAX_MOBJECT_POINTS {
                continue;
            }
            let mut finito = true;
            for p in &tramo {
                if !p[0].is_finite() || !p[1].is_finite() {
                    finito = false;
                    break;
                }
            }
            if !finito {
                continue;
            }
            let pts: Vec<[f64; 2]> = scratch.traza(&tramo, 1.0, false).to_vec();
            if pts.len() >= 2 {
                out.push(Mobject::Polygon { pts });
            }
        }
    }
    for flecha in &muestra.vectores {
        let (a, b) = match (
            muestra.camara.project_3d(flecha[0]),
            muestra.camara.project_3d(flecha[1]),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if !a[0].is_finite() || !a[1].is_finite() || !b[0].is_finite() || !b[1].is_finite() {
            continue;
        }
        if a[0] == b[0] && a[1] == b[1] {
            continue;
        }
        out.push(Mobject::Line { from: a, to: b });
    }
    for seg in muestra.ejes.ejes_segmentos() {
        let (a, b) = match (
            muestra.camara.project_3d(seg[0]),
            muestra.camara.project_3d(seg[1]),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if a[0] == b[0] && a[1] == b[1] {
            continue;
        }
        out.push(Mobject::Line { from: a, to: b });
    }
    out
}

// ── Tests inline ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tpl_3d_tests {
    use super::*;

    fn debe<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("esperaba Ok, fue Err: {e:?}"),
        }
    }

    fn es_kebab(id: &str) -> bool {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !id.starts_with('-')
            && !id.ends_with('-')
            && !id.contains("--")
    }

    fn params(id: &str) -> Params3D {
        Params3D::desde_mapa(id, &BTreeMap::new())
    }

    #[test]
    fn template_ids_kebab_unicos_y_descriptos() {
        assert_eq!(TEMPLATE_IDS.len(), 7);
        for (i, a) in TEMPLATE_IDS.iter().enumerate() {
            assert!(es_kebab(a), "no kebab: {a}");
            for b in &TEMPLATE_IDS[i + 1..] {
                assert_ne!(a, b, "duplicado: {a}");
            }
            assert!(describe(a).is_some(), "sin describe: {a}");
            for canonica in [
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
                "subspace",
                "fractal",
            ] {
                assert_ne!(*a, canonica);
            }
        }
        assert!(describe("no-existe").is_none());
        let t = debe(titulo_para("sup-onda-3d", 640, 480));
        let l = t.layout();
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
        assert_eq!(l.offset_y, (480.0 - l.bloque_h) / 2.0);
    }

    #[test]
    fn presupuestos_del_protocolo_pineados() {
        assert_eq!(TPL3D_MAX_FRAMES, 1500);
        assert_eq!(TPL3D_CHUNK_BYTES, 64 * 1024 * 1024);
        assert_eq!((TPL3D_CANVAS_MIN, TPL3D_CANVAS_MAX), (64, 4096));
        assert!(valida_frames(1).is_ok());
        assert!(valida_frames(1500).is_ok());
        assert!(valida_frames(0).is_err());
        assert!(valida_frames(1501).is_err());
        assert!(valida_pedido(640, 480, 48).is_ok());
        assert!(valida_pedido(63, 480, 48).is_err());
        assert!(valida_pedido(640, 4097, 48).is_err());
        assert!(valida_pedido(640, 480, 0).is_err());
        // Chunks P0.1 pineados como en el protocolo.
        assert_eq!(chunk_frames(1280, 720), 18);
        assert_eq!(chunk_frames(640, 480), 54);
        assert_eq!(chunk_frames(0, 480), 0);
    }

    #[test]
    fn paraboloide_plano_tangente_es_el_diferencial() {
        let esc = debe(ParaboloideTangente::try_new(16, 0.5, 0.5, false, 0.0, 0.6));
        let m = debe(esc.superficie());
        assert_eq!(m.puntos.len(), 17 * 17);
        // En alpha = 0 el punto está en (1.2, 0, 0.5·1.44).
        let q = esc.punto_en(0.0);
        assert!((q[0] - 1.2).abs() < 1e-12);
        assert!(q[1].abs() < 1e-12);
        assert!((q[2] - 0.5 * 1.44).abs() < 1e-9);
        // El parche tangente pasa por el punto y respeta el gradiente.
        let t = debe(esc.tangente_en(0.0));
        assert_eq!(t.puntos.len(), 81);
        let centro = t.puntos[40];
        assert!((centro[0] - q[0]).abs() < 1e-9);
        assert!((centro[1] - q[1]).abs() < 1e-9);
        assert!((centro[2] - q[2]).abs() < 1e-9);
        // Hiperbólico: la silla baja en y.
        let hip = debe(ParaboloideTangente::try_new(8, 0.5, 0.5, true, 0.0, 0.6));
        assert!(hip.altura(0.0, 1.0) < 0.0);
        assert!(esc.altura(0.0, 1.0) > 0.0);
        // Bordes honestos.
        assert!(ParaboloideTangente::try_new(1, 0.5, 0.5, false, 0.0, 0.6).is_err());
        assert!(ParaboloideTangente::try_new(8, 0.0, 0.5, false, 0.0, 0.6).is_err());
        assert!(ParaboloideTangente::try_new(8, 0.5, 0.5, false, f64::NAN, 0.6).is_err());
        assert!(ParaboloideTangente::try_new(8, 0.5, 0.5, false, 0.0, 2.0).is_err());
    }

    #[test]
    fn toro_cierra_la_vuelta_y_cabecea() {
        let esc = debe(ToroRotante::try_new(16, 2.0, 0.6, 1.0));
        let m0 = debe(esc.superficie_en(0.0));
        let m1 = debe(esc.superficie_en(1.0));
        // Una vuelta cierra: mismos puntos con tolerancia de giro completo.
        assert_eq!(m0.puntos.len(), m1.puntos.len());
        for (a, b) in m0.puntos.iter().zip(m1.puntos.iter()) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-9, "{a:?} vs {b:?}");
            }
        }
        // A mitad de clip el cabeceo movió la malla de verdad.
        let m05 = debe(esc.superficie_en(0.5));
        let mut movio = 0.0;
        for (a, b) in m0.puntos.iter().zip(m05.puntos.iter()) {
            let d = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
            if d > movio {
                movio = d;
            }
        }
        assert!(movio > 0.5, "cabeceo invisible: {movio}");
        assert!(ToroRotante::try_new(8, 0.6, 0.6, 1.0).is_err());
        assert!(ToroRotante::try_new(8, 0.6, 2.0, 1.0).is_err());
        assert!(ToroRotante::try_new(8, 2.0, 0.6, 0.0).is_err());
        assert!(ToroRotante::try_new(129, 2.0, 0.6, 1.0).is_err());
    }

    #[test]
    fn campo_sobre_superficie_flechas_sanas() {
        for modo in [false, true] {
            let esc = debe(CampoVectorial::try_new(12, 4, 0.5, modo));
            let v = debe(esc.vectores());
            assert_eq!(v.len(), 16);
            for [base, punta] in &v {
                assert!(fin3_acotado(base));
                assert!(fin3_acotado(punta));
                let dx = punta[0] - base[0];
                let dy = punta[1] - base[1];
                let dz = punta[2] - base[2];
                let l = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!((l - 0.5).abs() < 1e-9, "largo {l}");
            }
            let _ = debe(esc.base());
        }
        assert!(CampoVectorial::try_new(8, 0, 0.5, false).is_err());
        assert!(CampoVectorial::try_new(8, 17, 0.5, false).is_err());
        assert!(CampoVectorial::try_new(8, 4, 2.0, false).is_err());
    }

    #[test]
    fn interseccion_circulo_con_radio_pitagorico() {
        let esc = debe(Interseccion::try_new(16, 2.0, 0.8));
        // r = √(4 − 0.64) = √3.36.
        assert!((esc.radio_circulo() - 3.36_f64.sqrt()).abs() < 1e-12);
        let c = debe(esc.curva());
        assert_eq!(c.puntos.len(), 128);
        for p in &c.puntos {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - esc.radio_circulo()).abs() < 1e-9);
            assert!((p[2] - 0.8).abs() < 1e-12);
            // El círculo vive sobre la esfera.
            let n = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((n - 2.0).abs() < 1e-9);
        }
        let _ = debe(esc.esfera());
        let _ = debe(esc.plano());
        assert!(Interseccion::try_new(8, 2.0, 2.0).is_err());
        assert!(Interseccion::try_new(8, 2.0, 3.0).is_err());
        assert!(Interseccion::try_new(8, 0.1, 0.0).is_err());
    }

    #[test]
    fn onda_respira_un_periodo() {
        let esc = debe(Onda3D::try_new(16, 0.4, 2.0, 1.5));
        let m0 = debe(esc.superficie_en(0.0));
        let m1 = debe(esc.superficie_en(1.0));
        // Un período cierra la onda.
        for (a, b) in m0.puntos.iter().zip(m1.puntos.iter()) {
            assert!((a[2] - b[2]).abs() < 1e-9, "{} vs {}", a[2], b[2]);
        }
        // En el origen la altura es 0 en t = 0 (sin(r = 0) = 0).
        let centro = m0.puntos[(8 * 17 + 8) as usize];
        assert!(centro[2].abs() < 1e-12, "centro {:?}", centro);
        let p = debe(esc.perfil_en(0.25));
        assert_eq!(p.puntos.len(), 128);
        assert!(Onda3D::try_new(8, 0.0, 2.0, 1.5).is_err());
        assert!(Onda3D::try_new(8, 0.4, 0.1, 1.5).is_err());
    }

    #[test]
    fn silla_descenso_baja_y_revela() {
        let esc = debe(SillaDescenso::try_new(16, 64, 1.2, 0.4, 0.12));
        let s = debe(esc.sendero());
        assert!(s.puntos.len() >= 2);
        // Sobre la silla: cada punto respeta z = x² − y².
        for p in &s.puntos {
            assert!((p[2] - (p[0] * p[0] - p[1] * p[1])).abs() < 1e-9);
        }
        // El descenso baja (o al menos no sube) en z global.
        let ultimo = match s.puntos.last() {
            Some(p) => *p,
            None => panic!("sendero vacío"),
        };
        assert!(ultimo[2] <= s.puntos[0][2] + 1e-9);
        // El reveal avanza con alpha.
        let q0 = debe(esc.punto_en(0.0));
        let q1 = debe(esc.punto_en(1.0));
        assert_eq!(q0, s.puntos[0]);
        assert_eq!(q1, ultimo);
        assert!(SillaDescenso::try_new(8, 1, 1.0, 0.5, 0.1).is_err());
        assert!(SillaDescenso::try_new(8, 64, 0.0, 0.0, 0.1).is_err());
        assert!(SillaDescenso::try_new(8, 64, 1.0, 0.5, 1.0).is_err());
    }

    #[test]
    fn laplace_3d_modulo_curva_y_sonda() {
        let esc = debe(Laplace3D::try_new(16, 1.0));
        // F(s) = 1/(s+1): en s=0 vale 1, en s=1 vale 1/2.
        assert!((esc.modulo(0.0, 0.0) - 1.0).abs() < 1e-12);
        assert!((esc.modulo(1.0, 0.0) - 0.5).abs() < 1e-12);
        // El polo se recorta al tope, jamás inf.
        assert_eq!(esc.modulo(-1.0, 0.0), LAPLACE3D_ZMAX);
        // Malla finita y con las dimensiones pedidas.
        let sup = debe(esc.superficie());
        assert_eq!(sup.n_tris(), 2 * 16 * 16);
        // Curva real sobre F(σ) exacta.
        let curva = debe(esc.curva_real());
        assert_eq!(curva.puntos.len(), 32);
        for p in &curva.puntos {
            assert!((p[2] - 1.0 / (p[0] + 1.0)).abs() < 1e-9);
            assert_eq!(p[1], 0.0);
        }
        // La sonda se mueve con alpha (frame 0 != último).
        let q0 = esc.sonda_en(0.0);
        let q1 = esc.sonda_en(1.0);
        assert!((q1[0] - q0[0]).abs() > 1.0, "la sonda barre el eje");
        assert!((q0[2] - 1.0 / (q0[0] + 1.0)).abs() < 1e-9);
        // Presupuestos.
        assert!(Laplace3D::try_new(1, 1.0).is_err());
        assert!(Laplace3D::try_new(16, 0.0).is_err());
        assert!(Laplace3D::try_new(16, 100.0).is_err());
    }

    #[test]
    fn sampler_gira_y_morphnea_en_las_siete() {
        for id in TEMPLATE_IDS {
            let p = params(id);
            let m0 = debe(muestra_frame(id, &p, 0, 48, 8000));
            let m1 = debe(muestra_frame(id, &p, 47, 48, 8000));
            assert_eq!(m0.alpha, 0.0);
            assert_eq!(m1.alpha, 1.0);
            assert_eq!(m0.eased, 0.0);
            assert_eq!(m1.eased, 1.0);
            assert!(m1.t_ms > m0.t_ms);
            assert!(!m0.superficies.is_empty());
            // La órbita movió la cámara (un clip = una vuelta topada).
            let e0 = match m0.camara {
                Camera::Perspective { eye, .. } => eye,
                Camera::Ortho(_) => panic!("órbita debe dar perspectiva"),
            };
            let e1 = match m1.camara {
                Camera::Perspective { eye, .. } => eye,
                Camera::Ortho(_) => panic!("órbita debe dar perspectiva"),
            };
            let d = (e0[0] - e1[0]).abs() + (e0[1] - e1[1]).abs() + (e0[2] - e1[2]).abs();
            assert!(d < 1e-6, "{id}: la vuelta cierra ({d})");
            let m12 = debe(muestra_frame(id, &p, 24, 48, 8000));
            let e12 = match m12.camara {
                Camera::Perspective { eye, .. } => eye,
                Camera::Ortho(_) => panic!("órbita debe dar perspectiva"),
            };
            let d12 = (e0[0] - e12[0]).abs() + (e0[2] - e12[2]).abs();
            assert!(d12 > 0.5, "{id}: a mitad de clip giró ({d12})");
            // Toda la geometría finita y acotada.
            for s in m12.superficies.iter().chain(m0.superficies.iter()) {
                for pt in &s.puntos {
                    assert!(fin3_acotado(pt), "{id}: {pt:?}");
                }
            }
            // Proyección 2D honesta (sin zoom también anda).
            let mut scratch = Scratch::nuevo();
            let objs = a_poligonos(&m12, &mut scratch);
            for o in &objs {
                assert!(o.validate().is_ok(), "{o:?}");
            }
        }
        assert!(muestra_frame("no-existe", &params("sup-onda-3d"), 0, 48, 8000).is_err());
        assert!(muestra_frame("sup-onda-3d", &params("sup-onda-3d"), 48, 48, 8000).is_err());
        assert!(muestra_frame("sup-onda-3d", &params("sup-onda-3d"), 0, 0, 8000).is_err());
        assert!(muestra_frame("sup-onda-3d", &params("sup-onda-3d"), 0, 1501, 8000).is_err());
        assert!(muestra_frame("sup-onda-3d", &params("sup-onda-3d"), 0, 48, 0).is_err());
    }

    #[test]
    fn solido_y_alambre_pintan_lejos_primero() {
        let p = params("sup-paraboloide-tangente");
        let m = debe(muestra_frame("sup-paraboloide-tangente", &p, 12, 48, 8000));
        let base = &m.superficies[0];
        let caras = debe(base.solido(m.camara, [0.5, 0.8, 0.6]));
        assert!(!caras.is_empty());
        for par in caras.windows(2) {
            assert!(par[0].depth + 1e-9 >= par[1].depth);
        }
        for c in &caras {
            assert!((0.0..=1.0).contains(&c.shade));
        }
        let al = debe(base.alambre(m.camara));
        assert!(!al.is_empty());
        // Res default 32 → 2048 tris.
        assert_eq!(base.n_tris(), 2 * 32 * 32);
    }

    #[test]
    fn orbita_cierra_y_zoom_easea() {
        let orb = debe(orbita_por_defecto("sup-toro-rotante", 8000));
        let per = match orb.period_ms() {
            Some(p) => p,
            None => panic!("órbita con omega ≠ 0 debe tener período"),
        };
        assert!((per as i64 - 8000).abs() < 2, "período {per}");
        let a = orb.orbit_eye_at(0);
        let b = orb.orbit_eye_at(per);
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() < 1e-9, "{a:?} vs {b:?}");
        }
        // Clip corto: el giro se topa a 1 vuelta/s (honesto, sin mareo).
        let corta = debe(orbita_por_defecto("sup-toro-rotante", 100));
        assert!(corta.omega_rad_s <= std::f64::consts::TAU + 1e-12);
        let zoom = debe(zoom_por_defecto(8000));
        assert!((zoom.dist_at(0) - 7.0).abs() < 1e-12);
        assert!((zoom.dist_at(8000) - 5.2).abs() < 1e-12);
        assert!((zoom.fov_at(0) - 60.0).abs() < 1e-12);
        // El easing Smooth frena el arranque del zoom.
        assert!(zoom.fov_at(2000) > 60.0 - (60.0 - 46.0) * 0.25);
        let cam = debe(camara_para(orb, Some(zoom), 4000));
        match cam {
            Camera::Perspective { fov_deg, .. } => {
                assert!((fov_deg - zoom.fov_at(4000)).abs() < 1e-9);
            }
            Camera::Ortho(_) => panic!("zoom debe dar perspectiva"),
        }
        assert!(orbita_por_defecto("sup-toro-rotante", 0).is_err());
        assert!(zoom_por_defecto(61_000).is_err());
    }

    #[test]
    fn params_desde_mapa_y_bordes() {
        let mut mapa = BTreeMap::new();
        mapa.insert("res".to_string(), 16.0);
        mapa.insert("mayor".to_string(), 2.5);
        mapa.insert("menor".to_string(), 0.5);
        mapa.insert("freq".to_string(), 2.0);
        let p = Params3D::desde_mapa("sup-toro-rotante", &mapa);
        assert_eq!(p.res(), Ok(16));
        let esc = debe(ToroRotante::desde_params(&p));
        assert_eq!((esc.mayor, esc.menor, esc.vueltas), (2.5, 0.5, 2.0));
        // NaN/inf caen al default, nunca a basura.
        let mut malo = BTreeMap::new();
        malo.insert("res".to_string(), f64::NAN);
        let d = Params3D::desde_mapa("sup-onda-3d", &malo);
        assert_eq!(d.res(), Ok(32));
        let mut inf = BTreeMap::new();
        inf.insert("amp".to_string(), f64::INFINITY);
        let d2 = Params3D::desde_mapa("sup-onda-3d", &inf);
        assert!((d2.amp - 0.4).abs() < 1e-12);
        assert!(Params3D { res: 1.0, ..d2 }.res().is_err());
        assert!(Params3D { res: 129.0, ..d2 }.res().is_err());
    }
}
