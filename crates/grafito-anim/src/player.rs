//! Player puro Manim-en-Rust (P0-b/c/d + P1-f): `ScenePlayer`, `ValueTracker`,
//! creation/attention y `VMobject`.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red. El player compone
//! `PlacedMobject`s (qué dibujar, con qué opacidad/escala/centro); el raster
//! lo hace la Piel después con esa estructura.
//!
//! CONTRATO con la Piel (estable; si cambia de forma incompatible se
//! documenta acá y en el return):
//! - [`PlacedMobject`] = `{ mobject, opacity 0..1, scale >0 finito, center
//!   finito }`. La Piel dibuja `mobject` escalado por `scale` alrededor de
//!   `center` con alfa `opacity`.
//! - [`PlayedFrame`] = `{ objects: Vec<PlacedMobject> }` en orden de dibujo
//!   (0 = fondo). Sin píxeles, sin texturas.
//! - [`ScenePlayer::play`] es total: clampa presupuestos y SALTA el colocado
//!   inválido en vez de fallar (frames por item 1..=48, total ≤96, capas
//!   ≤32). [`ScenePlayer::try_play`] es la versión estricta que devuelve
//!   `Err` honesto si algo excede o algún colocado es inválido (además
//!   valida el plan de remuestreo vía `AnimationGroup::plan_remuestreo`
//!   cuando los items traen N distinto).
//! - [`PlacedMobject::opaco`] y los `placed_at` devuelven `SceneResult`
//!   (R6d): lo inválido jamás llega a pantalla como punto en el origen.
//! - `Write` con SVG opaco NO finge trazo parcial: el SVG viaja entero y el
//!   progreso se expresa por `opacity` (revelado honesto, documentado en
//!   [`WriteAnim`]).
//!
//! Presupuestos intactos: frames por anim 1..=48, total del `play` ≤96
//! (paridad con playlist; corto histórico), set 64 MiB (estimación honesta
//! en `try_play`), timeline 64 keys/60 s (P0.1 long-form; los tracks que
//! alimentan al player), capas 32, `Resolution` 64..=4096,
//! `AnimDuration` 0.1..=60 s (`run_ms` 100..=60000).
//!
//! El long-form (hasta 1500 frames por formato video) NO pasa por acá de
//! una: el frente compone por chunks de `max_chunk_frames` (jamás el `Vec`
//! total) y cada chunk respeta estos topes cortos.

use crate::scene::{
    par_remuestreado, Animation, Mobject, PathFunc, RateFunc, Scene, SceneError, SceneResult,
};
use serde::{Deserialize, Serialize};

// ── P0-c: ValueTracker ────────────────────────────────────────────────────

/// Rastreador de valor estilo Manim (`ValueTracker`).
///
/// Guarda un `f64` finito; el player lo evalúa por frame
/// ([`UpdateFromTracker`]). Puro, sin pánicos.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ValueTracker {
    value: f64,
}

impl ValueTracker {
    /// Constructor validado (solo finitos).
    pub fn try_new(value: f64) -> SceneResult<Self> {
        if !value.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "valor inicial no finito".to_string(),
            });
        }
        Ok(Self { value })
    }

    /// Valor actual.
    pub fn get(self) -> f64 {
        self.value
    }

    /// Fija el valor (`Err` honesto si no es finito; no cambia nada).
    pub fn set_value(&mut self, v: f64) -> SceneResult<()> {
        if !v.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: format!("{v} no es finito: el tracker queda como estaba"),
            });
        }
        self.value = v;
        Ok(())
    }

    /// Suma `dv` (`Err` honesto si `dv` no es finito o el resultado no lo es).
    pub fn increment_value(&mut self, dv: f64) -> SceneResult<()> {
        if !dv.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "incremento no finito".to_string(),
            });
        }
        let v = self.value + dv;
        if !v.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "el resultado desborda a no finito".to_string(),
            });
        }
        self.value = v;
        Ok(())
    }

    /// Interpola `start→end` con `alpha` crudo 0..1 (clamp + guardia finita;
    /// `alpha` no finito → `start`). `Err` honesto si los bordes no son
    /// finitos. Es lo que el player llama por frame con el easing ya
    /// aplicado por el llamador si quiere (ver [`UpdateFromTracker`]).
    pub fn interpolate(&mut self, start: f64, end: f64, alpha: f64) -> SceneResult<()> {
        if !start.is_finite() || !end.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "bordes no finitos".to_string(),
            });
        }
        let a = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = start + (end - start) * a;
        if !v.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "el interpolado desborda".to_string(),
            });
        }
        self.value = v;
        Ok(())
    }
}

// ── Colocado: el contrato con la Piel ─────────────────────────────────────

/// Un mobject listo para rasterizar: qué + con qué alfa/escala/centro.
///
/// La Piel dibuja `mobject` escalado por `scale` alrededor de `center` con
/// alfa global `opacity`. Todo validado en [`PlacedMobject::try_new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedMobject {
    pub mobject: Mobject,
    /// Alfa global 0..1 (finito).
    pub opacity: f32,
    /// Escala alrededor de `center` (>0, finita).
    pub scale: f32,
    /// Centro de la escala (finito).
    pub center: [f64; 2],
}

impl PlacedMobject {
    /// Constructor validado (todo `Err` honesto).
    pub fn try_new(
        mobject: Mobject,
        opacity: f32,
        scale: f32,
        center: [f64; 2],
    ) -> SceneResult<Self> {
        mobject.validate().map_err(|e| SceneError::EscenaInvalida {
            detalle: format!("colocado inválido: {e}"),
        })?;
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(SceneError::EscenaInvalida {
                detalle: format!("opacity {opacity} fuera de 0..=1"),
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(SceneError::EscenaInvalida {
                detalle: format!("scale {scale} debe ser >0 y finita"),
            });
        }
        if !center[0].is_finite() || !center[1].is_finite() {
            return Err(SceneError::EscenaInvalida {
                detalle: "center no finito".to_string(),
            });
        }
        Ok(Self {
            mobject,
            opacity,
            scale,
            center,
        })
    }

    /// Colocado opaco sin transformar (centro = centroide honesto).
    ///
    /// R6d: devuelve `SceneResult` — el mobject inválido es `Err` honesto
    /// (antes caía a un `Dot` en el origen, fingiendo contenido).
    pub fn opaco(mobject: Mobject) -> SceneResult<Self> {
        let center = centroide_de(&mobject);
        Self::try_new(mobject, 1.0, 1.0, center)
    }
}

/// Un frame reproducido: objetos en orden de dibujo (0 = fondo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayedFrame {
    pub objects: Vec<PlacedMobject>,
}

