//! Expansión de animaciones nivel 3Blue1Brown (Manim CE en Rust).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red. Todo lo que excede
//! presupuestos es `Err` honesto en rioplatense; jamás nada parcial en
//! silencio.
//!
//! - Rates puras `f64 -> f64`: [`linear`], [`smooth`], [`rush_in`],
//!   [`rush_out`], [`rush_in_out`], [`there_and_back`], [`wobble`].
//!   Fórmulas exactas Manim CE (`manim/utils/rate_functions.py`,
//!   `inflection=10`, `wiggles=2`): `smooth` es el smoothstep polinómico
//!   (`3t²-2t³`, igual que [`RateFunc::Smooth`]); `rush_in`/`rush_out` son
//!   `rush_into`/`rush_from` logísticos; `there_and_back` es el logístico
//!   CE (0→1→0 con meseta suave, NO el `sin(π·t)` de
//!   [`RateFunc::ThereAndBack`], que se conserva por compat); `wobble` es
//!   `wiggle` CE (`there_and_back·sin(2π·t)`, igual que
//!   `RateFunc::WiggleK(2)`).
//! - [`Animation`]: enum `Create`/`FadeIn`/`FadeOut`/`Write`/`Transform`/
//!   `Indicate`/`Circumscribe`/`FadeShift` con `sample(alpha)` (conveniencia)
//!   y `sample_con(alpha, scratch)` (sampler con [`Scratch`] reutilizable:
//!   cero allocs intermedios no acotados; la salida `PlacedMobject` es
//!   dueña de sus puntos por contrato con la Piel, así que cada frame
//!   reserva lo justo para el colocado —acotado a ≤4096 pts— y nada más).
//! - [`LaggedStart`]: N sub-anims con `lag_ratio` (timings exactos Manim
//!   `composition.py`: `start[i+1] = start[i] + run[i]·lag`, cada sub-anim
//!   aplica su propio rate al sub-alpha crudo). [`Succession`]: secuencia
//!   (`lag_ratio = 1`, solo la activa muestrea; las terminadas quedan como
//!   estado final en la escena del llamador).
//! - [`CircumscribeAnim`]: anillo temporal alrededor del mobject
//!   (`Rectangle` o `Circle`, `buff`, `time_width` fraccional, `fade_in`/
//!   `fade_out` como Manim `indication.py`); [`FadeShiftAnim`]: aparición
//!   con desplazamiento (Manim `FadeIn` con `shift`).
//!
//! Presupuestos (paridad con el resto del crate): frames por anim 1..=48
//! ([`PLAYER_MAX_FRAMES`]), total compuesto ≤96
//! ([`PLAYER_MAX_TOTAL_FRAMES`]), sub-anims 1..=32 (paridad
//! `MAX_GROUP_CHILDREN`), puntos por polilínea 1..=4096
//! (`MAX_MOBJECT_POINTS`), remuestreo 2..=512, `run` 100..=60000 ms.

use crate::player::{
    centroide_de, valida_frames_run, CreateAnim, FadeAnim, IndicateAnim, PlacedMobject, WriteAnim,
};
use crate::scene::{
    Animation as Animacion, Mobject, RateFunc, SceneError, SceneResult, TransformAnim,
    MAX_MOBJECT_POINTS,
};

// ── Rates puras (Manim CE exactas) ──────────────────────────────────────────

/// Guarda finita 0..1 (`NaN`/inf → 0). Pura.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Sigmoide logística Manim (`simple_functions.py`). Pura, sin `NaN`.
fn sigmoide(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_sign_positive() { 1.0 } else { 0.0 };
    }
    1.0 / (1.0 + (-x).exp())
}

/// `smooth` logístico Manim (`inflection=10`): 0, 0.5 y 1 exactos. Puro.
fn suave_logistico(t: f64) -> f64 {
    let t = clamp01(t);
    let err = sigmoide(-5.0);
    let v = (sigmoide(10.0 * (t - 0.5)) - err) / (1.0 - 2.0 * err);
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        t
    }
}

/// Identidad (`linear` Manim). Pura.
pub fn linear(t: f64) -> f64 {
    clamp01(t)
}

/// Smoothstep Manim (`3t²-2t³`; == `RateFunc::Smooth`). Pura y monótona.
pub fn smooth(t: f64) -> f64 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

/// Entrada con prisa Manim (`rush_into = 2·smooth(t/2)`). Pura.
pub fn rush_in(t: f64) -> f64 {
    let v = 2.0 * suave_logistico(clamp01(t) / 2.0);
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        clamp01(t)
    }
}

/// Salida con prisa Manim (`rush_from = 2·smooth(t/2+0.5)-1`). Pura.
pub fn rush_out(t: f64) -> f64 {
    let v = 2.0 * suave_logistico(clamp01(t) / 2.0 + 0.5) - 1.0;
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        clamp01(t)
    }
}

/// Rush simétrico cuadrático (== `RateFunc::RushInOut`). Puro.
pub fn rush_in_out(t: f64) -> f64 {
    let t = clamp01(t);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
    }
}

