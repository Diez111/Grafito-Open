//! Núcleo Manim-en-Rust (F1): escena, mobjects, rate-funcs y timeline por propiedad.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red. Todo lo que excede
//! presupuestos es `Err` honesto en rioplatense; jamás nada parcial en
//! silencio.
//!
//! - [`RateFunc`]: las 18 rate-funcs Manim (`linear/smooth/ease_in_out/
//!   rush_in_out/there_and_back/wiggle` históricas + `rush_into/rush_from/
//!   slow_into/double_smooth/squish/lingering/wiggle_k` exactas upstream +
//!   `there_and_back_with_pause/running_start/not_quite_there/
//!   exponential_decay/smootherstep` exactas P0 + alias `smoothstep`→`Smooth`).
//!   Unifica los 8 nombres legacy
//!   [`crate::protocol::EASING_NAMES`] (piel `grafito-ui/src/animation.rs`),
//!   [`crate::parametric::ShapeEasing`] y `grafito-geometry/src/morph.rs`:
//!   `from_name` acepta los tres vocabularios; `legacy_name` devuelve el
//!   equivalente exacto cuando existe (`None` honesto si no hay).
//! - [`Mobject`]/[`Scene`]: objetos vectoriales + cámara ortográfica + capas.
//! - [`Camera`]: ortográfica 2D (reusa [`Ortho`]) o perspectiva 3D con
//!   [`Camera::project_3d`] pinhole puro; [`MovingCamera`] viaja sobre
//!   [`PropertyTrack`]s.
//! - [`Animation`]: trait estilo Manim (`run_time_ms`, `rate`, `begin`,
//!   `interpolate(alpha)`, `finish`) + [`TransformAnim`] concreto sobre
//!   [`crate::parametric::PolylineMorph`].
//! - [`PropertyTrack`]/[`Clock`]: timeline por propiedad con easing por track
//!   (no global). Reusa [`crate::protocol::Playlist::schedule`] /
//!   [`crate::protocol::Playlist::sample_at`] y
//!   [`crate::protocol::playlist_frame_at`] vía los wrappers
//!   [`schedule_playlist`]/[`sample_playlist`]/[`frame_at_global`].
//! - `Transform`/`MatchingShapes`: [`matching_shapes_frames`] remuestrea por
//!   longitud de arco + suaviza con beziers cúbicos (B-spline) y acepta N
//!   distinto entre A y B. (Aclaración honesta: [`crate::parametric::
//!   PolylineMorph`] nunca exigió mismo N —remuestrea ambas—; el que exige
//!   mismo N remuestrea al máximo (vecino más cercano): el núcleo vía
//!   `plan_remuestreo` y el compositado nativo de la app a nivel píxel igual.)
//! - Taylor: [`taylor_anim_order_from_params`] cablea `terms` a orden 1..=7
//!   para `taylor-series` (el renderer W3 `render_taylor_frames_inner` lo lee;
//!   mapa vacío = histórico orden 3).
//!
//! Presupuestos (intactos, paridad con el resto del crate):
//! `samples` 2..=512, `frames` 1..=48, set 64 MiB, playlist 8 steps/96 frames,
//! timeline 64 keys/60 s (P0.1 long-form), tracks 1..=60000 ms, capas 32,
//! `Resolution` 64..=4096, `AnimDuration` 0.1..=60 s.
//!
//! `SCENE_MORPH_MAX_SAMPLES = 512` es ESPACIAL (puntos por forma
//! remuestreada), no temporal: P0.1 no lo toca (el tiempo largo va por
//! `MAX_TRACK_DURATION_MS` y por chunks del frente, jamás por más muestras).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Error honesto de la escena (siempre dice qué está mal, en rioplatense).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SceneError {
    #[error("rate-func desconocida: {got}")]
    RateDesconocida { got: String },
    #[error("mobject inválido ({donde}): {detalle}")]
    MobjectInvalido {
        donde: &'static str,
        detalle: String,
    },
    #[error("cámara inválida: {detalle}")]
    CamaraInvalida { detalle: String },
    #[error("escena inválida: {detalle}")]
    EscenaInvalida { detalle: String },
    #[error("track inválido ({prop}): {detalle}")]
    TrackInvalido { prop: String, detalle: String },
    #[error("reloj inválido: {detalle}")]
    RelojInvalido { detalle: String },
    #[error("morph inválido: {detalle}")]
    MorphInvalido { detalle: String },
    #[error("presupuesto excedido: {detalle}")]
    PresupuestoExcedido { detalle: String },
}

pub type SceneResult<T> = Result<T, SceneError>;

// ── RateFunc (las 13 Manim, unifica los 3 vocabularios) ────────────────────

/// Tope de `wiggles` de [`RateFunc::WiggleK`] (paridad con Manim `wiggle(w=2)`;
/// más de 16 es ruido ilegible en 48 frames).
pub const MAX_WIGGLE_K: u32 = 16;
/// Pausa default de `there_and_back_with_pause` Manim (`pause_ratio=1/3`).
pub const MANIM_PAUSE_RATIO: f64 = 1.0 / 3.0;
/// Tirón default de `running_start` Manim (`pull_factor=-0.5`).
pub const MANIM_RUNNING_PULL: f64 = -0.5;
/// Proporción default de `not_quite_there` Manim (`proportion=0.7` sobre `smooth`).
pub const MANIM_NOT_QUITE_PROPORTION: f64 = 0.7;
/// Vida media default de `exponential_decay` Manim (`half_life=0.1`).
pub const MANIM_EXP_HALF_LIFE: f64 = 0.1;
/// Inflexión logística de las rate-funcs Manim (`smooth`/`rush_*`/`double`,
/// `manim/utils/rate_functions.py`, `inflection = 10.0` por defecto).
pub const MANIM_SMOOTH_INFLECTION: f64 = 10.0;
/// Ventana del `squish` Manim (`squish_rate_func(_, a=0.4, b=0.6)`).
pub const MANIM_SQUISH_A: f64 = 0.4;
/// Ventana del `squish` Manim (ver [`MANIM_SQUISH_A`]).
pub const MANIM_SQUISH_B: f64 = 0.6;

/// Rate-func estilo Manim (mapea `alpha` lineal 0..1 a progreso eased).
///
/// Históricas (intactas):
/// - `Linear`: identidad (`t`).
/// - `Smooth`: smoothstep Manim (`3t²-2t³` == `smoothstep` upstream, NO la
///   logística `smooth`; ver nota abajo).
/// - `EaseInOut`: cúbica in-out (`4t³` / `1+4(t-1)³`; idéntica a
///   `ShapeEasing::CubicInOut` y a `easing::cubic_in_out` de la piel).
/// - `RushInOut`: cuadrática in-out (`2t²` / `1-2(1-t)²`; el "rush" simétrico).
/// - `ThereAndBack`: `sin(π·t)` (0→1→0; para salidas y regresos).
/// - `Wiggle`: `(1-t)·sin(3π·t)` histórica de este crate (oscila decayendo;
///   NO es el `wiggle` upstream — ese es [`RateFunc::WiggleK`]).
///
/// P1-core (exactas Manim `rate_functions.py`, sin aproximaciones):
/// - `RushInto`: `2·smooth(t/2)` con `smooth` logística (`inflection=10`).
/// - `RushFrom`: `2·smooth(t/2+0.5)-1` (logística, `inflection=10`).
/// - `SlowInto`: `sqrt(1-(1-t)²)` (cuarto de círculo, arranque brusco y
///   llegada suave).
/// - `DoubleSmooth`: `0.5·smooth(2t)` si `t<0.5`, `0.5·(1+smooth(2t-1))` si no
///   (logística, `inflection=10`).
/// - `Squish`: `squish_rate_func(smooth, 0.4, 0.6)` (0 antes, 1 después).
/// - `Lingering`: `squish_rate_func(identidad, 0, 0.8)` = `min(t/0.8, 1)`.
/// - `WiggleK(k)`: `there_and_back(t)·sin(k·π·t)` con `wiggles=k` (default
///   upstream `k=2`).
/// - `ThereAndBackWithPause`: `there_and_back_with_pause(t, 1/3)` exacto
///   upstream (meseta en 1 entre 1/3 y 2/3; `@zero` fuera de 0..1).
/// - `RunningStart`: `running_start(t, -0.5)` exacto upstream (Bézier
///   `[0,0,pull,pull,1,1,1]`; arranca hacia atrás si `pull<0`).
/// - `NotQuiteThere`: `not_quite_there(smooth, 0.7)(t)` exacto upstream
///   (`0.7·smooth(t)`; nunca llega a 1 por diseño).
/// - `ExponentialDecay`: `exponential_decay(t, 0.1)` exacto upstream
///   (`1-exp(-t/0.1)`; en `t=1` da 0.99995, no 1 exacto).
/// - `SmootherStep`: `smootherstep(t)` exacto upstream
///   (`6t⁵-15t⁴+10t³`; derivadas 1ª y 2ª nulas en los bordes).
///
/// Nota honesta: el `smooth` logístico upstream difiere del `Smooth` de este
/// crate (smoothstep polinómico `3t²-2t³` == `smoothstep` upstream). Se
/// conserva `Smooth` por compat (tests y piel lo pinean; `smoothstep` es su
/// alias) y las nuevas usan la logística con el nombre upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RateFunc {
    /// Progresión constante.
    #[default]
    Linear,
    /// Suave Manim (`3t²-2t³`).
    Smooth,
    /// Cúbica in-out (la más Manim; == `cubic_in_out` legacy).
    EaseInOut,
    /// Rush simétrico cuadrático.
    RushInOut,
    /// Ida y vuelta (`sin(π·t)`).
    ThereAndBack,
    /// Oscilación decayente histórica (`(1-t)·sin(3π·t)`).
    Wiggle,
    /// Entrada con prisa Manim (`2·smooth(t/2)`, logística `inflection=10`).
    RushInto,
    /// Salida con prisa Manim (`2·smooth(t/2+0.5)-1`, logística).
    RushFrom,
    /// Llegada suave Manim (`sqrt(1-(1-t)²)`).
    SlowInto,
    /// Doble suavizado Manim (logística por mitades).
    DoubleSmooth,
    /// Compresión Manim (`squish_rate_func(smooth, 0.4, 0.6)`).
    Squish,
    /// Permanencia Manim (`min(t/0.8, 1)`).
    Lingering,
    /// Wiggle Manim con `k` oscilaciones (`1..=16`; `there_and_back·sin`).
    WiggleK(u32),
    /// Ida-vuelta con meseta Manim (`pause_ratio=1/3`; `@zero`).
    ThereAndBackWithPause,
    /// Arranque con tirón Manim (`pull_factor=-0.5`; puede dar negativo).
    RunningStart,
    /// Casi-ahí Manim (`0.7·smooth(t)`; en `t=1` da 0.7, no 1).
    NotQuiteThere,
    /// Decaimiento exponencial Manim (`1-exp(-t/0.1)`; en `t=1` ≈ 0.99995).
    ExponentialDecay,
    /// Smootherstep Manim (`6t⁵-15t⁴+10t³`).
    SmootherStep,
}