impl PlayedFrame {
    /// Cantidad de objetos.
    pub fn len(&self) -> usize {
        self.objects.len()
    }
    /// ¿Vacío?
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

/// Centroide honesto de un mobject (media de puntos; figuras = su centro;
/// resto = origen). Puro, sin pánicos.
pub fn centroide_de(m: &Mobject) -> [f64; 2] {
    match m {
        Mobject::Dot { x, y } => [*x, *y],
        Mobject::Circle { cx, cy, .. } | Mobject::Square { cx, cy, .. } => [*cx, *cy],
        Mobject::Line { from, to } | Mobject::Arrow { from, to } => {
            [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0]
        }
        Mobject::Polygon { pts } => {
            if pts.is_empty() {
                return [0.0, 0.0];
            }
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut n = 0.0;
            for p in pts {
                if p[0].is_finite() && p[1].is_finite() {
                    sx += p[0];
                    sy += p[1];
                    n += 1.0;
                }
            }
            if n > 0.0 {
                [sx / n, sy / n]
            } else {
                [0.0, 0.0]
            }
        }
        Mobject::Group(hijos) => {
            if hijos.is_empty() {
                return [0.0, 0.0];
            }
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut n = 0.0;
            for h in hijos {
                let c = centroide_de(h);
                if c[0].is_finite() && c[1].is_finite() {
                    sx += c[0];
                    sy += c[1];
                    n += 1.0;
                }
            }
            if n > 0.0 {
                [sx / n, sy / n]
            } else {
                [0.0, 0.0]
            }
        }
        Mobject::Axes
        | Mobject::FunctionGraph { .. }
        | Mobject::ArrowField { .. }
        | Mobject::Tex { .. }
        | Mobject::NumberPlane { .. }
        | Mobject::VectorField { .. } => [0.0, 0.0],
    }
}

// ── P0-d: Creation / attention ────────────────────────────────────────────
// Todas implementan `Animation` (begin/interpolate/finish) para que el player
// las corra por el mismo camino que `TransformAnim`.

/// Tope de frames por anim del player (paridad con `PARAMETRIC_MAX_FRAMES`;
/// corto histórico intacto en P0.1: el largo va por chunks del frente).
pub const PLAYER_MAX_FRAMES: usize = 48;
/// Tope de frames totales de un `play` (paridad con playlist: 96;
/// corto histórico intacto en P0.1).
pub const PLAYER_MAX_TOTAL_FRAMES: usize = 96;

fn valida_frames_run(frames: usize, run_ms: u64, donde: &'static str) -> SceneResult<()> {
    if frames == 0 || frames > PLAYER_MAX_FRAMES {
        return Err(SceneError::MorphInvalido {
            detalle: format!(
                "{donde}: pediste {frames} fotogramas, usá entre 1 y {PLAYER_MAX_FRAMES}"
            ),
        });
    }
    if !(100..=60_000).contains(&run_ms) {
        return Err(SceneError::MorphInvalido {
            detalle: format!("{donde}: run_time {run_ms} ms fuera de 100..=60000"),
        });
    }
    Ok(())
}

/// `Create`: traza progresiva de una polilínea por longitud de arco.
///
/// El frame en `alpha` muestra el prefijo `eased·longitud` (remuestreo
/// arco-longitud existente). `frames=1` = figura completa.
#[derive(Debug, Clone)]
pub struct CreateAnim {
    poly: Vec<[f64; 2]>,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    closed: bool,
}

impl CreateAnim {
    /// Constructor validado (1..=4096 puntos finitos, frames 1..=48).
    pub fn try_new(
        poly: Vec<[f64; 2]>,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "Create")?;
        if poly.is_empty() || poly.len() > crate::scene::MAX_MOBJECT_POINTS {
            return Err(SceneError::MobjectInvalido {
                donde: "Create",
                detalle: format!(
                    "{} puntos (válido 1..={})",
                    poly.len(),
                    crate::scene::MAX_MOBJECT_POINTS
                ),
            });
        }
        for (i, p) in poly.iter().enumerate() {
            if !p[0].is_finite() || !p[1].is_finite() {
                return Err(SceneError::MobjectInvalido {
                    donde: "Create",
                    detalle: format!("punto no finito en el índice {i}"),
                });
            }
        }
        Ok(Self {
            poly,
            frames,
            run_ms,
            rate,
            closed,
        })
    }

    /// Fotogramas del player (los arma [`CreateAnim::placed_at`]).
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Prefijo trazado en `alpha` crudo 0..1 (easing aplicado).
    pub fn traza_en(&self, alpha: f64) -> Vec<[f64; 2]> {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        traza_prefijo(&self.poly, s, self.closed)
    }

    /// Colocado en `alpha` (polígono parcial opaco; R6d: `Err` honesto si
    /// la traza queda vacía, jamás punto en el origen).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let traza = self.traza_en(alpha);
        let center = centroide_de(&Mobject::Polygon {
            pts: self.poly.clone(),
        });
        let m = if traza.len() >= 2 || (traza.len() == 1 && self.poly.len() == 1) {
            Mobject::Polygon { pts: traza }
        } else if let Some(p) = traza.first().or(self.poly.first()) {
            Mobject::Dot { x: p[0], y: p[1] }
        } else {
            // Inalcanzable con constructor validado (polilínea no vacía);
            // `Err` honesto en vez del viejo `Dot` en el origen.
            return Err(SceneError::MobjectInvalido {
                donde: "Create",
                detalle: "traza vacía sin polilínea: no hay qué colocar".to_string(),
            });
        };
        PlacedMobject::try_new(m, 1.0, 1.0, center)
    }
}

impl Animation for CreateAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// Prefijo de polilínea hasta la fracción `s` de su longitud (0..1).
/// `s<=0` → primer punto como `Dot` honesto (un punto); `s>=1` → toda.
/// Puro, sin pánicos.
fn traza_prefijo(poly: &[[f64; 2]], s: f64, closed: bool) -> Vec<[f64; 2]> {
    if poly.is_empty() {
        return Vec::new();
    }
    if poly.len() == 1 || s >= 1.0 {
        return poly.to_vec();
    }
    if s <= 0.0 {
        return vec![poly[0]];
    }
    let n = poly.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    if segs == 0 {
        return vec![poly[0]];
    }
    let mut total = 0.0;
    let mut largos: Vec<f64> = Vec::with_capacity(segs);
    for k in 0..segs {
        let (a, b) = if closed {
            (poly[k % n], poly[(k + 1) % n])
        } else {
            (poly[k], poly[k + 1])
        };
        let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let d = if d.is_finite() && d > 0.0 { d } else { 0.0 };
        largos.push(d);
        total += d;
    }
    if total <= 0.0 {
        return vec![poly[0]];
    }
    let objetivo = total * s.clamp(0.0, 1.0);
    let mut out: Vec<[f64; 2]> = Vec::new();
    out.push(poly[0]);
    let mut acumulado = 0.0;
    for k in 0..segs {
        let (a, b) = if closed {
            (poly[k % n], poly[(k + 1) % n])
        } else {
            (poly[k], poly[k + 1])
        };
        let d = largos[k];
        if acumulado + d >= objetivo {
            let resto = objetivo - acumulado;
            let u = if d > 0.0 {
                (resto / d).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let x = a[0] + (b[0] - a[0]) * u;
            let y = a[1] + (b[1] - a[1]) * u;
            if x.is_finite() && y.is_finite() {
                out.push([x, y]);
            }
            break;
        }
        acumulado += d;
        if closed {
            out.push(poly[(k + 1) % n]);
        } else if k + 1 < n {
            out.push(poly[k + 1]);
        }
    }
    if out.len() < 2 {
        out.push(*poly.first().unwrap_or(&[0.0, 0.0]));
    }
    out
}

/// `Write`: revelado honesto de texto ya tipografiado.
///
/// El SVG viaja OPACO (sin parser de trazos en el cerebro): el progreso se
/// expresa por `opacity` eased, jamás subdividiendo el path a ciegas. Si
/// algún día hay parser, este es el punto de corte (documentado, no fingido).
#[derive(Debug, Clone)]
pub struct WriteAnim {
    mobject: Mobject,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
}

impl WriteAnim {
    /// Constructor validado (el mobject debe validar; frames 1..=48).
    pub fn try_new(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "Write")?;
        mobject.validate()?;
        Ok(Self {
            mobject,
            frames,
            run_ms,
            rate,
        })
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocado en `alpha` (mismo SVG entero, alfa eased; R6d: `Err`
    /// honesto si el colocado no valida).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let e = self.interpolate(alpha);
        let o = (if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        }) as f32;
        let center = centroide_de(&self.mobject);
        PlacedMobject::try_new(self.mobject.clone(), o, 1.0, center)
    }
}