/// Ida y vuelta logística Manim (`there_and_back`, 0→1→0). Pura.
pub fn there_and_back(t: f64) -> f64 {
    let t = clamp01(t);
    let v = if t < 0.5 {
        suave_logistico(2.0 * t)
    } else {
        suave_logistico(2.0 - 2.0 * t)
    };
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Bamboleo Manim (`wiggle` con `wiggles=2`). Puro, cero en 0/0.5/1.
pub fn wobble(t: f64) -> f64 {
    let t = clamp01(t);
    let v = there_and_back(t) * (2.0 * std::f64::consts::PI * t).sin();
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

// ── Scratch reutilizable ────────────────────────────────────────────────────

/// Buffers reutilizables del sampler (cero allocs por frame una vez
/// crecidos: `pts` guarda la traza/morph de salida, `cum` las longitudes
/// acumuladas; ambos se `clear` y rellenan por frame).
#[derive(Debug, Clone, Default)]
pub struct Scratch {
    pts: Vec<[f64; 2]>,
    cum: Vec<f64>,
}

impl Scratch {
    /// Nuevo (reserva 512 pts + 513 largos: el caso común sin regrow).
    pub fn nuevo() -> Self {
        Self {
            pts: Vec::with_capacity(512),
            cum: Vec::with_capacity(513),
        }
    }

    /// Prefijo de `poly` hasta la fracción `s` por longitud de arco,
    /// escribiendo en el buffer y devolviendo el slice (sin allocs).
    /// `s<=0` → primer punto; `s>=1` → toda; vacía → vacía.
    pub fn traza<'a>(&'a mut self, poly: &[[f64; 2]], s: f64, closed: bool) -> &'a [[f64; 2]] {
        self.pts.clear();
        self.cum.clear();
        if poly.is_empty() {
            return &self.pts;
        }
        if poly.len() == 1 || s >= 1.0 {
            self.pts.extend_from_slice(poly);
            return &self.pts;
        }
        if s <= 0.0 {
            self.pts.push(poly[0]);
            return &self.pts;
        }
        let n = poly.len();
        let segs = if closed { n } else { n.saturating_sub(1) };
        if segs == 0 {
            self.pts.push(poly[0]);
            return &self.pts;
        }
        let mut total = 0.0;
        self.cum.push(0.0);
        for k in 0..segs {
            let (a, b) = if closed {
                (poly[k % n], poly[(k + 1) % n])
            } else {
                (poly[k], poly[k + 1])
            };
            let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let d = if d.is_finite() && d > 0.0 { d } else { 0.0 };
            total += d;
            self.cum.push(total);
        }
        if total <= 0.0 {
            self.pts.push(poly[0]);
            return &self.pts;
        }
        let objetivo = total * s.clamp(0.0, 1.0);
        self.pts.push(poly[0]);
        let mut acumulado = 0.0;
        for k in 0..segs {
            let (a, b) = if closed {
                (poly[k % n], poly[(k + 1) % n])
            } else {
                (poly[k], poly[k + 1])
            };
            let d = self.cum.get(k + 1).copied().unwrap_or(0.0) - acumulado;
            // `d` sale del mismo `cum` recién calculado: si no es finito o
            // no avanza, se salta el segmento sin romper el prefijo.
            if !d.is_finite() || d <= 0.0 {
                continue;
            }
            if acumulado + d >= objetivo {
                let u = ((objetivo - acumulado) / d).clamp(0.0, 1.0);
                let x = a[0] + (b[0] - a[0]) * u;
                let y = a[1] + (b[1] - a[1]) * u;
                if x.is_finite() && y.is_finite() {
                    self.pts.push([x, y]);
                }
                break;
            }
            acumulado += d;
            if closed {
                self.pts.push(poly[(k + 1) % n]);
            } else if k + 1 < n {
                self.pts.push(poly[k + 1]);
            }
        }
        if self.pts.len() < 2 {
            self.pts.push(poly[0]);
        }
        &self.pts
    }
}

// ── Utilidades de validación ────────────────────────────────────────────────

fn valida_poly(poly: &[[f64; 2]], donde: &'static str) -> SceneResult<()> {
    if poly.is_empty() || poly.len() > MAX_MOBJECT_POINTS {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("{} puntos (válido 1..={MAX_MOBJECT_POINTS})", poly.len()),
        });
    }
    for (i, p) in poly.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde,
                detalle: format!("punto no finito en el índice {i}"),
            });
        }
    }
    Ok(())
}