impl RateFunc {
    /// Nombre canónico Manim (estable para wire/logs; `WiggleK` → `wiggle_k`
    /// sin el parámetro: el `k` viaja en el dato serde).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Smooth => "smooth",
            Self::EaseInOut => "ease_in_out",
            Self::RushInOut => "rush_in_out",
            Self::ThereAndBack => "there_and_back",
            Self::Wiggle => "wiggle",
            Self::RushInto => "rush_into",
            Self::RushFrom => "rush_from",
            Self::SlowInto => "slow_into",
            Self::DoubleSmooth => "double_smooth",
            Self::Squish => "squish",
            Self::Lingering => "lingering",
            Self::WiggleK(_) => "wiggle_k",
            Self::ThereAndBackWithPause => "there_and_back_with_pause",
            Self::RunningStart => "running_start",
            Self::NotQuiteThere => "not_quite_there",
            Self::ExponentialDecay => "exponential_decay",
            Self::SmootherStep => "smootherstep",
        }
    }

    /// Constructor validado del wiggle Manim (`k` 1..=`MAX_WIGGLE_K`).
    /// `None` honesto si `k` es 0 o excede el tope (ruido ilegible).
    pub fn wiggle_k(k: u32) -> Option<Self> {
        if (1..=MAX_WIGGLE_K).contains(&k) {
            Some(Self::WiggleK(k))
        } else {
            None
        }
    }

    /// Parsea por nombre: las 13 Manim + los 8 legacy
    /// (`EASING_NAMES`: `quadratic_in/out`, `cubic_in/out/in_out`,
    /// `sin_in_out`, `ease_out_back`) + alias (`lineal`, `suave`,
    /// con/sin guiones). `None` honesto si no matchea.
    ///
    /// Mapeo legacy (documentado, aproximado donde no hay exacto):
    /// `linear`→`Linear`; `sin_in_out`→`Smooth`; `cubic_in/out/in_out`→
    /// `EaseInOut`; `quadratic_in/out`→`RushInOut`; `ease_out_back`→`Wiggle`
    /// (ambos no-monotónicos con sobrepaso; no idénticos).
    /// `wiggle` histórico sigue siendo el decay propio; el upstream exacto
    /// es `wiggle_k`/`wiggle2`/`wiggle_manim` (= `WiggleK(2)`, default Manim).
    pub fn from_name(raw: &str) -> Option<Self> {
        let norm: String = raw
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        match norm.as_str() {
            "linear" | "lineal" => Some(Self::Linear),
            "smooth" | "suave" | "suavizado" | "sininout" | "sineinout" | "sinusoidal"
            | "smoothstep" => Some(Self::Smooth),
            "smootherstep" | "smoothererstep" => Some(Self::SmootherStep),
            "easeinout" | "cubicinout" | "cubicin" | "cubicout" => Some(Self::EaseInOut),
            "rushinout" | "quadraticin" | "quadraticout" => Some(Self::RushInOut),
            "rushinto" => Some(Self::RushInto),
            "rushfrom" => Some(Self::RushFrom),
            "slowinto" => Some(Self::SlowInto),
            "doublesmooth" => Some(Self::DoubleSmooth),
            "squish" | "squishratefunc" => Some(Self::Squish),
            "lingering" => Some(Self::Lingering),
            "wigglek" | "wiggle2" | "wigglemanim" => Some(Self::WiggleK(2)),
            "thereandback" | "idayvuelta" => Some(Self::ThereAndBack),
            "thereandbackwithpause" | "thereandbackconpausa" => Some(Self::ThereAndBackWithPause),
            "runningstart" => Some(Self::RunningStart),
            "notquitethere" | "casiahi" | "casiah" => Some(Self::NotQuiteThere),
            "exponentialdecay" | "decaimiento" => Some(Self::ExponentialDecay),
            "wiggle" | "easeoutback" | "back" | "rebote" => Some(Self::Wiggle),
            _ => None,
        }
    }

    /// Equivalente exacto en los 8 legacy (`EASING_NAMES`); `None` honesto
    /// cuando no hay exacto (`RushInOut`/`ThereAndBack`/`Wiggle`, las 7
    /// P1-core y las 5 nuevas P0 no existen tal cual en la piel: el wire
    /// legacy no las representa sin pérdida).
    pub const fn legacy_name(self) -> Option<&'static str> {
        match self {
            Self::Linear => Some("linear"),
            Self::Smooth => Some("sin_in_out"),
            Self::EaseInOut => Some("cubic_in_out"),
            Self::RushInOut
            | Self::ThereAndBack
            | Self::Wiggle
            | Self::RushInto
            | Self::RushFrom
            | Self::SlowInto
            | Self::DoubleSmooth
            | Self::Squish
            | Self::Lingering
            | Self::WiggleK(_)
            | Self::ThereAndBackWithPause
            | Self::RunningStart
            | Self::NotQuiteThere
            | Self::ExponentialDecay
            | Self::SmootherStep => None,
        }
    }

    /// Aplica la rate a `t` (clamp 0..1 + guardia finita, sin pánicos).
    /// `ThereAndBack`/`Wiggle`/`WiggleK`/`ThereAndBackWithPause` devuelven 0
    /// en `t=1` por diseño (vuelven). `NotQuiteThere` devuelve 0.7 en `t=1`
    /// y `ExponentialDecay` ≈ 0.99995 (no llegan a 1 a propósito).
    /// `RunningStart` puede dar negativo al inicio (`pull=-0.5`). Todas las
    /// fórmulas son las exactas upstream (`rate_functions.py`): `smooth`
    /// con `inflection=10`, `wiggles=k`, `squish 0.4/0.6`, `pause_ratio=1/3`,
    /// `pull_factor=-0.5`, `proportion=0.7`, `half_life=0.1`.
    pub fn apply(self, t: f64) -> f64 {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = match self {
            Self::Linear => t,
            Self::Smooth => t * t * (3.0 - 2.0 * t),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = t - 1.0;
                    4.0 * u * u * u + 1.0
                }
            }
            Self::RushInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }
            Self::ThereAndBack => (std::f64::consts::PI * t).sin(),
            Self::Wiggle => (1.0 - t) * (3.0 * std::f64::consts::PI * t).sin(),
            Self::RushInto => 2.0 * manim_smooth(t / 2.0),
            Self::RushFrom => 2.0 * manim_smooth(t / 2.0 + 0.5) - 1.0,
            Self::SlowInto => (1.0 - (1.0 - t) * (1.0 - t)).sqrt(),
            Self::DoubleSmooth => {
                if t < 0.5 {
                    0.5 * manim_smooth(2.0 * t)
                } else {
                    0.5 * (1.0 + manim_smooth(2.0 * t - 1.0))
                }
            }
            Self::Squish => {
                if t < MANIM_SQUISH_A {
                    0.0
                } else if t > MANIM_SQUISH_B {
                    1.0
                } else {
                    manim_smooth((t - MANIM_SQUISH_A) / (MANIM_SQUISH_B - MANIM_SQUISH_A))
                }
            }
            Self::Lingering => (t / 0.8).min(1.0),
            Self::WiggleK(k) => {
                let k = k.clamp(1, MAX_WIGGLE_K);
                manim_there_and_back(t) * ((f64::from(k) * std::f64::consts::PI * t).sin())
            }
            Self::ThereAndBackWithPause => manim_there_and_back_with_pause(t, MANIM_PAUSE_RATIO),
            Self::RunningStart => manim_running_start(t, MANIM_RUNNING_PULL),
            Self::NotQuiteThere => MANIM_NOT_QUITE_PROPORTION * manim_smooth(t),
            Self::ExponentialDecay => manim_exponential_decay(t, MANIM_EXP_HALF_LIFE),
            Self::SmootherStep => {
                let t2 = t * t;
                let t3 = t2 * t;
                6.0 * t3 * t2 - 15.0 * t2 * t2 + 10.0 * t3
            }
        };
        if v.is_finite() {
            v
        } else {
            t
        }
    }

    /// Versión `f32` para los tracks y `Timeline::sample_with`.
    pub fn apply_f32(self, t: f32) -> f32 {
        self.apply(f64::from(t)) as f32
    }
}

/// Sigmoide logística (`manim/utils/simple_functions.py::sigmoid`).
/// Pura; `exp` desbordado da 0/1 exactos, nunca NaN (denominador ≥ 1).
fn manim_sigmoid(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_sign_positive() { 1.0 } else { 0.0 };
    }
    1.0 / (1.0 + (-x).exp())
}

/// `smooth` logístico Manim (`rate_functions.py`, `inflection=10`):
/// `(sigmoid(10·(t-0.5)) - err) / (1 - 2·err)` con `err = sigmoid(-5)`,
/// clamp 0..1. `smooth(0)=0`, `smooth(0.5)=0.5`, `smooth(1)=1` exactos.
/// Puro, sin pánicos.
fn manim_smooth(t: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    let err = manim_sigmoid(-MANIM_SMOOTH_INFLECTION / 2.0);
    let v = (manim_sigmoid(MANIM_SMOOTH_INFLECTION * (t - 0.5)) - err) / (1.0 - 2.0 * err);
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        t
    }
}

/// `there_and_back` logístico Manim (el que usa `wiggle` upstream):
/// `smooth(2t)` si `t<0.5`, `smooth(2-2t)` si no. Puro.
fn manim_there_and_back(t: f64) -> f64 {
    if t < 0.5 {
        manim_smooth(2.0 * t)
    } else {
        manim_smooth(2.0 - 2.0 * t)
    }
}

/// `there_and_back_with_pause` exacto Manim (`rate_functions.py`,
/// `pause_ratio=1/3` por defecto): meseta en 1 entre `0.5±pause/2`.
/// `a = 2/(1-pause)`; `t` ya viene clamp 0..1. Puro, sin pánicos.
fn manim_there_and_back_with_pause(t: f64, pause_ratio: f64) -> f64 {
    let p = if pause_ratio.is_finite() {
        pause_ratio.clamp(0.0, 0.999_999)
    } else {
        MANIM_PAUSE_RATIO
    };
    let a = 2.0 / (1.0 - p);
    if !a.is_finite() || a <= 0.0 {
        return manim_there_and_back(t);
    }
    if t < 0.5 - p / 2.0 {
        manim_smooth(a * t)
    } else if t < 0.5 + p / 2.0 {
        1.0
    } else {
        manim_smooth(a - a * t)
    }
}

/// `running_start` exacto Manim (`pull_factor=-0.5` por defecto): Bézier
/// `[0,0,pull,pull,1,1,1]` evaluado en `t`. Puede dar negativo al inicio
/// (el tirón hacia atrás es el efecto buscado). Puro, sin pánicos.
fn manim_running_start(t: f64, pull: f64) -> f64 {
    let pull = if pull.is_finite() {
        pull
    } else {
        MANIM_RUNNING_PULL
    };
    let mt = 1.0 - t;
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    let t6 = t5 * t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let mt4 = mt3 * mt;
    15.0 * t2 * mt4 * pull + 20.0 * t3 * mt3 * pull + 15.0 * t4 * mt2 + 6.0 * t5 * mt + t6
}

/// `exponential_decay` exacto Manim (`half_life=0.1` por defecto):
/// `1-exp(-t/half_life)`. En `t=1` da ≈ 0.99995 (corte honesto upstream).
/// Puro, sin pánicos.
fn manim_exponential_decay(t: f64, half_life: f64) -> f64 {
    let h = if half_life.is_finite() && half_life > 0.0 {
        half_life
    } else {
        MANIM_EXP_HALF_LIFE
    };
    let v = 1.0 - (-t / h).exp();
    if v.is_finite() {
        v
    } else {
        t
    }
}

/// `fn` lineal identidad para `Timeline::sample` (compat histórico).
pub fn linear_unit_f32(f: f32) -> f32 {
    if f.is_finite() {
        f
    } else {
        0.0
    }
}

// ── Mobject / cámara / escena ─────────────────────────────────────────────

/// Tope de capas por escena (anti-OOM de la UI).
pub const MAX_SCENE_LAYERS: usize = 32;
/// Tope de puntos por polígono (paridad con `MORPH_MAX_INPUT_POINTS`).
pub const MAX_MOBJECT_POINTS: usize = 4096;
/// Muestras por forma remuestreada (paridad con `MORPH_MAX_SAMPLES`).
pub const SCENE_MORPH_MAX_SAMPLES: usize = 512;
/// Tope de bytes del SVG de un `Tex` (64 KiB, paridad con `line_cap`).
pub const MAX_TEX_SVG_BYTES: usize = 64 * 1024;
/// Profundidad máxima de `Group` anidados.
pub const MAX_GROUP_DEPTH: usize = 8;
/// Hijos máximos por `Group`.
pub const MAX_GROUP_CHILDREN: usize = 32;
/// Largo máximo de una expresión de `FunctionGraph` (== `MAX_EXPR_LENGTH`).
pub const MAX_GRAPH_EXPR_CHARS: usize = 2000;

/// Radio/lado mínimo de figuras (1e-9, paridad con `Ortho::try_new`).
pub const MIN_FIGURE_SIZE: f64 = 1e-9;
/// Radio/lado máximo de figuras (1e6, paridad con `Ortho::try_new`).
pub const MAX_FIGURE_SIZE: f64 = 1e6;
/// Cota máxima de las figuras P3 (`Rectangle`/`Ellipse`/`Arc`, unidades de
/// mundo): 4096 (paridad con `Resolution` 64..=4096 y `MAX_MOBJECT_POINTS`;
/// más estricta que el 1e6 legacy de `Circle`/`Square`, que se conserva por
/// compat y porque el viewport fijo [-3,3]² ya recorta lo gigante).
pub const MAX_P3_FIGURE_DIM: f64 = 4096.0;
/// Divisiones máximas por lado de una retícula (`NumberPlane`/`VectorField`,
/// paridad con `ArrowField` 64).
pub const MAX_FIELD_DIVISIONS: usize = 64;

/// Objeto vectorial de la escena (núcleo Manim, sin renderer).
///
/// `Tex` es SIN LaTeX por diseño: el SVG ya viene tipografiado (≤64 KiB);
/// para texto plano sin toolchain usá [`Mobject::tex_desde_texto`], que
/// escapa y envuelve sin invocar nada externo. Cotas `Group` intactas:
/// ≤32 hijos, anidado ≤8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mobject {
    /// Ejes cartesianos (el renderer decide ticks/labels).
    Axes,
    /// Gráfica `y = expr(x)` tal cual para el motor (nunca inventada).
    FunctionGraph {
        /// Expresión en `x` (1..=2000 chars, finita al evaluar la hace el motor).
        expr: String,
    },
    /// Polilínea/polígono en mundo.
    Polygon {
        /// 1..=4096 puntos finitos (2 si abierta, 3 si cerrada —lo valida el morph—).
        pts: Vec<[f64; 2]>,
    },
    /// Punto marcado en mundo.
    Dot { x: f64, y: f64 },
    /// Campo de flechas `nx × ny` (1..=64 por lado).
    ArrowField { nx: usize, ny: usize },
    /// Texto ya tipografiado como SVG (≤64 KiB, sin LaTeX).
    Tex { svg: String },
    /// Círculo en mundo (`r` 1e-9..=1e6, centro finito).
    Circle { cx: f64, cy: f64, r: f64 },
    /// Cuadrado centrado en mundo (`side` 1e-9..=1e6, centro finito).
    Square { cx: f64, cy: f64, side: f64 },
    /// Rectángulo centrado en mundo (`w`/`h` 1e-9..=4096, centro finito).
    /// El renderer dibuja 4 segmentos (sin relleno, como `Square`).
    Rectangle { cx: f64, cy: f64, w: f64, h: f64 },
    /// Elipse centrada en mundo (`rx`/`ry` 1e-9..=4096, centro finito).
    /// El renderer la muestrea adaptativo 32..=128 según el radio mayor.
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
    /// Arco circular en mundo, ÁNGULOS EN RADIANES (documentado: Manim usa
    /// radianes en `Arc`; grados solo en la Piel si los pide).
    /// (`r` 1e-9..=4096, centro finito, `end_rad > start_rad`, barrido
    /// `0 < end_rad - start_rad <= 2π`; vuelta completa = `start` a
    /// `start + 2π`, no más). El renderer submuestrea por ángulo.
    Arc {
        cx: f64,
        cy: f64,
        r: f64,
        start_rad: f64,
        end_rad: f64,
    },
    /// Segmento en mundo (extremos finitos y distintos; si coinciden usá `Dot`).
    Line { from: [f64; 2], to: [f64; 2] },
    /// Flecha en mundo (igual que `Line` pero con punta; el renderer decide).
    Arrow { from: [f64; 2], to: [f64; 2] },
    /// Plano numerado (retícula `x`/`y` con paso; divisiones 1..=64 por lado).
    NumberPlane {
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        x_step: f64,
        y_step: f64,
    },
    /// Campo vectorial `func(x, y)` sobre retícula `nx × ny` (1..=64).
    VectorField {
        /// Expresión en `x`,`y` (1..=2000 chars; la evalúa el motor).
        func: String,
        nx: usize,
        ny: usize,
    },
    /// Grupo de submobjects (≤32 hijos, anidado ≤8).
    Group(Vec<Mobject>),
}