impl Animation for WriteAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// `Fade{in/out}`: alfa eased sobre el mobject entero.
#[derive(Debug, Clone)]
pub struct FadeAnim {
    mobject: Mobject,
    fade_in: bool,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
}

impl FadeAnim {
    /// Constructor validado (`fade_in=true` = aparece, `false` = desaparece).
    pub fn try_new(
        mobject: Mobject,
        fade_in: bool,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "Fade")?;
        mobject.validate()?;
        Ok(Self {
            mobject,
            fade_in,
            frames,
            run_ms,
            rate,
        })
    }

    /// ¿Es aparición?
    pub fn is_in(&self) -> bool {
        self.fade_in
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocado en `alpha` (alfa eased o su complemento; R6d: `Err`
    /// honesto si el colocado no valida).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let o = (if self.fade_in { s } else { 1.0 - s }) as f32;
        let center = centroide_de(&self.mobject);
        PlacedMobject::try_new(self.mobject.clone(), o, 1.0, center)
    }
}

impl Animation for FadeAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// `GrowFromCenter`: escala eased 0→1 desde el centroide (opaco).
#[derive(Debug, Clone)]
pub struct GrowFromCenterAnim {
    mobject: Mobject,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
}

impl GrowFromCenterAnim {
    /// Constructor validado.
    pub fn try_new(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "GrowFromCenter")?;
        mobject.validate()?;
        Ok(Self {
            mobject,
            frames,
            run_ms,
            rate,
        })
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocado en `alpha` (escala eased, mínimo honesto 1e-6 para no
    /// colapsar a escala 0 inválida en el primer frame; R6d: `Err`
    /// honesto si el colocado no valida).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scale = (s.max(1e-6)) as f32;
        let center = centroide_de(&self.mobject);
        PlacedMobject::try_new(self.mobject.clone(), 1.0, scale, center)
    }
}

impl Animation for GrowFromCenterAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// `Indicate`: pulso ×1.2 ida-vuelta (escala `1+0.2·sin(π·eased)`).
#[derive(Debug, Clone)]
pub struct IndicateAnim {
    mobject: Mobject,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
}

impl IndicateAnim {
    /// Constructor validado.
    pub fn try_new(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "Indicate")?;
        mobject.validate()?;
        Ok(Self {
            mobject,
            frames,
            run_ms,
            rate,
        })
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocado en `alpha` (1 en los extremos, 1.2 en el medio eased;
    /// R6d: `Err` honesto si el colocado no valida).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scale = (1.0 + 0.2 * (std::f64::consts::PI * s).sin()) as f32;
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let center = centroide_de(&self.mobject);
        PlacedMobject::try_new(self.mobject.clone(), 1.0, scale, center)
    }
}

impl Animation for IndicateAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

// ── P0-c (player): a dónde mapea el tracker ───────────────────────────────

/// Destino del valor del tracker en el colocado (P0-c).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrackerMap {
    /// Alfa = `lo + (hi-lo)·s` con `s` = valor normalizado 0..1.
    Opacity { lo: f32, hi: f32 },
    /// Escala = `lo + (hi-lo)·s` (>0).
    Scale { lo: f32, hi: f32 },
    /// `center[0]` = `lo + (hi-lo)·s`.
    CenterX { lo: f64, hi: f64 },
    /// `center[1]` = `lo + (hi-lo)·s`.
    CenterY { lo: f64, hi: f64 },
}

impl TrackerMap {
    /// Valida rangos (alfas 0..1, escalas >0 finitas, centros finitos).
    pub fn validate(&self) -> SceneResult<()> {
        match *self {
            Self::Opacity { lo, hi } => {
                for (n, v) in [("lo", lo), ("hi", hi)] {
                    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                        return Err(SceneError::TrackInvalido {
                            prop: "tracker.map".to_string(),
                            detalle: format!("opacity.{n} {v} fuera de 0..=1"),
                        });
                    }
                }
                Ok(())
            }
            Self::Scale { lo, hi } => {
                for (n, v) in [("lo", lo), ("hi", hi)] {
                    if !v.is_finite() || v <= 0.0 {
                        return Err(SceneError::TrackInvalido {
                            prop: "tracker.map".to_string(),
                            detalle: format!("scale.{n} {v} debe ser >0"),
                        });
                    }
                }
                Ok(())
            }
            Self::CenterX { lo, hi } | Self::CenterY { lo, hi } => {
                if !lo.is_finite() || !hi.is_finite() {
                    return Err(SceneError::TrackInvalido {
                        prop: "tracker.map".to_string(),
                        detalle: "centro no finito".to_string(),
                    });
                }
                Ok(())
            }
        }
    }
}

/// `UpdateFromTracker`: el tracker barre `start→end` y cada frame mapea al
/// colocado según [`TrackerMap`]. Se evalúa por frame en el player.
#[derive(Debug, Clone)]
pub struct UpdateFromTracker {
    mobject: Mobject,
    start: f64,
    end: f64,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    map: TrackerMap,
}

impl UpdateFromTracker {
    /// Constructor validado (bordes finitos, frames 1..=48, mapa válido).
    pub fn try_new(
        mobject: Mobject,
        start: f64,
        end: f64,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        map: TrackerMap,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "UpdateFromTracker")?;
        mobject.validate()?;
        if !start.is_finite() || !end.is_finite() {
            return Err(SceneError::TrackInvalido {
                prop: "tracker".to_string(),
                detalle: "bordes no finitos".to_string(),
            });
        }
        map.validate()?;
        Ok(Self {
            mobject,
            start,
            end,
            frames,
            run_ms,
            rate,
            map,
        })
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Valor del tracker en `alpha` (rate aplicado + `ValueTracker::interpolate`).
    pub fn valor_en(&self, alpha: f64) -> f64 {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = self.start + (self.end - self.start) * s;
        if v.is_finite() {
            v
        } else {
            self.start
        }
    }