/// Eased clamp 0..1 (`alpha` crudo con el rate dado; no finito → 0). Puro.
fn eased(rate: RateFunc, alpha: f64) -> f64 {
    let e = rate.apply(alpha);
    if e.is_finite() {
        e.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

// ── Circumscribe ────────────────────────────────────────────────────────────

/// Forma del anillo de [`CircumscribeAnim`] (Manim: `Rectangle` o `Circle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormaResalto {
    /// Rectángulo alrededor (default Manim).
    #[default]
    Rectangulo,
    /// Círculo alrededor.
    Circulo,
}

/// Caja honesta del mobject (`None` = sin geometría de puntos: `Tex`,
/// `Axes`, campos; el llamador usa fallback centrado). Pura, sin allocs.
fn caja_de(m: &Mobject) -> Option<([f64; 2], [f64; 2])> {
    fn mete(caja: &mut Option<([f64; 2], [f64; 2])>, p: [f64; 2]) {
        if !p[0].is_finite() || !p[1].is_finite() {
            return;
        }
        match caja {
            None => *caja = Some((p, p)),
            Some((lo, hi)) => {
                lo[0] = lo[0].min(p[0]);
                lo[1] = lo[1].min(p[1]);
                hi[0] = hi[0].max(p[0]);
                hi[1] = hi[1].max(p[1]);
            }
        }
    }
    fn recorre(m: &Mobject, caja: &mut Option<([f64; 2], [f64; 2])>) {
        match m {
            Mobject::Dot { x, y } => mete(caja, [*x, *y]),
            Mobject::Polygon { pts } => {
                for p in pts {
                    mete(caja, *p);
                }
            }
            Mobject::Line { from, to } | Mobject::Arrow { from, to } => {
                mete(caja, *from);
                mete(caja, *to);
            }
            Mobject::Tri { a, b, c, .. } => {
                mete(caja, *a);
                mete(caja, *b);
                mete(caja, *c);
            }
            Mobject::Circle { cx, cy, r } => {
                mete(caja, [*cx - *r, *cy - *r]);
                mete(caja, [*cx + *r, *cy + *r]);
            }
            Mobject::Square { cx, cy, side } => {
                let h = *side / 2.0;
                mete(caja, [*cx - h, *cy - h]);
                mete(caja, [*cx + h, *cy + h]);
            }
            Mobject::Rectangle { cx, cy, w, h } => {
                mete(caja, [*cx - *w / 2.0, *cy - *h / 2.0]);
                mete(caja, [*cx + *w / 2.0, *cy + *h / 2.0]);
            }
            Mobject::Ellipse { cx, cy, rx, ry } => {
                mete(caja, [*cx - *rx, *cy - *ry]);
                mete(caja, [*cx + *rx, *cy + *ry]);
            }
            Mobject::Arc { cx, cy, r, .. } => {
                mete(caja, [*cx - *r, *cy - *r]);
                mete(caja, [*cx + *r, *cy + *r]);
            }
            Mobject::Group(hijos) => {
                for h in hijos {
                    recorre(h, caja);
                }
            }
            Mobject::Axes
            | Mobject::FunctionGraph { .. }
            | Mobject::ArrowField { .. }
            | Mobject::Tex { .. }
            | Mobject::NumberPlane { .. }
            | Mobject::VectorField { .. } => {}
        }
    }
    let mut caja = None;
    recorre(m, &mut caja);
    caja
}

/// `Circumscribe` Manim: anillo temporal que se dibuja, sostiene y se
/// borra (o `fade_in`/`fade_out`) alrededor del mobject.
#[derive(Debug, Clone)]
pub struct CircumscribeAnim {
    mobject: Mobject,
    anillo: Vec<[f64; 2]>,
    centro: [f64; 2],
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    /// Fracción del total para dibujar/borrar (0..=0.5, Manim `time_width`).
    tw: f64,
    fade_in: bool,
    fade_out: bool,
}

impl CircumscribeAnim {
    /// Constructor validado (`buff` 0..=4096 finito, `tw` 0..=0.5 finito).
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        forma: FormaResalto,
        buff: f64,
        tw: f64,
        fade_in: bool,
        fade_out: bool,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "Circumscribe")?;
        mobject.validate()?;
        if !buff.is_finite() || !(0.0..=4096.0).contains(&buff) {
            return Err(SceneError::MobjectInvalido {
                donde: "Circumscribe",
                detalle: format!("buff {buff} fuera de 0..=4096"),
            });
        }
        if !tw.is_finite() || !(0.0..=0.5).contains(&tw) {
            return Err(SceneError::MobjectInvalido {
                donde: "Circumscribe",
                detalle: format!("time_width {tw} fuera de 0..=0.5"),
            });
        }
        let centro = centroide_de(&mobject);
        // Sin caja (Tex/Axes/campos): rectángulo unitario centrado (honesto,
        // documentado; Manim usa `get_center` + tamaño del mobject igual).
        let (lo, hi) = caja_de(&mobject).unwrap_or((
            [centro[0] - 0.5, centro[1] - 0.5],
            [centro[0] + 0.5, centro[1] + 0.5],
        ));
        let anillo = match forma {
            FormaResalto::Rectangulo => vec![
                [lo[0] - buff, lo[1] - buff],
                [hi[0] + buff, lo[1] - buff],
                [hi[0] + buff, hi[1] + buff],
                [lo[0] - buff, hi[1] + buff],
            ],
            FormaResalto::Circulo => {
                let cx = (lo[0] + hi[0]) / 2.0;
                let cy = (lo[1] + hi[1]) / 2.0;
                let dx = (hi[0] - lo[0]) / 2.0;
                let dy = (hi[1] - lo[1]) / 2.0;
                let r = (dx * dx + dy * dy).sqrt() + buff;
                let r = if r.is_finite() && r > 0.0 {
                    r
                } else {
                    buff + 0.5
                };
                let mut anillo = Vec::with_capacity(24);
                for k in 0..24 {
                    let a = 2.0 * std::f64::consts::PI * (f64::from(k)) / 24.0;
                    anillo.push([cx + r * a.cos(), cy + r * a.sin()]);
                }
                anillo
            }
        };
        Ok(Self {
            mobject,
            anillo,
            centro,
            frames,
            run_ms,
            rate,
            tw,
            fade_in,
            fade_out,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Fracción visible del anillo en `alpha` crudo (0..1).
    fn fraccion_en(&self, alpha: f64) -> f64 {
        let e = eased(self.rate, alpha);
        if self.tw <= 0.0 {
            return 1.0;
        }
        if e < self.tw {
            e / self.tw
        } else if e > 1.0 - self.tw {
            (1.0 - e) / self.tw
        } else {
            1.0
        }
        .clamp(0.0, 1.0)
    }

    /// Opacidad del anillo en `alpha` crudo (ramps si hay fades).
    fn opacidad_en(&self, alpha: f64) -> f32 {
        let e = eased(self.rate, alpha);
        let mut o = 1.0;
        if self.fade_in && self.tw > 0.0 && e < self.tw {
            o *= e / self.tw;
        }
        if self.fade_out && self.tw > 0.0 && e > 1.0 - self.tw {
            o *= (1.0 - e) / self.tw;
        }
        (o.clamp(0.0, 1.0)) as f32
    }

    /// Colocado en `alpha` (conveniencia; reserva scratch temporal).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let mut scratch = Scratch::nuevo();
        self.placed_con(alpha, &mut scratch)
    }

    /// Colocado en `alpha` reutilizando `scratch` (sin allocs intermedios;
    /// solo el colocado de salida). Con `fade_in`/`fade_out` el anillo viaja
    /// entero y el progreso es opacidad (igual que `Write` con SVG opaco);
    /// sin fades se dibuja/borra por trazo. Puro.
    pub fn placed_con(&self, alpha: f64, scratch: &mut Scratch) -> SceneResult<PlacedMobject> {
        let e = eased(self.rate, alpha);
        let o = self.opacidad_en(alpha);
        // Fracción de trazo: 1.0 donde el fade reemplaza al dibujo.
        let en_dibujo = self.tw > 0.0 && e < self.tw;
        let en_borrado = self.tw > 0.0 && e > 1.0 - self.tw;
        let f = self.fraccion_en(alpha);
        let frac = if (en_dibujo && self.fade_in) || (en_borrado && self.fade_out) {
            1.0
        } else {
            f
        };
        let traza: Vec<[f64; 2]> = scratch.traza(&self.anillo, frac, true).to_vec();
        if traza.len() >= 2 {
            return PlacedMobject::try_new(Mobject::Polygon { pts: traza }, o, 1.0, self.centro);
        }
        // Anillo degenerado: el mobject original mudo (jamás punto falso).
        PlacedMobject::try_new(self.mobject.clone(), 0.0, 1.0, self.centro)
    }
}

impl Animacion for CircumscribeAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

// ── FadeShift ───────────────────────────────────────────────────────────────

/// `FadeIn` con desplazamiento Manim (`shift` finito; el centro viaja
/// `base+shift → base` mientras la opacidad va 0→1).
#[derive(Debug, Clone)]
pub struct FadeShiftAnim {
    mobject: Mobject,
    base: [f64; 2],
    shift: [f64; 2],
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
}

impl FadeShiftAnim {
    /// Constructor validado (`shift` finito).
    pub fn try_new(
        mobject: Mobject,
        shift: [f64; 2],
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        valida_frames_run(frames, run_ms, "FadeShift")?;
        mobject.validate()?;
        if !shift[0].is_finite() || !shift[1].is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "FadeShift",
                detalle: "shift no finito".to_string(),
            });
        }
        let base = centroide_de(&mobject);
        Ok(Self {
            mobject,
            base,
            shift,
            frames,
            run_ms,
            rate,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Colocado en `alpha` crudo (opacidad eased + centro desplazado).
    pub fn placed_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let e = eased(self.rate, alpha);
        let o = (e.clamp(0.0, 1.0)) as f32;
        let cx = self.base[0] + self.shift[0] * (1.0 - e);
        let cy = self.base[1] + self.shift[1] * (1.0 - e);
        let cx = if cx.is_finite() { cx } else { self.base[0] };
        let cy = if cy.is_finite() { cy } else { self.base[1] };
        PlacedMobject::try_new(self.mobject.clone(), o, 1.0, [cx, cy])
    }
}