impl Mobject {
    /// Valida el objeto (`Err` honesto, sin pánicos).
    pub fn validate(&self) -> SceneResult<()> {
        self.validate_con_profundidad(0)
    }

    fn validate_con_profundidad(&self, depth: usize) -> SceneResult<()> {
        match self {
            Self::Axes => Ok(()),
            Self::FunctionGraph { expr } => {
                let n = expr.chars().count();
                if expr.trim().is_empty() || n > MAX_GRAPH_EXPR_CHARS {
                    return Err(SceneError::MobjectInvalido {
                        donde: "FunctionGraph",
                        detalle: format!(
                            "expresión de {n} chars (válido 1..={MAX_GRAPH_EXPR_CHARS})"
                        ),
                    });
                }
                Ok(())
            }
            Self::Polygon { pts } => {
                if pts.is_empty() || pts.len() > MAX_MOBJECT_POINTS {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Polygon",
                        detalle: format!("{} puntos (válido 1..={MAX_MOBJECT_POINTS})", pts.len()),
                    });
                }
                for (i, p) in pts.iter().enumerate() {
                    if !p[0].is_finite() || !p[1].is_finite() {
                        return Err(SceneError::MobjectInvalido {
                            donde: "Polygon",
                            detalle: format!("punto no finito en el índice {i}"),
                        });
                    }
                }
                Ok(())
            }
            Self::Dot { x, y } => {
                if !x.is_finite() || !y.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Dot",
                        detalle: "coordenada no finita".to_string(),
                    });
                }
                Ok(())
            }
            Self::ArrowField { nx, ny } => {
                if *nx == 0 || *ny == 0 || *nx > 64 || *ny > 64 {
                    return Err(SceneError::MobjectInvalido {
                        donde: "ArrowField",
                        detalle: format!("retícula {nx}×{ny} (válido 1..=64 por lado)"),
                    });
                }
                Ok(())
            }
            Self::Tex { svg } => {
                if svg.is_empty() || svg.len() > MAX_TEX_SVG_BYTES {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Tex",
                        detalle: format!(
                            "svg de {} bytes (válido 1..={MAX_TEX_SVG_BYTES})",
                            svg.len()
                        ),
                    });
                }
                Ok(())
            }
            Self::Circle { cx, cy, r } => {
                if !cx.is_finite() || !cy.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Circle",
                        detalle: "centro no finito".to_string(),
                    });
                }
                if !r.is_finite() || !(MIN_FIGURE_SIZE..=MAX_FIGURE_SIZE).contains(r) {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Circle",
                        detalle: format!(
                            "radio {r} fuera de {MIN_FIGURE_SIZE}..={MAX_FIGURE_SIZE}"
                        ),
                    });
                }
                Ok(())
            }
            Self::Square { cx, cy, side } => {
                if !cx.is_finite() || !cy.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Square",
                        detalle: "centro no finito".to_string(),
                    });
                }
                if !side.is_finite() || !(MIN_FIGURE_SIZE..=MAX_FIGURE_SIZE).contains(side) {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Square",
                        detalle: format!(
                            "lado {side} fuera de {MIN_FIGURE_SIZE}..={MAX_FIGURE_SIZE}"
                        ),
                    });
                }
                Ok(())
            }
            Self::Line { from, to } => valida_segmento(from, to, "Line"),
            Self::Arrow { from, to } => valida_segmento(from, to, "Arrow"),
            Self::Rectangle { cx, cy, w, h } => {
                if !cx.is_finite() || !cy.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Rectangle",
                        detalle: "centro no finito".to_string(),
                    });
                }
                for (nombre, v) in [("w", *w), ("h", *h)] {
                    if !v.is_finite() || !(MIN_FIGURE_SIZE..=MAX_P3_FIGURE_DIM).contains(&v) {
                        return Err(SceneError::MobjectInvalido {
                            donde: "Rectangle",
                            detalle: format!(
                                "{nombre} {v} fuera de {MIN_FIGURE_SIZE}..={MAX_P3_FIGURE_DIM}"
                            ),
                        });
                    }
                }
                Ok(())
            }
            Self::Ellipse { cx, cy, rx, ry } => {
                if !cx.is_finite() || !cy.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Ellipse",
                        detalle: "centro no finito".to_string(),
                    });
                }
                for (nombre, v) in [("rx", *rx), ("ry", *ry)] {
                    if !v.is_finite() || !(MIN_FIGURE_SIZE..=MAX_P3_FIGURE_DIM).contains(&v) {
                        return Err(SceneError::MobjectInvalido {
                            donde: "Ellipse",
                            detalle: format!(
                                "{nombre} {v} fuera de {MIN_FIGURE_SIZE}..={MAX_P3_FIGURE_DIM}"
                            ),
                        });
                    }
                }
                Ok(())
            }
            Self::Arc {
                cx,
                cy,
                r,
                start_rad,
                end_rad,
            } => {
                if !cx.is_finite() || !cy.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Arc",
                        detalle: "centro no finito".to_string(),
                    });
                }
                if !r.is_finite() || !(MIN_FIGURE_SIZE..=MAX_P3_FIGURE_DIM).contains(r) {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Arc",
                        detalle: format!(
                            "radio {r} fuera de {MIN_FIGURE_SIZE}..={MAX_P3_FIGURE_DIM}"
                        ),
                    });
                }
                if !start_rad.is_finite() || !end_rad.is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Arc",
                        detalle: "ángulos no finitos (radianes)".to_string(),
                    });
                }
                let barrido = *end_rad - *start_rad;
                if !barrido.is_finite() || barrido <= 0.0 || barrido > std::f64::consts::TAU {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Arc",
                        detalle: format!(
                            "barrido {barrido} rad fuera de (0..=2π]: pedí end_rad > start_rad con vuelta máxima"
                        ),
                    });
                }
                Ok(())
            }
            Self::NumberPlane {
                x_min,
                x_max,
                y_min,
                y_max,
                x_step,
                y_step,
            } => {
                Ortho::try_new(*x_min, *x_max, *y_min, *y_max).map_err(|e| {
                    SceneError::MobjectInvalido {
                        donde: "NumberPlane",
                        detalle: format!("rango: {e}"),
                    }
                })?;
                for (nombre, paso, rango) in [
                    ("x_step", *x_step, *x_max - *x_min),
                    ("y_step", *y_step, *y_max - *y_min),
                ] {
                    if !paso.is_finite() || paso <= 0.0 || paso > rango {
                        return Err(SceneError::MobjectInvalido {
                            donde: "NumberPlane",
                            detalle: format!("{nombre} {paso} fuera de (0..={rango}]"),
                        });
                    }
                    let divisiones = rango / paso;
                    if !divisiones.is_finite()
                        || divisiones < 1.0
                        || divisiones > MAX_FIELD_DIVISIONS as f64
                    {
                        return Err(SceneError::MobjectInvalido {
                            donde: "NumberPlane",
                            detalle: format!(
                                "{nombre} da {divisiones} divisiones (válido 1..={MAX_FIELD_DIVISIONS})"
                            ),
                        });
                    }
                }
                Ok(())
            }
            Self::VectorField { func, nx, ny } => {
                let n = func.chars().count();
                if func.trim().is_empty() || n > MAX_GRAPH_EXPR_CHARS {
                    return Err(SceneError::MobjectInvalido {
                        donde: "VectorField",
                        detalle: format!(
                            "expresión de {n} chars (válido 1..={MAX_GRAPH_EXPR_CHARS})"
                        ),
                    });
                }
                if *nx == 0 || *ny == 0 || *nx > MAX_FIELD_DIVISIONS || *ny > MAX_FIELD_DIVISIONS {
                    return Err(SceneError::MobjectInvalido {
                        donde: "VectorField",
                        detalle: format!(
                            "retícula {nx}×{ny} (válido 1..={MAX_FIELD_DIVISIONS} por lado)"
                        ),
                    });
                }
                Ok(())
            }
            Self::Group(hijos) => {
                if depth >= MAX_GROUP_DEPTH {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Group",
                        detalle: format!("anidado más de {MAX_GROUP_DEPTH}: aplaná el grupo"),
                    });
                }
                if hijos.is_empty() || hijos.len() > MAX_GROUP_CHILDREN {
                    return Err(SceneError::MobjectInvalido {
                        donde: "Group",
                        detalle: format!("{} hijos (válido 1..={MAX_GROUP_CHILDREN})", hijos.len()),
                    });
                }
                for h in hijos {
                    h.validate_con_profundidad(depth.saturating_add(1))?;
                }
                Ok(())
            }
        }
    }

    /// `Tex` sin LaTeX desde texto plano: escapa (`&<>"'`) y envuelve en un
    /// SVG mínimo (`<svg><text>…</text></svg>`), sin invocar toolchain
    /// externo. `Err` honesto si el texto está vacío o el SVG excede 64 KiB.
    /// Puro, sin I/O, sin pánicos.
    pub fn tex_desde_texto(texto: &str) -> SceneResult<Self> {
        let recortado = texto.trim();
        if recortado.is_empty() {
            return Err(SceneError::MobjectInvalido {
                donde: "Tex",
                detalle: "texto vacío: pasame al menos un carácter".to_string(),
            });
        }
        let mut escapado = String::with_capacity(recortado.len());
        for ch in recortado.chars() {
            match ch {
                '&' => escapado.push_str("&amp;"),
                '<' => escapado.push_str("&lt;"),
                '>' => escapado.push_str("&gt;"),
                '"' => escapado.push_str("&quot;"),
                '\'' => escapado.push_str("&apos;"),
                otro => escapado.push(otro),
            }
            if escapado.len() > MAX_TEX_SVG_BYTES {
                return Err(SceneError::MobjectInvalido {
                    donde: "Tex",
                    detalle: format!("excede {MAX_TEX_SVG_BYTES} bytes: acortá el texto"),
                });
            }
        }
        let svg =
            format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><text>{escapado}</text></svg>");
        if svg.len() > MAX_TEX_SVG_BYTES {
            return Err(SceneError::MobjectInvalido {
                donde: "Tex",
                detalle: format!("excede {MAX_TEX_SVG_BYTES} bytes: acortá el texto"),
            });
        }
        let tex = Self::Tex { svg };
        tex.validate()?;
        Ok(tex)
    }
}

/// Valida un segmento `from → to`: extremos finitos y distintos (si
/// coinciden es un punto: usá `Dot`). Puro, sin pánicos.
fn valida_segmento(from: &[f64; 2], to: &[f64; 2], donde: &'static str) -> SceneResult<()> {
    for (nombre, p) in [("from", from), ("to", to)] {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde,
                detalle: format!("{nombre} no finito"),
            });
        }
    }
    if from[0] == to[0] && from[1] == to[1] {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: "extremos idénticos: es un punto, usá Dot".to_string(),
        });
    }
    Ok(())
}

/// Cámara ortográfica (rectángulo de mundo visible).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ortho {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl Ortho {
    /// Rectángulo validado (finitos, `min < max`, lado 1e-9..=1e6).
    pub fn try_new(x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> SceneResult<Self> {
        for (nombre, v) in [
            ("x_min", x_min),
            ("x_max", x_max),
            ("y_min", y_min),
            ("y_max", y_max),
        ] {
            if !v.is_finite() {
                return Err(SceneError::CamaraInvalida {
                    detalle: format!("{nombre} no es finito"),
                });
            }
        }
        let x_ok = matches!(x_min.partial_cmp(&x_max), Some(std::cmp::Ordering::Less));
        let y_ok = matches!(y_min.partial_cmp(&y_max), Some(std::cmp::Ordering::Less));
        if !x_ok || !y_ok {
            return Err(SceneError::CamaraInvalida {
                detalle: "necesito x_min<x_max e y_min<y_max".to_string(),
            });
        }
        for (nombre, lado) in [("ancho", x_max - x_min), ("alto", y_max - y_min)] {
            if !lado.is_finite() || !(1e-9..=1e6).contains(&lado) {
                return Err(SceneError::CamaraInvalida {
                    detalle: format!("{nombre} fuera de 1e-9..=1e6"),
                });
            }
        }
        Ok(Self {
            x_min,
            x_max,
            y_min,
            y_max,
        })
    }

    /// Cámara canónica 16:9 en mundo (`-8..8 × -4.5..4.5`).
    pub fn default_16_9() -> Self {
        Self {
            x_min: -8.0,
            x_max: 8.0,
            y_min: -4.5,
            y_max: 4.5,
        }
    }

    /// Interpola linealmente hacia `otra` con fracción `f` 0..1 (clamp +
    /// guardia finita; no finita → `self`). Puro, sin pánicos.
    pub fn lerp_hacia(self, otra: Self, f: f64) -> Self {
        let f = if f.is_finite() {
            f.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mezcla = |a: f64, b: f64| {
            let v = a + (b - a) * f;
            if v.is_finite() {
                v
            } else {
                a
            }
        };
        Self {
            x_min: mezcla(self.x_min, otra.x_min),
            x_max: mezcla(self.x_max, otra.x_max),
            y_min: mezcla(self.y_min, otra.y_min),
            y_max: mezcla(self.y_max, otra.y_max),
        }
    }
}

// ── Cámara 3D (P1-core) ──────────────────────────────────────────────────
// `Camera` cubre 2D (`Ortho`, reusa el existente) y 3D (`Perspective` con
// `fov`/`eye`/`center`). `project_3d` es pinhole puro: `Ortho` descarta `z`,
// `Perspective` proyecta con look-at (`None` honesto si el punto cae detrás
// o la base degenera). `MovingCamera{from, to}` interpola sobre
// `PropertyTrack` (los tracks reales salen por `as_tracks`).

/// Cámara de la escena: ortográfica 2D o perspectiva 3D.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Camera {
    /// Rectángulo de mundo visible (reusa [`Ortho`]).
    Ortho(Ortho),
    /// Perspectiva pinhole: `fov` vertical en grados (1..=179), `eye`
    /// posición y `center` objetivo (finitos y distintos).
    Perspective {
        fov_deg: f64,
        eye: [f64; 3],
        center: [f64; 3],
    },
}