    /// Colocado en `alpha`: normaliza el valor al rango y mapea.
    /// Rango degenerado (`start==end`) → usa el piso del mapa (honesto).
    /// R6d: devuelve `SceneResult` (el inválido es `Err`, jamás colocado
    /// sin validar).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let v = self.valor_en(alpha);
        let s = if self.start == self.end {
            0.0
        } else {
            let lo = self.start.min(self.end);
            let hi = self.start.max(self.end);
            if hi > lo {
                ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let s = if s.is_finite() { s } else { 0.0 };
        let base = centroide_de(&self.mobject);
        let (opacity, scale, center) = match self.map {
            TrackerMap::Opacity { lo, hi } => {
                let o = lo + (hi - lo) * (s as f32);
                let o = if o.is_finite() {
                    o.clamp(0.0, 1.0)
                } else {
                    1.0
                };
                (o, 1.0, base)
            }
            TrackerMap::Scale { lo, hi } => {
                let k = lo + (hi - lo) * (s as f32);
                let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
                (1.0, k, base)
            }
            TrackerMap::CenterX { lo, hi } => {
                let x = lo + (hi - lo) * s;
                let x = if x.is_finite() { x } else { base[0] };
                (1.0, 1.0, [x, base[1]])
            }
            TrackerMap::CenterY { lo, hi } => {
                let y = lo + (hi - lo) * s;
                let y = if y.is_finite() { y } else { base[1] };
                (1.0, 1.0, [base[0], y])
            }
        };
        PlacedMobject::try_new(self.mobject.clone(), opacity, scale, center)
    }
}

impl Animation for UpdateFromTracker {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

// ── P1-f: VMobject ────────────────────────────────────────────────────────

/// Vector-mobject con anclas y manijas (P1-f, estilo Manim `VMobject`).
///
/// `anchors`, `handles_in` y `handles_out` miden lo mismo (1..=4096 puntos
/// finitos). [`VMobject::align_data`] remuestrea por arco-longitud con el
/// pipeline existente (`par_remuestreado` de `scene.rs`) y reinterpola las manijas al
/// nuevo N (honesto: las manijas se reubican linealmente, no se reinventan
/// curvas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VMobject {
    pub anchors: Vec<[f64; 2]>,
    pub handles_in: Vec<[f64; 2]>,
    pub handles_out: Vec<[f64; 2]>,
}

impl VMobject {
    /// Constructor validado (mismo N 1..=4096, todo finito).
    pub fn try_new(
        anchors: Vec<[f64; 2]>,
        handles_in: Vec<[f64; 2]>,
        handles_out: Vec<[f64; 2]>,
    ) -> SceneResult<Self> {
        let n = anchors.len();
        if n == 0 || n > crate::scene::MAX_MOBJECT_POINTS {
            return Err(SceneError::MobjectInvalido {
                donde: "VMobject",
                detalle: format!(
                    "{n} anclas (válido 1..={})",
                    crate::scene::MAX_MOBJECT_POINTS
                ),
            });
        }
        if handles_in.len() != n || handles_out.len() != n {
            return Err(SceneError::MobjectInvalido {
                donde: "VMobject",
                detalle: format!(
                    "anclas {n} vs manijas {}/{}: pasalas 1 a 1",
                    handles_in.len(),
                    handles_out.len()
                ),
            });
        }
        for (nombre, pts) in [
            ("anchors", &anchors),
            ("handles_in", &handles_in),
            ("handles_out", &handles_out),
        ] {
            for (i, p) in pts.iter().enumerate() {
                if !p[0].is_finite() || !p[1].is_finite() {
                    return Err(SceneError::MobjectInvalido {
                        donde: "VMobject",
                        detalle: format!("{nombre} no finito en {i}"),
                    });
                }
            }
        }
        Ok(Self {
            anchors,
            handles_in,
            handles_out,
        })
    }

    /// Cantidad de puntos.
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// ¿Vacío? (nunca tras `try_new`, pero la deserialización puede).
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Alinea los datos a `samples` puntos (2..=512) por arco-longitud con el
    /// pipeline existente; las manijas se reinterpolan linealmente al nuevo
    /// N. Devuelve el alineado (no muta). `Err` honesto si `samples` o los
    /// puntos no pasan el pipeline.
    pub fn align_data(&self, samples: usize, closed: bool) -> SceneResult<Self> {
        if !(2..=512).contains(&samples) {
            return Err(SceneError::MorphInvalido {
                detalle: format!("muestras fuera de rango: pediste {samples}, usá entre 2 y 512"),
            });
        }
        if self.is_empty() {
            return Err(SceneError::MobjectInvalido {
                donde: "VMobject",
                detalle: "vacío: nada para alinear".to_string(),
            });
        }
        // Grilla nueva: remuestrea las anclas (el pipeline valida todo).
        let (nuevas, _) = par_remuestreado(&self.anchors, &self.anchors, samples, closed)?;
        let n_viejo = self.len();
        let reinterpola = |serie: &[[f64; 2]]| -> Vec<[f64; 2]> {
            if n_viejo <= 1 {
                return vec![serie[0]; samples];
            }
            let mut out = Vec::with_capacity(samples);
            for j in 0..samples {
                let pos = if samples <= 1 {
                    0.0
                } else {
                    (j as f64) / ((samples - 1) as f64) * ((n_viejo - 1) as f64)
                };
                let i0 = (pos.floor() as usize).min(n_viejo.saturating_sub(1));
                let i1 = (i0 + 1).min(n_viejo.saturating_sub(1));
                let u = (pos - i0 as f64).clamp(0.0, 1.0);
                let a = serie[i0];
                let b = serie[i1];
                let x = a[0] + (b[0] - a[0]) * u;
                let y = a[1] + (b[1] - a[1]) * u;
                if x.is_finite() && y.is_finite() {
                    out.push([x, y]);
                } else {
                    out.push(a);
                }
            }
            out
        };
        Ok(Self {
            anchors: nuevas,
            handles_in: reinterpola(&self.handles_in),
            handles_out: reinterpola(&self.handles_out),
        })
    }