impl Animacion for FadeShiftAnim {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

// ── Animation (enum unificado) ──────────────────────────────────────────────

/// Animación 3Blue1Brown unificada: `sample(alpha)` da el colocado en el
/// progreso crudo 0..1 (cada variante aplica su propio rate, como Manim).
#[derive(Debug, Clone)]
pub enum Animation {
    /// Trazo progresivo de polilínea.
    Create(CreateAnim),
    /// Aparición por alfa.
    FadeIn(FadeAnim),
    /// Desaparición por alfa.
    FadeOut(FadeAnim),
    /// Revelado por trazo (u opacidad honesta sin trazo).
    Write(WriteAnim),
    /// Morph polilínea→polilínea (guarda extremos para reconstruir).
    Transform {
        from: Vec<[f64; 2]>,
        to: Vec<[f64; 2]>,
        samples: usize,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
    },
    /// Pulso ×1.2 ida-vuelta.
    Indicate(IndicateAnim),
    /// Anillo temporal alrededor.
    Circumscribe(CircumscribeAnim),
    /// Aparición con desplazamiento.
    FadeShift(FadeShiftAnim),
}

impl Animation {
    /// `Create` validado.
    pub fn create(
        poly: Vec<[f64; 2]>,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
    ) -> SceneResult<Self> {
        Ok(Self::Create(CreateAnim::try_new(
            poly, frames, run_ms, rate, closed,
        )?))
    }

    /// `FadeIn` validado.
    pub fn fade_in(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self::FadeIn(FadeAnim::try_new(
            mobject, true, frames, run_ms, rate,
        )?))
    }