impl Camera {
    /// Perspectiva validada (`fov` 1..=179 finito, `eye`/`center` finitos y
    /// a más de 1e-9 de distancia). Todo `Err` honesto.
    pub fn perspective(fov_deg: f64, eye: [f64; 3], center: [f64; 3]) -> SceneResult<Self> {
        if !fov_deg.is_finite() || !(1.0..=179.0).contains(&fov_deg) {
            return Err(SceneError::CamaraInvalida {
                detalle: format!("fov {fov_deg} fuera de 1..=179"),
            });
        }
        for (nombre, p) in [("eye", &eye), ("center", &center)] {
            if !p.iter().all(|v| v.is_finite()) {
                return Err(SceneError::CamaraInvalida {
                    detalle: format!("{nombre} no finito"),
                });
            }
        }
        let dx = center[0] - eye[0];
        let dy = center[1] - eye[1];
        let dz = center[2] - eye[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if !dist.is_finite() || dist < 1e-9 {
            return Err(SceneError::CamaraInvalida {
                detalle: "eye y center coinciden: separá la cámara del objetivo".to_string(),
            });
        }
        Ok(Self::Perspective {
            fov_deg,
            eye,
            center,
        })
    }

    /// ¿Es ortográfica?
    pub fn is_ortho(self) -> bool {
        matches!(self, Self::Ortho(_))
    }

    /// Proyecta un punto 3D a 2D de mundo (pinhole puro, sin pánicos).
    ///
    /// - `Ortho`: descarta `z` (`Some([x, y])`; `None` si no finito).
    /// - `Perspective`: base look-at (`forward=center-eye`, `up=[0,1,0]` salvo
    ///   degeneración) y `x/y = dot·escala` con
    ///   `escala = 1/(tan(fov/2)·profundidad)`; `None` honesto si el punto
    ///   cae detrás (`profundidad ≤ 0`) o la base degenera.
    pub fn project_3d(self, p: [f64; 3]) -> Option<[f64; 2]> {
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        match self {
            Self::Ortho(_) => Some([p[0], p[1]]),
            Self::Perspective {
                fov_deg,
                eye,
                center,
            } => {
                let mut fx = center[0] - eye[0];
                let mut fy = center[1] - eye[1];
                let mut fz = center[2] - eye[2];
                let flen = (fx * fx + fy * fy + fz * fz).sqrt();
                if !flen.is_finite() || flen < 1e-9 {
                    return None;
                }
                fx /= flen;
                fy /= flen;
                fz /= flen;
                // right = forward × up (up=[0,1,0]; si degenera, up=[1,0,0]).
                // cross(forward, [0,1,0]) = (-fz, 0, fx).
                let mut rx = -fz;
                let mut ry = 0.0;
                let mut rz = fx;
                let mut rlen = (rx * rx + ry * ry + rz * rz).sqrt();
                if !rlen.is_finite() || rlen < 1e-9 {
                    // forward ∥ up: usar up=[1,0,0] → right = forward × [1,0,0]
                    // = (fy*0 - fz*0, fz*1 - fx*0, fx*0 - fy*1) = (0, fz, -fy).
                    rx = 0.0;
                    ry = fz;
                    rz = -fy;
                    rlen = (ry * ry + rz * rz).sqrt();
                    if !rlen.is_finite() || rlen < 1e-9 {
                        return None;
                    }
                }
                rx /= rlen;
                ry /= rlen;
                rz /= rlen;
                // up2 = right × forward.
                let ux = ry * fz - rz * fy;
                let uy = rz * fx - rx * fz;
                let uz = rx * fy - ry * fx;
                let vx = p[0] - eye[0];
                let vy = p[1] - eye[1];
                let vz = p[2] - eye[2];
                let prof = vx * fx + vy * fy + vz * fz;
                if !prof.is_finite() || prof <= 0.0 {
                    return None;
                }
                let mitad = (fov_deg * std::f64::consts::PI / 360.0).tan();
                if !mitad.is_finite() || mitad <= 0.0 {
                    return None;
                }
                let escala = 1.0 / (mitad * prof);
                if !escala.is_finite() {
                    return None;
                }
                let x = (vx * rx + vy * ry + vz * rz) * escala;
                let y = (vx * ux + vy * uy + vz * uz) * escala;
                if x.is_finite() && y.is_finite() {
                    Some([x, y])
                } else {
                    None
                }
            }
        }
    }

    /// Interpola hacia `otra` (misma variante) con fracción `f` 0..1.
    /// Variante distinta → `self` (el constructor ya lo impide; total).
    pub fn lerp_hacia(self, otra: Self, f: f64) -> Self {
        let f = if f.is_finite() {
            f.clamp(0.0, 1.0)
        } else {
            0.0
        };
        match (self, otra) {
            (Self::Ortho(a), Self::Ortho(b)) => Self::Ortho(a.lerp_hacia(b, f)),
            (
                Self::Perspective {
                    fov_deg: fa,
                    eye: ea,
                    center: ca,
                },
                Self::Perspective {
                    fov_deg: fb,
                    eye: eb,
                    center: cb,
                },
            ) => {
                let mezcla = |a: f64, b: f64| {
                    let v = a + (b - a) * f;
                    if v.is_finite() {
                        v
                    } else {
                        a
                    }
                };
                let mut eye = [0.0; 3];
                let mut center = [0.0; 3];
                for k in 0..3 {
                    eye[k] = mezcla(ea[k], eb[k]);
                    center[k] = mezcla(ca[k], cb[k]);
                }
                Self::Perspective {
                    fov_deg: mezcla(fa, fb),
                    eye,
                    center,
                }
            }
            (misma, _) => misma,
        }
    }
}

/// Cámara móvil `from → to` (misma variante) con easing propio.
///
/// El movimiento SE expresa como [`PropertyTrack`]s: [`MovingCamera::as_tracks`]
/// devuelve un track por grado de libertad (4 en `Ortho`, 7 en
/// `Perspective`) con la misma `duration_ms` y `easing`; [`MovingCamera::sample`]
/// es el atajo puro que interpola con el easing aplicado.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MovingCamera {
    pub from: Camera,
    pub to: Camera,
    /// Duración en ms (1..=60000, paridad con tracks, P0.1 long-form).
    pub duration_ms: u64,
    /// Easing del travelling.
    pub easing: RateFunc,
}

impl MovingCamera {
    /// Constructor validado: misma variante en `from`/`to`, duración
    /// 1..=60000 (P0.1 long-form). Todo `Err` honesto.
    pub fn try_new(
        from: Camera,
        to: Camera,
        duration_ms: u64,
        easing: RateFunc,
    ) -> SceneResult<Self> {
        let misma = matches!(
            (&from, &to),
            (Camera::Ortho(_), Camera::Ortho(_))
                | (Camera::Perspective { .. }, Camera::Perspective { .. })
        );
        if !misma {
            return Err(SceneError::CamaraInvalida {
                detalle:
                    "from y to deben ser la misma variante (Ortho→Ortho o Perspective→Perspective)"
                        .to_string(),
            });
        }
        if duration_ms == 0 || duration_ms > MAX_TRACK_DURATION_MS {
            return Err(SceneError::CamaraInvalida {
                detalle: format!("duración {duration_ms} fuera de 1..={MAX_TRACK_DURATION_MS}"),
            });
        }
        Ok(Self {
            from,
            to,
            duration_ms,
            easing,
        })
    }

    /// Cámara en `t_ms` (easing aplicado a la fracción, clamp en extremos).
    /// Pura, sin pánicos.
    pub fn sample(self, t_ms: u64) -> Camera {
        let f = if self.duration_ms == 0 {
            1.0
        } else {
            (t_ms.min(self.duration_ms) as f64) / (self.duration_ms as f64)
        };
        let e = self.easing.apply(f);
        let e = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.from.lerp_hacia(self.to, e)
    }

    /// El travelling como [`PropertyTrack`]s (uno por grado de libertad).
    ///
    /// `Ortho` → 4 tracks (`cam.x_min/x_max/y_min/y_max`); `Perspective` →
    /// 7 (`cam.fov`, `cam.eye_{x,y,z}`, `cam.center_{x,y,z}`), cada uno con
    /// keys `{0: from, duration: to}` y el easing del travelling. `Err`
    /// honesto si un valor no entra en `f32` (nunca con cámaras validadas).
    pub fn as_tracks(self) -> SceneResult<Vec<PropertyTrack>> {
        let mut pares: Vec<(String, f64, f64)> = Vec::new();
        match (self.from, self.to) {
            (Camera::Ortho(a), Camera::Ortho(b)) => {
                pares.push(("cam.x_min".to_string(), a.x_min, b.x_min));
                pares.push(("cam.x_max".to_string(), a.x_max, b.x_max));
                pares.push(("cam.y_min".to_string(), a.y_min, b.y_min));
                pares.push(("cam.y_max".to_string(), a.y_max, b.y_max));
            }
            (
                Camera::Perspective {
                    fov_deg: fa,
                    eye: ea,
                    center: ca,
                },
                Camera::Perspective {
                    fov_deg: fb,
                    eye: eb,
                    center: cb,
                },
            ) => {
                pares.push(("cam.fov".to_string(), fa, fb));
                for (k, nombre) in ["x", "y", "z"].iter().enumerate() {
                    pares.push((format!("cam.eye_{nombre}"), ea[k], eb[k]));
                }
                for (k, nombre) in ["x", "y", "z"].iter().enumerate() {
                    pares.push((format!("cam.center_{nombre}"), ca[k], cb[k]));
                }
            }
            _ => {
                return Err(SceneError::CamaraInvalida {
                    detalle: "from y to deben ser la misma variante".to_string(),
                });
            }
        }
        let mut tracks = Vec::with_capacity(pares.len());
        for (id, a, b) in pares {
            let a32 = a as f32;
            let b32 = b as f32;
            if !a32.is_finite() || !b32.is_finite() {
                return Err(SceneError::TrackInvalido {
                    prop: id,
                    detalle: "valor fuera del rango f32 del track".to_string(),
                });
            }
            tracks.push(PropertyTrack::try_new(
                id,
                vec![
                    TrackKey {
                        t_ms: 0,
                        value: a32,
                    },
                    TrackKey {
                        t_ms: self.duration_ms,
                        value: b32,
                    },
                ],
                self.duration_ms,
                self.easing,
            )?);
        }
        Ok(tracks)
    }
}

/// Escena Manim: cámara + capas + fondo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub camera: Ortho,
    /// 1..=32 capas en orden de dibujo (0 = fondo).
    pub layers: Vec<Mobject>,
    /// Fondo RGB 0..=255.
    pub bg: [u8; 3],
}

impl Scene {
    /// Constructor validado (todo `Err` honesto).
    pub fn try_new(camera: Ortho, layers: Vec<Mobject>, bg: [u8; 3]) -> SceneResult<Self> {
        if layers.is_empty() || layers.len() > MAX_SCENE_LAYERS {
            return Err(SceneError::EscenaInvalida {
                detalle: format!("{} capas (válido 1..={MAX_SCENE_LAYERS})", layers.len()),
            });
        }
        for (i, capa) in layers.iter().enumerate() {
            capa.validate().map_err(|e| SceneError::EscenaInvalida {
                detalle: format!("capa {i}: {e}"),
            })?;
        }
        Ok(Self { camera, layers, bg })
    }

    /// Cantidad de capas.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// ¿Vacía? (nunca tras `try_new`, pero la deserialización puede).
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

// ── Animation trait + Transform concreto ───────────────────────────────────

/// Animación estilo Manim sobre la escena.
///
/// `interpolate(alpha)` recibe el `alpha` CRUDO 0..1 y por defecto devuelve
/// el eased (`rate.apply(alpha)`); los concretos (`TransformAnim`) lo usan
/// para indexar sus frames vía [`TransformAnim::frame_at`]. `begin`/`finish`
/// dejan el estado listo/limpio (acá no-ops puros: el cerebro no toca la UI).
pub trait Animation {
    /// Duración de corrida en ms (100..=60000, paridad con `AnimDuration`, P0.1 long-form).
    fn run_time_ms(&self) -> u64;
    /// Rate-func de la animación.
    fn rate(&self) -> RateFunc;
    /// Prepara el estado inicial (no-op puro por defecto).
    fn begin(&mut self) {}
    /// Mapea `alpha` crudo 0..1 a progreso (default: `rate.apply(alpha)`).
    fn interpolate(&self, alpha: f64) -> f64 {
        self.rate().apply(alpha)
    }
    /// Limpia el estado final (no-op puro por defecto).
    fn finish(&mut self) {}
}

/// Función de camino de un `Transform` (P1-f): cómo viaja cada punto de A a B.
///
/// - `Straight`: lerp directo (comportamiento histórico, default).
/// - `Arc`: lerp + panza perpendicular `sin(π·s)·0.25·dist` (arco honesto
///   estilo Manim `path_arc`; el signo sale del giro A→B, sin pánicos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PathFunc {
    /// Lerp directo A→B (histórico).
    #[default]
    Straight,
    /// Arco con panza perpendicular (estilo Manim).
    Arc,
}

impl PathFunc {
    /// Nombre estable para wire/logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Straight => "straight",
            Self::Arc => "arc",
        }
    }
    /// Parsea por nombre (`straight`/`recto`, `arc`/`arco`); `None` honesto.
    pub fn from_name(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "straight" | "recto" | "recta" => Some(Self::Straight),
            "arc" | "arco" => Some(Self::Arc),
            _ => None,
        }
    }
    /// Interpola un punto `a→b` con fracción eased `s` 0..1 (clamp + guardia
    /// finita; no finita → `a`). Puro, sin pánicos.
    pub fn interpola(self, a: [f64; 2], b: [f64; 2], s: f64) -> [f64; 2] {
        let s = if s.is_finite() {
            s.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let x = a[0] + (b[0] - a[0]) * s;
        let y = a[1] + (b[1] - a[1]) * s;
        let (x, y) = if x.is_finite() && y.is_finite() {
            (x, y)
        } else {
            return a;
        };
        match self {
            Self::Straight => [x, y],
            Self::Arc => {
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let dist = (dx * dx + dy * dy).sqrt();
                if !dist.is_finite() || dist <= 0.0 {
                    return [x, y];
                }
                let panza = (std::f64::consts::PI * s).sin() * 0.25 * dist;
                if !panza.is_finite() {
                    return [x, y];
                }
                let nx = -dy / dist;
                let ny = dx / dist;
                let ox = x + nx * panza;
                let oy = y + ny * panza;
                if ox.is_finite() && oy.is_finite() {
                    [ox, oy]
                } else {
                    [x, y]
                }
            }
        }
    }
}