    /// Polilínea de anclas como `Mobject` (para el player).
    pub fn como_mobject(&self) -> Mobject {
        Mobject::Polygon {
            pts: self.anchors.clone(),
        }
    }
}

/// `TransformMatchingShapes` por submobject (P1-f, estilo Manim).
///
/// Empareja `from[i]→to[i]` hasta `min(N,M)` con el pipeline de remuestreo;
/// los sobrantes de `from` desvanecen (alfa 1→0) y los de `to` aparecen
/// (alfa 0→1). `frame_at(alpha)` devuelve los colocados en orden.
#[derive(Debug, Clone)]
pub struct TransformMatchingShapes {
    from: Vec<Mobject>,
    to: Vec<Mobject>,
    samples: usize,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    closed: bool,
    path: PathFunc,
}

impl TransformMatchingShapes {
    /// Constructor validado (1..=32 submobjects por lado, frames 1..=48).
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        from: Vec<Mobject>,
        to: Vec<Mobject>,
        samples: usize,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
        path: PathFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "TransformMatchingShapes")?;
        if !(2..=512).contains(&samples) {
            return Err(SceneError::MorphInvalido {
                detalle: format!("muestras fuera de rango: pediste {samples}, usá entre 2 y 512"),
            });
        }
        if from.is_empty() || from.len() > 32 || to.is_empty() || to.len() > 32 {
            return Err(SceneError::MobjectInvalido {
                donde: "TransformMatchingShapes",
                detalle: format!(
                    "from {} / to {} (válido 1..=32 por lado)",
                    from.len(),
                    to.len()
                ),
            });
        }
        for m in from.iter().chain(to.iter()) {
            m.validate()?;
        }
        Ok(Self {
            from,
            to,
            samples,
            frames,
            run_ms,
            rate,
            closed,
            path,
        })
    }

    /// Fotogramas del player.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocados en `alpha` crudo 0..1 (easing aplicado; R6d: `Err`
    /// honesto si algún colocado no valida — jamás punto en el origen).
    pub fn frame_at(&self, alpha: f64) -> SceneResult<Vec<PlacedMobject>> {
        let e = self.interpolate(alpha);
        let s = if e.is_finite() {
            e.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut out = Vec::new();
        let pares = self.from.len().min(self.to.len());
        for i in 0..pares {
            let (a, b) = (polilinea_de(&self.from[i]), polilinea_de(&self.to[i]));
            let colocado = if a.is_empty() || b.is_empty() {
                let m = if s < 0.5 {
                    self.from[i].clone()
                } else {
                    self.to[i].clone()
                };
                PlacedMobject::opaco(m)?
            } else {
                match par_remuestreado(&a, &b, self.samples, self.closed) {
                    Ok((ra, rb)) => {
                        let pts: Vec<[f64; 2]> = ra
                            .iter()
                            .zip(rb.iter())
                            .map(|(pa, pb)| self.path.interpola(*pa, *pb, s))
                            .collect();
                        let center = punto_medio(&ra, &rb, s);
                        PlacedMobject::try_new(Mobject::Polygon { pts }, 1.0, 1.0, center)?
                    }
                    Err(_) => PlacedMobject::opaco(self.to[i].clone())?,
                }
            };
            out.push(colocado);
        }
        // Sobrantes de `from`: desvanecen; sobrantes de `to`: aparecen.
        let o_fuera = (1.0 - s) as f32;
        for m in self.from.iter().skip(pares) {
            let center = centroide_de(m);
            let o = if o_fuera.is_finite() {
                o_fuera.clamp(0.0, 1.0)
            } else {
                0.0
            };
            if let Ok(p) = PlacedMobject::try_new(m.clone(), o.max(1e-6), 1.0, center) {
                out.push(p);
            }
        }
        let o_dentro = s as f32;
        for m in self.to.iter().skip(pares) {
            let center = centroide_de(m);
            let o = if o_dentro.is_finite() {
                o_dentro.clamp(0.0, 1.0)
            } else {
                0.0
            };
            if let Ok(p) = PlacedMobject::try_new(m.clone(), o.max(1e-6), 1.0, center) {
                out.push(p);
            }
        }
        Ok(out)
    }
}

impl Animation for TransformMatchingShapes {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// Polilínea representativa de un mobject para el matching (vacía si no hay
/// geometría de puntos: `Axes`/`Tex`/campos van por opacidad en el llamador).
fn polilinea_de(m: &Mobject) -> Vec<[f64; 2]> {
    match m {
        Mobject::Polygon { pts } => pts.clone(),
        Mobject::Dot { x, y } => vec![[*x, *y]],
        Mobject::Line { from, to } | Mobject::Arrow { from, to } => vec![*from, *to],
        Mobject::Circle { cx, cy, r } => {
            let mut out = Vec::with_capacity(25);
            for k in 0..24 {
                let a = 2.0 * std::f64::consts::PI * (k as f64) / 24.0;
                out.push([cx + r * a.cos(), cy + r * a.sin()]);
            }
            out.push([cx + r, *cy]);
            out
        }
        Mobject::Square { cx, cy, side } => {
            let h = side / 2.0;
            vec![
                [cx - h, cy - h],
                [cx + h, cy - h],
                [cx + h, cy + h],
                [cx - h, cy + h],
            ]
        }
        Mobject::Group(hijos) => {
            let mut out = Vec::new();
            for h in hijos {
                out.extend(polilinea_de(h));
            }
            out
        }
        Mobject::Axes
        | Mobject::FunctionGraph { .. }
        | Mobject::ArrowField { .. }
        | Mobject::Tex { .. }
        | Mobject::NumberPlane { .. }
        | Mobject::VectorField { .. } => Vec::new(),
    }
}

/// Punto medio honesto entre dos remuestreos en `s` (para el centro del
/// colocado del matching).
fn punto_medio(ra: &[[f64; 2]], rb: &[[f64; 2]], s: f64) -> [f64; 2] {
    let ca = centroide_pts(ra);
    let cb = centroide_pts(rb);
    let x = ca[0] + (cb[0] - ca[0]) * s;
    let y = ca[1] + (cb[1] - ca[1]) * s;
    if x.is_finite() && y.is_finite() {
        [x, y]
    } else {
        ca
    }
}

fn centroide_pts(pts: &[[f64; 2]]) -> [f64; 2] {
    centroide_de(&Mobject::Polygon { pts: pts.to_vec() })
}

// ── P0-b: PlayItem + ScenePlayer ──────────────────────────────────────────

/// Un paso del player (se reproducen en orden, como `Succession`).
#[derive(Debug, Clone)]
pub enum PlayItem {
    /// Morph polilínea→polilínea (reusa el pipeline de remuestreo).
    Transform(crate::scene::TransformAnim),
    /// Traza progresiva.
    Create(CreateAnim),
    /// Revelado honesto de texto/SVG opaco.
    Write(WriteAnim),
    /// Aparición / desaparición por alfa.
    Fade(FadeAnim),
    /// Escala 0→1 desde el centroide.
    GrowFromCenter(GrowFromCenterAnim),
    /// Pulso ×1.2 ida-vuelta.
    Indicate(IndicateAnim),
    /// Tracker evaluado por frame.
    Tracker(UpdateFromTracker),
    /// Matching por submobjects.
    MatchingShapes(TransformMatchingShapes),
    /// Espera: congela el último frame N veces.
    Wait(WaitAnim),
}

/// Espera silenciosa (congela el último frame; 1..=48 repeticiones).
#[derive(Debug, Clone, Copy)]
pub struct WaitAnim {
    frames: usize,
}

impl WaitAnim {
    /// Constructor validado (1..=48).
    pub fn try_new(frames: usize) -> SceneResult<Self> {
        if frames == 0 || frames > PLAYER_MAX_FRAMES {
            return Err(SceneError::MorphInvalido {
                detalle: format!(
                    "Wait: pediste {frames} fotogramas, usá entre 1 y {PLAYER_MAX_FRAMES}"
                ),
            });
        }
        Ok(Self { frames })
    }

    /// Repeticiones.
    pub fn frames(self) -> usize {
        self.frames
    }
}

impl PlayItem {
    /// Fotogramas que aporta el item.
    pub fn frames(&self) -> usize {
        match self {
            Self::Transform(a) => a.frames_suavizados().map(|f| f.len()).unwrap_or(0),
            Self::Create(a) => a.frames(),
            Self::Write(a) => a.frames(),
            Self::Fade(a) => a.frames(),
            Self::GrowFromCenter(a) => a.frames(),
            Self::Indicate(a) => a.frames(),
            Self::Tracker(a) => a.frames(),
            Self::MatchingShapes(a) => a.frames(),
            Self::Wait(a) => a.frames(),
        }
    }
}

/// Player puro escena→frames colocados.
///
/// `play` es total (clampa presupuestos y salta el colocado inválido);
/// `try_play` es estricta (falla honesto + valida remuestreo con N distinto).
/// Ambas aplican `begin`/`interpolate`/`finish` por anim, componen
/// fondo (capas de la escena) + animado, y dejan la escena en su estado
/// final (las capas pasan a valer el último frame animado cuando el item
/// produce geometría final conocida: `Transform`/`Create`/matching).
pub struct ScenePlayer;

impl ScenePlayer {
    /// Reproduce (total): clampa frames por item a 1..=48 y el total a 96,
    /// y SALTA el colocado inválido (jamás punto en el origen).
    /// Nunca falla; si no hay items devuelve vacío.
    pub fn play(scene: &mut Scene, items: Vec<PlayItem>) -> Vec<PlayedFrame> {
        // En modo total el inválido se salta, así que el `Err` es
        // inalcanzable; se mapea a vacío por tipo (sin `unwrap`).
        Self::play_clamp(scene, items, true).unwrap_or_default()
    }