    /// `FadeOut` validado.
    pub fn fade_out(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self::FadeOut(FadeAnim::try_new(
            mobject, false, frames, run_ms, rate,
        )?))
    }

    /// `Write` validado.
    pub fn write(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self::Write(WriteAnim::try_new(
            mobject, frames, run_ms, rate,
        )?))
    }

    /// `Transform` validado (valida extremos ahora; el `TransformAnim` se
    /// reconstruye por muestra —costo acotado a ≤512 pts—).
    pub fn transform(
        from: Vec<[f64; 2]>,
        to: Vec<[f64; 2]>,
        samples: usize,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        closed: bool,
    ) -> SceneResult<Self> {
        valida_poly(&from, "Transform.from")?;
        valida_poly(&to, "Transform.to")?;
        // Falla temprano si los parámetros no pasan el pipeline.
        TransformAnim::try_new(
            from.clone(),
            to.clone(),
            samples,
            frames,
            run_ms,
            rate,
            closed,
        )?;
        Ok(Self::Transform {
            from,
            to,
            samples,
            frames,
            run_ms,
            rate,
            closed,
        })
    }

    /// `Indicate` validado.
    pub fn indicate(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self::Indicate(IndicateAnim::try_new(
            mobject, frames, run_ms, rate,
        )?))
    }

    /// `Circumscribe` validado.
    #[allow(clippy::too_many_arguments)]
    pub fn circumscribe(
        mobject: Mobject,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
        forma: FormaResalto,
        buff: f64,
        tw: f64,
        fade_in: bool,
        fade_out: bool,
    ) -> SceneResult<Self> {
        Ok(Self::Circumscribe(CircumscribeAnim::try_new(
            mobject, frames, run_ms, rate, forma, buff, tw, fade_in, fade_out,
        )?))
    }

    /// `FadeShift` validado.
    pub fn fade_shift(
        mobject: Mobject,
        shift: [f64; 2],
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self::FadeShift(FadeShiftAnim::try_new(
            mobject, shift, frames, run_ms, rate,
        )?))
    }

    /// Fotogramas de la animación.
    pub fn frames(&self) -> usize {
        match self {
            Self::Create(a) => a.frames(),
            Self::FadeIn(a) | Self::FadeOut(a) => a.frames(),
            Self::Write(a) => a.frames(),
            Self::Transform { frames, .. } => *frames,
            Self::Indicate(a) => a.frames(),
            Self::Circumscribe(a) => a.frames(),
            Self::FadeShift(a) => a.frames(),
        }
    }

    /// Duración en ms.
    pub fn run_ms(&self) -> u64 {
        match self {
            Self::Create(a) => a.run_time_ms(),
            Self::FadeIn(a) | Self::FadeOut(a) => a.run_time_ms(),
            Self::Write(a) => a.run_time_ms(),
            Self::Transform { run_ms, .. } => *run_ms,
            Self::Indicate(a) => a.run_time_ms(),
            Self::Circumscribe(a) => a.run_time_ms(),
            Self::FadeShift(a) => a.run_time_ms(),
        }
    }

    /// Rate de la animación.
    pub fn rate(&self) -> RateFunc {
        match self {
            Self::Create(a) => a.rate(),
            Self::FadeIn(a) | Self::FadeOut(a) => a.rate(),
            Self::Write(a) => a.rate(),
            Self::Transform { rate, .. } => *rate,
            Self::Indicate(a) => a.rate(),
            Self::Circumscribe(a) => a.rate(),
            Self::FadeShift(a) => a.rate(),
        }
    }

    /// Muestra en `alpha` crudo (conveniencia; para loops usar
    /// [`Animation::sample_con`] con [`Scratch`] reutilizado).
    pub fn sample(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let mut scratch = Scratch::nuevo();
        self.sample_con(alpha, &mut scratch)
    }

    /// Muestra en `alpha` crudo reutilizando `scratch` (las variantes de
    /// trazo no allocan intermedios; la salida reserva lo justo).
    pub fn sample_con(&self, alpha: f64, scratch: &mut Scratch) -> SceneResult<PlacedMobject> {
        match self {
            Self::Create(a) => a.placed_at(alpha),
            Self::FadeIn(a) | Self::FadeOut(a) => a.placed_at(alpha),
            Self::Write(a) => a.placed_at(alpha),
            Self::Transform {
                from,
                to,
                samples,
                frames,
                run_ms,
                rate,
                closed,
            } => {
                let anim = TransformAnim::try_new(
                    from.clone(),
                    to.clone(),
                    *samples,
                    *frames,
                    *run_ms,
                    *rate,
                    *closed,
                )?;
                let fila = anim.frame_at(alpha)?;
                let centro = centroide_de(&Mobject::Polygon { pts: fila.clone() });
                let _ = scratch;
                PlacedMobject::try_new(Mobject::Polygon { pts: fila }, 1.0, 1.0, centro)
            }
            Self::Indicate(a) => a.placed_at(alpha),
            Self::Circumscribe(a) => a.placed_con(alpha, scratch),
            Self::FadeShift(a) => a.placed_at(alpha),
        }
    }
}

// ── LaggedStart / Succession ────────────────────────────────────────────────

/// Tope de sub-anims por grupo (paridad `MAX_GROUP_CHILDREN`).
pub const ANIMS_MAX_SUBANIMS: usize = 32;

/// `LaggedStart` Manim: N sub-anims con `lag_ratio` (0 = todo junto,
/// 1 = secuencia). Timings exactos `composition.py`, cada sub-anim con su
/// propio rate sobre el sub-alpha crudo.
#[derive(Debug, Clone)]
pub struct LaggedStart {
    anims: Vec<Animation>,
    lag_ratio: f64,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
    inicios: Vec<f64>,
    fines: Vec<f64>,
    fin_max: f64,
}