/// `Transform` concreto polilínea→polilínea (N distinto OK).
///
/// Envuelve el remuestreo arco-longitud + beziers de
/// [`matching_shapes_frames`]; `frame_at(alpha)` devuelve la polilínea
/// interpolada con el easing ya aplicado y el [`PathFunc`] (`Straight` por
/// defecto = histórico exacto; `Arc` suma la panza por punto).
#[derive(Debug, Clone)]
pub struct TransformAnim {
    from: Vec<[f64; 2]>,
    to: Vec<[f64; 2]>,
    samples: usize,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    closed: bool,
    path: PathFunc,
}

/// Par remuestreado y alineado A/B (el mismo pipeline que
/// [`matching_shapes_frames`): arco-longitud a `samples`, alineación de
/// inicio (cerrada rota al más cercano, abierta invierte si quedó al revés)
/// y suavizado bezier. Lo usan [`TransformAnim`] con `Arc` y `VMobject` sin
/// duplicar la grilla temporal. `Err` honesto con los mismos presupuestos.
#[allow(clippy::type_complexity)]
pub(crate) fn par_remuestreado(
    a: &[[f64; 2]],
    b: &[[f64; 2]],
    samples: usize,
    closed: bool,
) -> SceneResult<(Vec<[f64; 2]>, Vec<[f64; 2]>)> {
    if !(2..=512).contains(&samples) {
        return Err(SceneError::MorphInvalido {
            detalle: format!("muestras fuera de rango: pediste {samples}, usá entre 2 y 512"),
        });
    }
    valida_puntos(a, "A")?;
    valida_puntos(b, "B")?;
    if closed && (a.len() == 2 || b.len() == 2) {
        return Err(SceneError::MorphInvalido {
            detalle: "cerrada con 2 puntos: pasá 3 (o 1 si es un punto que crece)".to_string(),
        });
    }
    let ra = bezier_suaviza(&resample_arclen(a, samples, closed)?, closed);
    let rb_crudo = resample_arclen(b, samples, closed)?;
    let primero = ra.first().copied().unwrap_or([0.0, 0.0]);
    let mut rb = alinea_inicio(primero, &rb_crudo, closed, false);
    if !closed && ra.len() == rb.len() && ra.len() >= 2 {
        let ultimo = ra.len().saturating_sub(1);
        let directo = dist2(ra[0], rb[0]) + dist2(ra[ultimo], rb[ultimo]);
        let cruzado = dist2(ra[0], rb[ultimo]) + dist2(ra[ultimo], rb[0]);
        if cruzado.is_finite() && directo.is_finite() && cruzado < directo {
            rb.reverse();
        }
    }
    let rb = bezier_suaviza(&rb, closed);
    if ra.len() != rb.len() || ra.is_empty() {
        return Err(SceneError::MorphInvalido {
            detalle: "remuestreo inconsistente: probá con otras formas".to_string(),
        });
    }
    Ok((ra, rb))
}

impl TransformAnim {
    /// Constructor validado (`samples` 2..=512, `frames` 1..=48, `run`
    /// 100..=60000 ms, puntos finitos y acotados).
    pub fn try_new(
        from: Vec<[f64; 2]>,
        to: Vec<[f64; 2]>,
        samples: usize,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
    ) -> SceneResult<Self> {
        if !(2..=SCENE_MORPH_MAX_SAMPLES).contains(&samples) {
            return Err(SceneError::MorphInvalido {
                detalle: format!(
                    "muestras fuera de rango: pediste {samples}, usá entre 2 y {SCENE_MORPH_MAX_SAMPLES}"
                ),
            });
        }
        if frames == 0 || frames > crate::parametric::PARAMETRIC_MAX_FRAMES {
            return Err(SceneError::MorphInvalido {
                detalle: format!(
                    "fotogramas fuera de rango: pediste {frames}, usá entre 1 y {}",
                    crate::parametric::PARAMETRIC_MAX_FRAMES
                ),
            });
        }
        if !(100..=60_000).contains(&run_ms) {
            return Err(SceneError::MorphInvalido {
                detalle: format!("run_time {run_ms} ms fuera de 100..=60000"),
            });
        }
        valida_puntos(&from, "from")?;
        valida_puntos(&to, "to")?;
        Ok(Self {
            from,
            to,
            samples,
            frames,
            run_ms,
            rate,
            closed,
            path: PathFunc::Straight,
        })
    }

    /// Constructor validado con [`PathFunc`] explícito (misma validación que
    /// [`TransformAnim::try_new`]; `Straight` = histórico exacto).
    #[allow(clippy::too_many_arguments)]
    pub fn try_new_con_path(
        from: Vec<[f64; 2]>,
        to: Vec<[f64; 2]>,
        samples: usize,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
        path: PathFunc,
    ) -> SceneResult<Self> {
        let mut anim = Self::try_new(from, to, samples, frames, run_ms, rate, closed)?;
        anim.path = path;
        Ok(anim)
    }

    /// Camino del morph.
    pub fn path(self) -> PathFunc {
        self.path
    }

    /// Cambia el camino (puro, consume y devuelve).
    pub fn with_path(mut self, path: PathFunc) -> Self {
        self.path = path;
        self
    }

    /// Set completo de frames suavizados (`frames × samples`).
    ///
    /// Con `Straight` es idéntico a [`matching_shapes_frames`] (histórico
    /// exacto); con `Arc` interpola cada par con la panza por punto.
    pub fn frames_suavizados(&self) -> SceneResult<Vec<Vec<[f64; 2]>>> {
        self.frames_suavizados_con_path(self.path)
    }

    /// Set con camino explícito (misma grilla temporal que `frames_suavizados`).
    pub fn frames_suavizados_con_path(&self, path: PathFunc) -> SceneResult<Vec<Vec<[f64; 2]>>> {
        if path == PathFunc::Straight {
            return matching_shapes_frames(
                &self.from,
                &self.to,
                self.samples,
                self.frames,
                self.closed,
                self.rate,
            );
        }
        let (ra, rb) = par_remuestreado(&self.from, &self.to, self.samples, self.closed)?;
        let mut out: Vec<Vec<[f64; 2]>> = Vec::with_capacity(self.frames);
        for fi in 0..self.frames {
            let s = if self.frames <= 1 {
                1.0
            } else {
                (fi as f64) / ((self.frames.saturating_sub(1)) as f64)
            };
            let e = self.rate.apply(s);
            let e = if e.is_finite() {
                e.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut fila = Vec::with_capacity(ra.len());
            for (pa, pb) in ra.iter().zip(rb.iter()) {
                fila.push(path.interpola(*pa, *pb, e));
            }
            out.push(fila);
        }
        Ok(out)
    }

    /// Polilínea en `alpha` crudo 0..1 (easing aplicado + clamp a frames).
    pub fn frame_at(&self, alpha: f64) -> SceneResult<Vec<[f64; 2]>> {
        // Con `Arc` se interpola directo (evita cuantizar el arco a la grilla
        // de frames); con `Straight` se indexa el set histórico exacto.
        if self.path != PathFunc::Straight {
            let (ra, rb) = par_remuestreado(&self.from, &self.to, self.samples, self.closed)?;
            let eased = self.interpolate(alpha);
            let s = if eased.is_finite() {
                eased.clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Ok(ra
                .iter()
                .zip(rb.iter())
                .map(|(pa, pb)| self.path.interpola(*pa, *pb, s))
                .collect());
        }
        let frames = self.frames_suavizados()?;
        if frames.is_empty() {
            return Err(SceneError::MorphInvalido {
                detalle: "set vacío: revisá from/to".to_string(),
            });
        }
        let eased = self.interpolate(alpha);
        let finita = if eased.is_finite() {
            eased.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let idx = ((finita * ((frames.len().saturating_sub(1)) as f64)).round() as usize)
            .min(frames.len().saturating_sub(1));
        frames.get(idx).cloned().ok_or(SceneError::MorphInvalido {
            detalle: "índice fuera del set".to_string(),
        })
    }
}

impl Animation for TransformAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

fn valida_puntos(pts: &[[f64; 2]], cual: &str) -> SceneResult<()> {
    if pts.is_empty() || pts.len() > MAX_MOBJECT_POINTS {
        return Err(SceneError::MorphInvalido {
            detalle: format!(
                "la forma {cual} tiene {} puntos (válido 1..={MAX_MOBJECT_POINTS})",
                pts.len()
            ),
        });
    }
    for (i, p) in pts.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(SceneError::MorphInvalido {
                detalle: format!("la forma {cual} tiene un punto no finito en {i}"),
            });
        }
    }
    Ok(())
}

// ── Transform / MatchingShapes (N distinto OK, beziers cúbicos) ────────────

/// Remuestrea una polilínea a `samples` puntos equiespaciados por longitud
/// de arco (espejo de `grafito-geometry/src/morph.rs::resample_polyline`,
/// acá en `[f64;2]` para no duplicar dependencias en el núcleo de escena).
/// Acepta CUALQUIER N de entrada (1..=4096); degenerada (largo cero o 1
/// punto) → replica el primero estilo Manim punto→figura. Sin pánicos.
pub fn resample_arclen(
    pts: &[[f64; 2]],
    samples: usize,
    closed: bool,
) -> SceneResult<Vec<[f64; 2]>> {
    if !(2..=512).contains(&samples) {
        return Err(SceneError::MorphInvalido {
            detalle: format!("muestras fuera de rango: pediste {samples}, usá entre 2 y 512"),
        });
    }
    valida_puntos(pts, "forma")?;
    if pts.len() == 1 {
        return Ok(vec![pts[0]; samples]);
    }
    let n = pts.len();
    let segmentos = if closed { n } else { n.saturating_sub(1) };
    if segmentos == 0 {
        return Ok(vec![pts[0]; samples]);
    }
    let mut cum: Vec<f64> = Vec::with_capacity(segmentos.saturating_add(1));
    cum.push(0.0);
    let mut total = 0.0_f64;
    for k in 0..segmentos {
        let (p0, p1) = if closed {
            (pts[k % n], pts[(k + 1) % n])
        } else {
            (pts[k], pts[k + 1])
        };
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let d = (dx * dx + dy * dy).sqrt();
        if d.is_finite() && d > 0.0 {
            total += d;
        }
        cum.push(total);
    }
    if !total.is_finite() || total <= 0.0 {
        return Ok(vec![pts[0]; samples]);
    }
    let denom = if closed {
        samples as f64
    } else {
        (samples.saturating_sub(1)) as f64
    };
    if !denom.is_finite() || denom <= 0.0 {
        return Err(SceneError::MorphInvalido {
            detalle: format!("muestras fuera de rango: {samples}"),
        });
    }
    let mut out = Vec::with_capacity(samples);
    let mut seg = 0_usize;
    for j in 0..samples {
        let objetivo = total * (j as f64) / denom;
        while seg + 1 < cum.len() && cum[seg + 1] < objetivo {
            seg = seg.saturating_add(1);
        }
        let seg_idx = seg.min(segmentos.saturating_sub(1));
        let base = cum[seg_idx];
        let tope = cum[seg_idx + 1];
        let largo = tope - base;
        let u = if largo.is_finite() && largo > 0.0 {
            ((objetivo - base) / largo).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (p0, p1) = if closed {
            (pts[seg_idx % n], pts[(seg_idx + 1) % n])
        } else {
            (pts[seg_idx], pts[seg_idx + 1])
        };
        let x = p0[0] + (p1[0] - p0[0]) * u;
        let y = p0[1] + (p1[1] - p0[1]) * u;
        if x.is_finite() && y.is_finite() {
            out.push([x, y]);
        } else {
            out.push(p0);
        }
    }
    Ok(out)
}

/// Suaviza una polilínea con beziers cúbicos (B-spline cúbica, un paso,
/// preserva N): `q[i] = (p[i-1] + 4·p[i] + p[i+1]) / 6` (extremos
/// clampados si abierta; vecinos circulares si cerrada). Pura, sin pánicos;
/// puntos no finitos se conservan tal cual (no inventa).
pub fn bezier_suaviza(pts: &[[f64; 2]], closed: bool) -> Vec<[f64; 2]> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b, c) = if closed {
            (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n])
        } else if i == 0 {
            (pts[0], pts[0], pts[1])
        } else if i + 1 >= n {
            (pts[n - 2], pts[n - 1], pts[n - 1])
        } else {
            (pts[i - 1], pts[i], pts[i + 1])
        };
        let x = (a[0] + 4.0 * b[0] + c[0]) / 6.0;
        let y = (a[1] + 4.0 * b[1] + c[1]) / 6.0;
        if x.is_finite() && y.is_finite() {
            out.push([x, y]);
        } else {
            out.push(b);
        }
    }
    out
}

fn alinea_inicio(a0: [f64; 2], b: &[[f64; 2]], closed: bool, abierta_ok: bool) -> Vec<[f64; 2]> {
    if b.len() < 2 {
        return b.to_vec();
    }
    if closed {
        let mut mejor = 0_usize;
        let mut mejor_d = f64::INFINITY;
        for (k, p) in b.iter().enumerate() {
            let dx = p[0] - a0[0];
            let dy = p[1] - a0[1];
            let d = dx * dx + dy * dy;
            if d.is_finite() && d < mejor_d {
                mejor_d = d;
                mejor = k;
            }
        }
        (0..b.len()).map(|i| b[(mejor + i) % b.len()]).collect()
    } else if abierta_ok {
        let ultimo = b.len().saturating_sub(1);
        let directo = dist2(a0, b[0]) + dist2(a0, b[ultimo]);
        let _ = directo;
        b.to_vec()
    } else {
        b.to_vec()
    }
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let d = dx * dx + dy * dy;
    if d.is_finite() {
        d
    } else {
        f64::INFINITY
    }
}