    /// Reproduce estricto: `Err` honesto si algún item excede frames, si el
    /// total excede 96, si el set estimado supera 64 MiB o si algún colocado
    /// (fondo o animado) es inválido. Con N distinto entre items valida el
    /// plan de remuestreo al máximo (espejo de `Guion::items_para`).
    pub fn try_play(scene: &mut Scene, items: Vec<PlayItem>) -> SceneResult<Vec<PlayedFrame>> {
        if items.is_empty() {
            return Err(SceneError::EscenaInvalida {
                detalle: "sin items: pasame al menos 1 animación".to_string(),
            });
        }
        let mut total = 0usize;
        for (i, item) in items.iter().enumerate() {
            let n = item.frames();
            if n == 0 || n > PLAYER_MAX_FRAMES {
                return Err(SceneError::PresupuestoExcedido {
                    detalle: format!(
                        "el item {i} trae {n} frames (válido 1..={PLAYER_MAX_FRAMES})"
                    ),
                });
            }
            total = total.saturating_add(n);
            if total > PLAYER_MAX_TOTAL_FRAMES {
                return Err(SceneError::PresupuestoExcedido {
                    detalle: format!(
                        "el total {total} excede {PLAYER_MAX_TOTAL_FRAMES}: partí la escena en dos"
                    ),
                });
            }
        }
        // Estimación honesta del set (64 MiB, paridad con el resto del crate).
        if let Some(bytes) = total.checked_mul(16 * 512) {
            if bytes > 64 * 1024 * 1024 {
                return Err(SceneError::PresupuestoExcedido {
                    detalle: "el set estimado excede 64 MiB: bajá muestras o fotogramas"
                        .to_string(),
                });
            }
        }
        // R6d: N distinto → valida el plan de remuestreo al máximo (vecino
        // más cercano) en vez de componer a ciegas. El player compone en
        // secuencia (no simultáneo) y no conoce viewports: el plan se valida
        // sobre conteos con viewport unitario; el viewport real se valida
        // donde sí se conoce (`AnimationGroup::plan_remuestreo` en el
        // protocolo y `Guion::items_para`/`a_playlist`).
        let conteos: Vec<usize> = items.iter().map(PlayItem::frames).collect();
        if conteos.len() >= 2 && conteos.windows(2).any(|w| w[0] != w[1]) {
            let indices: Vec<usize> = (0..conteos.len()).collect();
            let grupo = crate::protocol::AnimationGroup::try_new(indices, 0.0).map_err(|e| {
                SceneError::PresupuestoExcedido {
                    detalle: format!("remuestreo imposible: {e}"),
                }
            })?;
            let tamanos = vec![(1usize, 1usize); conteos.len()];
            grupo.plan_remuestreo(&conteos, &tamanos).map_err(|e| {
                SceneError::PresupuestoExcedido {
                    detalle: format!("remuestreo imposible: {e}"),
                }
            })?;
        }
        Self::play_clamp(scene, items, false)
    }