impl LaggedStart {
    /// Constructor validado (1..=32 sub-anims, `lag` 0..=1 finito,
    /// frames 1..=48, `run` 100..=60000 ms).
    pub fn try_new(
        anims: Vec<Animation>,
        lag_ratio: f64,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        if anims.is_empty() || anims.len() > ANIMS_MAX_SUBANIMS {
            return Err(SceneError::MobjectInvalido {
                donde: "LaggedStart",
                detalle: format!(
                    "{} sub-anims (válido 1..={ANIMS_MAX_SUBANIMS})",
                    anims.len()
                ),
            });
        }
        if !lag_ratio.is_finite() || !(0.0..=1.0).contains(&lag_ratio) {
            return Err(SceneError::MobjectInvalido {
                donde: "LaggedStart",
                detalle: format!("lag_ratio {lag_ratio} fuera de 0..=1"),
            });
        }
        valida_frames_run(frames, run_ms, "LaggedStart")?;
        // Pesos = run_ms de cada sub-anim (Manim usa sus `run_time`).
        let mut inicios = Vec::with_capacity(anims.len());
        let mut fines = Vec::with_capacity(anims.len());
        let mut inicio = 0.0;
        for a in &anims {
            let peso = (a.run_ms().max(1)) as f64;
            inicios.push(inicio);
            fines.push(inicio + peso);
            inicio += peso * lag_ratio;
        }
        let mut fin_max = 0.0;
        for f in &fines {
            if f.is_finite() && *f > fin_max {
                fin_max = *f;
            }
        }
        if !fin_max.is_finite() || fin_max <= 0.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "LaggedStart",
                detalle: "timings degenerados: revisá los run_ms".to_string(),
            });
        }
        Ok(Self {
            anims,
            lag_ratio,
            frames,
            run_ms,
            rate,
            inicios,
            fines,
            fin_max,
        })
    }

    /// Sub-animaciones.
    pub fn anims(&self) -> &[Animation] {
        &self.anims
    }

    /// `lag_ratio`.
    pub fn lag_ratio(&self) -> f64 {
        self.lag_ratio
    }

    /// Fotogramas del grupo.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Tiempo global eased en `alpha` crudo. Puro.
    fn tiempo_en(&self, alpha: f64) -> f64 {
        eased(self.rate, alpha) * self.fin_max
    }

    /// Sub-alpha crudo de la sub-anim `i` en el tiempo global `t`
    /// (clamp 0..1; peso nulo → 1 si ya empezó). Puro.
    fn sub_alpha(&self, i: usize, t: f64) -> f64 {
        let inicio = self.inicios.get(i).copied().unwrap_or(0.0);
        let fin = self.fines.get(i).copied().unwrap_or(0.0);
        let peso = fin - inicio;
        if !peso.is_finite() || peso <= 0.0 {
            return if t >= inicio { 1.0 } else { 0.0 };
        }
        ((t - inicio) / peso).clamp(0.0, 1.0)
    }

    /// ¿La sub-anim `i` ya empezó en `t`?
    fn empezada(&self, i: usize, t: f64) -> bool {
        self.inicios.get(i).copied().unwrap_or(0.0) <= t
    }

    /// Colocados en `alpha` crudo: todas las empezadas (las terminadas en
    /// su estado final, como la escena Manim). `Err` honesto si alguna
    /// empezada no coloca.
    pub fn sample_at(&self, alpha: f64) -> SceneResult<Vec<PlacedMobject>> {
        let mut out = Vec::new();
        self.sample_en(alpha, &mut out)?;
        Ok(out)
    }

    /// Idem reutilizando `out` (solo reserva lo justo para los colocados).
    pub fn sample_en(&self, alpha: f64, out: &mut Vec<PlacedMobject>) -> SceneResult<()> {
        out.clear();
        let t = self.tiempo_en(alpha);
        for (i, anim) in self.anims.iter().enumerate() {
            if !self.empezada(i, t) {
                continue;
            }
            out.push(anim.sample(self.sub_alpha(i, t))?);
        }
        Ok(())
    }
}

impl Animacion for LaggedStart {
    fn run_time_ms(&self) -> u64 {
        self.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.rate
    }
}

/// `Succession` Manim: secuencia (`lag_ratio = 1`); solo la activa
/// muestrea en `alpha` (las terminadas quedan como estado final en la
/// escena del llamador, igual que Manim tras `next_animation`).
#[derive(Debug, Clone)]
pub struct Succession {
    grupo: LaggedStart,
}

impl Succession {
    /// Constructor validado (misma validación que [`LaggedStart` con lag 1).
    pub fn try_new(
        anims: Vec<Animation>,
        frames: usize,
        run_ms: u64,
        rate: RateFunc,
    ) -> SceneResult<Self> {
        Ok(Self {
            grupo: LaggedStart::try_new(anims, 1.0, frames, run_ms, rate)?,
        })
    }

    /// Fotogramas de la secuencia.
    pub fn frames(&self) -> usize {
        self.grupo.frames()
    }

    /// Índice de la sub-anim activa en `alpha` crudo (la última empezada).
    pub fn indice_activo(&self, alpha: f64) -> usize {
        let t = self.grupo.tiempo_en(alpha);
        let mut activo = 0;
        for (i, _) in self.grupo.anims.iter().enumerate() {
            if self.grupo.empezada(i, t) {
                activo = i;
            }
        }
        activo
    }

    /// Colocado de la activa en `alpha` crudo.
    pub fn sample_at(&self, alpha: f64) -> SceneResult<PlacedMobject> {
        let t = self.grupo.tiempo_en(alpha);
        let i = self.indice_activo(alpha);
        let anim = self.grupo.anims.get(i).ok_or(SceneError::MobjectInvalido {
            donde: "Succession",
            detalle: "sin sub-anims: nada para muestrear".to_string(),
        })?;
        anim.sample(self.grupo.sub_alpha(i, t))
    }
}