/// `MatchingShapes` honesto: interpola CUALQUIER par de formas con N
/// distinto (las remuestrea a `samples` por arco-longitud, alinea el inicio
/// y suaviza con beziers cúbicos) y devuelve `frames` fotogramas con
/// `easing` aplicado.
///
/// Presupuestos: `samples` 2..=512, `frames` 1..=48, entradas 1..=4096
/// puntos finitos. El frame 0 es A remuestreada+suave y el último es B
/// (salvo `Wiggle`/`ThereAndBack`, que vuelven a propósito). Sin pánicos.
pub fn matching_shapes_frames(
    a: &[[f64; 2]],
    b: &[[f64; 2]],
    samples: usize,
    frames: usize,
    closed: bool,
    easing: RateFunc,
) -> SceneResult<Vec<Vec<[f64; 2]>>> {
    if !(2..=512).contains(&samples) {
        return Err(SceneError::MorphInvalido {
            detalle: format!("muestras fuera de rango: pediste {samples}, usá entre 2 y 512"),
        });
    }
    if frames == 0 || frames > crate::parametric::PARAMETRIC_MAX_FRAMES {
        return Err(SceneError::MorphInvalido {
            detalle: format!(
                "fotogramas fuera de rango: pediste {frames}, usá entre 1 y {}",
                crate::parametric::PARAMETRIC_MAX_FRAMES
            ),
        });
    }
    valida_puntos(a, "A")?;
    valida_puntos(b, "B")?;
    if closed && (a.len() == 2 || b.len() == 2) {
        return Err(SceneError::MorphInvalido {
            detalle: "cerrada con 2 puntos: pasá 3 (o 1 si es un punto que crece)".to_string(),
        });
    }
    let ra = bezier_suaviza(&resample_arclen(a, samples, closed)?, closed);
    let rb_crudo = resample_arclen(b, samples, closed)?;
    // Alineación de inicio: cerrada rota B al punto más cercano a A[0];
    // abierta invierte B si quedó al revés (comparación por extremos).
    let mut rb = alinea_inicio(
        ra.first().copied().unwrap_or([0.0, 0.0]),
        &rb_crudo,
        closed,
        false,
    );
    if !closed && ra.len() == rb.len() && ra.len() >= 2 {
        let ultimo = ra.len().saturating_sub(1);
        let directo = dist2(ra[0], rb[0]) + dist2(ra[ultimo], rb[ultimo]);
        let cruzado = dist2(ra[0], rb[ultimo]) + dist2(ra[ultimo], rb[0]);
        if cruzado.is_finite() && directo.is_finite() && cruzado < directo {
            rb.reverse();
        }
    }
    let rb = bezier_suaviza(&rb, closed);
    if ra.len() != rb.len() || ra.is_empty() {
        return Err(SceneError::MorphInvalido {
            detalle: "remuestreo inconsistente: probá con otras formas".to_string(),
        });
    }
    // Presupuesto del set con `checked` (samples*frames*16).
    let bytes = samples.checked_mul(frames).and_then(|v| v.checked_mul(16));
    match bytes {
        Some(got) if got <= 1024 * 1024 => {}
        _ => {
            return Err(SceneError::PresupuestoExcedido {
                detalle: "el set del morph excede 1 MiB: bajá muestras o fotogramas".to_string(),
            });
        }
    }
    let mut out: Vec<Vec<[f64; 2]>> = Vec::with_capacity(frames);
    for fi in 0..frames {
        let s = if frames <= 1 {
            1.0
        } else {
            (fi as f64) / ((frames.saturating_sub(1)) as f64)
        };
        let e = easing.apply(s);
        let mut fila = Vec::with_capacity(ra.len());
        for (pa, pb) in ra.iter().zip(rb.iter()) {
            let x = pa[0] + (pb[0] - pa[0]) * e;
            let y = pa[1] + (pb[1] - pa[1]) * e;
            if x.is_finite() && y.is_finite() {
                fila.push([x, y]);
            } else {
                fila.push(*pa);
            }
        }
        out.push(fila);
    }
    Ok(out)
}

// ── Timeline por propiedad + reloj ─────────────────────────────────────────

/// Tope de keys por track (paridad con `MAX_TIMELINE_KEYFRAMES`).
pub const MAX_TRACK_KEYS: usize = 64;
/// Duración máxima de un track (paridad con `MAX_TIMELINE_DURATION_MS`, P0.1 long-form: 60 s).
pub const MAX_TRACK_DURATION_MS: u64 = 60_000;

/// Un key en `t_ms` con `value`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackKey {
    pub t_ms: u64,
    pub value: f32,
}

/// Track de UNA propiedad (`prop_id` estable, p. ej. `opacity`, `x`, `run`).
/// Cada track tiene su propio easing (no global): el sample aplica
/// `easing` a la fracción del segmento antes del lerp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyTrack {
    /// Id 1..=64 (`[A-Za-z0-9_.-]`).
    pub prop_id: String,
    /// 1..=64 keys estrictamente crecientes en `0..=duration_ms`.
    pub keys: Vec<TrackKey>,
    /// Duración del track en ms (1..=60000, P0.1 long-form).
    pub duration_ms: u64,
    /// Easing propio del track.
    pub easing: RateFunc,
}

impl PropertyTrack {
    /// Constructor validado (todo `Err` honesto).
    pub fn try_new(
        prop_id: String,
        keys: Vec<TrackKey>,
        duration_ms: u64,
        easing: RateFunc,
    ) -> SceneResult<Self> {
        let id = prop_id.trim().to_string();
        if id.is_empty()
            || id.len() > 64
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        {
            return Err(SceneError::TrackInvalido {
                prop: prop_id,
                detalle: "prop_id 1..=64 ([A-Za-z0-9_.-])".to_string(),
            });
        }
        if duration_ms == 0 || duration_ms > MAX_TRACK_DURATION_MS {
            return Err(SceneError::TrackInvalido {
                prop: id,
                detalle: format!("duración {duration_ms} fuera de 1..={MAX_TRACK_DURATION_MS}"),
            });
        }
        if keys.is_empty() || keys.len() > MAX_TRACK_KEYS {
            return Err(SceneError::TrackInvalido {
                prop: id,
                detalle: format!("{} keys (válido 1..={MAX_TRACK_KEYS})", keys.len()),
            });
        }
        let mut prev: Option<u64> = None;
        for k in &keys {
            if !k.value.is_finite() {
                return Err(SceneError::TrackInvalido {
                    prop: id.clone(),
                    detalle: "valor no finito".to_string(),
                });
            }
            if k.t_ms > duration_ms {
                return Err(SceneError::TrackInvalido {
                    prop: id.clone(),
                    detalle: format!("t={} excede duración {duration_ms}", k.t_ms),
                });
            }
            if let Some(p) = prev {
                if k.t_ms <= p {
                    return Err(SceneError::TrackInvalido {
                        prop: id.clone(),
                        detalle: "t debe ser estrictamente creciente".to_string(),
                    });
                }
            }
            prev = Some(k.t_ms);
        }
        Ok(Self {
            prop_id: id,
            keys,
            duration_ms,
            easing,
        })
    }

    /// Muestra el valor en `t_ms` (easing del track aplicado a la fracción
    /// del segmento; clamp en extremos; vacío→0.0 imposible tras `try_new`
    /// pero total ante deserialización). Nunca panics.
    pub fn sample(&self, t_ms: u64) -> f32 {
        let keys = &self.keys;
        if keys.is_empty() {
            return 0.0;
        }
        if keys.len() == 1 {
            return keys[0].value;
        }
        if t_ms <= keys[0].t_ms {
            return keys[0].value;
        }
        if let Some(last) = keys.last() {
            if t_ms >= last.t_ms {
                return last.value;
            }
        }
        for pair in keys.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if t_ms >= a.t_ms && t_ms <= b.t_ms {
                let span = b.t_ms.saturating_sub(a.t_ms);
                if span == 0 {
                    return a.value;
                }
                let f = (t_ms.saturating_sub(a.t_ms) as f32) / (span as f32);
                let e = self.easing.apply_f32(f);
                let e = if e.is_finite() {
                    e.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return a.value + (b.value - a.value) * e;
            }
        }
        keys.last().map_or(0.0, |k| k.value)
    }
}

/// Reloj de reproducción (tiempo global + velocidad).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    /// Tiempo global vivido en ms.
    pub t_global: u64,
    /// Velocidad (0 = pausa, 1 = normal, hasta 8×; finita).
    pub rate: f64,
}

impl Clock {
    /// Constructor validado (`rate` finita 0..=8).
    pub fn try_new(t_global: u64, rate: f64) -> SceneResult<Self> {
        if !rate.is_finite() || !(0.0..=8.0).contains(&rate) {
            return Err(SceneError::RelojInvalido {
                detalle: format!("rate {rate} fuera de 0..=8"),
            });
        }
        Ok(Self { t_global, rate })
    }

    /// Avanza `dt_ms` reales a `rate` actual (saturado, sin pánicos).
    pub fn tick(&mut self, dt_ms: u64) {
        let scaled = (f64::from(dt_ms as u32) * self.rate).round();
        let paso = if !scaled.is_finite() || scaled <= 0.0 {
            0
        } else if scaled >= u64::MAX as f64 {
            u64::MAX
        } else {
            scaled as u64
        };
        // `dt` mayor a u32 se procesa por tramos para no perder tiempo.
        let resto = dt_ms.saturating_sub(u32::MAX as u64);
        let extra = if resto > 0 && self.rate > 0.0 {
            let e = (resto as f64) * self.rate;
            if !e.is_finite() || e <= 0.0 {
                0
            } else if e >= u64::MAX as f64 {
                u64::MAX
            } else {
                e.round() as u64
            }
        } else {
            0
        };
        self.t_global = self.t_global.saturating_add(paso).saturating_add(extra);
    }
}

// ── Reuso de Playlist (Succession) y scrub global ──────────────────────────

/// Agenda secuencial reutilizando [`crate::protocol::Playlist::schedule`].
pub fn schedule_playlist(
    playlist: &crate::protocol::Playlist,
) -> Vec<crate::protocol::ScheduledStep> {
    playlist.schedule()
}

/// Mapea tiempo global → `(step, t_local)` reutilizando
/// [`crate::protocol::Playlist::sample_at`].
pub fn sample_playlist(
    playlist: &crate::protocol::Playlist,
    global_ms: u64,
) -> Option<(usize, u64)> {
    playlist.sample_at(global_ms)
}

/// Scrub global reutilizando [`crate::protocol::playlist_frame_at`].
pub fn frame_at_global(
    timeline: &crate::protocol::Timeline,
    t_ms: u64,
    total_frames: usize,
) -> usize {
    crate::protocol::playlist_frame_at(timeline, t_ms, total_frames)
}

// ── Taylor: orden animado 1..=7 desde `terms` ──────────────────────────────

/// Orden mínimo del `taylor-series` animado.
pub const TAYLOR_ANIM_ORDER_MIN: usize = 1;
/// Orden máximo del `taylor-series` animado (el slider vivo llega a 10;
/// la animación corta en 7 para que el set de 48 frames siga legible).
pub const TAYLOR_ANIM_ORDER_MAX: usize = 7;
/// Orden histórico cuando `terms` falta o no es finito.
pub const TAYLOR_ANIM_ORDER_DEFAULT: usize = 3;

/// Orden animado desde un `terms` crudo (`None`/NaN/inf → 3; trunca como
/// el slider y clampa a 1..=7). Puro, sin pánicos.
pub fn taylor_anim_order_from_terms(terms: Option<f64>) -> usize {
    match terms {
        Some(v) if v.is_finite() => {
            (v as usize).clamp(TAYLOR_ANIM_ORDER_MIN, TAYLOR_ANIM_ORDER_MAX)
        }
        _ => TAYLOR_ANIM_ORDER_DEFAULT,
    }
}

/// Orden animado desde `AnimRequest.params["terms"]` (ausente → 3).
/// Es lo que `render_taylor_frames_inner` (W3) debe leer para dibujar
/// `P_n` con el orden pedido en vez del fade fijo histórico.
pub fn taylor_anim_order_from_params(params: &BTreeMap<String, f64>) -> usize {
    taylor_anim_order_from_terms(params.get(crate::protocol::SCENE_PARAM_TERMS).copied())
}