    fn play_clamp(
        scene: &mut Scene,
        items: Vec<PlayItem>,
        clamp: bool,
    ) -> SceneResult<Vec<PlayedFrame>> {
        let mut fondo = fondo_desde(scene, clamp)?;
        if fondo.len() > crate::scene::MAX_SCENE_LAYERS {
            fondo.truncate(crate::scene::MAX_SCENE_LAYERS);
        }
        let mut out: Vec<PlayedFrame> = Vec::new();
        for item in items {
            if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                break;
            }
            match item {
                PlayItem::Transform(mut anim) => {
                    let n = clamp_frames_de_set(&anim, clamp);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let alpha = alpha_en(fi, n);
                        let fila = match anim.frame_at(alpha) {
                            Ok(fila) => fila,
                            Err(e) => {
                                if clamp {
                                    Vec::new()
                                } else {
                                    return Err(e);
                                }
                            }
                        };
                        let center = centroide_pts(&fila);
                        let mut frame = fondo.clone();
                        if !fila.is_empty() {
                            let colocado = PlacedMobject::try_new(
                                Mobject::Polygon { pts: fila.clone() },
                                1.0,
                                1.0,
                                center,
                            );
                            empuja_colocado(&mut frame, colocado, clamp)?;
                        }
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                    let fila = match anim.frame_at(1.0) {
                        Ok(fila) => fila,
                        Err(e) => {
                            if clamp {
                                Vec::new()
                            } else {
                                return Err(e);
                            }
                        }
                    };
                    if !fila.is_empty() {
                        scene.layers.push(Mobject::Polygon { pts: fila });
                        acota_capas(scene);
                        fondo = fondo_desde(scene, clamp)?;
                    }
                }
                PlayItem::Create(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        let mut frame = fondo.clone();
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                    let traza = anim.traza_en(1.0);
                    if traza.len() >= 2 {
                        scene.layers.push(Mobject::Polygon { pts: traza });
                    } else if let Some(p) = traza.first() {
                        scene.layers.push(Mobject::Dot { x: p[0], y: p[1] });
                    }
                    acota_capas(scene);
                    fondo = fondo_desde(scene, clamp)?;
                }
                PlayItem::Write(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::Fade(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::GrowFromCenter(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::Indicate(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::Tracker(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        let colocado = anim.placed_at(alpha_en(fi, n));
                        empuja_colocado(&mut frame, colocado, clamp)?;
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::MatchingShapes(mut anim) => {
                    let n = anim.frames().clamp(1, PLAYER_MAX_FRAMES);
                    anim.begin();
                    for fi in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        let mut frame = fondo.clone();
                        match anim.frame_at(alpha_en(fi, n)) {
                            Ok(colocados) => frame.extend(colocados),
                            Err(e) => {
                                if !clamp {
                                    return Err(e);
                                }
                            }
                        }
                        out.push(PlayedFrame { objects: frame });
                    }
                    anim.finish();
                }
                PlayItem::Wait(w) => {
                    let n = w.frames().clamp(1, PLAYER_MAX_FRAMES);
                    let congelado = out
                        .last()
                        .map(|f| f.objects.clone())
                        .unwrap_or(fondo.clone());
                    for _ in 0..n {
                        if out.len() >= PLAYER_MAX_TOTAL_FRAMES {
                            break;
                        }
                        out.push(PlayedFrame {
                            objects: congelado.clone(),
                        });
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Fondo colocado desde las capas (R6d): en modo total salta la capa
/// inválida; en estricto es `Err` honesto. Puro, sin pánicos.
fn fondo_desde(scene: &Scene, clamp: bool) -> SceneResult<Vec<PlacedMobject>> {
    let mut fondo = Vec::with_capacity(scene.layers.len());
    for m in &scene.layers {
        match PlacedMobject::opaco(m.clone()) {
            Ok(p) => fondo.push(p),
            Err(e) => {
                if !clamp {
                    return Err(e);
                }
            }
        }
    }
    Ok(fondo)
}

/// Empuja un colocado al frame (R6d): en modo total salta el inválido;
/// en estricto es `Err` honesto. Puro, sin pánicos.
fn empuja_colocado(
    frame: &mut Vec<PlacedMobject>,
    colocado: SceneResult<PlacedMobject>,
    clamp: bool,
) -> SceneResult<()> {
    match colocado {
        Ok(p) => {
            frame.push(p);
            Ok(())
        }
        Err(e) => {
            if clamp {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Frames del set de un `TransformAnim` para el clamp del player.
/// El `TransformAnim` guarda `frames` privado: se estima vía `frames_suavizados`
/// (acotado a 48) sin exponer el campo.
fn clamp_frames_de_set(anim: &crate::scene::TransformAnim, clamp: bool) -> usize {
    let n = anim
        .frames_suavizados()
        .map(|f| f.len())
        .unwrap_or(1)
        .clamp(1, PLAYER_MAX_FRAMES);
    let _ = clamp;
    n
}

/// `alpha` crudo del frame `fi` en un item de `n` (0 en el primero, 1 en el
/// último; `n<=1` → 1.0 directo). Puro.
fn alpha_en(fi: usize, n: usize) -> f64 {
    if n <= 1 {
        1.0
    } else {
        ((fi.min(n.saturating_sub(1))) as f64) / ((n.saturating_sub(1)) as f64)
    }
}

fn acota_capas(scene: &mut Scene) {
    while scene.layers.len() > crate::scene::MAX_SCENE_LAYERS {
        scene.layers.remove(0);
    }
}

#[cfg(test)]
mod player_tests {
    use super::*;
    use crate::scene::{Ortho, TransformAnim};

    fn escena1() -> Scene {
        Scene::try_new(Ortho::default_16_9(), vec![Mobject::Axes], [10, 12, 16]).unwrap()
    }

    #[test]
    fn value_tracker_fija_suma_e_interpola() {
        let mut t = ValueTracker::try_new(0.5).unwrap();
        assert_eq!(t.get(), 0.5);
        assert!(ValueTracker::try_new(f64::NAN).is_err());
        t.set_value(2.0).unwrap();
        assert_eq!(t.get(), 2.0);
        assert!(t.set_value(f64::INFINITY).is_err());
        assert_eq!(t.get(), 2.0);
        t.increment_value(0.5).unwrap();
        assert_eq!(t.get(), 2.5);
        assert!(t.increment_value(f64::NAN).is_err());
        t.interpolate(0.0, 10.0, 0.25).unwrap();
        assert!((t.get() - 2.5).abs() < 1e-12);
        t.interpolate(0.0, 10.0, 2.0).unwrap();
        assert_eq!(t.get(), 10.0);
        assert!(t.interpolate(f64::NAN, 1.0, 0.5).is_err());
    }

    #[test]
    fn update_from_tracker_mapea_por_frame() {
        let u = UpdateFromTracker::try_new(
            Mobject::Dot { x: 0.0, y: 0.0 },
            0.0,
            10.0,
            5,
            1000,
            RateFunc::Linear,
            TrackerMap::CenterX { lo: -4.0, hi: 4.0 },
        )
        .unwrap();
        assert!((u.valor_en(0.0) - 0.0).abs() < 1e-12);
        assert!((u.valor_en(1.0) - 10.0).abs() < 1e-12);
        let p0 = u.placed_at(0.0).expect("tracker válido coloca");
        let p1 = u.placed_at(1.0).expect("tracker válido coloca");
        assert_eq!(p0.center, [-4.0, 0.0]);
        assert_eq!(p1.center, [4.0, 0.0]);
        // Opacidad mapea al rango pedido.
        let uo = UpdateFromTracker::try_new(
            Mobject::Dot { x: 0.0, y: 0.0 },
            0.0,
            1.0,
            3,
            500,
            RateFunc::Linear,
            TrackerMap::Opacity { lo: 0.0, hi: 1.0 },
        )
        .unwrap();
        assert_eq!(uo.placed_at(0.0).expect("tracker válido").opacity, 0.0);
        assert_eq!(uo.placed_at(1.0).expect("tracker válido").opacity, 1.0);
        assert!(UpdateFromTracker::try_new(
            Mobject::Dot { x: 0.0, y: 0.0 },
            f64::NAN,
            1.0,
            3,
            500,
            RateFunc::Linear,
            TrackerMap::Opacity { lo: 0.0, hi: 1.0 },
        )
        .is_err());
    }

    #[test]
    fn p01_run_60s_y_frames_cortos_intactos() {
        // P0.1: `run` acepta 60 s (60001 no); frames 48/96 corto intactos.
        assert_eq!(PLAYER_MAX_FRAMES, 48);
        assert_eq!(PLAYER_MAX_TOTAL_FRAMES, 96);
        let m = Mobject::Dot { x: 0.0, y: 0.0 };
        assert!(FadeAnim::try_new(m.clone(), true, 4, 60_000, RateFunc::Linear).is_ok());
        assert!(FadeAnim::try_new(m.clone(), true, 4, 60_001, RateFunc::Linear).is_err());
        assert!(FadeAnim::try_new(m.clone(), true, 49, 1000, RateFunc::Linear).is_err());
        assert!(FadeAnim::try_new(m, true, 4, 99, RateFunc::Linear).is_err());
    }

    #[test]
    fn create_traza_progresiva_y_cierra() {
        let poly = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let c = CreateAnim::try_new(poly, 5, 1000, RateFunc::Linear, false).unwrap();
        let t0 = c.traza_en(0.0);
        assert_eq!(t0.len(), 1);
        let t1 = c.traza_en(1.0);
        assert_eq!(t1.len(), 3);
        let t05 = c.traza_en(0.5);
        assert!(t05.len() >= 2);
        let p = c.placed_at(1.0).expect("create válido coloca");
        assert_eq!(p.opacity, 1.0);
        assert!(CreateAnim::try_new(vec![], 5, 1000, RateFunc::Linear, false).is_err());
    }

    #[test]
    fn write_es_revelado_opaco_honesto() {
        let tex = Mobject::tex_desde_texto("hola").unwrap();
        let w = WriteAnim::try_new(tex.clone(), 4, 1000, RateFunc::Smooth).unwrap();
        // El SVG viaja entero en todos los frames; solo cambia el alfa.
        assert_eq!(w.placed_at(0.0).expect("write válido").mobject, tex);
        assert_eq!(w.placed_at(1.0).expect("write válido").mobject, tex);
        assert!(
            w.placed_at(0.0).expect("write válido").opacity
                < w.placed_at(1.0).expect("write válido").opacity
        );
        assert_eq!(w.placed_at(1.0).expect("write válido").opacity, 1.0);
    }

    #[test]
    fn fade_grow_indicate_extremos() {
        let m = Mobject::Dot { x: 1.0, y: 2.0 };
        let fi = FadeAnim::try_new(m.clone(), true, 4, 500, RateFunc::Linear).unwrap();
        assert_eq!(fi.placed_at(0.0).expect("fade válido").opacity, 0.0);
        assert_eq!(fi.placed_at(1.0).expect("fade válido").opacity, 1.0);
        let fo = FadeAnim::try_new(m.clone(), false, 4, 500, RateFunc::Linear).unwrap();
        assert_eq!(fo.placed_at(0.0).expect("fade válido").opacity, 1.0);
        assert_eq!(fo.placed_at(1.0).expect("fade válido").opacity, 0.0);
        let g = GrowFromCenterAnim::try_new(m.clone(), 4, 500, RateFunc::Linear).unwrap();
        assert!(g.placed_at(0.0).expect("grow válido").scale <= 1e-5);
        assert_eq!(g.placed_at(1.0).expect("grow válido").scale, 1.0);
        let ind = IndicateAnim::try_new(m, 5, 500, RateFunc::Linear).unwrap();
        assert_eq!(ind.placed_at(0.0).expect("indicate válido").scale, 1.0);
        assert!((ind.placed_at(0.5).expect("indicate válido").scale - 1.2).abs() < 1e-6);
        assert_eq!(ind.placed_at(1.0).expect("indicate válido").scale, 1.0);
    }

    #[test]
    fn player_compne_fondo_mas_animado_y_congela_wait() {
        let mut escena = escena1();
        let items = vec![
            PlayItem::Create(
                CreateAnim::try_new(
                    vec![[0.0, 0.0], [2.0, 0.0]],
                    4,
                    1000,
                    RateFunc::Linear,
                    false,
                )
                .unwrap(),
            ),
            PlayItem::Wait(WaitAnim::try_new(2).unwrap()),
        ];
        let frames = ScenePlayer::play(&mut escena, items);
        assert_eq!(frames.len(), 6);
        // Cada frame trae fondo (1) + animado (1).
        assert!(frames.iter().all(|f| f.len() == 2));
        // El Wait congela: últimos 2 iguales.
        assert_eq!(frames[4].objects, frames[5].objects);
        // La escena final sumó la traza.
        assert!(escena.len() >= 2);
    }

    #[test]
    fn try_play_falla_honesto_en_presupuesto() {
        let mut escena = escena1();
        assert!(ScenePlayer::try_play(&mut escena, vec![]).is_err());
        // 97 waits de 1 superan el total 96.
        let muchos: Vec<PlayItem> = (0..97)
            .map(|_| PlayItem::Wait(WaitAnim::try_new(1).unwrap()))
            .collect();
        assert!(ScenePlayer::try_play(&mut escena, muchos).is_err());
        // `play` total en cambio clampa a 96 sin fallar.
        let muchos2: Vec<PlayItem> = (0..97)
            .map(|_| PlayItem::Wait(WaitAnim::try_new(1).unwrap()))
            .collect();
        let frames = ScenePlayer::play(&mut escena, muchos2);
        assert_eq!(frames.len(), PLAYER_MAX_TOTAL_FRAMES);
    }

    #[test]
    fn opaco_invalido_es_err_y_play_total_lo_salta() {
        // R6d: lo inválido jamás llega a pantalla como punto en el origen.
        let malo = Mobject::Circle {
            cx: 0.0,
            cy: 0.0,
            r: 0.0,
        };
        assert!(PlacedMobject::opaco(malo.clone()).is_err());
        // Escena con capa inválida (literal, sin `try_new`): `play` total
        // salta la capa y compone solo el animado; `try_play` falla honesto.
        let mut escena = Scene {
            camera: Ortho::default_16_9(),
            layers: vec![malo],
            bg: [10, 12, 16],
        };
        let item = || {
            PlayItem::Create(
                CreateAnim::try_new(
                    vec![[0.0, 0.0], [2.0, 0.0]],
                    4,
                    1000,
                    RateFunc::Linear,
                    false,
                )
                .unwrap(),
            )
        };
        let frames = ScenePlayer::play(&mut escena, vec![item()]);
        assert_eq!(frames.len(), 4);
        assert!(frames.iter().all(|f| f.len() == 1), "solo el animado");
        let mut escena2 = Scene {
            camera: Ortho::default_16_9(),
            layers: vec![Mobject::Circle {
                cx: 0.0,
                cy: 0.0,
                r: 0.0,
            }],
            bg: [10, 12, 16],
        };
        assert!(ScenePlayer::try_play(&mut escena2, vec![item()]).is_err());
    }

    #[test]
    fn try_play_valida_remuestreo_con_n_distinto() {
        // R6d: espejo de `Guion::items_para` — N distinto (8 vs 12) valida
        // el plan al máximo y reproduce; N inválido sigue fallando.
        let mut escena = escena1();
        let items = vec![
            PlayItem::Wait(WaitAnim::try_new(8).unwrap()),
            PlayItem::Wait(WaitAnim::try_new(12).unwrap()),
        ];
        let frames = ScenePlayer::try_play(&mut escena, items).expect("N distinto válido");
        assert_eq!(frames.len(), 20);
    }

    #[test]
    fn vmobject_alinea_y_transform_matching_empareja() {
        let v = VMobject::try_new(
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        )
        .unwrap();
        let a = v.align_data(8, false).unwrap();
        assert_eq!(a.len(), 8);
        assert_eq!(a.handles_in.len(), 8);
        assert!(VMobject::try_new(vec![], vec![], vec![]).is_err());
        assert!(v.align_data(1, false).is_err());
        // Matching 2→1: un par + un sobrante que desvanece.
        let m = TransformMatchingShapes::try_new(
            vec![
                Mobject::Dot { x: 0.0, y: 0.0 },
                Mobject::Dot { x: 5.0, y: 5.0 },
            ],
            vec![Mobject::Dot { x: 1.0, y: 1.0 }],
            8,
            3,
            1000,
            RateFunc::Linear,
            false,
            PathFunc::Straight,
        )
        .unwrap();
        let f0 = m.frame_at(0.0).expect("matching válido");
        assert_eq!(f0.len(), 2);
        let f1 = m.frame_at(1.0).expect("matching válido");
        assert_eq!(f1.len(), 2);
        // Transform con Arc mete panza perpendicular al viaje: la forma sube
        // 2 en Y y el arco la aparta en X (panza `sin(π/2)·0.25·2 = 0.5`).
        let t = TransformAnim::try_new(
            vec![[0.0, 0.0], [2.0, 0.0]],
            vec![[0.0, 2.0], [2.0, 2.0]],
            8,
            4,
            1000,
            RateFunc::Linear,
            false,
        )
        .unwrap()
        .with_path(PathFunc::Arc);
        let medio = t.frame_at(0.5).unwrap();
        // Recto con la misma grilla: la diferencia es la panza del arco.
        let r = TransformAnim::try_new(
            vec![[0.0, 0.0], [2.0, 0.0]],
            vec![[0.0, 2.0], [2.0, 2.0]],
            8,
            4,
            1000,
            RateFunc::Linear,
            false,
        )
        .unwrap();
        let recto = r.frame_at(0.5).unwrap();
        assert_eq!(medio.len(), recto.len());
        let max_d: f64 = medio
            .iter()
            .zip(recto.iter())
            .map(|(a, b)| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt())
            .fold(0.0, f64::max);
        assert!(max_d > 0.1, "el arco se aparta del recto, max_d={max_d}");
        // Extremos idénticos en ambos caminos.
        let a0 = t.frame_at(0.0).unwrap();
        let r0 = r.frame_at(0.0).unwrap();
        assert_eq!(a0.len(), r0.len());
    }
}