impl Animacion for Succession {
    fn run_time_ms(&self) -> u64 {
        self.grupo.run_ms
    }
    fn rate(&self) -> RateFunc {
        self.grupo.rate
    }
}

// ── Tests inline (evita cableado extra: mismo gate `cargo test`) ────────────

#[cfg(test)]
mod anims_tests {
    use super::*;

    fn punto() -> Mobject {
        Mobject::Dot { x: 0.0, y: 0.0 }
    }

    #[test]
    fn rates_en_bordes() {
        // Bordes exactos Manim (`unit_interval`/`zero` upstream).
        assert_eq!(linear(0.0), 0.0);
        assert_eq!(linear(1.0), 1.0);
        assert_eq!(smooth(0.0), 0.0);
        assert_eq!(smooth(1.0), 1.0);
        assert!((smooth(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(rush_in(0.0), 0.0);
        assert!((rush_in(1.0) - 1.0).abs() < 1e-9);
        assert_eq!(rush_out(0.0), 0.0);
        assert!((rush_out(1.0) - 1.0).abs() < 1e-9);
        assert_eq!(rush_in_out(0.0), 0.0);
        assert_eq!(rush_in_out(1.0), 1.0);
        assert_eq!(there_and_back(0.0), 0.0);
        assert!((there_and_back(0.5) - 1.0).abs() < 1e-9);
        assert_eq!(there_and_back(1.0), 0.0);
        assert_eq!(wobble(0.0), 0.0);
        assert!(wobble(0.5).abs() < 1e-9);
        assert!(wobble(1.0).abs() < 1e-9);
        // No finitos → guardia honesta, sin `NaN` ni pánico.
        for f in [
            linear,
            smooth,
            rush_in,
            rush_out,
            rush_in_out,
            there_and_back,
            wobble,
        ] {
            for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5, 1.5] {
                let v = f(t);
                assert!(v.is_finite(), "rate({t}) dio {v}");
            }
        }
    }

    #[test]
    fn smooth_es_monotona() {
        // `smooth` nunca retrocede (101 muestras) y arranca/termina suave.
        let mut anterior = 0.0;
        for k in 0..=100 {
            let v = smooth(f64::from(k) / 100.0);
            assert!(v + 1e-12 >= anterior, "bajó en k={k}: {v} < {anterior}");
            anterior = v;
        }
        assert!(smooth(0.1) < 0.1, "arranque suave");
        assert!(smooth(0.9) > 0.9, "llegada suave");
        // `rush_in` arranca más lento que lineal y `rush_out` termina igual.
        assert!(rush_in(0.25) < 0.25);
        assert!(rush_out(0.75) > 0.75);
    }

    #[test]
    fn lagged_start_solapa() {
        // 3 fades de igual run con lag 0.5: arranques en 0, 0.5·w y 1.0·w.
        let fade = || Animation::fade_in(punto(), 4, 1000, RateFunc::Linear).unwrap();
        let g = LaggedStart::try_new(vec![fade(), fade(), fade()], 0.5, 8, 3000, RateFunc::Linear)
            .unwrap();
        // Mitad global (t=1000): la 1ª terminada, la 2ª a mitad y la 3ª
        // recién empezada (sub-alpha 0, opacidad 0): solape real de 3.
        let mut out = Vec::new();
        g.sample_en(0.5, &mut out).unwrap();
        assert_eq!(out.len(), 3, "las 3 empezadas, fueron {}", out.len());
        assert_eq!(out[0].opacity, 1.0);
        assert!((out[1].opacity - 0.5).abs() < 1e-9);
        assert_eq!(out[2].opacity, 0.0);
        // Final: las 3 empezadas en estado final.
        let fin = g.sample_at(1.0).unwrap();
        assert_eq!(fin.len(), 3);
        assert!(fin.iter().all(|p| p.opacity == 1.0));
        // Inicio: solo la primera (alfa 0).
        let ini = g.sample_at(0.0).unwrap();
        assert_eq!(ini.len(), 1);
        assert_eq!(ini[0].opacity, 0.0);
        // Lag 0 = todo junto; constructores inválidos fallan honesto.
        let junto =
            LaggedStart::try_new(vec![fade(), fade()], 0.0, 4, 2000, RateFunc::Linear).unwrap();
        assert_eq!(junto.sample_at(0.5).unwrap().len(), 2);
        assert!(LaggedStart::try_new(vec![], 0.5, 4, 1000, RateFunc::Linear).is_err());
        assert!(LaggedStart::try_new(vec![fade()], 2.0, 4, 1000, RateFunc::Linear).is_err());
    }

    #[test]
    fn succession_activa_una_por_vez() {
        let fade = || Animation::fade_in(punto(), 4, 1000, RateFunc::Linear).unwrap();
        let s =
            Succession::try_new(vec![fade(), fade(), fade()], 9, 3000, RateFunc::Linear).unwrap();
        assert_eq!(s.indice_activo(0.0), 0);
        assert_eq!(s.indice_activo(0.4), 1);
        assert_eq!(s.indice_activo(0.8), 2);
        assert_eq!(s.indice_activo(1.0), 2);
        // La activa del medio va a mitad de opacidad.
        let p = s.sample_at(0.5).unwrap();
        assert!((p.opacity - 0.5).abs() < 1e-9);
        assert!(Succession::try_new(vec![], 4, 1000, RateFunc::Linear).is_err());
    }

    #[test]
    fn transform_interpola_centroides() {
        // Viaje [0,0]→[10,0]: el centroide va 1→11 con rate lineal.
        // Nota honesta: `TransformAnim::frame_at` con camino recto indexa
        // el set histórico (cuantiza a la grilla de frames), así que el
        // punto medio cae a medio paso de la grilla como máximo.
        let t = Animation::transform(
            vec![[0.0, 0.0], [2.0, 0.0]],
            vec![[10.0, 0.0], [12.0, 0.0]],
            8,
            48,
            1000,
            RateFunc::Linear,
            false,
        )
        .unwrap();
        let c = |a: f64| centroide_de(&t.sample(a).unwrap().mobject);
        let c0 = c(0.0);
        let c05 = c(0.5);
        let c1 = c(1.0);
        assert!((c0[0] - 1.0).abs() < 1e-9, "got {c0:?}");
        assert!((c05[0] - 6.0).abs() < 0.12, "got {c05:?}");
        assert!((c1[0] - 11.0).abs() < 1e-9, "got {c1:?}");
        assert!(c05[1].abs() < 1e-9);
        // Monótono no-decreciente en X sobre la grilla.
        let mut anterior = c0[0];
        for k in 0..=48 {
            let x = c(f64::from(k) / 48.0)[0];
            assert!(x + 1e-9 >= anterior, "retrocede en k={k}");
            anterior = x;
        }
        // Extremos exactos + scratch reutilizado da lo mismo.
        let mut scratch = Scratch::nuevo();
        let p1 = t.sample_con(0.5, &mut scratch).unwrap();
        let p2 = t.sample_con(1.0, &mut scratch).unwrap();
        assert_eq!(p1.mobject, t.sample(0.5).unwrap().mobject);
        assert!((centroide_de(&p2.mobject)[0] - 11.0).abs() < 1e-9);
    }

    #[test]
    fn circumscribe_dibuja_sostiene_y_borra() {
        let caja = Mobject::Rectangle {
            cx: 0.0,
            cy: 0.0,
            w: 4.0,
            h: 2.0,
        };
        let c = Animation::circumscribe(
            caja,
            8,
            1000,
            RateFunc::Linear,
            FormaResalto::Rectangulo,
            0.1,
            0.3,
            false,
            false,
        )
        .unwrap();
        // Mitad: anillo completo opaco (4 esquinas + cierre = 5 pts).
        let medio = c.sample(0.5).unwrap();
        assert_eq!(medio.opacity, 1.0);
        assert!(matches!(medio.mobject, Mobject::Polygon { .. }));
        // Inicio/fin (tw=0.3): casi vacío al principio y al final.
        let ini = c.sample(0.0).unwrap();
        let fin = c.sample(1.0).unwrap();
        let n = |p: &PlacedMobject| match &p.mobject {
            Mobject::Polygon { pts } => pts.len(),
            _ => 0,
        };
        assert!(n(&ini) < n(&medio), "dibuja progresivo");
        assert!(n(&fin) < n(&medio), "borra progresivo");
        // Con fades el anillo viaja entero y manda la opacidad.
        let f = Animation::circumscribe(
            punto(),
            4,
            500,
            RateFunc::Linear,
            FormaResalto::Circulo,
            0.1,
            0.3,
            true,
            true,
        )
        .unwrap();
        assert!(f.sample(0.0).unwrap().opacity < f.sample(0.5).unwrap().opacity);
        assert!(f.sample(1.0).unwrap().opacity < f.sample(0.5).unwrap().opacity);
        assert!(Animation::circumscribe(
            punto(),
            4,
            500,
            RateFunc::Linear,
            FormaResalto::Rectangulo,
            f64::NAN,
            0.3,
            false,
            false,
        )
        .is_err());
    }

    #[test]
    fn fade_shift_viaja_y_aparece() {
        let f = Animation::fade_shift(punto(), [4.0, 0.0], 4, 500, RateFunc::Linear).unwrap();
        let p0 = f.sample(0.0).unwrap();
        let p1 = f.sample(1.0).unwrap();
        assert_eq!(p0.opacity, 0.0);
        assert_eq!(p0.center, [4.0, 0.0]);
        assert_eq!(p1.opacity, 1.0);
        assert_eq!(p1.center, [0.0, 0.0]);
        assert!(
            Animation::fade_shift(punto(), [f64::INFINITY, 0.0], 4, 500, RateFunc::Linear).is_err()
        );
    }

    #[test]
    fn anim_constructors_validan_presupuestos() {
        assert!(Animation::create(vec![], 4, 1000, RateFunc::Linear, false).is_err());
        assert!(Animation::fade_in(punto(), 0, 1000, RateFunc::Linear).is_err());
        assert!(Animation::fade_out(punto(), 49, 1000, RateFunc::Linear).is_err());
        assert!(Animation::write(punto(), 4, 99, RateFunc::Linear).is_err());
        assert!(Animation::transform(
            vec![[0.0, 0.0]],
            vec![[1.0, 1.0]],
            1,
            4,
            1000,
            RateFunc::Linear,
            false
        )
        .is_err());
        assert!(Animation::indicate(punto(), 4, 1000, RateFunc::Linear).is_ok());
        // frames()/run_ms()/rate() accesores.
        let a = Animation::indicate(punto(), 5, 2000, RateFunc::Smooth).unwrap();
        assert_eq!(a.frames(), 5);
        assert_eq!(a.run_ms(), 2000);
        assert_eq!(a.rate(), RateFunc::Smooth);
    }
}