#[cfg(test)]
mod scene_f1_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn rate_funcs_extremos_y_nombres() {
        for r in [
            RateFunc::Linear,
            RateFunc::Smooth,
            RateFunc::EaseInOut,
            RateFunc::RushInOut,
            RateFunc::RushInto,
            RateFunc::RushFrom,
            RateFunc::SlowInto,
            RateFunc::DoubleSmooth,
            RateFunc::Squish,
            RateFunc::Lingering,
            RateFunc::RunningStart,
            RateFunc::SmootherStep,
        ] {
            assert!((r.apply(0.0)).abs() < 1e-12, "{r:?}");
            assert!((r.apply(1.0) - 1.0).abs() < 1e-9, "{r:?}");
            assert_eq!(RateFunc::from_name(r.as_str()), Some(r));
        }
        // Vuelven a 0 por diseño (`@zero` upstream): meseta en 1 en el medio.
        assert_eq!(
            RateFunc::from_name(RateFunc::ThereAndBackWithPause.as_str()),
            Some(RateFunc::ThereAndBackWithPause)
        );
        assert!((RateFunc::ThereAndBackWithPause.apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::ThereAndBackWithPause.apply(1.0)).abs() < 1e-9);
        assert!((RateFunc::ThereAndBackWithPause.apply(0.5) - 1.0).abs() < 1e-12);
        // No-llegan-a-1 por diseño upstream: NotQuiteThere→0.7,
        // ExponentialDecay→≈0.99995.
        assert!((RateFunc::NotQuiteThere.apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::NotQuiteThere.apply(1.0) - 0.7).abs() < 1e-9);
        assert!((RateFunc::ExponentialDecay.apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::ExponentialDecay.apply(1.0) - 0.999_954_600_070_237_5).abs() < 1e-9);
        assert_eq!(
            RateFunc::from_name(RateFunc::NotQuiteThere.as_str()),
            Some(RateFunc::NotQuiteThere)
        );
        assert_eq!(
            RateFunc::from_name(RateFunc::ExponentialDecay.as_str()),
            Some(RateFunc::ExponentialDecay)
        );
        // Ida y vuelta / wiggle vuelven a 0.
        assert!((RateFunc::ThereAndBack.apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::ThereAndBack.apply(1.0)).abs() < 1e-9);
        assert!((RateFunc::ThereAndBack.apply(0.5) - 1.0).abs() < 1e-12);
        assert!((RateFunc::Wiggle.apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::Wiggle.apply(1.0)).abs() < 1e-9);
        // No finitos nunca panics.
        assert_eq!(RateFunc::Smooth.apply(f64::NAN), 0.0);
        assert_eq!(RateFunc::EaseInOut.apply(f64::INFINITY), 0.0);
    }

    #[test]
    fn rate_unifica_los_tres_vocabulario() {
        // Las 18 Manim (6 históricas + 7 P1-core + 5 P0 exactas upstream).
        for nombre in [
            "linear",
            "smooth",
            "ease_in_out",
            "rush_in_out",
            "there_and_back",
            "wiggle",
            "rush_into",
            "rush_from",
            "slow_into",
            "double_smooth",
            "squish",
            "lingering",
            "wiggle_k",
            "there_and_back_with_pause",
            "running_start",
            "not_quite_there",
            "exponential_decay",
            "smootherstep",
        ] {
            assert!(RateFunc::from_name(nombre).is_some(), "{nombre}");
        }
        // Alias smoothstep/smootherstep (P0-e): smoothstep es Smooth exacto.
        assert_eq!(RateFunc::from_name("smoothstep"), Some(RateFunc::Smooth));
        assert_eq!(
            RateFunc::from_name("smootherstep"),
            Some(RateFunc::SmootherStep)
        );
        // Los 8 legacy de EASING_NAMES.
        for nombre in crate::protocol::EASING_NAMES {
            assert!(RateFunc::from_name(nombre).is_some(), "{nombre}");
        }
        // Exactos legacy solo donde hay exacto.
        assert_eq!(RateFunc::Linear.legacy_name(), Some("linear"));
        assert_eq!(RateFunc::Smooth.legacy_name(), Some("sin_in_out"));
        assert_eq!(RateFunc::EaseInOut.legacy_name(), Some("cubic_in_out"));
        assert_eq!(RateFunc::RushInOut.legacy_name(), None);
        assert_eq!(RateFunc::ThereAndBack.legacy_name(), None);
        assert_eq!(RateFunc::Wiggle.legacy_name(), None);
        // P1-core + P0 nuevas: ninguna tiene equivalente legacy exacto.
        for r in [
            RateFunc::RushInto,
            RateFunc::RushFrom,
            RateFunc::SlowInto,
            RateFunc::DoubleSmooth,
            RateFunc::Squish,
            RateFunc::Lingering,
            RateFunc::WiggleK(2),
            RateFunc::ThereAndBackWithPause,
            RateFunc::RunningStart,
            RateFunc::NotQuiteThere,
            RateFunc::ExponentialDecay,
            RateFunc::SmootherStep,
        ] {
            assert_eq!(r.legacy_name(), None, "{r:?}");
        }
        // rush_into/from son variantes propias (ya no alias de RushInOut).
        assert_eq!(RateFunc::from_name("rush_into"), Some(RateFunc::RushInto));
        assert_eq!(RateFunc::from_name("rush_from"), Some(RateFunc::RushFrom));
        assert_eq!(RateFunc::from_name("wiggle_k"), Some(RateFunc::WiggleK(2)));
        assert_eq!(RateFunc::from_name("wiggle2"), Some(RateFunc::WiggleK(2)));
        // wiggle histórico intacto (decay propio, no upstream).
        assert_eq!(RateFunc::from_name("wiggle"), Some(RateFunc::Wiggle));
        assert!(RateFunc::wiggle_k(0).is_none());
        assert!(RateFunc::wiggle_k(17).is_none());
        assert_eq!(RateFunc::wiggle_k(3), Some(RateFunc::WiggleK(3)));
        assert!(RateFunc::from_name("no-existe").is_none());
    }

    #[test]
    fn rate_p1_valores_exactos_upstream() {
        // Valores de `manim/utils/rate_functions.py` (defaults:
        // inflection=10, wiggles=2, squish 0.4/0.6). Tolerancia 1e-9.
        let cerca = |got: f64, esperado: f64| {
            assert!(
                (got - esperado).abs() < 1e-9,
                "got {got} esperado {esperado}"
            );
        };
        // rush_into(t) = 2·smooth(t/2).
        cerca(RateFunc::RushInto.apply(0.0), 0.0);
        cerca(RateFunc::RushInto.apply(0.5), 0.1402074330902163);
        cerca(RateFunc::RushInto.apply(1.0), 1.0);
        // rush_from(t) = 2·smooth(t/2+0.5)-1.
        cerca(RateFunc::RushFrom.apply(0.0), 0.0);
        cerca(RateFunc::RushFrom.apply(0.5), 0.8597925669097839);
        cerca(RateFunc::RushFrom.apply(1.0), 1.0);
        // slow_into(t) = sqrt(1-(1-t)²).
        cerca(RateFunc::SlowInto.apply(0.0), 0.0);
        cerca(RateFunc::SlowInto.apply(0.5), 0.8660254037844386);
        cerca(RateFunc::SlowInto.apply(1.0), 1.0);
        // double_smooth: 0.25→0.25, 0.5→0.5, 0.75→0.75 exactos.
        cerca(RateFunc::DoubleSmooth.apply(0.0), 0.0);
        cerca(RateFunc::DoubleSmooth.apply(0.25), 0.25);
        cerca(RateFunc::DoubleSmooth.apply(0.5), 0.5);
        cerca(RateFunc::DoubleSmooth.apply(0.75), 0.75);
        cerca(RateFunc::DoubleSmooth.apply(1.0), 1.0);
        // squish(smooth, 0.4, 0.6): 0 antes, 1 después, 0.5 en el medio.
        assert_eq!(RateFunc::Squish.apply(0.2), 0.0);
        cerca(RateFunc::Squish.apply(0.5), 0.5);
        assert_eq!(RateFunc::Squish.apply(0.8), 1.0);
        // lingering(t) = min(t/0.8, 1).
        cerca(RateFunc::Lingering.apply(0.4), 0.5);
        cerca(RateFunc::Lingering.apply(0.8), 1.0);
        assert_eq!(RateFunc::Lingering.apply(1.0), 1.0);
        // wiggle(k=2): there_and_back·sin(2πt): 0.25→0.5, extremos 0.
        cerca(RateFunc::WiggleK(2).apply(0.25), 0.5);
        assert!((RateFunc::WiggleK(2).apply(0.0)).abs() < 1e-12);
        assert!((RateFunc::WiggleK(2).apply(1.0)).abs() < 1e-9);
        assert!((RateFunc::WiggleK(2).apply(0.5)).abs() < 1e-9);
        // wiggle(k=3) en 0.25: 0.5·sin(3π/4).
        cerca(RateFunc::WiggleK(3).apply(0.25), 0.3535533905932738);
        // P0-e exactas upstream (defaults: pause 1/3, pull -0.5,
        // proportion 0.7, half_life 0.1). Tolerancia 1e-9.
        cerca(
            RateFunc::ThereAndBackWithPause.apply(0.25),
            0.9298962834548917,
        );
        assert_eq!(RateFunc::ThereAndBackWithPause.apply(0.5), 1.0);
        cerca(
            RateFunc::ThereAndBackWithPause.apply(0.75),
            0.929896283454892,
        );
        assert!((RateFunc::ThereAndBackWithPause.apply(1.0)).abs() < 1e-9);
        cerca(RateFunc::RunningStart.apply(0.25), -0.1766357421875);
        cerca(RateFunc::RunningStart.apply(0.5), 0.0703125);
        cerca(RateFunc::RunningStart.apply(0.75), 0.7481689453125);
        cerca(RateFunc::RunningStart.apply(1.0), 1.0);
        cerca(RateFunc::NotQuiteThere.apply(0.5), 0.35);
        cerca(RateFunc::NotQuiteThere.apply(1.0), 0.7);
        cerca(RateFunc::ExponentialDecay.apply(0.25), 0.9179150013761012);
        cerca(RateFunc::ExponentialDecay.apply(0.5), 0.9932620530009145);
        cerca(RateFunc::SmootherStep.apply(0.25), 0.103515625);
        cerca(RateFunc::SmootherStep.apply(0.5), 0.5);
        cerca(RateFunc::SmootherStep.apply(0.75), 0.896484375);
        // Nombres canónicos del wire.
        assert_eq!(RateFunc::RushInto.as_str(), "rush_into");
        assert_eq!(RateFunc::SlowInto.as_str(), "slow_into");
        assert_eq!(RateFunc::DoubleSmooth.as_str(), "double_smooth");
        assert_eq!(RateFunc::Squish.as_str(), "squish");
        assert_eq!(RateFunc::Lingering.as_str(), "lingering");
        assert_eq!(RateFunc::WiggleK(2).as_str(), "wiggle_k");
        // No finitos nunca panics.
        assert_eq!(RateFunc::RushInto.apply(f64::NAN), 0.0);
        assert_eq!(RateFunc::WiggleK(2).apply(f64::INFINITY), 0.0);
    }

    #[test]
    fn mobject_p1_figuras_y_campos_validan() {
        Mobject::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
        }
        .validate()
        .unwrap();
        assert!(Mobject::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 0.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Circle {
            cx: f64::NAN,
            cy: 0.0,
            r: 1.0
        }
        .validate()
        .is_err());
        Mobject::Square {
            cx: 1.0,
            cy: 2.0,
            side: 3.0,
        }
        .validate()
        .unwrap();
        assert!(Mobject::Square {
            cx: 0.0,
            cy: 0.0,
            side: f64::INFINITY
        }
        .validate()
        .is_err());
        Mobject::Line {
            from: [0.0, 0.0],
            to: [1.0, 1.0],
        }
        .validate()
        .unwrap();
        // Extremos idénticos → Dot honesto.
        assert!(Mobject::Line {
            from: [1.0, 1.0],
            to: [1.0, 1.0]
        }
        .validate()
        .is_err());
        Mobject::Arrow {
            from: [0.0, 0.0],
            to: [2.0, 0.0],
        }
        .validate()
        .unwrap();
        assert!(Mobject::Arrow {
            from: [0.0, f64::NAN],
            to: [1.0, 0.0]
        }
        .validate()
        .is_err());
        Mobject::NumberPlane {
            x_min: -4.0,
            x_max: 4.0,
            y_min: -3.0,
            y_max: 3.0,
            x_step: 1.0,
            y_step: 1.0,
        }
        .validate()
        .unwrap();
        // Paso mayor que el rango o divisiones > 64 → Err.
        assert!(Mobject::NumberPlane {
            x_min: 0.0,
            x_max: 1.0,
            y_min: 0.0,
            y_max: 1.0,
            x_step: 2.0,
            y_step: 0.5,
        }
        .validate()
        .is_err());
        assert!(Mobject::NumberPlane {
            x_min: 0.0,
            x_max: 100.0,
            y_min: 0.0,
            y_max: 1.0,
            x_step: 0.5,
            y_step: 0.5,
        }
        .validate()
        .is_err());
        Mobject::VectorField {
            func: "x + y".to_string(),
            nx: 8,
            ny: 8,
        }
        .validate()
        .unwrap();
        assert!(Mobject::VectorField {
            func: String::new(),
            nx: 8,
            ny: 8
        }
        .validate()
        .is_err());
        assert!(Mobject::VectorField {
            func: "x".to_string(),
            nx: 65,
            ny: 1
        }
        .validate()
        .is_err());
        // Tex sin LaTeX: texto plano → SVG escapado válido.
        let tex = Mobject::tex_desde_texto("hola & <mundo>").unwrap();
        match &tex {
            Mobject::Tex { svg } => {
                assert!(svg.contains("&amp;"));
                assert!(svg.contains("&lt;mundo&gt;"));
            }
            otro => panic!("esperaba Tex, got {otro:?}"),
        }
        assert!(Mobject::tex_desde_texto("   ").is_err());
        // Cotas Group intactas con figuras nuevas adentro.
        let grupo = Mobject::Group(vec![
            Mobject::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 1.0,
            },
            Mobject::Arrow {
                from: [0.0, 0.0],
                to: [1.0, 0.0],
            },
        ]);
        grupo.validate().unwrap();
        let muchos = vec![Mobject::Dot { x: 0.0, y: 0.0 }; MAX_GROUP_CHILDREN + 1];
        assert!(Mobject::Group(muchos).validate().is_err());
    }

    #[test]
    fn p3_rect_ellipse_arc_validan_finitos_positivos_y_cotas() {
        // Válidos canónicos.
        Mobject::Rectangle {
            cx: 0.0,
            cy: 0.0,
            w: 4.0,
            h: 2.0,
        }
        .validate()
        .unwrap();
        Mobject::Ellipse {
            cx: 1.0,
            cy: -1.0,
            rx: 2.0,
            ry: 1.0,
        }
        .validate()
        .unwrap();
        Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: 0.0,
            end_rad: std::f64::consts::PI,
        }
        .validate()
        .unwrap();
        // Vuelta completa exacta (barrido = 2π) pasa.
        Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: 0.5,
            end_rad: 0.5 + std::f64::consts::TAU,
        }
        .validate()
        .unwrap();
        // Centros no finitos.
        assert!(Mobject::Rectangle {
            cx: f64::NAN,
            cy: 0.0,
            w: 1.0,
            h: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Ellipse {
            cx: 0.0,
            cy: f64::INFINITY,
            rx: 1.0,
            ry: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: f64::NAN,
            end_rad: 1.0
        }
        .validate()
        .is_err());
        // No positivos / cero.
        assert!(Mobject::Rectangle {
            cx: 0.0,
            cy: 0.0,
            w: 0.0,
            h: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Ellipse {
            cx: 0.0,
            cy: 0.0,
            rx: -2.0,
            ry: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 0.0,
            start_rad: 0.0,
            end_rad: 1.0
        }
        .validate()
        .is_err());
        // Cota max 4096 (más allá falla aunque sea finita).
        assert!(Mobject::Rectangle {
            cx: 0.0,
            cy: 0.0,
            w: 4097.0,
            h: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Ellipse {
            cx: 0.0,
            cy: 0.0,
            rx: 1.0,
            ry: 5000.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1e7,
            start_rad: 0.0,
            end_rad: 1.0
        }
        .validate()
        .is_err());
        // Arco: end <= start y barrido > 2π fallan honesto.
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: 1.0,
            end_rad: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: 2.0,
            end_rad: 1.0
        }
        .validate()
        .is_err());
        assert!(Mobject::Arc {
            cx: 0.0,
            cy: 0.0,
            r: 1.0,
            start_rad: 0.0,
            end_rad: std::f64::consts::TAU + 0.1
        }
        .validate()
        .is_err());
    }

    #[test]
    fn camara_proyecta_y_viaja_sobre_tracks() {
        // Ortho: descarta z.
        let orto = Camera::Ortho(Ortho::default_16_9());
        assert!(orto.is_ortho());
        assert_eq!(orto.project_3d([3.0, 4.0, 99.0]), Some([3.0, 4.0]));
        assert_eq!(orto.project_3d([0.0, 0.0, f64::NAN]), None);
        // Perspectiva canónica: eye=(0,0,5) mira al origen, fov 90.
        let persp = Camera::perspective(90.0, [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]).unwrap();
        assert!(!persp.is_ortho());
        // El centro proyecta al origen 2D.
        let c = persp.project_3d([0.0, 0.0, 0.0]).unwrap();
        assert!(c[0].abs() < 1e-12 && c[1].abs() < 1e-12);
        // Un punto a la derecha cae en +x (tan(45°)=1, prof=5 → x=1/5).
        let d = persp.project_3d([1.0, 0.0, 0.0]).unwrap();
        assert!((d[0] - 0.2).abs() < 1e-12, "got {d:?}");
        assert!(d[1].abs() < 1e-12);
        // Detrás de la cámara → None honesto.
        assert_eq!(persp.project_3d([0.0, 0.0, 6.0]), None);
        // Malformadas honestas.
        assert!(Camera::perspective(0.0, [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]).is_err());
        assert!(Camera::perspective(200.0, [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]).is_err());
        assert!(Camera::perspective(90.0, [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]).is_err());
        // MovingCamera ortho: sample interpola, tracks son 4 PropertyTrack.
        let travel = MovingCamera::try_new(
            Camera::Ortho(Ortho::try_new(-8.0, 8.0, -4.5, 4.5).unwrap()),
            Camera::Ortho(Ortho::try_new(-4.0, 4.0, -2.25, 2.25).unwrap()),
            1000,
            RateFunc::Linear,
        )
        .unwrap();
        match travel.sample(0) {
            Camera::Ortho(o) => assert_eq!(o.x_min, -8.0),
            otro => panic!("esperaba Ortho, got {otro:?}"),
        }
        match travel.sample(500) {
            Camera::Ortho(o) => assert!((o.x_min + 6.0).abs() < 1e-9, "got {o:?}"),
            otro => panic!("esperaba Ortho, got {otro:?}"),
        }
        match travel.sample(99_999) {
            Camera::Ortho(o) => assert_eq!(o.x_min, -4.0),
            otro => panic!("esperaba Ortho, got {otro:?}"),
        }
        let tracks = travel.as_tracks().unwrap();
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0].prop_id, "cam.x_min");
        assert_eq!(tracks[0].sample(0), -8.0);
        assert_eq!(tracks[0].sample(1000), -4.0);
        // MovingCamera perspectiva: 7 tracks.
        let travel3d = MovingCamera::try_new(
            Camera::perspective(90.0, [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]).unwrap(),
            Camera::perspective(60.0, [0.0, 0.0, 8.0], [0.0, 0.0, 0.0]).unwrap(),
            2000,
            RateFunc::Smooth,
        )
        .unwrap();
        assert_eq!(travel3d.as_tracks().unwrap().len(), 7);
        // Mezcla de variantes y duración 0 → Err.
        assert!(MovingCamera::try_new(orto, persp, 1000, RateFunc::Linear).is_err());
        assert!(MovingCamera::try_new(orto, orto, 0, RateFunc::Linear).is_err());
    }

    #[test]
    fn mobject_y_escena_validan_presupuesto() {
        Mobject::Axes.validate().unwrap();
        Mobject::FunctionGraph {
            expr: "sin(x)".to_string(),
        }
        .validate()
        .unwrap();
        assert!(Mobject::FunctionGraph {
            expr: String::new()
        }
        .validate()
        .is_err());
        assert!(Mobject::FunctionGraph {
            expr: "x".repeat(2001)
        }
        .validate()
        .is_err());
        assert!(Mobject::Dot {
            x: f64::NAN,
            y: 0.0
        }
        .validate()
        .is_err());
        assert!(Mobject::ArrowField { nx: 0, ny: 4 }.validate().is_err());
        assert!(Mobject::ArrowField { nx: 65, ny: 1 }.validate().is_err());
        assert!(Mobject::Tex { svg: String::new() }.validate().is_err());
        let escena = Scene::try_new(
            Ortho::default_16_9(),
            vec![Mobject::Axes, Mobject::Dot { x: 1.0, y: 2.0 }],
            [10, 12, 16],
        )
        .unwrap();
        assert_eq!(escena.len(), 2);
        assert!(!escena.is_empty());
        assert!(Ortho::try_new(1.0, 1.0, 0.0, 1.0).is_err());
        assert!(Ortho::try_new(0.0, 1.0, 0.0, f64::INFINITY).is_err());
        assert!(Scene::try_new(Ortho::default_16_9(), vec![], [0, 0, 0]).is_err());
        let muchas = vec![Mobject::Axes; MAX_SCENE_LAYERS + 1];
        assert!(Scene::try_new(Ortho::default_16_9(), muchas, [0, 0, 0]).is_err());
    }

    #[test]
    fn track_con_easing_propio_no_global() {
        let lineal = PropertyTrack::try_new(
            "x".to_string(),
            vec![
                TrackKey {
                    t_ms: 0,
                    value: 0.0,
                },
                TrackKey {
                    t_ms: 1000,
                    value: 10.0,
                },
            ],
            1000,
            RateFunc::Linear,
        )
        .unwrap();
        let suave = PropertyTrack::try_new(
            "x".to_string(),
            vec![
                TrackKey {
                    t_ms: 0,
                    value: 0.0,
                },
                TrackKey {
                    t_ms: 1000,
                    value: 10.0,
                },
            ],
            1000,
            RateFunc::Smooth,
        )
        .unwrap();
        // Extremos iguales, mitad distinta (el easing por track manda).
        assert_eq!(lineal.sample(0), 0.0);
        assert_eq!(lineal.sample(1000), 10.0);
        assert!((lineal.sample(500) - 5.0).abs() < 1e-6);
        assert!((suave.sample(500) - 5.0).abs() < 1e-6, "smooth(0.5)=0.5");
        assert!(
            suave.sample(250) < lineal.sample(250),
            "smooth arranca lento"
        );
        // Malformados honestos.
        assert!(PropertyTrack::try_new(
            String::new(),
            vec![TrackKey {
                t_ms: 0,
                value: 0.0
            }],
            1000,
            RateFunc::Linear
        )
        .is_err());
        assert!(PropertyTrack::try_new("x".to_string(), vec![], 1000, RateFunc::Linear).is_err());
        assert!(PropertyTrack::try_new(
            "x".to_string(),
            vec![TrackKey {
                t_ms: 0,
                value: f32::NAN
            }],
            1000,
            RateFunc::Linear
        )
        .is_err());
    }

    #[test]
    fn reloj_avanza_a_rate_y_pausa_en_cero() {
        let mut c = Clock::try_new(0, 1.0).unwrap();
        c.tick(500);
        assert_eq!(c.t_global, 500);
        let mut doble = Clock::try_new(0, 2.0).unwrap();
        doble.tick(500);
        assert_eq!(doble.t_global, 1000);
        let mut pausa = Clock::try_new(100, 0.0).unwrap();
        pausa.tick(9999);
        assert_eq!(pausa.t_global, 100);
        assert!(Clock::try_new(0, f64::NAN).is_err());
        assert!(Clock::try_new(0, 9.0).is_err());
    }

    #[test]
    fn matching_acepta_n_distinto_y_bezier_preserva_muestras() {
        let a = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let b = vec![[0.0, 0.0], [2.0, 0.0], [1.0, 2.0]];
        let frames = matching_shapes_frames(&a, &b, 16, 5, false, RateFunc::Linear).unwrap();
        assert_eq!(frames.len(), 5);
        assert!(frames.iter().all(|f| f.len() == 16));
        // Extremos: frame 0 ≈ A, último ≈ B (suavizados, tolerancia de bezier).
        assert!((frames[0][0][0] - frames[0][0][0]).abs() < 1e-12);
        assert!(frames[0]
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite()));
        // Punto→figura con N=1 también vale.
        let punto = vec![[0.5, 0.5]];
        let crece = matching_shapes_frames(&punto, &a, 8, 3, false, RateFunc::Smooth).unwrap();
        assert!(crece[0]
            .iter()
            .all(|p| (p[0] - 0.5).abs() < 0.6 && (p[1] - 0.5).abs() < 0.6));
        // Presupuestos honestos.
        assert!(matching_shapes_frames(&a, &b, 1, 5, false, RateFunc::Linear).is_err());
        assert!(matching_shapes_frames(&a, &b, 16, 49, false, RateFunc::Linear).is_err());
        assert!(matching_shapes_frames(&[], &b, 16, 5, false, RateFunc::Linear).is_err());
    }

    #[test]
    fn transform_anim_corre_alpha_con_rate() {
        let a = vec![[0.0, 0.0], [1.0, 0.0]];
        let b = vec![[0.0, 0.0], [1.0, 1.0]];
        let t = TransformAnim::try_new(a, b, 8, 4, 2000, RateFunc::Linear, false).unwrap();
        assert_eq!(t.run_time_ms(), 2000);
        assert_eq!(t.rate(), RateFunc::Linear);
        let f0 = t.frame_at(0.0).unwrap();
        let f1 = t.frame_at(1.0).unwrap();
        assert_eq!(f0.len(), 8);
        assert!((f1[7][1] - f0[7][1]).abs() > 1e-9, "algo se movió");
        assert!(TransformAnim::try_new(
            vec![],
            vec![[0.0, 0.0]],
            8,
            4,
            2000,
            RateFunc::Linear,
            false
        )
        .is_err());
        assert!(TransformAnim::try_new(
            vec![[0.0, 0.0]],
            vec![[1.0, 1.0]],
            8,
            4,
            50,
            RateFunc::Linear,
            false
        )
        .is_err());
    }

    #[test]
    fn taylor_terms_cablea_1_a_7() {
        assert_eq!(taylor_anim_order_from_terms(None), 3);
        assert_eq!(taylor_anim_order_from_terms(Some(f64::NAN)), 3);
        assert_eq!(taylor_anim_order_from_terms(Some(1.0)), 1);
        assert_eq!(taylor_anim_order_from_terms(Some(7.0)), 7);
        assert_eq!(taylor_anim_order_from_terms(Some(99.0)), 7);
        assert_eq!(taylor_anim_order_from_terms(Some(0.0)), 1);
        let mut params = BTreeMap::new();
        assert_eq!(taylor_anim_order_from_params(&params), 3);
        params.insert("terms".to_string(), 5.0);
        assert_eq!(taylor_anim_order_from_params(&params), 5);
    }

    #[test]
    fn playlist_reusa_schedule_sample_y_frame() {
        use crate::protocol::{AnimRequest, ExportFormat, Playlist, PlaylistStep};
        let pedido = |t: &str| AnimRequest {
            template: t.to_string(),
            concept: t.to_string(),
            params: BTreeMap::new(),
            spec: None,
            export: ExportFormat::Gif,
            canvas: (640, 480),
            duration_ms: 2000,
            audio: None,
        };
        let lista = Playlist::try_new(vec![
            PlaylistStep::anim(pedido("derivative-slope"), 2000, 0).unwrap(),
            PlaylistStep::anim(pedido("integral-area"), 1000, 0).unwrap(),
        ])
        .unwrap();
        assert_eq!(schedule_playlist(&lista), lista.schedule());
        assert_eq!(sample_playlist(&lista, 500), lista.sample_at(500));
        let timeline = lista.global_timeline(&[48, 48]).unwrap();
        assert_eq!(frame_at_global(&timeline, 0, 96), 0);
        assert_eq!(frame_at_global(&timeline, 3000, 96), 95);
    }

    #[test]
    fn p01_tracks_y_anim_60s() {
        // P0.1 long-form: tracks/cámara/anim aceptan 60 s (60001 no);
        // `SCENE_MORPH_MAX_SAMPLES` sigue espacial en 512.
        assert_eq!(MAX_TRACK_DURATION_MS, 60_000);
        assert_eq!(SCENE_MORPH_MAX_SAMPLES, 512);
        let keys = vec![
            TrackKey {
                t_ms: 0,
                value: 0.0,
            },
            TrackKey {
                t_ms: 60_000,
                value: 1.0,
            },
        ];
        assert!(
            PropertyTrack::try_new("x".to_string(), keys.clone(), 60_000, RateFunc::Linear).is_ok()
        );
        assert!(PropertyTrack::try_new("x".to_string(), keys, 60_001, RateFunc::Linear).is_err());
        let cam = MovingCamera::try_new(
            Camera::Ortho(Ortho::default_16_9()),
            Camera::Ortho(Ortho::default_16_9()),
            60_000,
            RateFunc::Linear,
        );
        assert!(cam.is_ok());
        assert!(MovingCamera::try_new(
            Camera::Ortho(Ortho::default_16_9()),
            Camera::Ortho(Ortho::default_16_9()),
            60_001,
            RateFunc::Linear,
        )
        .is_err());
        let a = vec![[0.0, 0.0], [1.0, 1.0]];
        let b = vec![[1.0, 0.0], [0.0, 1.0]];
        assert!(TransformAnim::try_new(
            a.clone(),
            b.clone(),
            8,
            4,
            60_000,
            RateFunc::Linear,
            false
        )
        .is_ok());
        assert!(TransformAnim::try_new(a, b, 8, 4, 60_001, RateFunc::Linear, false).is_err());
    }
}
